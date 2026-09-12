//! Cluster/fingerprint tests, including the §6.2/§6.3 worked example (synthetic).

use super::*;

fn candidate(steps: &[&str], agent: &str, session: &str, when: &str) -> Candidate {
    Candidate {
        steps: steps.iter().map(|s| s.to_string()).collect(),
        agent: agent.to_string(),
        session_id: session.to_string(),
        when: when.to_string(),
    }
}

const TEMPLATE: [&str; 2] = [
    "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP",
    "kubectl --context $CONTEXT -n $NAMESPACE logs deploy/$APP --tail=$N",
];

#[test]
fn identical_templates_from_different_sessions_and_agents_form_one_cluster() {
    let candidates = vec![
        candidate(&TEMPLATE, "grok", "s1", "2026-09-01T00:00:00Z"),
        candidate(&TEMPLATE, "codex", "s2", "2026-09-04T00:00:00Z"),
        candidate(&TEMPLATE, "grok", "s3", "2026-09-09T00:00:00Z"),
    ];

    let clusters = cluster_candidates(&candidates);

    assert_eq!(clusters.len(), 1);
    let cluster = &clusters[0];
    assert_eq!(cluster.freq(), 3);
    assert_eq!(cluster.unique_sessions(), 3);
    assert_eq!(cluster.unique_agents(), 2);
    assert_eq!(cluster.step_count, 2);
    assert_eq!(cluster.first_seen, "2026-09-01T00:00:00Z");
    assert_eq!(cluster.last_seen, "2026-09-09T00:00:00Z");
}

#[test]
fn fingerprint_is_stable_for_the_same_steps() {
    let a = fingerprint(&TEMPLATE.map(String::from));
    let b = fingerprint(&TEMPLATE.map(String::from));
    assert_eq!(a, b);
    assert_eq!(a.len(), 64, "sha256 hex digest is 64 chars: {a}");
}

#[test]
fn different_templates_produce_different_fingerprints() {
    let a = fingerprint(&["kubectl get pods".to_string()]);
    let b = fingerprint(&["kubectl get nodes".to_string()]);
    assert_ne!(a, b);
}

#[test]
fn a_prefix_extension_merges_into_the_same_cluster() {
    let base = candidate(&["kubectl -n $NAMESPACE get pods"], "codex", "s1", "2026-09-01T00:00:00Z");
    let extended = candidate(
        &["kubectl -n $NAMESPACE get pods | tail -n $N"],
        "codex",
        "s2",
        "2026-09-02T00:00:00Z",
    );

    let clusters = cluster_candidates(&[base, extended]);

    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].freq(), 2);
}

#[test]
fn unrelated_templates_stay_in_separate_clusters() {
    let a = candidate(&["git status"], "codex", "s1", "2026-09-01T00:00:00Z");
    let b = candidate(&["helm upgrade release chart"], "grok", "s2", "2026-09-02T00:00:00Z");

    let clusters = cluster_candidates(&[a, b]);

    assert_eq!(clusters.len(), 2);
}

#[test]
fn the_medoid_is_the_most_frequent_exact_template_in_the_cluster() {
    let common = candidate(&["kubectl get pods"], "codex", "s1", "2026-09-01T00:00:00Z");
    let common2 = candidate(&["kubectl get pods"], "codex", "s2", "2026-09-02T00:00:00Z");
    let rare_extension = candidate(
        &["kubectl get pods | tail -n $N"],
        "codex",
        "s3",
        "2026-09-03T00:00:00Z",
    );

    let clusters = cluster_candidates(&[common, common2, rare_extension]);

    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].template, "kubectl get pods");
    assert_eq!(clusters[0].freq(), 3);
}
