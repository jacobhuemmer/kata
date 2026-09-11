//! The §6.2 visibility formula, on the MCP side, plus the §4.5 project-local trust gate.
//!
//! ```text
//! human_ceiling(f)  = folder[f].max_risk ?? max_risk
//! agent_ceiling(f)  = min(agent.max_risk, folder[f].agent_max_risk ?? agent.max_risk, --max-risk, human_ceiling(f))
//! visible(k, f)     = trusted(f) and rank(k.risk) ≤ agent_ceiling(f)     [MCP]
//! ```
//!
//! The sentinel folder key `"."` is used for project-local kata's ceiling lookup: real
//! `[folder.<name>]` config sections are keyed by a folder's actual on-disk name (`sesami`,
//! `starter`, …), which a project-local id (`./deploy`) can never collide with — so a
//! project-local kata simply falls back to `agent.max_risk`/`max_risk` like any folder with no
//! specific policy, exactly as the formula intends.

use std::path::{Path, PathBuf};

use kadou_core::{Config, RiskLevel};

/// Sentinel folder key for project-local kata's `[folder.<name>]` lookup (see module docs).
pub const PROJECT_LOCAL_FOLDER_KEY: &str = ".";

pub fn human_ceiling(config: &Config, folder: &str) -> RiskLevel {
    config
        .folder
        .get(folder)
        .and_then(|f| f.max_risk)
        .unwrap_or(config.max_risk)
}

/// `agent_ceiling(f)` (§6.2). `max_risk_flag` is `kadou mcp serve --max-risk`, which may only
/// narrow (§5.1) — it participates in the same `min()` as every other term.
pub fn agent_ceiling(config: &Config, folder: &str, max_risk_flag: Option<RiskLevel>) -> RiskLevel {
    let mut ceiling = config.agent.max_risk;
    ceiling = ceiling.min(
        config
            .folder
            .get(folder)
            .and_then(|f| f.agent_max_risk)
            .unwrap_or(config.agent.max_risk),
    );
    if let Some(flag) = max_risk_flag {
        ceiling = ceiling.min(flag);
    }
    ceiling.min(human_ceiling(config, folder))
}

/// A kata at the ceiling is allowed; only *exceeding* it hides the kata (§6.2 "A kata at the
/// ceiling is allowed").
pub fn is_visible_risk(risk: RiskLevel, ceiling: RiskLevel) -> bool {
    risk <= ceiling
}

/// `true` when `kata_dir` (an absolute, discovered project-local `kata/` directory) is listed
/// in `[trust] paths` (§4.5). Compared as a canonicalized path so `kadou trust`'s recorded
/// absolute path and a freshly-discovered one agree even across symlinked mounts.
pub fn is_trusted(config: &Config, kata_dir: &Path) -> bool {
    let canonical = kata_dir
        .canonicalize()
        .unwrap_or_else(|_| kata_dir.to_path_buf());
    config.trust.paths.iter().any(|p| {
        let candidate = p.canonicalize().unwrap_or_else(|_| p.clone());
        candidate == canonical
    })
}

/// Walks up from `cwd` to find the nearest `kata/` directory, stopping at the git root or
/// `home` (§4.5 "kadou walks up to the nearest `kata/` directory, stopping at the git root or
/// `$HOME`"). Returns the discovered `kata/` directory itself, not its parent.
pub fn discover_project_local(cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let mut dir = cwd.to_path_buf();
    loop {
        let candidate = dir.join("kata");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if dir.join(".git").exists() {
            return None;
        }
        if home.is_some_and(|home| home == dir) {
            return None;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadou_core::FolderConfig;

    fn config_with(max_risk: RiskLevel, agent_max_risk: RiskLevel) -> Config {
        Config {
            max_risk,
            agent: kadou_core::AgentConfig {
                max_risk: agent_max_risk,
                allow: Vec::new(),
            },
            ..Config::default()
        }
    }

    #[test]
    fn agent_ceiling_defaults_to_low() {
        let config = Config::default();
        assert_eq!(agent_ceiling(&config, "sesami", None), RiskLevel::Low);
    }

    #[test]
    fn agent_ceiling_never_exceeds_the_human_ceiling() {
        let mut config = config_with(RiskLevel::Medium, RiskLevel::Critical);
        config
            .folder
            .insert("sesami".to_string(), FolderConfig::default());
        // Human ceiling for `sesami` falls back to the global `medium`; even though the
        // agent's own max_risk is `critical`, it cannot exceed that.
        assert_eq!(agent_ceiling(&config, "sesami", None), RiskLevel::Medium);
    }

    #[test]
    fn folder_agent_max_risk_can_only_narrow_not_widen() {
        let mut config = config_with(RiskLevel::Critical, RiskLevel::Critical);
        config.folder.insert(
            "sesami".to_string(),
            FolderConfig {
                max_risk: Some(RiskLevel::Critical),
                agent_max_risk: Some(RiskLevel::Low),
            },
        );
        assert_eq!(agent_ceiling(&config, "sesami", None), RiskLevel::Low);
    }

    #[test]
    fn max_risk_flag_can_only_narrow() {
        let config = config_with(RiskLevel::Critical, RiskLevel::Critical);
        assert_eq!(
            agent_ceiling(&config, "sesami", Some(RiskLevel::Medium)),
            RiskLevel::Medium
        );
    }

    #[test]
    fn kata_at_the_ceiling_is_visible_strictly_above_is_not() {
        assert!(is_visible_risk(RiskLevel::Low, RiskLevel::Low));
        assert!(!is_visible_risk(RiskLevel::Medium, RiskLevel::Low));
    }

    #[test]
    fn discover_project_local_walks_up_to_a_kata_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("project/kata")).unwrap();
        std::fs::create_dir_all(dir.path().join("project/sub")).unwrap();

        let found = discover_project_local(&dir.path().join("project/sub"), None).unwrap();
        assert_eq!(found, dir.path().join("project/kata"));
    }

    #[test]
    fn discover_project_local_stops_at_git_root_without_finding_kata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
        std::fs::create_dir_all(dir.path().join("repo/sub")).unwrap();
        std::fs::create_dir_all(dir.path().join("kata")).unwrap(); // above the git root

        let found = discover_project_local(&dir.path().join("repo/sub"), None);
        assert!(found.is_none());
    }

    #[test]
    fn discover_project_local_stops_at_home() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("home/sub")).unwrap();
        std::fs::create_dir_all(dir.path().join("kata")).unwrap(); // above home

        let found =
            discover_project_local(&dir.path().join("home/sub"), Some(&dir.path().join("home")));
        assert!(found.is_none());
    }

    #[test]
    fn is_trusted_matches_a_recorded_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("project/kata");
        std::fs::create_dir_all(&kata_dir).unwrap();

        let mut config = Config::default();
        assert!(!is_trusted(&config, &kata_dir));

        config.trust.paths.push(kata_dir.clone());
        assert!(is_trusted(&config, &kata_dir));
    }
}
