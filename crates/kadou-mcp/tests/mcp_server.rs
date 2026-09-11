//! Drives the real [`KadouMcpServer`] end to end over an in-process `tokio::io::duplex`,
//! using rmcp's own client (`docs/design/05-prd.md` §9 slice 5: "Use rmcp's own client in
//! tests to drive the server over an in-process duplex or a spawned stdio child.").
//!
//! Every invariant §9 slice 5 lists is exercised here except stdin/stdout purity (covered by
//! the spawned-subprocess test in the `kadou` bin crate, where a real OS pipe is the only way
//! to observe a stray byte on the server's own stdout) and the tools/list byte-identity gate
//! (covered directly in `kadou-mcp`'s own unit tests against the static schema).

use std::path::{Path, PathBuf};

use kadou_core::{Config, KadouPaths, RiskLevel, Vault, VaultStore};
use kadou_mcp::{KadouMcpServer, ServerState};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};

const HELLO: &str = "#!/bin/sh\n# ---\n# about: Print a greeting\n# risk:  low\n# args:\n#   name: text = world  # Who to greet\n# ---\necho \"hello, ${NAME}\"\n";

fn fixture_src_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/sesami-shaped/src"
    ))
}

struct TestHome {
    _dir: tempfile::TempDir,
    paths: KadouPaths,
}

/// A temp `KadouPaths` root with the sesami-shaped fixture imported and the starter `hello`
/// kata materialized, config saved per `configure`.
fn setup_sesami(configure: impl FnOnce(&mut Config)) -> TestHome {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = KadouPaths {
        config_dir: dir.path().join("config"),
        data_dir: dir.path().join("data"),
        state_dir: dir.path().join("state"),
    };
    kadou_core::import_catalog(&fixture_src_dir(), &paths.kata_dir(), "sesami")
        .expect("sesami-shaped fixture imports");

    std::fs::create_dir_all(paths.kata_dir().join("starter")).unwrap();
    std::fs::write(paths.kata_dir().join("starter/hello.sh"), HELLO).unwrap();

    let mut config = Config::default();
    configure(&mut config);
    config.save(&paths.config_file()).unwrap();

    TestHome { _dir: dir, paths }
}

fn write_kata(kata_dir: &Path, rel: &str, content: &str) {
    let path = kata_dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

type Client = rmcp::service::RunningService<rmcp::RoleClient, ()>;

async fn spawn_server(state: ServerState) -> Client {
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        let running = KadouMcpServer::new(state)
            .serve(server_io)
            .await
            .expect("server initializes");
        let _ = running.waiting().await;
    });
    ().serve(client_io).await.expect("client initializes")
}

fn as_text(result: &CallToolResult) -> String {
    match result.content.first() {
        Some(ContentBlock::Text(text)) => text.text.clone(),
        other => panic!("expected exactly one text content block, got {other:?}"),
    }
}

async fn call(
    client: &Client,
    name: &str,
    arguments: serde_json::Value,
) -> (serde_json::Value, bool) {
    let mut params = CallToolRequestParams::new(name.to_string());
    if let Some(object) = arguments.as_object() {
        params = params.with_arguments(object.clone());
    }
    let result = client
        .call_tool(params)
        .await
        .expect("call_tool transport succeeds");
    let value: serde_json::Value =
        serde_json::from_str(&as_text(&result)).expect("valid JSON body");
    (value, result.is_error.unwrap_or(false))
}

#[tokio::test]
async fn tools_list_over_the_wire_is_the_four_tools_in_order() {
    let home = setup_sesami(|_| {});
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let result = client.list_tools(None).await.expect("list_tools");
    let names: Vec<String> = result.tools.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(
        names,
        vec!["list_kata", "describe_kata", "run_kata", "propose_kata"]
    );
}

#[tokio::test]
async fn list_kata_at_default_ceiling_returns_exactly_the_five_low_risk_sesami_ids() {
    let home = setup_sesami(|_| {}); // default: human medium, agent low
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "list_kata",
        serde_json::json!({"folder": "sesami", "limit": 50}),
    )
    .await;
    assert!(!is_error);

    let mut ids: Vec<String> = value["kata"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "sesami/clone-ses-repos",
            "sesami/device-log-metrics",
            "sesami/helm-package",
            "sesami/sdo-k8s-ses",
            "sesami/ses-automation",
        ]
    );
    // §6.2: high (ses-release-build) and critical (ses-deploy) never reach a default agent.
    assert!(!ids.iter().any(|id| id.contains("ses-deploy")));
    assert!(!ids.iter().any(|id| id.contains("ses-release-build")));
}

