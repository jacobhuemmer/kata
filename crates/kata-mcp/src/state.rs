//! Shared, per-connection server state (`docs/design/05-prd.md` §5, §6.1, §6.2).
//!
//! Config and the vault are re-read fresh on every call (same as every CLI command) rather
//! than cached, so a human editing `kata.toml` or running `kata vault set` while the server
//! is up takes effect on the next tool call without a restart.

use std::path::PathBuf;

use kata_core::{Config, KataPaths, RiskLevel, Vault, VaultStore};

use crate::concurrency::Concurrency;

pub struct ServerState {
    pub paths: KataPaths,
    /// `kata mcp serve --max-risk`: narrows every folder's agent ceiling (§5.1, §6.2). May
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
        paths: KataPaths,
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

    /// A broken `kata.toml` silently reverting to defaults would silently widen the agent's
    /// ceiling back to the default; the stdio transport leaves stderr free precisely so this
    /// isn't silent (A6, `docs/design/05-prd.md` §3.1 tracing row).
    pub fn load_config(&self) -> Config {
        Config::load(&self.paths.config_file()).unwrap_or_else(|err| {
            tracing::warn!(
                path = %self.paths.config_file().display(),
                error = %err,
                "failed to load config; using defaults"
            );
            Config::default()
        })
    }

    /// A vault the identity can no longer decrypt silently becoming "every need is missing"
    /// is the same class of swallowed failure as [`Self::load_config`] (A6).
    pub fn load_vault(&self) -> Vault {
        VaultStore::new(&self.paths.data_dir)
            .load()
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "failed to load the vault; treating every need as missing");
                Vault::default()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A minimal `MakeWriter` capturing everything written to it, so a test can assert a
    /// `tracing::warn!` actually fired without depending on a real stderr capture (A6).
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Captured;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn a_broken_config_load_warns_on_stderr_instead_of_silently_using_defaults() {
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .finish();

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("kata.toml");
        std::fs::write(&config_path, "[exec]\ntimeout = \"30 minuts\"\n").unwrap();
        let state = ServerState::new(
            KataPaths {
                config_dir: dir.path().to_path_buf(),
                data_dir: dir.path().to_path_buf(),
                state_dir: dir.path().to_path_buf(),
            },
            None,
            2,
            dir.path(),
        );

        tracing::subscriber::with_default(subscriber, || {
            let config = state.load_config();
            assert_eq!(config, Config::default());
        });

        let text = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert!(
            text.contains("failed to load config"),
            "expected a warning on stderr, got: {text}"
        );
    }
}
