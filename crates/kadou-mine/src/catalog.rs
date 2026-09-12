//! Catalog-collision lookup for the rank penalty (`docs/design/06-session-mining.md` §2.6:
//! `catalog_penalty # 0.1 if an existing runbook already covers argv0+subcommand`). D6
//! (`docs/design/12-mvp-review.md` §3): the formula already existed in [`crate::rank`], but
//! its one call site hard-coded `false` rather than actually looking the library up.

use std::path::Path;

#[cfg(test)]
mod tests;

/// How many tokens after an `argv0` occurrence to search for the subcommand -- enough to
/// skip a couple of `--flag value` pairs without treating an unrelated later invocation of
/// the same tool as a match.
const SUBCOMMAND_LOOKAHEAD: usize = 6;

/// The template's argv0 and its first non-flag, non-placeholder word (`06` §2.4 keeps flag
/// names and argv0 literal, and an ordinary subcommand keyword like `get`/`upgrade` is left
/// literal too -- it is exactly the token this function is looking for). `None` when the
/// template has no such second word to call a subcommand.
pub fn argv0_and_subcommand(template: &str) -> Option<(&str, &str)> {
    let mut words = template.split_whitespace();
    let argv0 = words.next()?;
    let subcommand = words.find(|w| !w.starts_with('-') && !w.starts_with('$'))?;
    Some((argv0, subcommand))
}

/// `true` when `source`'s tokens contain `argv0` followed, within [`SUBCOMMAND_LOOKAHEAD`]
/// tokens, by `subcommand` -- a cheap, conservative proxy for "this script already invokes
/// this tool's subcommand", tolerant of flags in between (`kubectl --context prod get pods`).
fn contains_token_pair(source: &str, argv0: &str, subcommand: &str) -> bool {
    let tokens: Vec<&str> = source.split_whitespace().collect();
    tokens.iter().enumerate().any(|(i, &t)| {
        t == argv0
            && tokens[i + 1..]
                .iter()
                .take(SUBCOMMAND_LOOKAHEAD)
                .any(|&w| w == subcommand)
    })
}

/// `true` when some kata already scanned under `kata_dir` already covers `template`'s
/// argv0+subcommand (`06` §2.6). Uses [`kadou_core::scan_kata_dir`] -- the same scanner
/// `kadou check`/`kadou list` use -- rather than kadou-mine re-implementing directory walking
/// or depending on kadou-mcp's types (`06` §9 slice 9: "no MCP types").
pub fn covers(kata_dir: &Path, template: &str) -> bool {
    let Some((argv0, subcommand)) = argv0_and_subcommand(template) else {
        return false;
    };
    let Ok(folders) = kadou_core::scan_kata_dir(kata_dir) else {
        return false;
    };
    folders
        .iter()
        .flat_map(|(_, files)| files)
        .any(|f| contains_token_pair(&f.source, argv0, subcommand))
}