#[tokio::test]
async fn describe_a_medium_kata_lists_needs_names_but_never_values() {
    let home = setup_sesami(|c| {
        c.max_risk = RiskLevel::Medium;
        c.agent.max_risk = RiskLevel::Medium;
    });
    let store = VaultStore::new(&home.paths.data_dir);
    let mut vault = Vault::default();
    vault.set("jenkins_user", "ci-user-value", false);
    vault.set("jenkins_token", "not-a-real-token1", true);
    store.save(&vault).unwrap();

    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "describe_kata",
        serde_json::json!({"id": "sesami/cc4-aaa"}),
    )
    .await;
    assert!(!is_error);
    let needs: Vec<String> = value["needs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    assert!(needs.contains(&"jenkins_user".to_string()));
    assert!(needs.contains(&"jenkins_token".to_string()));

    let raw = value.to_string();
    assert!(!raw.contains("ci-user-value"));
    assert!(!raw.contains("not-a-real-token1"));
}

#[tokio::test]
async fn run_starter_hello_succeeds_with_an_mcp_interface_history_record() {
    let home = setup_sesami(|_| {});
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "starter/hello"}),
    )
    .await;
    assert!(!is_error, "{value}");
    assert_eq!(value["status"], "success");
    assert!(value["output"].as_str().unwrap().contains("hello, world"));

    let records_dir = home.paths.state_dir.join("history/records");
    let mut entries = std::fs::read_dir(&records_dir).unwrap();
    let entry = entries.next().expect("one history record").unwrap();
    assert!(entries.next().is_none(), "exactly one history record");

    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(entry.path()).unwrap()).unwrap();
    assert_eq!(record["interface"], "mcp");
    assert_eq!(record["status"], "success");
    assert_eq!(record["id"], "starter/hello");
}

#[tokio::test]
async fn run_above_ceiling_is_iserror_no_such_kata() {
    let home = setup_sesami(|_| {});
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "sesami/ses-deploy"}),
    )
    .await;
    assert!(is_error);
    assert_eq!(value["error"], "no_such_kata");
    assert_eq!(value["isError"], true);
}

#[tokio::test]
async fn secret_need_values_are_redacted_in_both_the_result_and_the_on_disk_log() {
    let home = setup_sesami(|_| {});
    write_kata(
        &home.paths.kata_dir(),
        "team/echo-secret.sh",
        "#!/bin/sh\n# ---\n# about: Echo a secret\n# risk:  low\n# needs: api_token\n# ---\necho \"token=$API_TOKEN\"\n",
    );
    let store = VaultStore::new(&home.paths.data_dir);
    let mut vault = Vault::default();
    vault.set("api_token", "not-a-real-secret1", true);
    store.save(&vault).unwrap();

    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/echo-secret"}),
    )
    .await;
    assert!(!is_error, "{value}");
    let output = value["output"].as_str().unwrap();
    assert!(
        output.contains("****"),
        "result output not redacted: {output}"
    );
    assert!(!output.contains("not-a-real-secret1"));

    let log_path = value["log_path"].as_str().unwrap();
    let log = std::fs::read_to_string(log_path).unwrap();
    assert!(log.contains("****"), "on-disk log not redacted: {log}");
    assert!(!log.contains("not-a-real-secret1"));
}

#[tokio::test]
async fn a_parent_env_value_on_the_server_process_never_reaches_the_child() {
    let home = setup_sesami(|_| {});
    write_kata(
        &home.paths.kata_dir(),
        "team/env-check.sh",
        "#!/bin/sh\n# ---\n# about: Check env leak\n# risk:  low\n# ---\necho \"leak=${CARGO_MANIFEST_DIR:-none}\"\n",
    );
    assert!(
        std::env::var("CARGO_MANIFEST_DIR").is_ok(),
        "sanity: this var really is set in the test process"
    );

    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/env-check"}),
    )
    .await;
    assert!(!is_error, "{value}");
    assert_eq!(value["output"], "leak=none");
}

#[tokio::test]
async fn args_naming_a_need_is_invalid_args_not_a_shell_injection() {
    let home = setup_sesami(|c| {
        c.max_risk = RiskLevel::Medium;
        c.agent.max_risk = RiskLevel::Medium;
    });
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "sesami/cc4-aaa", "args": {"jenkins_url": "https://evil.example.com"}}),
    )
    .await;
    assert!(is_error);
    assert_eq!(value["error"], "invalid_args");
}

