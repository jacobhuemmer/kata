//! CI-safe compatibility suite (`docs/design/05-prd.md` decision 11, §9 slice 2). The real
//! `~/Bitbucket/sdo-dops-catalog` only exists on Mason's machine; CI imports and checks the
//! sanitized, shape-preserving fixture at `tests/fixtures/sesami-shaped/` instead — same
//! kata count, same arg-type shape (86 booleans + 1 integer coerced), invented hostnames.

use std::path::PathBuf;

fn fixture_src_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/sesami-shaped/src"
    ))
}

#[test]
fn sanitized_fixture_imports_and_checks_clean() {
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join("kata");

    let summary =
        kadou_core::import_catalog(&fixture_src_dir(), &kata_dir, "sesami").expect("import");

    assert_eq!(
        summary.kata_written, 32,
        "expected all 32 fixture kata to convert"
    );
    assert_eq!(summary.booleans_coerced, 86, "boolean default coercions");
    assert_eq!(summary.integers_coerced, 1, "integer default coercions");

    let report = kadou_core::check_folder(&kata_dir, "sesami", &kadou_core::Vault::default())
        .expect("check");
    assert_eq!(report.kata_count(), 32, "checked kata count");
    assert_eq!(
        report.error_count(),
        0,
        "expected zero errors, got: {:#?}",
        report
            .files
            .iter()
            .flat_map(|f| f.diagnostics.iter())
            .filter(|d| d.is_error())
            .collect::<Vec<_>>()
    );
}

#[test]
fn empty_default_but_required_arg_survives_the_round_trip() {
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join("kata");
    kadou_core::import_catalog(&fixture_src_dir(), &kata_dir, "sesami").expect("import");

    let content = std::fs::read_to_string(kata_dir.join("sesami/ses-argocd-sync.sh")).unwrap();
    let (header, diags) = kadou_core::parse_header(&content);
    assert!(
        !diags.iter().any(kadou_core::Diagnostic::is_error),
        "{diags:?}"
    );
    let header = header.expect("header should parse");

    let app_name = header
        .args
        .iter()
        .find(|a| a.name == "app_name")
        .expect("app_name arg");
    assert!(
        app_name.is_required(),
        "app_name was `required: true, default: \"\"` in the source runbook — §4.6 says \
         that becomes a required arg with no default"
    );

    let k8s_context = header
        .args
        .iter()
        .find(|a| a.name == "k8s_context")
        .expect("k8s_context arg");
    assert!(
        !k8s_context.is_required(),
        "k8s_context has a non-empty default (`dev`) so it stays optional despite \
         `required: true` in the source"
    );

    let timeout = header
        .args
        .iter()
        .find(|a| a.name == "timeout")
        .expect("timeout arg");
    assert_eq!(
        timeout.default,
        Some(kadou_core::ArgDefault::Int(60)),
        "the `type: number` timeout coerces to `int = 60`"
    );
}

#[test]
fn the_repo_root_trigger_idiom_is_rewritten_to_kadou_root() {
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join("kata");
    kadou_core::import_catalog(&fixture_src_dir(), &kata_dir, "sesami").expect("import");

    let content = std::fs::read_to_string(kata_dir.join("sesami/cc4-aaa.sh")).unwrap();
    assert!(content.contains("KADOU_ROOT"));
    assert!(!content.contains("REPO_ROOT"));
}

#[test]
fn device_log_metrics_stays_a_multi_file_kata_with_helpers_intact() {
    let home = tempfile::tempdir().unwrap();
    let kata_dir = home.path().join("kata");
    kadou_core::import_catalog(&fixture_src_dir(), &kata_dir, "sesami").expect("import");

    assert!(kata_dir.join("sesami/device-log-metrics/kata.sh").is_file());
    assert!(
        kata_dir
            .join("sesami/device-log-metrics/lib/es_connect.sh")
            .is_file()
    );
    assert!(
        kata_dir
            .join("sesami/device-log-metrics/envs/prod.env")
            .is_file()
    );
}
