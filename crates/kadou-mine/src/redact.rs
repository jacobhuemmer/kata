//! Redaction rules R1-R13 (`docs/design/06-session-mining.md` §4.2) plus the
//! `redact-extra.txt` customer/environment denylist (R9) and a fail-closed high-entropy check
//! (R14) that drops a candidate outright rather than trust an unproven redaction (`06` §4.1
//! "If redaction cannot be proven, the candidate is dropped").
//!
//! Every stage after extraction runs text through [`redact_all`] before it can reach the mine
//! queue, `meta.json`, `audit.jsonl`, or any log file (§4.1's never-written list).

use std::sync::LazyLock;

use regex::{Captures, Regex};

#[cfg(test)]
mod tests;

/// One redaction rule's id, exactly as used in tests and failure logs (`06` §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleId {
    PemPrivateKey,
    Bearer,
    Assign,
    KnownTokens,
    Connection,
    Email,
    Hostname,
    Ip,
    Customer,
    Kubeconfig,
    OpRef,
    Home,
    SessionQuote,
    HighEntropy,
}

impl RuleId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PemPrivateKey => "R1_PEM",
            Self::Bearer => "R2_BEARER",
            Self::Assign => "R3_ASSIGN",
            Self::KnownTokens => "R4_KNOWN_TOKENS",
            Self::Connection => "R5_CONNECTION",
            Self::Email => "R6_EMAIL",
            Self::Hostname => "R7_HOSTNAME",
            Self::Ip => "R8_IP",
            Self::Customer => "R9_CUSTOMER",
            Self::Kubeconfig => "R10_KUBECONFIG",
            Self::OpRef => "R11_OP_REF",
            Self::Home => "R12_HOME",
            Self::SessionQuote => "R13_SESSION_QUOTE",
            Self::HighEntropy => "R14_HIGH_ENTROPY",
        }
    }
}

/// Text after every rule has run, plus which rules actually matched something (used for
/// `redaction-failures.jsonl` rows and tests).
#[derive(Debug, Clone)]
pub struct Redacted {
    pub text: String,
    pub rules_hit: Vec<RuleId>,
}

/// Domains R7 allows through as public documentation hosts, never rewritten to `$HOST`
/// (`06` §4.2 "Allowlist for R7").
const HOSTNAME_ALLOWLIST: [&str; 6] = [
    "github.com",
    "gitlab.com",
    "kubernetes.io",
    "go.dev",
    "pkg.go.dev",
    "developer.hashicorp.com",
];

/// Compiles a hand-written, unit-tested regex literal fixed at compile time. A bad pattern
/// here is a compile-time-caught bug (this module's own test suite exercises every one of
/// them), never something a runtime input can trigger -- the one documented `expect` carve-out
/// for this module (workspace `Cargo.toml`'s panic-hygiene note; `08` R6).
#[allow(clippy::expect_used)]
fn static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("redact.rs regex literal must compile")
}

static PEM: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----")
});

static BEARER: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"(?i)(authorization\s*:\s*)(?:bearer|basic)\s+\S+"));

static ASSIGN: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"(?i)\b([a-z0-9_-]*(?:api[_-]?key|token|secret|password|passwd|authorization)[a-z0-9_-]*)\s*[:=]\s*(\S{8,})",
    )
});

static KNOWN_TOKENS: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(concat!(
        r"sk-[A-Za-z0-9]{16,}",
        r"|ghp_[A-Za-z0-9]{36,}",
        r"|gho_[A-Za-z0-9]{36,}",
        r"|github_pat_[A-Za-z0-9_]{20,}",
        r"|xox[baprs]-[A-Za-z0-9-]+",
        r"|glpat-[A-Za-z0-9_-]{20,}",
        r"|AKIA[0-9A-Z]{16}",
        r"|AIza[0-9A-Za-z_-]{35}",
        r"|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
    ))
});

static CONNECTION_DSN: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"(?i)(postgres(?:ql)?|mongodb(?:\+srv)?|redis|amqp)://[^:/@\s]+:[^@/\s]+@([^/\s]+)(/[^\s]*)?",
    )
});

static CONNECTION_URL_CREDS: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"(?i)(https?)://[^:/@\s]+:[^@/\s]+@([^/\s]+)"));

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"[A-Za-z0-9.+_-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"));

// Requires 3+ labels (2+ dots) so an ordinary two-part filename (`run.sh`, `config.yaml`) or
// the R7 allowlist's own two-label domains never enter the match at all -- a real internal
// FQDN worth redacting is `api.customer.example`, not `run.sh`.
static HOSTNAME: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r"\b(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.){2,}[a-zA-Z]{2,}\b")
});

