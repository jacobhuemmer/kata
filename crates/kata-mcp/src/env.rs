//! The MCP child environment allowlist (`docs/design/05-prd.md` §6.1 "MCP child environment").
//!
//! Unlike the CLI (which keeps the full parent environment — a human's own shell context),
//! an MCP-spawned child starts from an explicit allowlist plus `[exec] pass_env`. Resolved
//! args and needs are layered on top by the caller ([`crate::tools::run_kata`]), same as the
//! CLI path.

const ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "TZ",
    "TMPDIR",
    "SSH_AUTH_SOCK",
    "KUBECONFIG",
];

/// Builds the MCP child's starting environment from `parent_env` (the server process's own
/// environment): the fixed allowlist, any `LC_*` variable, and anything named in `pass_env`
/// (§6.1 "an operator-editable `[exec] pass_env = []` for anything else a team folder
/// genuinely needs"), plus a fixed `TERM=dumb`. `KADOU_*` context vars and resolved
/// args/needs are added by the caller afterward.
pub fn build_mcp_env(
    parent_env: impl IntoIterator<Item = (String, String)>,
    pass_env: &[String],
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = parent_env
        .into_iter()
        .filter(|(name, _)| {
            ALLOWLIST.contains(&name.as_str())
                || name.starts_with("LC_")
                || pass_env.iter().any(|p| p == name)
        })
        .collect();
    out.push(("TERM".to_string(), "dumb".to_string()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlisted_names_pass_through() {
        let parent = vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("HOME".to_string(), "/home/mason".to_string()),
            ("LC_ALL".to_string(), "C".to_string()),
        ];
        let env = build_mcp_env(parent, &[]);
        assert!(env.contains(&("PATH".to_string(), "/usr/bin".to_string())));
        assert!(env.contains(&("HOME".to_string(), "/home/mason".to_string())));
        assert!(env.contains(&("LC_ALL".to_string(), "C".to_string())));
    }

    #[test]
    fn a_parent_env_secret_is_not_visible_in_the_child() {
        let parent = vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("FOO_TOKEN".to_string(), "leaked".to_string()),
        ];
        let env = build_mcp_env(parent, &[]);
        assert!(!env.iter().any(|(k, _)| k == "FOO_TOKEN"));
    }

    #[test]
    fn pass_env_admits_a_named_extra() {
        let parent = vec![("KUBECONFIG_EXTRA".to_string(), "x".to_string())];
        let env = build_mcp_env(parent.clone(), &["KUBECONFIG_EXTRA".to_string()]);
        assert!(env.contains(&("KUBECONFIG_EXTRA".to_string(), "x".to_string())));

        let without = build_mcp_env(parent, &[]);
        assert!(!without.iter().any(|(k, _)| k == "KUBECONFIG_EXTRA"));
    }

    #[test]
    fn term_is_always_dumb() {
        let env = build_mcp_env(Vec::new(), &[]);
        assert!(env.contains(&("TERM".to_string(), "dumb".to_string())));
    }
}