#[tokio::test]
async fn mcp_never_writes_the_vault() {
    let home = setup_sesami(|_| {});
    let store = VaultStore::new(&home.paths.data_dir);
    let mut vault = Vault::default();
    vault.set("plain_val", "not-a-secret-value", false);
    store.save(&vault).unwrap();

    let vault_file = store.vault_file();
    let before_bytes = std::fs::read(&vault_file).unwrap();
    let before_mtime = std::fs::metadata(&vault_file).unwrap().modified().unwrap();

    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let _ = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "starter/hello"}),
    )
    .await;
    let _ = call(
        &client,
        "describe_kata",
        serde_json::json!({"id": "starter/hello"}),
    )
    .await;
    let _ = call(
        &client,
        "propose_kata",
        serde_json::json!({"id": "sesami/new-thing", "source": HELLO}),
    )
    .await;

    let after_bytes = std::fs::read(&vault_file).unwrap();
    let after_mtime = std::fs::metadata(&vault_file).unwrap().modified().unwrap();
    assert_eq!(before_bytes, after_bytes, "vault content must be untouched");
    assert_eq!(before_mtime, after_mtime, "vault mtime must be untouched");
}

#[tokio::test]
async fn max_wait_returns_running_with_a_pollable_log_path() {
    let home = setup_sesami(|c| c.mcp.max_wait = "150ms".to_string());
    write_kata(
        &home.paths.kata_dir(),
        "team/slow.sh",
        "#!/bin/sh\n# ---\n# about: Slow\n# risk:  low\n# ---\nsleep 1\necho done\n",
    );

    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(&client, "run_kata", serde_json::json!({"id": "team/slow"})).await;
    assert!(!is_error, "{value}");
    assert_eq!(value["status"], "running");
    let log_path = PathBuf::from(value["log_path"].as_str().unwrap());
    assert!(log_path.is_file(), "log_path must already be a real file");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let log = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log.contains("done"),
        "background run should finish and update the log: {log}"
    );
}

#[tokio::test]
async fn the_server_wide_concurrency_limit_returns_busy() {
    let home = setup_sesami(|c| c.mcp.max_wait = "100ms".to_string());
    write_kata(
        &home.paths.kata_dir(),
        "team/slow-a.sh",
        "#!/bin/sh\n# ---\n# about: Slow A\n# risk:  low\n# ---\nsleep 2\n",
    );
    write_kata(
        &home.paths.kata_dir(),
        "team/slow-b.sh",
        "#!/bin/sh\n# ---\n# about: Slow B\n# risk:  low\n# ---\nsleep 2\n",
    );

    let state = ServerState::new(home.paths.clone(), None, 1, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (first, first_is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/slow-a"}),
    )
    .await;
    assert!(!first_is_error, "{first}");
    assert_eq!(first["status"], "running");

    let (second, second_is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/slow-b"}),
    )
    .await;
    assert!(second_is_error);
    assert_eq!(second["error"], "busy");
}

#[tokio::test]
async fn untrusted_project_local_kata_is_absent_from_list_kata_and_cannot_run() {
    let home = setup_sesami(|_| {});
    let project = tempfile::tempdir().unwrap();
    write_kata(
        &project.path().join("kata"),
        "tidy.sh",
        "#!/bin/sh\n# ---\n# about: Tidy\n# risk:  low\n# needs: jenkins_token\n# ---\necho tidy\n",
    );

    let state = ServerState::new(home.paths.clone(), None, 2, project.path());
    let client = spawn_server(state).await;

    let (value, _) = call(&client, "list_kata", serde_json::json!({"limit": 200})).await;
    let ids: Vec<String> = value["kata"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["id"].as_str().unwrap().to_string())
        .collect();
    assert!(!ids.iter().any(|id| id.contains("tidy")), "{ids:?}");

    let (run_value, run_is_error) =
        call(&client, "run_kata", serde_json::json!({"id": "./tidy"})).await;
    assert!(run_is_error);
    assert_eq!(run_value["error"], "no_such_kata");
}

#[tokio::test]
async fn trusted_project_local_kata_is_visible_and_runs() {
    let home = setup_sesami(|_| {});
    let project = tempfile::tempdir().unwrap();
    write_kata(
        &project.path().join("kata"),
        "deploy.sh",
        "#!/bin/sh\n# ---\n# about: Local deploy\n# risk:  low\n# ---\necho local-ok\n",
    );

    let canonical_kata_dir = project.path().join("kata").canonicalize().unwrap();
    let mut config = Config::load(&home.paths.config_file()).unwrap();
    config.trust.paths.push(canonical_kata_dir);
    config.save(&home.paths.config_file()).unwrap();

    let state = ServerState::new(home.paths.clone(), None, 2, project.path());
    let client = spawn_server(state).await;

    let (list_value, _) = call(&client, "list_kata", serde_json::json!({"limit": 200})).await;
    let ids: Vec<String> = list_value["kata"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&"./deploy".to_string()), "{ids:?}");

    let (run_value, is_error) =
        call(&client, "run_kata", serde_json::json!({"id": "./deploy"})).await;
    assert!(!is_error, "{run_value}");
    assert_eq!(run_value["output"], "local-ok");
}

