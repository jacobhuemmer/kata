//! Template + parameter-map normalization (`docs/design/06-session-mining.md` §2.4). Produces
//! the only form later stages persist besides the redacted proposal: argv0 and flag names are
//! kept; paths, ids, timestamps, numbers, quoted values, IPs, hostnames, emails, and ticket
//! ids become template tokens (`$HOME`, `$ROOT`, `$PATH_N`, `$ID`, `$TS`, `$N`, `$VAL`,
//! `$IP`, `$HOST`, `$EMAIL`, `$TICKET`), and a small set of well-known flags/bare `key=value`
//! tokens get a named placeholder instead (`--namespace` / `-n` -> `$NAMESPACE`, a bare
//! `app=api` -> `app=$APP`) so the worked example in `06` §6.2 round-trips.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

#[cfg(test)]
mod tests;

/// One normalized command: the clustering template, and the concrete value each placeholder
/// stood for (used later to seed heuristic arg defaults; never persisted raw past propose).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub template: String,
    /// Placeholder name (without `$`) -> the literal value it replaced, in first-seen order.
    pub params: BTreeMap<String, String>,
}

/// Flags whose value gets a named placeholder instead of a shape-based one. Kept short and
/// explicit (`06` §2.4's only named example is `--namespace`); anything else falls through to
/// shape classification, which is why `--tail=200`'s value becomes `$N`, not `$TAIL`. The
/// short form `-n` is scoped to `kubectl`-shaped commands: elsewhere (`tail -n`, `sed -n`,
/// `grep -n`) it means something else entirely, and only the long flag name is safe to treat
/// as unambiguous across every argv0.
fn named_flag(argv0: &str, flag: &str) -> Option<&'static str> {
    match flag {
        "--namespace" => Some("NAMESPACE"),
        "--context" => Some("CONTEXT"),
        "-n" if argv0 == "kubectl" => Some("NAMESPACE"),
        _ => None,
    }
}

#[allow(clippy::expect_used)]
fn static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("normalize.rs regex literal must compile")
}

static UUID_OR_SHA: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"^(?:[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|[0-9a-fA-F]{7,40})$",
    )
});

static RFC3339: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}"));

static INTEGER: LazyLock<Regex> = LazyLock::new(|| static_regex(r"^-?\d+$"));

static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"^(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)$",
    )
});

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"^[A-Za-z0-9.+_-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$"));

static HOSTNAME: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.){2,}[a-zA-Z]{2,}$")
});

static TICKET: LazyLock<Regex> = LazyLock::new(|| static_regex(r"^[A-Z]{2,10}-\d+$"));

static BARE_KV: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"^([a-zA-Z][a-zA-Z0-9_-]*)=(.+)$"));

/// Splits on whitespace while keeping single/double-quoted spans intact (the quote characters
/// stay in the returned token; classification strips them). Good enough for the shell one-
/// liners and `kubectl`-shaped commands mining actually sees -- not a POSIX shell parser.
fn split_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in command.chars() {
        match quote {
            Some(q) if c == q => {
                current.push(c);
                quote = None;
            }
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                current.push(c);
            }
            None if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            None => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn strip_quotes(token: &str) -> Option<&str> {
    let bytes = token.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'\'' || first == b'"') && first == last {
            return Some(&token[1..token.len() - 1]);
        }
    }
    None
}

fn sanitize_param_name(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Home-relative and repo-root-relative absolute paths get the stable `$HOME`/`$ROOT` tokens;
/// any other absolute path gets a per-template `$PATH_N` counter (`06` §2.4).
struct PathTokens {
    counter: usize,
    seen: BTreeMap<String, String>,
}

impl PathTokens {
    fn new() -> Self {
        Self {
            counter: 0,
            seen: BTreeMap::new(),
        }
    }

    fn token_for(&mut self, path: &str) -> String {
        if let Some(existing) = self.seen.get(path) {
            return existing.clone();
        }
        self.counter += 1;
        let token = format!("$PATH_{}", self.counter);
        self.seen.insert(path.to_string(), token.clone());
        token
    }
}

fn classify_absolute_path(
    path: &str,
    home: Option<&str>,
    root: Option<&str>,
    paths: &mut PathTokens,
) -> String {
    if let Some(home) = home
        && let Some(rest) = path.strip_prefix(home)
    {
        return format!("$HOME{rest}");
    }
    if let Some(root) = root
        && let Some(rest) = path.strip_prefix(root)
    {
        return format!("$ROOT{rest}");
    }
    paths.token_for(path)
}

/// Classifies `value` by shape (path, id, timestamp, integer, ticket, email, IP, hostname).
/// Returns `None` when nothing matches -- an ordinary word like a subcommand name (`get`,
/// `checkout`) or an argument kadou doesn't have a pattern for, which the caller keeps
/// literal rather than genericizing into `$VAL` (`06` §2.4: `$VAL` is specifically for
/// "single-quoted / double-quoted values", not every unrecognized bare word).
fn classify_shaped_value(value: &str) -> Option<(String, String)> {
    if RFC3339.is_match(value) {
        return Some(("$TS".to_string(), "TS".to_string()));
    }
    if INTEGER.is_match(value) {
        // A 9+ digit run at this magnitude is a unix timestamp on this planet for the next
        // few centuries, not an ordinary count/flag value (`06` §2.4 "RFC3339 / unix
        // timestamps"). Checked before the hex/SHA pattern below: a purely-decimal run is a
        // number, never a hash, even though every decimal digit is also a valid hex digit.
        if value.trim_start_matches('-').len() >= 9 {
            return Some(("$TS".to_string(), "TS".to_string()));
        }
        return Some(("$N".to_string(), "N".to_string()));
    }
    if UUID_OR_SHA.is_match(value) {
        return Some(("$ID".to_string(), "ID".to_string()));
    }
    if TICKET.is_match(value) {
        return Some(("$TICKET".to_string(), "TICKET".to_string()));
    }
    if EMAIL.is_match(value) {
        return Some(("$EMAIL".to_string(), "EMAIL".to_string()));
    }
    if IPV4.is_match(value) {
        return Some(("$IP".to_string(), "IP".to_string()));
    }
    if HOSTNAME.is_match(value) {
        return Some(("$HOST".to_string(), "HOST".to_string()));
    }
    None
}

/// Bundles the per-command context (argv0, path bases, and the params accumulated so far) so
/// the per-word helpers below don't each need a five-argument signature.
struct NormalizeCtx<'a> {
    argv0: &'a str,
    home: Option<&'a str>,
    root: Option<&'a str>,
    paths: PathTokens,
    params: BTreeMap<String, String>,
}

