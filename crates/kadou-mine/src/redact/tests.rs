//! Table-driven redaction tests (`docs/design/06-session-mining.md` §4.3). Every input is
//! synthetic -- invented for this test, never copied from a real session.

use super::*;

fn redact(text: &str) -> Redacted {
    redact_all(text, &[])
}

#[test]
fn r1_pem_block_is_fully_redacted() {
    let pem = "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIFAKEKEYDATAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\nafter";
    let out = redact(pem);
    assert!(out.text.contains("[REDACTED PRIVATE KEY]"), "{}", out.text);
    assert!(!out.text.contains("BEGIN"), "{}", out.text);
    assert!(!out.text.contains("MIIFAKEKEYDATA"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::PemPrivateKey));
}

#[test]
fn r2_bearer_token_is_redacted() {
    let out = redact("curl -H \"Authorization: Bearer abcdefghijklmnop\" https://api.example");
    assert!(out.text.contains("[REDACTED]"), "{}", out.text);
    assert!(!out.text.contains("abcdefghijklmnop"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::Bearer));
}

#[test]
fn r3_key_value_assignment_keeps_key_redacts_value() {
    let out = redact("API_TOKEN=s3cretvalue99");
    assert_eq!(out.text, "API_TOKEN=[REDACTED]");
    assert!(out.rules_hit.contains(&RuleId::Assign));
}

#[test]
fn r4_github_pat_is_redacted() {
    let token = format!("ghp_{}", "A".repeat(36));
    let out = redact(&format!("git push using {token}"));
    assert!(!out.text.contains("ghp_"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::KnownTokens));
}

#[test]
fn r4_slack_bot_token_is_redacted() {
    let out = redact("token=xoxb-1234567890-abcdefg");
    assert!(!out.text.contains("xoxb-"), "{}", out.text);
}

#[test]
fn r4_jwt_is_redacted() {
    let out = redact("saved session eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig to disk");
    assert!(!out.text.contains("eyJhbGciOiJIUzI1NiJ9"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::KnownTokens));
}

#[test]
fn r4_aws_access_key_is_redacted() {
    let key = format!("AKIA{}", "A".repeat(16));
    let out = redact(&format!("aws configure set aws_access_key_id {key}"));
    assert!(!out.text.contains("AKIA"), "{}", out.text);
}

#[test]
fn r5_postgres_dsn_drops_password_and_hostname() {
    let out = redact("postgres://u:p@db.internal:5432/app");
    assert!(!out.text.contains(":p@"), "{}", out.text);
    assert!(!out.text.contains("db.internal"), "{}", out.text);
    assert!(out.text.contains("$HOST"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::Connection));
}

#[test]
fn r6_email_is_replaced() {
    let out = redact("contact person@example.com for access");
    assert_eq!(out.text, "contact $EMAIL for access");
    assert!(out.rules_hit.contains(&RuleId::Email));
}

#[test]
fn r7_fqdn_becomes_host() {
    let out = redact("curl https://api.customer.example/status");
    assert!(!out.text.contains("customer.example"), "{}", out.text);
    assert!(out.text.contains("$HOST"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::Hostname));
}

#[test]
fn r7_public_docs_domain_is_left_unchanged() {
    let out = redact("see https://kubernetes.io/docs for reference");
    assert_eq!(out.text, "see https://kubernetes.io/docs for reference");
    assert!(!out.rules_hit.contains(&RuleId::Hostname));
}

#[test]
fn r7_a_subdomain_of_an_allowlisted_three_label_host_is_also_left_unchanged() {
    // Exercises hostname_allowed's `ends_with` branch specifically (distinct from an exact
    // match against the allowlist entry itself, already covered above).
    let out = redact("see https://docs.pkg.go.dev/reference for reference");
    assert_eq!(
        out.text,
        "see https://docs.pkg.go.dev/reference for reference"
    );
    assert!(!out.rules_hit.contains(&RuleId::Hostname));
}

#[test]
fn r8_ipv4_becomes_ip() {
    let out = redact("ping 10.1.2.3 to check reachability");
    assert_eq!(out.text, "ping $IP to check reachability");
    assert!(out.rules_hit.contains(&RuleId::Ip));
}

#[test]
fn r9_redact_extra_terms_are_replaced() {
    let out = redact_all("deploying to acmecorp now", &["acmecorp".to_string()]);
    assert!(!out.text.contains("acmecorp"), "{}", out.text);
    assert!(out.text.contains("$CUSTOMER"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::Customer));
}

#[test]
fn r10_kubeconfig_field_value_is_redacted() {
    // `certificate-authority-data` / `client-key-data` never collide with R3's key-name
    // alternation, so this exercises R10 unambiguously; `token:` (also an R3 keyword) is
    // covered separately below and may legitimately be caught by either rule.
    let out = redact("certificate-authority-data: LS0tLQ==verylongbase64value");
    assert!(!out.text.contains("LS0tLQ"), "{}", out.text);
    assert!(out.text.contains("[REDACTED]"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::Kubeconfig));
}

