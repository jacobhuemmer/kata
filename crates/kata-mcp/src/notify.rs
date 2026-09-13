//! The desktop notification (`docs/design/05-prd.md` §7.8): posted on `pending_grant` creation
//! and on `propose_kata`. Best-effort and never blocking — a missing `osascript`/
//! `notify-send`, or `[notify] enabled = false`, is silently a no-op, never a server error.
//!
//! No notification crate: `osascript -e 'display notification ...'` on macOS, `notify-send` on
//! Linux, nothing anywhere else. `Command::spawn` failing (binary absent) is discarded the
//! same way a disabled notification is — the caller can't tell the difference and doesn't need
//! to.

use std::process::Command;

use kata_core::Config;

/// Env var that forces every notification in this process to the no-op path, regardless of
/// `[notify] enabled` — set by tests so a notification test never pops a real toast on the
/// machine running them.
const TEST_NOOP_VAR: &str = "KATA_NOTIFY_TEST_NOOP";

/// `true` when [`notify`] actually attempted to shell out (as opposed to being skipped by
/// config or the test gate) — exposed so tests can assert the no-op path was taken without
/// ever exercising the real `Command::spawn`.
pub fn notify(config: &Config, title: &str, body: &str) -> bool {
    if !config.notify.enabled {
        return false;
    }
    if std::env::var_os(TEST_NOOP_VAR).is_some() {
        return false;
    }
    send(title, body);
    true
}

/// Runs `cmd`, scoped to `path_override` when given (tests only -- production always passes
/// `None`, so it searches the real `PATH`). `spawn`'s `Err` (the binary isn't on that `PATH`
/// at all) is discarded here, not propagated: §7.8 "silently skipped when neither binary
/// exists" is this line, the same for every platform.
fn spawn_scoped(mut cmd: Command, path_override: Option<&std::ffi::OsStr>) {
    if let Some(path) = path_override {
        cmd.env_clear().env("PATH", path);
    }
    let _ = cmd.spawn();
}

#[cfg(target_os = "macos")]
fn send(title: &str, body: &str) {
    send_with(title, body, None);
}

#[cfg(target_os = "macos")]
fn send_with(title: &str, body: &str, path_override: Option<&std::ffi::OsStr>) {
    // AppleScript string literals: escape `"` and `\`; every other character in kadou's own
    // notification bodies (ids, risk words, a `kata grant approve ...` line) is already
    // AppleScript-safe.
    fn quote(s: &str) -> String {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
    let script = format!(
        "display notification {} with title {}",
        quote(body),
        quote(title)
    );
    let mut cmd = Command::new("osascript");
    cmd.arg("-e").arg(script);
    spawn_scoped(cmd, path_override);
}

#[cfg(target_os = "linux")]
fn send(title: &str, body: &str) {
    send_with(title, body, None);
}

#[cfg(target_os = "linux")]
fn send_with(title: &str, body: &str, path_override: Option<&std::ffi::OsStr>) {
    let mut cmd = Command::new("notify-send");
    cmd.arg(title).arg(body);
    spawn_scoped(cmd, path_override);
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn send(_title: &str, _body: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_is_a_no_op() {
        let mut config = Config::default();
        config.notify.enabled = false;
        assert!(!notify(&config, "t", "b"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn missing_osascript_binary_is_not_an_error() {
        // §7.8: "silently skipped when neither binary exists" -- scoped to an empty PATH so
        // this never depends on (or pops a real toast via) whatever is actually installed.
        let empty = tempfile::tempdir().unwrap();
        send_with("t", "b", Some(empty.path().as_os_str()));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn missing_notify_send_binary_is_not_an_error() {
        let empty = tempfile::tempdir().unwrap();
        send_with("t", "b", Some(empty.path().as_os_str()));
    }

    #[test]
    fn the_test_noop_env_var_forces_a_no_op_even_when_enabled() {
        // Never spawns osascript/notify-send: this is the path every other notification test
        // in this workspace must run through so CI never pops a real toast.
        // SAFETY: this test does not run concurrently with other tests that read this var —
        // it is a `KADOU_`-prefixed name unique to this crate's own notify module.
        unsafe {
            std::env::set_var(TEST_NOOP_VAR, "1");
        }
        let config = Config::default();
        assert!(config.notify.enabled, "sanity: enabled by default");
        let attempted = notify(&config, "t", "b");
        unsafe {
            std::env::remove_var(TEST_NOOP_VAR);
        }
        assert!(!attempted);
    }
}
