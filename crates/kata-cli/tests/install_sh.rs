//! `install.sh` (`docs/design/05-prd.md` §7.5, §9 slice 8): POSIX installer with SHA-256
//! verification. Every test here points `KADOU_INSTALL_BASE_URL` at a local `file://` fixture
//! -- no real network, per the repo's no-network test gate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn install_sh() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../install.sh"))
}

fn asset_name(version: &str) -> String {
    let os = std::process::Command::new("uname")
        .arg("-s")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_lowercase())
        .unwrap_or_default();
    let arch = std::process::Command::new("uname")
        .arg("-m")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    format!("kata-{version}-{os}-{arch}.tar.gz")
}

/// Builds a release fixture at `<root>/<version>/{<asset>,SHA256SUMS}` -- a fake `kata`
/// binary tarred up, with a real (`correct_checksum: true`) or deliberately wrong SHA256SUMS
/// entry.
fn build_release_fixture(root: &Path, version: &str, correct_checksum: bool) -> PathBuf {
    let release_dir = root.join(version);
    std::fs::create_dir_all(&release_dir).unwrap();

    let stage = root.join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join("kata"), b"#!/bin/sh\necho fake-kadou\n").unwrap();

    let asset = asset_name(version);
    let asset_path = release_dir.join(&asset);
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&asset_path)
        .arg("-C")
        .arg(&stage)
        .arg("kata")
        .status()
        .unwrap();
    assert!(status.success(), "tar must succeed building the fixture");

    let sha256 = kata_core::file_sha256(&asset_path)
        .unwrap()
        .trim_start_matches("sha256:")
        .to_string();
    let sums_sha = if correct_checksum {
        sha256
    } else {
        "0".repeat(64)
    };
    std::fs::write(
        release_dir.join("SHA256SUMS"),
        format!("{sums_sha}  {asset}\n"),
    )
    .unwrap();

    release_dir
}

fn install_cmd(base_url_dir: &Path, install_dir: &Path, version: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg(install_sh())
        .env("KATA_VERSION", version)
        .env(
            "KATA_INSTALL_BASE_URL",
            format!("file://{}", base_url_dir.display()),
        )
        .env("KATA_INSTALL_DIR", install_dir);
    cmd
}

#[test]
fn dry_run_verifies_the_checksum_but_installs_nothing() {
    let root = tempfile::tempdir().unwrap();
    build_release_fixture(root.path(), "0.1.0-test", true);
    let install_dir = root.path().join("bin");

    let output = install_cmd(root.path(), &install_dir, "0.1.0-test")
        .arg("--dry-run")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sha256 ok"), "{stdout}");
    assert!(stdout.contains("dry run"), "{stdout}");
    assert!(
        !install_dir.join("kata").exists(),
        "dry-run must not write the binary"
    );
}

#[test]
fn a_real_install_places_an_executable_binary_at_the_temp_prefix() {
    let root = tempfile::tempdir().unwrap();
    build_release_fixture(root.path(), "0.1.0-test", true);
    let install_dir = root.path().join("bin");

    let output = install_cmd(root.path(), &install_dir, "0.1.0-test")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = install_dir.join("kata");
    assert!(installed.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&installed).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "installed binary must be executable");
    }
}

#[test]
fn a_checksum_mismatch_aborts_and_installs_nothing() {
    let root = tempfile::tempdir().unwrap();
    build_release_fixture(root.path(), "0.1.0-test", false);
    let install_dir = root.path().join("bin");

    let output = install_cmd(root.path(), &install_dir, "0.1.0-test")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("checksum mismatch"), "{stderr}");
    assert!(!install_dir.join("kata").exists());
}

#[test]
fn an_unknown_argument_is_a_clean_error() {
    let root = tempfile::tempdir().unwrap();
    build_release_fixture(root.path(), "0.1.0-test", true);
    let install_dir = root.path().join("bin");

    let output = install_cmd(root.path(), &install_dir, "0.1.0-test")
        .arg("--bogus")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown argument"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
