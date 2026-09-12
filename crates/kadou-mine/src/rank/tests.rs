//! Rank/cutoff tests (`docs/design/06-session-mining.md` §2.6, §6.3 worked example).

use std::time::{Duration, SystemTime};

use super::*;
use crate::cluster::Member;

fn member(agent: &str, session: &str, when: &str) -> Member {
    Member {
        agent: agent.to_string(),
        session_id: session.to_string(),
        when: when.to_string(),
    }
}

fn cluster(members: Vec<Member>, step_count: usize, last_seen: &str) -> Cluster {
    Cluster {
        fingerprint: "f".repeat(64),
        template: "kubectl get pods".to_string(),
        step_count,
        members,
        first_seen: "2026-09-01T00:00:00Z".to_string(),
        last_seen: last_seen.to_string(),
    }
}

#[test]
fn worked_example_passes_cutoff_on_three_unique_sessions() {
    let c = cluster(
        vec![
            member("grok", "s1", "2026-09-01T00:00:00Z"),
            member("codex", "s2", "2026-09-04T00:00:00Z"),
            member("grok", "s3", "2026-09-09T00:00:00Z"),
        ],
        2,
        "2026-09-09T00:00:00Z",
    );
    assert!(passes_cutoff(&c));
}

#[test]
fn a_single_session_never_passes_cutoff_regardless_of_frequency() {
    let c = cluster(
        (0..20)
            .map(|i| member("codex", "s1", &format!("2026-09-{:02}T00:00:00Z", (i % 28) + 1)))
            .collect(),
        1,
        "2026-09-09T00:00:00Z",
    );
    assert!(!passes_cutoff(&c));
}

#[test]
fn two_sessions_pass_with_enough_frequency() {
    let mut members = Vec::new();
    for i in 0..8 {
        let session = if i % 2 == 0 { "s1" } else { "s2" };
        members.push(member("codex", session, "2026-09-01T00:00:00Z"));
    }
    let c = cluster(members, 1, "2026-09-01T00:00:00Z");
    assert!(passes_cutoff(&c));
}

#[test]
fn two_sessions_with_low_frequency_do_not_pass() {
    let c = cluster(
        vec![
            member("codex", "s1", "2026-09-01T00:00:00Z"),
            member("codex", "s2", "2026-09-02T00:00:00Z"),
        ],
        1,
        "2026-09-02T00:00:00Z",
    );
    assert!(!passes_cutoff(&c));
}

#[test]
fn score_increases_with_frequency_recency_agents_and_sequence_length() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_757_462_400); // 2026-09-10T00:00:00Z-ish anchor
    let small = cluster(
        vec![
            member("codex", "s1", "2026-09-01T00:00:00Z"),
            member("codex", "s2", "2026-09-01T00:00:00Z"),
            member("codex", "s3", "2026-09-01T00:00:00Z"),
        ],
        1,
        "2026-09-01T00:00:00Z",
    );
    let bigger = cluster(
        vec![
            member("codex", "s1", "2026-09-09T00:00:00Z"),
            member("grok", "s2", "2026-09-09T00:00:00Z"),
            member("codex", "s3", "2026-09-09T00:00:00Z"),
            member("grok", "s4", "2026-09-09T00:00:00Z"),
        ],
        3,
        "2026-09-09T00:00:00Z",
    );

    let score_small = score(&small, now, false);
    let score_bigger = score(&bigger, now, false);
    assert!(
        score_bigger > score_small,
        "bigger cluster should score higher: {score_bigger} vs {score_small}"
    );
}

#[test]
fn a_catalog_conflict_heavily_penalizes_the_score() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_757_462_400);
    let c = cluster(
        vec![
            member("codex", "s1", "2026-09-09T00:00:00Z"),
            member("codex", "s2", "2026-09-09T00:00:00Z"),
            member("codex", "s3", "2026-09-09T00:00:00Z"),
        ],
        1,
        "2026-09-09T00:00:00Z",
    );
    let without_conflict = score(&c, now, false);
    let with_conflict = score(&c, now, true);
    assert!(with_conflict < without_conflict / 5.0);
}

#[test]
fn score_is_finite_and_nonnegative() {
    let now = SystemTime::now();
    let c = cluster(vec![member("codex", "s1", "2026-09-01T00:00:00Z")], 1, "2026-09-01T00:00:00Z");
    let s = score(&c, now, false);
    assert!(s.is_finite());
    assert!(s >= 0.0);
}
