//! Sequence-grouping and drop-filter tests (`docs/design/06-session-mining.md` §2.3).

use super::*;

fn event(command: &str, when: &str) -> RawShellEvent {
    RawShellEvent {
        command: command.to_string(),
        when: when.to_string(),
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
fn an_unparseable_timestamp_starts_a_new_sequence_rather_than_merging() {
    let events = vec![
        event("kubectl get pods", "not-a-timestamp"),
        event("kubectl logs deploy/api", "also-not-a-timestamp"),
    ];
    let sequences = extract_sequences(&events);
    assert_eq!(sequences.len(), 2);
}