#[tokio::test]
async fn propose_kata_writes_a_draft_that_never_registers_or_runs() {
    let home = setup_sesami(|_| {});
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let source = "#!/bin/sh\n# ---\n# about: New kata\n# risk:  low\n# ---\necho hi\n";
    let (propose_value, propose_is_error) = call(
        &client,
        "propose_kata",
        serde_json::json!({"id": "sesami/new-thing", "source": source}),
    )
    .await;
    assert!(!propose_is_error, "{propose_value}");
    assert_eq!(propose_value["status"], "proposed");
    assert_eq!(propose_value["id"], "proposed/sesami/new-thing");
    assert_eq!(propose_value["accept"], "kadou accept sesami/new-thing");

    // The kata library itself never sees it.
    assert!(!home.paths.kata_dir().join("sesami/new-thing.sh").exists());

    let (run_value, run_is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "proposed/sesami/new-thing"}),
    )
    .await;
    assert!(run_is_error);
    assert_eq!(run_value["error"], "draft");

    let (describe_value, describe_is_error) = call(
        &client,
        "describe_kata",
        serde_json::json!({"id": "proposed/sesami/new-thing"}),
    )
    .await;
    assert!(!describe_is_error, "{describe_value}");
    assert_eq!(describe_value["about"], "New kata");
}

#[tokio::test]
async fn visible_critical_kata_not_allow_listed_returns_pending_grant() {
    // §9 slice 6: folder policy critical + agent max_risk critical makes ses-deploy
    // *visible*, but it is not in [agent].allow, so it must not just run.
    let home = setup_sesami(|c| {
        c.folder.insert(
            "sesami".to_string(),
            kadou_core::FolderConfig {
                max_risk: Some(RiskLevel::Critical),
                agent_max_risk: None,
            },
        );
        c.agent.max_risk = RiskLevel::Critical;
    });
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "sesami/ses-deploy", "args": {"version": "1", "oke_cluster": "uat"}}),
    )
    .await;
    assert!(!is_error, "{value}");
    assert_eq!(value["status"], "pending_grant");
    assert_eq!(value["id"], "sesami/ses-deploy");
    assert_eq!(value["risk"], "critical");
    assert!(value["pending_id"].as_str().is_some());
    assert!(
        value["approve"]
            .as_str()
            .unwrap()
            .starts_with("kadou grant approve ")
    );
    assert!(value["expires"].as_str().is_some());

    let pending_path = PathBuf::from(value["pending_path"].as_str().unwrap());
    assert!(pending_path.is_file(), "pending_path must be a real file");
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&pending_path).unwrap()).unwrap();
    assert_eq!(record["requester"], "mcp");
    assert_eq!(record["args"]["version"], "1");
    // Needs never appear in the pending record's args (§6.4 item 1).
    assert!(record.get("jenkins_token").is_none());
    assert!(record["sha256"].as_str().unwrap().starts_with("sha256:"));

    // No run ever happened: no history record was written for this attempt.
    let records_dir = home.paths.state_dir.join("history/records");
    assert!(
        !records_dir.is_dir() || std::fs::read_dir(&records_dir).unwrap().next().is_none(),
        "a pending_grant must not itself execute anything"
    );
}

#[tokio::test]
async fn at_the_default_ceiling_the_same_critical_kata_is_simply_invisible() {
    let home = setup_sesami(|_| {}); // default: human medium, agent low
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "sesami/ses-deploy"}),
    )
    .await;
    assert!(is_error);
    assert_eq!(value["error"], "no_such_kata");
}

#[tokio::test]
async fn allow_listed_kata_runs_instead_of_pending() {
    let home = setup_sesami(|c| {
        c.max_risk = RiskLevel::Critical;
        c.agent.max_risk = RiskLevel::Critical;
        c.agent.allow = vec!["team/allowed-critical".to_string()];
    });
    write_kata(
        &home.paths.kata_dir(),
        "team/allowed-critical.sh",
        "#!/bin/sh\n# ---\n# about: Allowed critical\n# risk:  critical\n# ---\necho allowed-ran\n",
    );
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/allowed-critical"}),
    )
    .await;
    assert!(!is_error, "{value}");
    assert_eq!(value["status"], "success");
    assert_eq!(value["output"], "allowed-ran");
}

