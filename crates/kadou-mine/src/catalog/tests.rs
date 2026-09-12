//! Catalog-collision lookup tests (`docs/design/06-session-mining.md` §2.6, D6).

use super::*;

fn write_kata(dir: &Path, rel: &str, body: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!("#!/bin/sh\n# ---\n# about: existing\n# risk:  low\n# ---\n{body}\n"),
    )
    .unwrap();
}

#[test]
fn argv0_and_subcommand_skips_flags_and_placeholders() {
    assert_eq!(
        argv0_and_subcommand("kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP"),
        Some(("kubectl", "get"))
    );
}

#[test]
fn argv0_and_subcommand_is_none_with_no_second_bare_word() {
    assert_eq!(argv0_and_subcommand("kubectl --context $CONTEXT"), None);
    assert_eq!(argv0_and_subcommand(""), None);
}

#[test]
fn covers_is_true_when_an_existing_kata_already_invokes_the_same_argv0_and_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let kata_dir = dir.path().join("kata");
    write_kata(
        &kata_dir,
        "sesami/device-logs.sh",
        "kubectl -n payments get pods -l app=api",
    );

    assert!(covers(
        &kata_dir,
        "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP"
    ));
}

#[test]
fn covers_is_false_when_no_kata_shares_the_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let kata_dir = dir.path().join("kata");
    write_kata(&kata_dir, "sesami/deploy.sh", "helm upgrade myapp ./chart");

    assert!(!covers(
        &kata_dir,
        "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP"
    ));
}

#[test]
fn covers_is_false_for_a_missing_kata_dir() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!covers(&dir.path().join("nope"), "kubectl get pods"));
}
