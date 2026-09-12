//! Draft-writer tests (`docs/design/06-session-mining.md` §2.7-2.8, §6.4 worked example).

use std::collections::BTreeMap;

use super::*;
use crate::cluster::Member;

fn member(agent: &str, session: &str, when: &str) -> Member {
    Member {
        agent: agent.to_string(),
        session_id: session.to_string(),
        when: when.to_string(),
    }
}

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn worked_example_cluster() -> (Cluster, Vec<StepWithParams>) {
    let cluster = Cluster {
        fingerprint: "abc123".to_string(),
        template: "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP\nkubectl --context $CONTEXT -n $NAMESPACE logs deploy/$APP --tail=$N".to_string(),
        step_count: 2,
        members: vec![
            member("grok", "s1", "2026-09-01T00:00:00Z"),
            member("codex", "s2", "2026-09-04T00:00:00Z"),
            member("grok", "s3", "2026-09-09T00:00:00Z"),
        ],
        first_seen: "2026-09-01T00:00:00Z".to_string(),
        last_seen: "2026-09-09T00:00:00Z".to_string(),
    };
    let steps = vec![
        StepWithParams {
            template: "kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP".to_string(),
            params: params(&[
                ("CONTEXT", "eks-dev"),
                ("NAMESPACE", "payments"),
                ("APP", "api"),
            ]),
        },
        StepWithParams {
            template: "kubectl --context $CONTEXT -n $NAMESPACE logs deploy/$APP --tail=$N"
                .to_string(),
            params: params(&[
                ("CONTEXT", "eks-dev"),
                ("NAMESPACE", "payments"),
                ("N", "200"),
            ]),
        },
    ];
    (cluster, steps)
}

#[test]
fn builds_a_valid_header_that_kadou_check_accepts() {
    let (cluster, steps) = worked_example_cluster();
    let proposal = build_proposal(&cluster, &steps, 4.2, RiskLevel::Medium, &[]).unwrap();

    let (header, diagnostics) = kadou_core::parse_header(&proposal.kata_source);
    assert!(
        header.is_some(),
        "expected a valid header, got diagnostics: {diagnostics:?}"
    );
    let header = header.unwrap();
    assert_eq!(header.risk, RiskLevel::Medium);
    assert!(!header.about.is_empty());
}

#[test]
fn declared_params_become_quoted_env_references_in_the_body() {
    let (cluster, steps) = worked_example_cluster();
    let proposal = build_proposal(&cluster, &steps, 4.2, RiskLevel::Medium, &[]).unwrap();

    assert!(proposal.kata_source.contains("CONTEXT=\"${CONTEXT:?"));
    assert!(proposal.kata_source.contains("\"${CONTEXT}\""));
    assert!(proposal.kata_source.contains("\"${NAMESPACE}\""));
    assert!(proposal.kata_source.contains("\"${APP}\""));
    assert!(proposal.kata_source.contains("\"${N}\""));
    // $N must not have matched inside $NAMESPACE.
    assert!(!proposal.kata_source.contains("\"${N}AMESPACE\""));
}

#[test]
fn generic_shape_params_never_appear_as_a_named_arg_default() {
    let cluster = Cluster {
        fingerprint: "def456".to_string(),
        template: "curl $HOST".to_string(),
        step_count: 1,
        members: vec![member("codex", "s1", "2026-09-01T00:00:00Z")],
        first_seen: "2026-09-01T00:00:00Z".to_string(),
        last_seen: "2026-09-01T00:00:00Z".to_string(),
    };
    let steps = vec![StepWithParams {
        template: "curl $HOST".to_string(),
        params: params(&[("HOST", "api.customer.example")]),
    }];

    let proposal = build_proposal(&cluster, &steps, 1.0, RiskLevel::High, &[]).unwrap();

    assert!(!proposal.kata_source.contains("api.customer.example"));
    assert!(!proposal.kata_source.to_lowercase().contains("host: text"));
}

