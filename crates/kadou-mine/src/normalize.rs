//! Template + parameter-map normalization (`docs/design/06-session-mining.md` §2.4). Produces
//! the only form later stages persist besides the redacted proposal: argv0 and flag names are
//! kept; paths, ids, timestamps, numbers, quoted values, IPs, hostnames, emails, and ticket
//! ids become template tokens (`$HOME`, `$ROOT`, `$PATH_N`, `$ID`, `$TS`, `$N`, `$VAL`,
//! `$IP`, `$HOST`, `$EMAIL`, `$TICKET`), and a small set of well-known flags/bare `key=value`
//! tokens get a named placeholder instead (`--namespace` / `-n` -> `$NAMESPACE`, a bare
//! `app=api` -> `app=$APP`) so the worked example in `06` §6.2 round-trips.

use std::collections::BTreeMap;

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

/// Normalizes one shell command into a clustering template plus its parameter map
/// (`06` §2.4). `home`/`root` are absolute-path prefixes already known to the caller (a
/// real `$HOME`, and the session's repo/worktree root) so those paths collapse to the stable
/// `$HOME`/`$ROOT` tokens instead of a numbered `$PATH_N`.
pub fn normalize_command(command: &str, home: Option<&str>, root: Option<&str>) -> Normalized {
    todo!("{command} {home:?} {root:?}")
}
