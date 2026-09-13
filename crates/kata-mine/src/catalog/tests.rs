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
fn contains_token_pair_requires_argv0_to_actually_precede_the_subcommand() {
    // Exercises contains_token_pair directly (no header preamble diluting token positions,
    // unlike a real kata file) so each case pins down one specific part of "argv0 at position
    // i, subcommand within LOOKAHEAD tokens after i": the subcommand appearing without the
    // right argv0 nearby must not match, and neither must argv0 appearing without the
    // subcommand nearby.
    assert!(contains_token_pair("kubectl get pods", "kubectl", "get"));
    assert!(!contains_token_pair("helm get pods", "kubectl", "get"));
    assert!(!contains_token_pair(
        "kubectl delete pod x",
        "kubectl",
        "get"
    ));
}

#[test]
fn contains_token_pair_respects_the_lookahead_window_starting_after_argv0() {
    // The subcommand sits exactly SUBCOMMAND_LOOKAHEAD tokens after argv0 -- within the
    // correct window (which starts right after argv0), but outside a window that started at
    // argv0's own position instead.
    assert!(contains_token_pair(
        "kubectl a b c d e get",
        "kubectl",
        "get"
    ));
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
fn covers_is_true_when_the_subcommand_immediately_follows_argv0() {
    // Distinguishes real argv0-then-subcommand adjacency (t == argv0, checked before the
    // lookahead) from a looser match that would also fire on some *other* token merely being
    // followed by the subcommand somewhere in its own window.
    let dir = tempfile::tempdir().unwrap();
    let kata_dir = dir.path().join("kata");
    write_kata(&kata_dir, "sesami/exact.sh", "kubectl get pods -l app=api");

    assert!(covers(
        &kata_dir,
        "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP"
    ));
}

#[test]
fn covers_is_false_when_argv0_appears_but_the_subcommand_never_follows_it() {
    // argv0 ("kubectl") is present, but no occurrence of it is followed by "get" within the
    // lookahead window -- covers must stay false rather than matching on argv0 alone, or on
    // the subcommand appearing anywhere regardless of which token precedes it.
    let dir = tempfile::tempdir().unwrap();
    let kata_dir = dir.path().join("kata");
    write_kata(&kata_dir, "sesami/other.sh", "kubectl delete pod x");

    assert!(!covers(
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
