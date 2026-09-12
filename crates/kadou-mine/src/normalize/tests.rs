//! Synthetic-input tests for template normalization (`docs/design/06-session-mining.md` §2.4,
//! §6.2 worked example).

use super::*;

#[test]
fn worked_example_two_step_sequence_normalizes_to_the_documented_template() {
    let step1 = normalize_command(
        "kubectl --context eks-dev -n payments get pods -l app=api",
        None,
        None,
    );
    assert_eq!(
        step1.template,
        "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP"
    );
    assert_eq!(step1.params.get("CONTEXT").unwrap(), "eks-dev");
    assert_eq!(step1.params.get("NAMESPACE").unwrap(), "payments");
    assert_eq!(step1.params.get("APP").unwrap(), "api");

    let step2 = normalize_command(
        "kubectl --context eks-dev -n payments logs deploy/api --tail=200",
        None,
        None,
    );
    assert_eq!(
        step2.template,
        "kubectl --context $CONTEXT -n $NAMESPACE logs deploy/api --tail=$N"
    );
    assert_eq!(step2.params.get("N").unwrap(), "200");
}

#[test]
fn a_second_session_with_different_values_normalizes_to_the_same_template() {
    let a = normalize_command(
        "kubectl --context eks-dev -n payments get pods -l app=api",
        None,
        None,
    );
    let b = normalize_command(
        "kubectl --context eks-prod -n billing get pods -l app=api",
        None,
        None,
    );
    assert_eq!(a.template, b.template);
}

#[test]
fn home_path_becomes_the_home_token() {
    let n = normalize_command("cat /Users/op/notes.txt", Some("/Users/op"), None);
    assert_eq!(n.template, "cat $HOME/notes.txt");
}

#[test]
fn root_path_becomes_the_root_token() {
    let n = normalize_command(
        "ls /repo/worktree/scripts",
        None,
        Some("/repo/worktree"),
    );
    assert_eq!(n.template, "ls $ROOT/scripts");
}

#[test]
fn other_absolute_paths_get_numbered_and_reused() {
    let n = normalize_command("diff /var/log/a.log /var/log/a.log", None, None);
    assert_eq!(n.template, "diff $PATH_1 $PATH_1");
}

#[test]
fn distinct_absolute_paths_get_distinct_numbers() {
    let n = normalize_command("diff /var/log/a.log /var/log/b.log", None, None);
    assert_eq!(n.template, "diff $PATH_1 $PATH_2");
}

#[test]
fn uuid_becomes_id_token() {
    let n = normalize_command(
        "kubectl get pod 550e8400-e29b-41d4-a716-446655440000",
        None,
        None,
    );
    assert_eq!(n.template, "kubectl get pod $ID");
    assert_eq!(n.params.get("ID").unwrap(), "550e8400-e29b-41d4-a716-446655440000");
}

#[test]
fn git_sha_becomes_id_token() {
    let n = normalize_command("git checkout 4b1c9f2a8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b", None, None);
    assert_eq!(n.template, "git checkout $ID");
}

#[test]
fn rfc3339_timestamp_becomes_ts_token() {
    let n = normalize_command("kadou history --since 2026-09-01T12:00:00Z", None, None);
    assert_eq!(n.template, "kadou history --since $TS");
}

#[test]
fn unix_epoch_integer_becomes_ts_token() {
    // A bare 9+ digit token is treated as a unix timestamp, not an ordinary count (`06` §2.4).
    let n = normalize_command("record-event 1757606400", None, None);
    assert_eq!(n.template, "record-event $TS");
}

#[test]
fn small_integer_becomes_n_token() {
    let n = normalize_command("tail -n 200 file.log", None, None);
    assert_eq!(n.template, "tail -n $N file.log");
    assert_eq!(n.params.get("N").unwrap(), "200");
}

#[test]
fn quoted_value_becomes_val_token() {
    let n = normalize_command("echo 'hello world'", None, None);
    assert_eq!(n.template, "echo $VAL");
    assert_eq!(n.params.get("VAL").unwrap(), "hello world");
}

#[test]
fn ipv4_becomes_ip_token() {
    let n = normalize_command("ping 10.1.2.3", None, None);
    assert_eq!(n.template, "ping $IP");
}

#[test]
fn hostname_becomes_host_token() {
    let n = normalize_command("curl api.customer.example", None, None);
    assert_eq!(n.template, "curl $HOST");
}

#[test]
fn email_becomes_email_token() {
    let n = normalize_command("notify person@example.com", None, None);
    assert_eq!(n.template, "notify $EMAIL");
}

#[test]
fn ticket_id_becomes_ticket_token() {
    let n = normalize_command("git commit -m SDO-123", None, None);
    assert_eq!(n.template, "git commit -m $TICKET");
}

#[test]
fn argv0_and_unknown_flags_are_kept_verbatim() {
    let n = normalize_command("helm upgrade --install release chart", None, None);
    assert_eq!(n.template, "helm upgrade --install $VAL $VAL");
}