#[test]
fn a_secret_shaped_param_value_that_slipped_through_is_dropped_fail_closed() {
    let blob = "Zm9vYmFyYmF6cXV1eGNvcmdlZ3JhdWx0Z2FycGx5"; // synthetic high-entropy, no known prefix
    let cluster = Cluster {
        fingerprint: "ghi789".to_string(),
        template: "deploy token=$TOKEN".to_string(),
        step_count: 1,
        members: vec![member("codex", "s1", "2026-09-01T00:00:00Z")],
        first_seen: "2026-09-01T00:00:00Z".to_string(),
        last_seen: "2026-09-01T00:00:00Z".to_string(),
    };
    let steps = vec![StepWithParams {
        template: "deploy token=$TOKEN".to_string(),
        params: params(&[("TOKEN", blob)]),
    }];

    let err = build_proposal(&cluster, &steps, 1.0, RiskLevel::High, &[]).unwrap_err();
    assert_eq!(err, ProposeError::RedactionUnproven);
}

#[test]
fn meta_json_carries_no_raw_command_text() {
    let (cluster, steps) = worked_example_cluster();
    let proposal = build_proposal(&cluster, &steps, 4.2, RiskLevel::Medium, &[]).unwrap();

    let value: serde_json::Value = serde_json::from_str(&proposal.meta_json).unwrap();
    assert_eq!(value["fingerprint"], "abc123");
    assert_eq!(value["freq"], 3);
    assert_eq!(value["unique_sessions"], 3);
    assert_eq!(value["unique_agents"], 2);
    assert_eq!(value["risk"], "medium");
    assert!(!proposal.meta_json.contains("kubectl"));
    assert!(!proposal.meta_json.contains("get pods"));
}

#[test]
fn slug_is_derived_from_the_first_step_and_is_a_valid_id_segment() {
    let (cluster, steps) = worked_example_cluster();
    let proposal = build_proposal(&cluster, &steps, 4.2, RiskLevel::Medium, &[]).unwrap();
    assert!(
        proposal
            .slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    );
    assert!(!proposal.slug.is_empty());
    assert!(proposal.slug.starts_with("kubectl"));
}

#[test]
fn context_and_namespace_are_required_with_no_default_never_leaking_the_example_value() {
    // 06 §6.4's own worked example: context/namespace are `required: true` with no default at
    // all, unlike app/tail which do get one. A captured example like "eks-dev" is exactly the
    // per-environment identifier a human should supply fresh, not a value worth hardcoding --
    // and never writing it also means it can never leak an internal environment name.
    let (cluster, steps) = worked_example_cluster();
    let proposal = build_proposal(&cluster, &steps, 4.2, RiskLevel::Medium, &[]).unwrap();

    let (header, _) = kadou_core::parse_header(&proposal.kata_source);
    let header = header.unwrap();
    let context_arg = header.args.iter().find(|a| a.name == "context").unwrap();
    let namespace_arg = header.args.iter().find(|a| a.name == "namespace").unwrap();
    assert!(context_arg.is_required(), "{context_arg:?}");
    assert!(namespace_arg.is_required(), "{namespace_arg:?}");

    assert!(!proposal.kata_source.contains("eks-dev"));
    assert!(!proposal.kata_source.contains("payments"));
    assert!(proposal.kata_source.contains("CONTEXT=\"${CONTEXT:?"));
    assert!(proposal.kata_source.contains("NAMESPACE=\"${NAMESPACE:?"));

    // app/tail still get their observed value as a default.
    let app_arg = header.args.iter().find(|a| a.name == "app").unwrap();
    assert!(!app_arg.is_required(), "{app_arg:?}");
    assert!(proposal.kata_source.contains("APP=\"${APP:-api}\""));
}

#[test]
fn redact_extra_terms_apply_to_the_about_line_and_body() {
    let cluster = Cluster {
        fingerprint: "jkl012".to_string(),
        template: "deploy-tool run acmecorp".to_string(),
        step_count: 1,
        members: vec![member("codex", "s1", "2026-09-01T00:00:00Z")],
        first_seen: "2026-09-01T00:00:00Z".to_string(),
        last_seen: "2026-09-01T00:00:00Z".to_string(),
    };
    let steps = vec![StepWithParams {
        template: "deploy-tool run acmecorp".to_string(),
        params: BTreeMap::new(),
    }];

    let proposal = build_proposal(
        &cluster,
        &steps,
        1.0,
        RiskLevel::High,
        &["acmecorp".to_string()],
    )
    .unwrap();

    assert!(!proposal.kata_source.contains("acmecorp"));
}
