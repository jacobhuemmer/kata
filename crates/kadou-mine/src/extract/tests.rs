//! Sequence-grouping and drop-filter tests (`docs/design/06-session-mining.md` §2.3).

use super::*;

fn event(command: &str, when: &str) -> RawShellEvent {
    RawShellEvent {
        command: command.to_string(),
        when: when.to_string(),
        kind: EventKind::Shell,
    }
}

fn script_event(kind: EventKind, command: &str, when: &str) -> RawShellEvent {
    RawShellEvent {
        command: command.to_string(),
        when: when.to_string(),
        kind,
    }
}

#[test]
fn two_commands_within_the_gap_become_one_sequence() {
    let events = vec![
        event("kubectl get pods", "2026-09-01T00:00:00Z"),
        event("kubectl logs deploy/api", "2026-09-01T00:01:00Z"),
    ];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 1);
    assert_eq!(sequences[0].steps.len(), 2);
    assert_eq!(sequences[0].when, "2026-09-01T00:00:00Z");
}

#[test]
fn a_gap_of_two_minutes_or_more_starts_a_new_sequence() {
    let events = vec![
        event("kubectl get pods", "2026-09-01T00:00:00Z"),
        event("kubectl logs deploy/api", "2026-09-01T00:02:00Z"),
    ];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 2);
}

#[test]
fn a_chain_of_gaps_each_under_the_threshold_stays_one_sequence() {
    let events = vec![
        event("kubectl get pods", "2026-09-01T00:00:00Z"),
        event("kubectl describe pod x", "2026-09-01T00:01:00Z"),
        event("kubectl logs deploy/api", "2026-09-01T00:02:00Z"),
    ];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 1);
    assert_eq!(sequences[0].steps.len(), 3);
}

#[test]
fn a_lone_noise_command_is_dropped() {
    let events = vec![event("git status", "2026-09-01T00:00:00Z")];
    assert!(extract_sequences(&events).is_empty());

    for noise in ["ls", "pwd", "whoami"] {
        let events = vec![event(noise, "2026-09-01T00:00:00Z")];
        assert!(extract_sequences(&events).is_empty(), "{noise}");
    }
}

#[test]
fn noise_commands_survive_inside_a_longer_sequence() {
    let events = vec![
        event("git status", "2026-09-01T00:00:00Z"),
        event("git add -A", "2026-09-01T00:00:30Z"),
    ];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 1);
    assert_eq!(sequences[0].steps, vec!["git status", "git add -A"]);
}

#[test]
fn a_noise_looking_command_with_extra_args_is_not_dropped() {
    let events = vec![event("git status --short", "2026-09-01T00:00:00Z")];
    assert_eq!(extract_sequences(&events).len(), 1);
}

#[test]
fn a_sequence_containing_an_apparent_secret_is_dropped_entirely() {
    let token = format!("ghp_{}", "A".repeat(36));
    let events = vec![
        event("git remote -v", "2026-09-01T00:00:00Z"),
        event(
            &format!("git push https://{token}@github.com/x/y.git"),
            "2026-09-01T00:00:30Z",
        ),
    ];
    assert!(extract_sequences(&events).is_empty());
}

#[test]
fn an_ordinary_kubectl_sequence_is_never_flagged_as_a_secret() {
    let events = vec![
        event(
            "kubectl --context eks-dev -n payments get pods -l app=api",
            "2026-09-01T00:00:00Z",
        ),
        event(
            "kubectl --context eks-dev -n payments logs deploy/api --tail=200",
            "2026-09-01T00:01:00Z",
        ),
    ];
    assert_eq!(extract_sequences(&events).len(), 1);
}

#[test]
fn a_script_file_event_becomes_its_own_sequence_of_its_lines() {
    // 06 §2.3's "Generated script file" kind: a whole written .sh body is already a
    // multi-step sequence, not something extract_sequences groups by time gap.
    let body = "#!/bin/sh\nkubectl get pods\nkubectl logs deploy/api\n";
    let events = vec![script_event(
        EventKind::ScriptFile,
        body,
        "2026-09-01T00:00:00Z",
    )];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 1);
    assert_eq!(
        sequences[0].steps,
        vec!["kubectl get pods", "kubectl logs deploy/api"]
    );
}

#[test]
fn an_empty_script_file_body_is_dropped() {
    let events = vec![script_event(
        EventKind::ScriptFile,
        "#!/bin/sh\n\n",
        "2026-09-01T00:00:00Z",
    )];
    assert!(extract_sequences(&events).is_empty());
}

#[test]
fn a_fenced_script_with_fewer_than_three_lines_is_dropped() {
    // 06 §2.3: fenced scripts need >=3 command lines; script_file has no such minimum.
    let body = "kubectl get pods\nkubectl logs deploy/api\n";
    let events = vec![script_event(
        EventKind::ScriptFence,
        body,
        "2026-09-01T00:00:00Z",
    )];
    assert!(extract_sequences(&events).is_empty());
}

#[test]
fn a_fenced_script_with_three_or_more_lines_is_kept() {
    let body = "kubectl get pods\nkubectl describe pod x\nkubectl logs deploy/api\n";
    let events = vec![script_event(
        EventKind::ScriptFence,
        body,
        "2026-09-01T00:00:00Z",
    )];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 1);
    assert_eq!(sequences[0].steps.len(), 3);
}

#[test]
fn a_script_file_event_is_not_merged_with_a_neighboring_shell_event() {
    let events = vec![
        event("kubectl get pods", "2026-09-01T00:00:00Z"),
        script_event(
            EventKind::ScriptFile,
            "kubectl apply -f x.yaml\n",
            "2026-09-01T00:00:30Z",
        ),
    ];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 2);
}

#[test]
fn an_unparseable_timestamp_starts_a_new_sequence_rather_than_merging() {
    let events = vec![
        event("kubectl get pods", "not-a-timestamp"),
        event("kubectl logs deploy/api", "also-not-a-timestamp"),
    ];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 2);
}
