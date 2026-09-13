//! The §6.2 risk-ceiling formula — the one place `human_ceiling`/`agent_ceiling`/
//! `is_visible_risk` are computed, shared by the CLI and MCP server so the two projections of
//! the same config can never drift (R12, `docs/design/11-code-review.md` C2/C3: this exact
//! duplication — `kadou/src/commands.rs`'s own copy and `kata-mcp/src/visibility.rs`'s own
//! copy — was decision 4's whole reason for existing).
//!
//! ```text
//! human_ceiling(f)  = folder[f].max_risk ?? max_risk
//! agent_ceiling(f)  = min(agent.max_risk, folder[f].agent_max_risk ?? agent.max_risk, --max-risk, human_ceiling(f))
//! visible(k, f)     = rank(k.risk) ≤ human_ceiling(f)             [CLI]
//! visible(k, f)     = trusted(f) and rank(k.risk) ≤ agent_ceiling(f)     [MCP]
//! ```

use crate::config::Config;
use crate::risk::RiskLevel;

/// `human_ceiling(f)` (§6.2, §2 decision 5): the human's own ceiling for `folder`.
pub fn human_ceiling(config: &Config, folder: &str) -> RiskLevel {
    config
        .folder
        .get(folder)
        .and_then(|f| f.max_risk)
        .unwrap_or(config.max_risk)
}

/// `agent_ceiling(f)` (§6.2). `max_risk_flag` is `kata mcp serve --max-risk`, which may only
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentConfig, FolderConfig};

    fn config_with(max_risk: RiskLevel, agent_max_risk: RiskLevel) -> Config {
        Config {
            max_risk,
            agent: AgentConfig {
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
    fn cli_and_mcp_ceilings_agree_on_the_same_config() {
        // R12: the CLI's human_ceiling and MCP's agent_ceiling both read the same Config
        // through this one module now — a kata visible to a human under a given config is
        // never invisible to an agent whose own ceiling that config also widens to match.
        let mut config = config_with(RiskLevel::High, RiskLevel::High);
        config.folder.insert(
            "sesami".to_string(),
            FolderConfig {
                max_risk: Some(RiskLevel::High),
                agent_max_risk: Some(RiskLevel::High),
            },
        );
        assert_eq!(human_ceiling(&config, "sesami"), RiskLevel::High);
        assert_eq!(agent_ceiling(&config, "sesami", None), RiskLevel::High);
    }
}
