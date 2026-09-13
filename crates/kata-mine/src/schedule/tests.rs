//! Schedule tests (`docs/design/06-session-mining.md` §3.1). Never loads a LaunchAgent --
//! only [`write_plist`] (a pure file write) is exercised here.

use std::path::Path;

use super::*;

#[test]
fn label_is_dev_kata_mine_not_a_dops_legacy_name() {
    assert_eq!(LABEL, "dev.kata.mine");
}

#[test]
fn plist_names_the_binary_and_the_once_flag_at_0315() {
    let xml = render_plist(
        Path::new("/usr/local/bin/kata"),
        Path::new("/tmp/launchd.out.log"),
    );
    assert!(xml.contains("<string>dev.kata.mine</string>"));
    assert!(xml.contains("<string>/usr/local/bin/kata</string>"));
    assert!(xml.contains("<string>mine</string>"));
    assert!(xml.contains("<string>run</string>"));
    assert!(xml.contains("<string>--once</string>"));
    assert!(xml.contains("<integer>3</integer>"));
    assert!(xml.contains("<integer>15</integer>"));
    assert!(xml.contains("<key>LowPriorityIO</key>"));
    assert!(xml.contains("/tmp/launchd.out.log"));
}

#[test]
fn crontab_line_runs_the_same_command_at_the_same_time() {
    let line = render_crontab_line(Path::new("/usr/local/bin/kata"));
    assert_eq!(line, "15 3 * * * /usr/local/bin/kata mine run --once");
}

#[test]
fn write_plist_creates_the_file_under_the_given_directory() {
    let dir = tempfile::tempdir().unwrap();
    let launch_agents_dir = dir.path().join("LaunchAgents");

    let path = write_plist(
        &launch_agents_dir,
        Path::new("/usr/local/bin/kata"),
        &dir.path().join("mine/logs/launchd.out.log"),
    )
    .unwrap();

    assert_eq!(path, launch_agents_dir.join("dev.kata.mine.plist"));
    assert!(path.is_file());
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("dev.kata.mine"));
}

#[test]
fn write_plist_never_invokes_launchctl() {
    // Structural guarantee, not a mock assertion: write_plist's body has no process spawn at
    // all, so there is nothing here that could load a LaunchAgent during a test run.
    let dir = tempfile::tempdir().unwrap();
    write_plist(
        &dir.path().join("LaunchAgents"),
        Path::new("/usr/local/bin/kata"),
        &dir.path().join("log"),
    )
    .unwrap();
}
