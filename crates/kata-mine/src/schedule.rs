//! `kata mine install-schedule`: a macOS user LaunchAgent, or a printable crontab line
//! elsewhere (`docs/design/06-session-mining.md` §3.1; `docs/design/05-prd.md` §6.8 "LaunchAgent
//! `dev.kadou.mine`"). Writing the plist is pure and tested; actually loading it via
//! `launchctl` is a thin, untested side effect the CLI only takes when `--load` is passed --
//! "Never load a LaunchAgent in tests" is a hard rule for this crate's own test suite.

use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// The launchd label -- a reverse-DNS-shaped identifier, not a product name
/// (`06` §3.1: "Label: `dev.dops.mine` (launchd label, not a product name)").
pub const LABEL: &str = "dev.kata.mine";

/// The plist XML for a daily 03:15 `kata mine run --once` (`06` §3.1): low priority I/O and
/// nice'd, logging counts/fingerprints only to `log_path` (never command text -- the process
/// itself never prints one, so this is true by construction, not by redacting the log).
pub fn render_plist(kadou_binary: &Path, log_path: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>mine</string>
        <string>run</string>
        <string>--once</string>
    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Hour</key>
        <integer>3</integer>
        <key>Minute</key>
        <integer>15</integer>
    </dict>
    <key>Nice</key>
    <integer>10</integer>
    <key>LowPriorityIO</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        binary = kadou_binary.display(),
        log = log_path.display(),
    )
}

/// The Linux fallback: a crontab line running the same command at the same time (`06` §3.1
/// "Linux: a user crontab line with the same command").
pub fn render_crontab_line(kadou_binary: &Path) -> String {
    format!("15 3 * * * {} mine run --once", kadou_binary.display())
}

/// `true` on the platform [`render_plist`]'s LaunchAgent path applies to.
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

/// Writes the plist to `<launch_agents_dir>/dev.kata.mine.plist`. Never loads it -- that is
/// the CLI's own explicit `--load` side effect, not something this function does.
pub fn write_plist(
    launch_agents_dir: &Path,
    kadou_binary: &Path,
    log_path: &Path,
) -> Result<PathBuf, ScheduleError> {
    std::fs::create_dir_all(launch_agents_dir).map_err(|source| ScheduleError::Write {
        path: launch_agents_dir.to_path_buf(),
        source,
    })?;
    let path = launch_agents_dir.join(format!("{LABEL}.plist"));
    std::fs::write(&path, render_plist(kadou_binary, log_path)).map_err(|source| {
        ScheduleError::Write {
            path: path.clone(),
            source,
        }
    })?;
    Ok(path)
}
