//! Shared, per-connection server state (`docs/design/05-prd.md` §5, §6.1, §6.2).
//!
//! Config and the vault are re-read fresh on every call (same as every CLI command) rather
//! than cached, so a human editing `kadou.toml` or running `kadou vault set` while the server
//! is up takes effect on the next tool call without a restart.

use std::path::PathBuf;

use kadou_core::{Config, KadouPaths, RiskLevel, Vault, VaultStore};

use crate::concurrency::Concurrency;

pub struct ServerState {
    pub paths: KadouPaths,
    /// `kadou mcp serve --max-risk`: narrows every folder's agent ceiling (§5.1, §6.2). May
    /// only narrow, never exceed, `[agent].max_risk`.
    pub max_risk_flag: Option<RiskLevel>,
    pub concurrency: Concurrency,
    /// The project-local `kata/` directory discovered by walking up from the server's cwd at
    /// startup (§4.5), if any. `None` when no `kata/` was found before hitting the git root
    /// or `$HOME`.
    pub project_local_kata_dir: Option<PathBuf>,
}

impl ServerState {
    pub fn new(
        paths: KadouPaths,
        max_risk_flag: Option<RiskLevel>,
        concurrency_limit: usize,
        cwd: &std::path::Path,
    ) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Self {
            paths,
            max_risk_flag,
            concurrency: Concurrency::new(concurrency_limit),
            project_local_kata_dir: crate::visibility::discover_project_local(cwd, home.as_deref()),
        }
    }

    pub fn load_config(&self) -> Config {
        Config::load(&self.paths.config_file()).unwrap_or_default()
    }

    pub fn load_vault(&self) -> Vault {
        VaultStore::new(&self.paths.data_dir)
            .load()
            .unwrap_or_default()
    }
}