#[tokio::test]
async fn a_pinned_allow_entry_stops_granting_once_the_kata_file_changes() {
    let home = setup_sesami(|c| {
        c.max_risk = RiskLevel::Critical;
        c.agent.max_risk = RiskLevel::Critical;
    });
    write_kata(
        &home.paths.kata_dir(),
        "team/pinned.sh",
        "#!/bin/sh\n# ---\n# about: Pinned\n# risk:  critical\n# ---\necho v1\n",
    );
    let sha_v1 = kadou_core::file_sha256(&home.paths.kata_dir().join("team/pinned.sh")).unwrap();

    let mut config = Config::load(&home.paths.config_file()).unwrap();
    config.agent.allow = vec![format!("team/pinned@{sha_v1}")];
    config.save(&home.paths.config_file()).unwrap();

    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    // Pinned to the current file: runs.
    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/pinned"}),
    )
    .await;
    assert!(!is_error, "{value}");
    assert_eq!(value["status"], "success");

    // The file changes; the pin no longer matches, so it falls back to pending_grant.
    write_kata(
        &home.paths.kata_dir(),
        "team/pinned.sh",
        "#!/bin/sh\n# ---\n# about: Pinned\n# risk:  critical\n# ---\necho v2\n",
    );
    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/pinned"}),
    )
    .await;
    assert!(!is_error, "{value}");
    assert_eq!(value["status"], "pending_grant");
}

#[tokio::test]
async fn a_second_identical_request_dedupes_onto_the_same_pending_id() {
    let home = setup_sesami(|c| {
        c.max_risk = RiskLevel::Critical;
        c.agent.max_risk = RiskLevel::Critical;
    });
    write_kata(
        &home.paths.kata_dir(),
        "team/dedupe.sh",
        "#!/bin/sh\n# ---\n# about: Dedupe\n# risk:  critical\n# args:\n#   name: text = x\n# ---\necho hi\n",
    );
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (first, _) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/dedupe", "args": {"name": "a"}}),
    )
    .await;
    let (second, _) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/dedupe", "args": {"name": "a"}}),
    )
    .await;
    assert_eq!(first["pending_id"], second["pending_id"]);

    // Different args are a different pending record, not deduped onto the first.
    let (third, _) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "team/dedupe", "args": {"name": "b"}}),
    )
    .await;
    assert_ne!(first["pending_id"], third["pending_id"]);

    let pending_dir = home.paths.state_dir.join("pending");
    let count = std::fs::read_dir(&pending_dir).unwrap().count();
    assert_eq!(count, 2, "exactly two distinct pending records on disk");
}

#[tokio::test]
async fn an_expired_pending_record_no_longer_dedupes() {
    let home = setup_sesami(|c| {
        c.max_risk = RiskLevel::Critical;
        c.agent.max_risk = RiskLevel::Critical;
    });
    write_kata(
        &home.paths.kata_dir(),
        "team/ttl.sh",
        "#!/bin/sh\n# ---\n# about: TTL\n# risk:  critical\n# ---\necho hi\n",
    );
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (first, _) = call(&client, "run_kata", serde_json::json!({"id": "team/ttl"})).await;
    let pending_id = first["pending_id"].as_str().unwrap().to_string();

    // Back-date the record's `expires` past "now" to simulate the 24h TTL elapsing.
    let store = kadou_mcp::pending::PendingStore::new(&home.paths.state_dir);
    let mut record = store.get(&pending_id).unwrap();
    record.expires = humantime::format_rfc3339_seconds(
        std::time::SystemTime::now() - std::time::Duration::from_secs(60),
    )
    .to_string();
    store.save(&record).unwrap();
    assert!(store.get(&pending_id).unwrap().is_expired());

    let (second, _) = call(&client, "run_kata", serde_json::json!({"id": "team/ttl"})).await;
    assert_ne!(
        second["pending_id"], pending_id,
        "an expired record must not be deduped onto"
    );
}

#[tokio::test]
async fn dry_run_resolves_env_without_executing() {
    let home = setup_sesami(|_| {});
    let state = ServerState::new(home.paths.clone(), None, 2, &std::env::temp_dir());
    let client = spawn_server(state).await;

    let (value, is_error) = call(
        &client,
        "run_kata",
        serde_json::json!({"id": "starter/hello", "dry_run": true}),
    )
    .await;
    assert!(!is_error, "{value}");
    assert_eq!(value["status"], "dry_run");
    assert!(
        value["env_names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == "NAME")
    );
    assert_eq!(value["env_public"]["NAME"], "world");
}
