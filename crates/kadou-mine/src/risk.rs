//! Risk defaulting before a human ever sees the draft (`docs/design/06-session-mining.md`
//! §4.4). The miner never assigns `low` or `critical` -- a mined script is unreviewed, and
//! `critical` is a human's own choice.

use kadou_core::RiskLevel;

#[cfg(test)]
mod tests;

/// Risk for one `(tool, subcommand-token)` pair the template's argv0 and token set match, or
/// `None` when this tool contributes no opinion (falls through to the caller's catch-all).
/// Token-based rather than substring-based: `kubectl --context $CONTEXT -n $NAMESPACE get
/// pods` must still recognize `get` even though it is not adjacent to `kubectl` (`06` §4.4).
fn tool_risk(tool: &str, has: impl Fn(&str) -> bool) -> Option<RiskLevel> {
    match tool {
        "kubectl" => {
            if has("apply") || has("delete") {
                Some(RiskLevel::High)
            } else if has("get") || has("describe") || has("logs") {
                Some(RiskLevel::Medium)
            } else {
                None
            }
        }
        "helm" => {
            if has("upgrade") || has("uninstall") {
                Some(RiskLevel::High)
            } else if has("template") || has("status") {
                Some(RiskLevel::Medium)
            } else {
                None
            }
        }
        "terraform" if has("apply") => Some(RiskLevel::High),
        "docker" if has("push") => Some(RiskLevel::High),
        "oci" if has("delete") => Some(RiskLevel::High),
        "mongo" | "mongosh" => Some(RiskLevel::High),
        "git" if has("log") || has("status") || has("diff") => Some(RiskLevel::Medium),
        _ => None,
    }
}

/// Defaults a mined cluster's risk from its template's verb shape (`06` §4.4). Never returns
/// `Low` or `Critical`. An unparseable or mixed-signal template -- an unrecognized tool, or a
/// recognized tool with no matching subcommand -- defaults to `High`, the same as the doc's
/// own catch-all row.
pub fn default_risk(template: &str) -> RiskLevel {
    let tokens: Vec<String> = template.split_whitespace().map(str::to_lowercase).collect();
    let Some(tool) = tokens.first() else {
        return RiskLevel::High;
    };
    let has = |word: &str| tokens.iter().any(|t| t == word);
    tool_risk(tool, has).unwrap_or(RiskLevel::High)
}