impl<'a> NormalizeCtx<'a> {
    fn record(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.params.insert(name.into(), value.into());
    }

    /// A `--flag=value` token: named-flag, then path, then shape, then unchanged.
    fn long_flag_eq(&mut self, flag_name: &str, value: &str) -> String {
        if let Some(name) = named_flag(self.argv0, &format!("--{flag_name}")) {
            self.record(name, value);
            return format!("--{flag_name}=${name}");
        }
        if value.starts_with('/') {
            let token = classify_absolute_path(value, self.home, self.root, &mut self.paths);
            return format!("--{flag_name}={token}");
        }
        match classify_shaped_value(value) {
            Some((token, param_name)) => {
                self.record(param_name, value);
                format!("--{flag_name}={token}")
            }
            None => format!("--{flag_name}={value}"),
        }
    }

    /// A bare (unquoted, non-flag, non-`key=value`) token: path, then shape, then unchanged
    /// literal -- an ordinary subcommand keyword is not genericized (`06` §2.4).
    fn bare_word(&mut self, word: &str) -> String {
        if word.starts_with('/') {
            return classify_absolute_path(word, self.home, self.root, &mut self.paths);
        }
        match classify_shaped_value(word) {
            Some((token, param_name)) => {
                self.record(param_name, word);
                token
            }
            None => word.to_string(),
        }
    }
}

/// Normalizes one shell command into a clustering template plus its parameter map
/// (`06` §2.4). `home`/`root` are absolute-path prefixes already known to the caller (a
/// real `$HOME`, and the session's repo/worktree root) so those paths collapse to the stable
/// `$HOME`/`$ROOT` tokens instead of a numbered `$PATH_N`.
pub fn normalize_command(command: &str, home: Option<&str>, root: Option<&str>) -> Normalized {
    let words = split_words(command);
    let argv0 = words.first().map(String::as_str).unwrap_or_default();
    let mut ctx = NormalizeCtx {
        argv0,
        home,
        root,
        paths: PathTokens::new(),
        params: BTreeMap::new(),
    };
    let mut out_words: Vec<String> = Vec::with_capacity(words.len());
    let mut pending_named_flag: Option<&'static str> = None;

    for (i, word) in words.iter().enumerate() {
        if i == 0 {
            out_words.push(word.clone());
            continue;
        }

        if let Some(name) = pending_named_flag.take() {
            let value = strip_quotes(word).unwrap_or(word).to_string();
            ctx.record(name, value);
            out_words.push(format!("${name}"));
            continue;
        }

        if let Some(flag) = word.strip_prefix("--")
            && let Some((flag_name, value)) = flag.split_once('=')
        {
            out_words.push(ctx.long_flag_eq(flag_name, value));
            continue;
        }

        if word.starts_with('-') && named_flag(argv0, word).is_some() {
            pending_named_flag = named_flag(argv0, word);
            out_words.push(word.clone());
            continue;
        }

        if word.starts_with('-') {
            // An unrecognized flag: kept verbatim (`06` §2.4 "Keep flag names").
            out_words.push(word.clone());
            continue;
        }

        if let Some(caps) = BARE_KV.captures(word) {
            let key = &caps[1];
            let value = &caps[2];
            let name = sanitize_param_name(key);
            ctx.record(name.clone(), value.to_string());
            out_words.push(format!("{key}=${name}"));
            continue;
        }

        if let Some(inner) = strip_quotes(word) {
            // A quoted token is a value by construction (`06` §2.4): fall back to the
            // generic `$VAL` when its content matches no more specific shape.
            let (token, param_name) = classify_shaped_value(inner)
                .unwrap_or_else(|| ("$VAL".to_string(), "VAL".to_string()));
            ctx.record(param_name, inner.to_string());
            out_words.push(token);
            continue;
        }

        out_words.push(ctx.bare_word(word));
    }

    Normalized {
        template: out_words.join(" "),
        params: ctx.params,
    }
}
