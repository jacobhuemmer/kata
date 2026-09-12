//! Redaction rules R1-R13 (`docs/design/06-session-mining.md` §4.2) plus the
//! `redact-extra.txt` customer/environment denylist (R9) and a fail-closed high-entropy check
//! (R14) that drops a candidate outright rather than trust an unproven redaction (`06` §4.1
//! "If redaction cannot be proven, the candidate is dropped").
//!
//! Every stage after extraction runs text through [`redact_all`] before it can reach the mine
//! queue, `meta.json`, `audit.jsonl`, or any log file (§4.1's never-written list).

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

pub fn redact_all(text: &str, extra_terms: &[String]) -> Redacted {
    todo!("R1-R13 in order: {text} {extra_terms:?}")
}

/// R14: `true` when `redacted_text` still contains something that looks like an unredacted
/// secret after R1-R13 ran -- a long high-entropy run that isn't a path. The caller drops the
/// whole candidate when this is `true` (fail closed, `06` §4.1/§4.3).
pub fn has_high_entropy_leak(redacted_text: &str) -> bool {
    todo!("{redacted_text}")
}
