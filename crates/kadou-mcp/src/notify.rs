//! The desktop notification (`docs/design/05-prd.md` §7.8): posted on `pending_grant` creation
//! and on `propose_kata`. Best-effort and never blocking — a missing `osascript`/
//! `notify-send`, or `[notify] enabled = false`, is silently a no-op, never a server error.
//!
//! No notification crate: `osascript -e 'display notification ...'` on macOS, `notify-send` on
//! Linux, nothing anywhere else. `Command::spawn` failing (binary absent) is discarded the
//! same way a disabled notification is — the caller can't tell the difference and doesn't need
//! to.

use std::process::Command;

use kadou_core::Config;

/// Env var that forces every notification in this process to the no-op path, regardless of
/// `[notify] enabled` — set by tests so a notification test never pops a real toast on the
/// machine running them.
const TEST_NOOP_VAR: &str = "KADOU_NOTIFY_TEST_NOOP";

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

#[cfg(target_os = "macos")]
fn send(title: &str, body: &str) {
    // AppleScript string literals: escape `"` and `\`; every other character in kadou's own
    // notification bodies (ids, risk words, a `kadou grant approve ...` line) is already
    // AppleScript-safe.
    fn quote(s: &str) -> String {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
    let script = format!(
        "display notification {} with title {}",
        quote(body),
        quote(title)
    );
    let _ = Command::new("osascript").arg("-e").arg(script).spawn();
}

#[cfg(target_os = "linux")]
fn send(title: &str, body: &str) {
    let _ = Command::new("notify-send").arg(title).arg(body).spawn();
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
