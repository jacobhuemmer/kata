//! `kadou mine install-schedule`: a macOS user LaunchAgent, or a printable crontab line
//! elsewhere (`docs/design/06-session-mining.md` §3.1; `docs/design/05-prd.md` §6.8 "LaunchAgent
//! `dev.kadou.mine`"). Writing the plist is pure and tested; actually loading it via
//! `launchctl` is a thin, untested side effect the CLI only takes when `--load` is passed --
//! "Never load a LaunchAgent in tests" is a hard rule for this crate's own test suite.

use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

pub const LABEL: &str = "dev.kadou.mine";

pub fn render_plist(kadou_binary: &Path, log_path: &Path) -> String {
    todo!("{kadou_binary:?} {log_path:?}")
}

pub fn render_crontab_line(kadou_binary: &Path) -> String {
    todo!("{kadou_binary:?}")
}

pub fn use_launch_agent() -> bool {
    cfg!(target_os = "macos")
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn write_plist(
    launch_agents_dir: &Path,
    kadou_binary: &Path,
    log_path: &Path,
) -> Result<PathBuf, ScheduleError> {
    todo!("{launch_agents_dir:?} {kadou_binary:?} {log_path:?}")
}