#[test]
fn kubeconfig_style_token_value_is_redacted_by_some_rule() {
    let out = redact("token: k8s-secret-token-value");
    assert!(!out.text.contains("k8s-secret-token-value"), "{}", out.text);
    assert!(out.text.contains("[REDACTED]"), "{}", out.text);
}

#[test]
fn r11_op_reference_is_redacted() {
    let out = redact("op://Employee/item/password");
    assert!(!out.text.contains("password"), "{}", out.text);
    assert!(out.rules_hit.contains(&RuleId::OpRef));
}

#[test]
fn r12_home_path_becomes_home_token() {
    let out = redact("/Users/someone/src/repo/run.sh");
    assert_eq!(out.text, "$HOME/src/repo/run.sh");
    assert!(out.rules_hit.contains(&RuleId::Home));
}

#[test]
fn r13_chat_turn_lines_are_dropped() {
    let out = redact(
        "kubectl get pods\nHuman: do the thing\n### Turn 3 (Human)\nAssistant: ok\nkubectl get pods",
    );
    assert!(!out.text.contains("Human:"), "{}", out.text);
    assert!(!out.text.contains("Assistant:"), "{}", out.text);
    assert!(!out.text.contains("### Turn"), "{}", out.text);
    assert_eq!(out.text.matches("kubectl get pods").count(), 2);
    assert!(out.rules_hit.contains(&RuleId::SessionQuote));
}

#[test]
fn negative_plain_kubectl_command_is_unchanged() {
    let out = redact("kubectl -n kube-system get pods");
    assert_eq!(out.text, "kubectl -n kube-system get pods");
    assert!(out.rules_hit.is_empty(), "{:?}", out.rules_hit);
}

#[test]
fn fail_closed_high_entropy_leak_is_detected_after_redaction() {
    // A bare high-entropy blob with no recognizable key= prefix or known-token shape --
    // R1-R13 have nothing to match, so the fail-closed R14 check is what has to catch it.
    let blob = "Zm9vYmFyYmF6cXV1eGNvcmdlZ3JhdWx0Z2FycGx5"; // synthetic, not a real credential
    let out = redact(&format!("run helper {blob} now"));
    assert!(has_high_entropy_leak(&out.text), "{}", out.text);
}

#[test]
fn ordinary_short_tokens_do_not_trigger_the_high_entropy_check() {
    let out = redact("kubectl --context $CONTEXT -n $NAMESPACE get pods -l app=$APP");
    assert!(!has_high_entropy_leak(&out.text), "{}", out.text);
}

#[test]
fn a_blob_with_only_one_slash_is_still_flagged_not_mistaken_for_a_path() {
    // Later-7/L2 (`docs/design/12-mvp-review.md` §2 P2, §4, §6): standard base64's alphabet
    // includes `/`, so a real unredacted secret commonly contains exactly one by chance over
    // a 24+ char run -- treating "the match contains a slash at all" as "this is a path" (the
    // old rule) missed the single most common shape of an unredacted secret. `/` is itself in
    // HIGH_ENTROPY's character class, so a leading or trailing slash joins the match rather
    // than bounding it; a single joined slash on either side must not exempt it.
    let blob = "Zm9vYmFyYmF6cXV1eGNvcmdlZ3JhdWx0Z2FycGx5";
    assert!(has_high_entropy_leak(&format!("/{blob} trailing text")));
    assert!(has_high_entropy_leak(&format!("leading text {blob}/")));
}

#[test]
fn a_real_multi_segment_path_is_not_flagged() {
    // Two or more `/`-separated segments is what actually distinguishes a filesystem path
    // from a bare secret blob that happens to contain one stray slash.
    assert!(!has_high_entropy_leak(
        "reading /very/long/nested/directory/path/that/exceeds/the/24/char/bound now"
    ));
}

#[test]
fn a_variable_prefixed_path_reference_is_not_flagged() {
    // `$` isn't in HIGH_ENTROPY's own character class, so the match starts right after it --
    // the character immediately before the match (in the original text, not the match itself)
    // is the signal that this run continues a variable-based path reference like
    // `$PATH_1/segment` rather than being a bare secret.
    assert!(!has_high_entropy_leak(
        "cd $PATH_1/some-long-enough-segment-name-here && ls"
    ));
}

#[test]
fn a_long_kebab_case_flag_is_not_flagged() {
    // `06` §4.3's own carve-out: "strings ... that are not paths or --flags".
    assert!(!has_high_entropy_leak(
        "run --some-extremely-long-kebab-case-flag-name-right-here now"
    ));
}

#[test]
fn no_ghp_substring_survives_redaction() {
    let token = format!("ghp_{}", "A".repeat(36));
    let out = redact(&format!(
        "git remote set-url origin https://{token}@github.com/x/y.git"
    ));
    assert!(!out.text.contains("ghp_"), "{}", out.text);
}