static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\b",
    )
});

static IPV6: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"\b(?:[0-9a-fA-F]{1,4}:){2,7}[0-9a-fA-F]{1,4}\b"));

static KUBECONFIG_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r"(?i)(certificate-authority-data|client-key-data|token)\s*:\s*\S+")
});

static OP_REF: LazyLock<Regex> = LazyLock::new(|| static_regex(r"op://\S+"));

static HOME_PATH: LazyLock<Regex> = LazyLock::new(|| static_regex(r"(?:/Users/|/home/)[^/\s]+"));

static SESSION_QUOTE_LINE: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"^\s*(Human|Assistant|System)\s*:|^\s*#{1,4}\s*Turn\b"));

static HIGH_ENTROPY: LazyLock<Regex> = LazyLock::new(|| static_regex(r"[A-Za-z0-9+/=_-]{24,}"));

fn hostname_allowed(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    HOSTNAME_ALLOWLIST
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// Runs one rule's transform over `out` and, if it changed anything, records `rule` as hit.
fn step(
    out: &mut String,
    rules_hit: &mut Vec<RuleId>,
    rule: RuleId,
    transform: impl FnOnce(&str) -> Option<String>,
) {
    if let Some(next) = transform(out) {
        *out = next;
        rules_hit.push(rule);
    }
}

/// R13: drops lines that look like a copied chat turn outright, before any other rule scans
/// their content.
fn drop_session_quote_lines(text: &str) -> Option<String> {
    let mut kept = Vec::new();
    let mut dropped = false;
    for line in text.lines() {
        if SESSION_QUOTE_LINE.is_match(line) {
            dropped = true;
        } else {
            kept.push(line);
        }
    }
    if !dropped {
        return None;
    }
    let mut joined = kept.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    Some(joined)
}

fn redact_pem(text: &str) -> Option<String> {
    PEM.is_match(text)
        .then(|| PEM.replace_all(text, "[REDACTED PRIVATE KEY]").into_owned())
}

fn redact_bearer(text: &str) -> Option<String> {
    BEARER
        .is_match(text)
        .then(|| BEARER.replace_all(text, "${1}[REDACTED]").into_owned())
}

fn redact_assign(text: &str) -> Option<String> {
    ASSIGN
        .is_match(text)
        .then(|| ASSIGN.replace_all(text, "${1}=[REDACTED]").into_owned())
}

fn redact_known_tokens(text: &str) -> Option<String> {
    KNOWN_TOKENS
        .is_match(text)
        .then(|| KNOWN_TOKENS.replace_all(text, "[REDACTED]").into_owned())
}

/// R5: DSN-shaped (`scheme://user:pass@host/db`) and bare URL-credential
/// (`https://user:pass@host`) connection strings both drop the password and rewrite the host.
fn redact_connection(text: &str) -> Option<String> {
    let mut hit = false;
    let mut out = text.to_string();
    if CONNECTION_DSN.is_match(&out) {
        hit = true;
        out = CONNECTION_DSN
            .replace_all(&out, |caps: &Captures| {
                let scheme = &caps[1];
                match caps.get(3) {
                    Some(_) => format!("{scheme}://$USER@$HOST/$DB"),
                    None => format!("{scheme}://$USER@$HOST"),
                }
            })
            .into_owned();
    }
    if CONNECTION_URL_CREDS.is_match(&out) {
        hit = true;
        out = CONNECTION_URL_CREDS
            .replace_all(&out, |caps: &Captures| {
                format!("{}://$USER@$HOST", &caps[1])
            })
            .into_owned();
    }
    hit.then_some(out)
}

fn redact_email(text: &str) -> Option<String> {
    EMAIL
        .is_match(text)
        .then(|| EMAIL.replace_all(text, "$$EMAIL").into_owned())
}

fn redact_hostname(text: &str) -> Option<String> {
    let mut hit = false;
    let out = HOSTNAME
        .replace_all(text, |caps: &Captures| {
            let matched = &caps[0];
            if hostname_allowed(matched) {
                matched.to_string()
            } else {
                hit = true;
                "$HOST".to_string()
            }
        })
        .into_owned();
    hit.then_some(out)
}

fn redact_ip(text: &str) -> Option<String> {
    let mut hit = false;
    let mut out = text.to_string();
    if IPV4.is_match(&out) {
        hit = true;
        out = IPV4.replace_all(&out, "$$IP").into_owned();
    }
    if IPV6.is_match(&out) {
        hit = true;
        out = IPV6.replace_all(&out, "$$IP").into_owned();
    }
    hit.then_some(out)
}

/// R9: the `redact-extra.txt` denylist of customer/environment terms (`06` §4.2), a plain
/// substring replace since these are exact operator-configured words, not a pattern shape.
fn redact_customer(text: &str, extra_terms: &[String]) -> Option<String> {
    let mut hit = false;
    let mut out = text.to_string();
    for term in extra_terms.iter().filter(|t| !t.is_empty()) {
        if out.contains(term.as_str()) {
            hit = true;
            out = out.replace(term.as_str(), "$CUSTOMER");
        }
    }
    hit.then_some(out)
}

fn redact_kubeconfig(text: &str) -> Option<String> {
    KUBECONFIG_FIELD.is_match(text).then(|| {
        KUBECONFIG_FIELD
            .replace_all(text, "${1}: [REDACTED]")
            .into_owned()
    })
}

fn redact_op_ref(text: &str) -> Option<String> {
    OP_REF
        .is_match(text)
        .then(|| OP_REF.replace_all(text, "[REDACTED OP]").into_owned())
}

fn redact_home(text: &str) -> Option<String> {
    HOME_PATH
        .is_match(text)
        .then(|| HOME_PATH.replace_all(text, "$$HOME").into_owned())
}

/// Runs redaction rules R1-R13 in order, plus R9's `redact-extra.txt` terms, over `text`.
/// Returns the redacted text and which rules actually matched something.
pub fn redact_all(text: &str, extra_terms: &[String]) -> Redacted {
    let mut rules_hit = Vec::new();
    let mut out = text.to_string();

    step(
        &mut out,
        &mut rules_hit,
        RuleId::SessionQuote,
        drop_session_quote_lines,
    );
    step(&mut out, &mut rules_hit, RuleId::PemPrivateKey, redact_pem);
    step(&mut out, &mut rules_hit, RuleId::Bearer, redact_bearer);
    step(&mut out, &mut rules_hit, RuleId::Assign, redact_assign);
    step(
        &mut out,
        &mut rules_hit,
        RuleId::KnownTokens,
        redact_known_tokens,
    );
    step(
        &mut out,
        &mut rules_hit,
        RuleId::Connection,
        redact_connection,
    );
    step(&mut out, &mut rules_hit, RuleId::Email, redact_email);
    step(&mut out, &mut rules_hit, RuleId::Hostname, redact_hostname);
    step(&mut out, &mut rules_hit, RuleId::Ip, redact_ip);
    step(&mut out, &mut rules_hit, RuleId::Customer, |t| {
        redact_customer(t, extra_terms)
    });
    step(
        &mut out,
        &mut rules_hit,
        RuleId::Kubeconfig,
        redact_kubeconfig,
    );
    step(&mut out, &mut rules_hit, RuleId::OpRef, redact_op_ref);
    step(&mut out, &mut rules_hit, RuleId::Home, redact_home);

    Redacted {
        text: out,
        rules_hit,
    }
}

/// R14: `true` when `redacted_text` still contains something that looks like an unredacted
/// secret after R1-R13 ran -- a run of 24+ base64/token-alphabet characters that is not a
/// path or a `--flag` (`06` §4.3: "strings ... that are not paths or `--flags`"). The caller
/// drops the whole candidate when this is `true` (fail closed, `06` §4.1/§4.3).
pub fn has_high_entropy_leak(redacted_text: &str) -> bool {
    HIGH_ENTROPY
        .find_iter(redacted_text)
        .any(|m| !looks_like_a_path_or_flag(redacted_text, m.start(), m.as_str()))
}

/// `true` when a [`HIGH_ENTROPY`] match is a path or a `--flag`, not a bare secret blob (`06`
/// §4.3). `/` is itself one of `HIGH_ENTROPY`'s own matched characters, and standard base64's
/// alphabet includes `/` too -- a single slash absorbed into the match is well within chance
/// for a real secret over a 24+ char run (P2/L2, `docs/design/12-mvp-review.md` §2, §6), so it
/// is not enough evidence on its own. Two or more slashes is what actually distinguishes a
/// multi-segment filesystem path; a single slash is only trusted when the character
/// immediately *before* the match (not part of the match itself, since `$` isn't in
/// `HIGH_ENTROPY`'s class) is `$` -- a variable-prefixed reference like `$PATH_1/segment`.
fn looks_like_a_path_or_flag(text: &str, match_start: usize, matched: &str) -> bool {
    if matched.starts_with("--") {
        return true;
    }
    if matched.matches('/').count() >= 2 {
        return true;
    }
    match_start > 0 && text.as_bytes()[match_start - 1] == b'$'
}
