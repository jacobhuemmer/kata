//! Queue/audit/approve/reject store tests (`docs/design/06-session-mining.md` §2.9, §3.3-3.4,
//! §4.6).

use super::*;

const KATA: &str = "#!/bin/sh\n# ---\n# about: Mined automation\n# risk:  medium\n# ---\necho hi\n";

#[test]
fn write_then_read_round_trips_a_queue_entry() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());

    write_queue_entry(&home, "fp1", KATA, "{\"fingerprint\":\"fp1\"}").unwrap();
    let entry = read_queue_entry(&home, "fp1").unwrap();

    assert_eq!(entry.kata_source, KATA);
    assert_eq!(entry.meta_json, "{\"fingerprint\":\"fp1\"}");
}

#[test]
fn queue_dirs_and_files_are_private() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{}").unwrap();

    let entry_dir = home.entry_dir("fp1");
    assert_eq!(
        std::fs::metadata(&entry_dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(entry_dir.join("kata.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn re_writing_the_same_fingerprint_overwrites_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{\"score\":1}").unwrap();
    write_queue_entry(&home, "fp1", KATA, "{\"score\":2}").unwrap();

    assert_eq!(list_queue(&home).len(), 1);
    let entry = read_queue_entry(&home, "fp1").unwrap();
    assert_eq!(entry.meta_json, "{\"score\":2}");
}

#[test]
fn list_queue_returns_every_entry_sorted_by_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "bbb", KATA, "{}").unwrap();
    write_queue_entry(&home, "aaa", KATA, "{}").unwrap();

    let entries = list_queue(&home);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].fingerprint, "aaa");
    assert_eq!(entries[1].fingerprint, "bbb");
}

#[test]
fn audit_log_is_append_only_and_private() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());

    append_audit(
        &home,
        &AuditRow {
            when: "2026-09-01T00:00:00Z".to_string(),
            action: "queued".to_string(),
            fingerprint: "fp1".to_string(),
            actor: "schedule".to_string(),
            reason: None,
            catalog_id: None,
            script_sha256: None,
        },
    )
    .unwrap();
    append_audit(
        &home,
        &AuditRow {
            when: "2026-09-02T00:00:00Z".to_string(),
            action: "skipped".to_string(),
            fingerprint: "fp1".to_string(),
            actor: "user".to_string(),
            reason: None,
            catalog_id: None,
            script_sha256: None,
        },
    )
    .unwrap();

    let rows = read_audit(&home);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].action, "queued");
    assert_eq!(rows[1].action, "skipped");
    assert_eq!(latest_action(&rows, "fp1"), Some("skipped".to_string()));

    let mode = std::fs::metadata(home.audit_log_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn approve_copies_the_draft_into_the_mined_namespace_and_audits_it() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{}").unwrap();

    let target = approve(&home, "fp1", "k8s-pod-logs").unwrap();

    assert_eq!(target, dir.path().join("mined/k8s-pod-logs.sh"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), KATA);

    let rows = read_audit(&home);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action, "approved");
    assert_eq!(rows[0].catalog_id.as_deref(), Some("mined.k8s-pod-logs"));
    assert!(rows[0].script_sha256.is_some());
}

#[test]
fn approve_refuses_a_name_that_escapes_the_mined_directory() {
    // B4/D16 (`docs/design/12-mvp-review.md` §3, §6): `--into <name>` joined `name` straight
    // onto `mined_dir` with no segment check, so `kadou mine approve <fp> --into
    // ../../../.config/kadou/kata/sesami/pwn` wrote an executable kata straight into the
    // library, bypassing `kadou accept`'s diff-and-confirm review gate entirely.
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{}").unwrap();

    let err = approve(&home, "fp1", "../../../etc/pwn").unwrap_err();
    assert!(matches!(err, ApproveError::InvalidName(_)), "{err}");

    // No file landed anywhere -- not under mined/ (which was never even created) and not at
    // the escaped path the traversal aimed for.
    assert!(!dir.path().join("mined").exists());
    assert!(
        !dir.path()
            .parent()
            .and_then(Path::parent)
            .is_some_and(|p| p.join("etc/pwn.sh").exists())
    );
    assert!(
        read_audit(&home).is_empty(),
        "no approval should be recorded for a rejected name"
    );
}

#[test]
fn approving_an_unknown_fingerprint_is_a_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    let err = approve(&home, "nope", "x").unwrap_err();
    assert!(matches!(err, ApproveError::NotFound(_)));
}

#[test]
fn approving_twice_is_refused_the_second_time() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{}").unwrap();
    approve(&home, "fp1", "x").unwrap();

    let err = approve(&home, "fp1", "x").unwrap_err();
    assert!(matches!(err, ApproveError::AlreadyDecided(_, _)));
}

#[test]
fn reject_records_a_reason_and_bans_future_approval() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{}").unwrap();

    reject(&home, "fp1", "too risky").unwrap();

    let rows = read_audit(&home);
    assert_eq!(rows[0].action, "rejected");
    assert_eq!(rows[0].reason.as_deref(), Some("too risky"));

    let err = approve(&home, "fp1", "x").unwrap_err();
    assert!(matches!(err, ApproveError::AlreadyDecided(_, _)));
}

#[test]
fn skip_records_a_skipped_action_and_leaves_the_draft_queued() {
    // D11 (`docs/design/12-mvp-review.md` §3): 06 §2.9's "skip: leave queued" -- unlike
    // approve/reject, skip is never terminal and never removes the draft from the queue.
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{}").unwrap();

    skip(&home, "fp1").unwrap();

    let rows = read_audit(&home);
    assert_eq!(rows[0].action, "skipped");
    assert!(read_queue_entry(&home, "fp1").is_some());
}

#[test]
fn skip_can_be_recorded_more_than_once_and_never_bans_a_later_approval() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());
    write_queue_entry(&home, "fp1", KATA, "{}").unwrap();

    skip(&home, "fp1").unwrap();
    skip(&home, "fp1").unwrap();

    assert_eq!(read_audit(&home).len(), 2);
    approve(&home, "fp1", "x").unwrap();
}

#[test]
fn skipping_an_unknown_fingerprint_is_a_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());

    let err = skip(&home, "nope").unwrap_err();
    assert!(matches!(err, ApproveError::NotFound(_)));
}

#[test]
fn audit_rows_never_contain_command_text_fields() {
    // Structural guarantee: AuditRow has no field that could hold a command, so this is a
    // compile-time property as much as a runtime one -- this test just pins the JSON shape.
    let row = AuditRow {
        when: "2026-09-01T00:00:00Z".to_string(),
        action: "queued".to_string(),
        fingerprint: "fp1".to_string(),
        actor: "schedule".to_string(),
        reason: None,
        catalog_id: None,
        script_sha256: None,
    };
    let json = serde_json::to_string(&row).unwrap();
    for field in ["command", "steps", "template", "source"] {
        assert!(!json.contains(field), "{json}");
    }
}
