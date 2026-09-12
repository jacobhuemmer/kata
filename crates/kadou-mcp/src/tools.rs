//! Domain logic for the four tools (`docs/design/05-prd.md` §5.5 result shapes). Each
//! function returns `(serde_json::Value, is_error)` — the exact JSON to serialize into the
//! single MCP text content block, and whether the wire-level `isError` flag should be set.
//! [`crate::server`] is the only caller; it owns the rmcp plumbing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kadou_core::{Arg, ArgDefault, ArgType, Kata, LookupResult, RiskLevel};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::drafts;
use crate::env;
use crate::history::{self, HistoryRecord, HistoryStore};
use crate::notify;
use crate::pending::{self, PendingStore};
use crate::state::ServerState;
use crate::visibility::{self, PROJECT_LOCAL_FOLDER_KEY};

const MAX_OUTPUT_BYTES: usize = 8192;
const SOURCE_CAP_BYTES: usize = 16 * 1024;

fn top_folder(id: &str) -> &str {
    id.split('/').next().unwrap_or(id)
}

fn no_such_kata(id: &str) -> (Value, bool) {
    (
        json!({
            "status": "error",
            "error": "no_such_kata",
            "isError": true,
            "id": id,
            "message": format!("no such kata `{id}`"),
        }),
        true,
    )
}

/// Resolves `id` to a [`Kata`] that is visible to this agent (§6.2): the risk ceiling and,
/// for a project-local id, the trust gate. Returns the kata (with its public-facing id — for
/// a project-local kata, rewritten back to `./name` — §4.5) and the folder key to use for
/// further ceiling lookups. Every failure mode (not found, invalid header, above ceiling,
/// untrusted) collapses to `None`, since §5.5 says not to distinguish hidden from missing.
fn resolve_visible(
    state: &ServerState,
    config: &kadou_core::Config,
    id: &str,
) -> Option<(Kata, String)> {
    if let Some(stripped) = id.strip_prefix("./") {
        let project_dir = state.project_local_kata_dir.as_ref()?;
        if !visibility::is_trusted(config, project_dir) {
            return None;
        }
        let parent = project_dir.parent()?;
        let internal_id = format!("kata/{stripped}");
        let mut kata = match kadou_core::find_kata(parent, &internal_id).ok()? {
            LookupResult::Found(kata) => kata,
            _ => return None,
        };
        kata.id = format!("./{stripped}");
        let ceiling =
            visibility::agent_ceiling(config, PROJECT_LOCAL_FOLDER_KEY, state.max_risk_flag);
        if !visibility::is_visible_risk(kata.risk, ceiling) {
            return None;
        }
        Some((kata, PROJECT_LOCAL_FOLDER_KEY.to_string()))
    } else {
        let folder = top_folder(id).to_string();
        let kata = match kadou_core::find_kata(&state.paths.kata_dir(), id).ok()? {
            LookupResult::Found(kata) => kata,
            _ => return None,
        };
        let ceiling = visibility::agent_ceiling(config, &folder, state.max_risk_flag);
        if !visibility::is_visible_risk(kata.risk, ceiling) {
            return None;
        }
        Some((kata, folder))
    }
}

// ---------------------------------------------------------------------------
// list_kata
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ListArgs {
    pub query: Option<String>,
    pub folder: Option<String>,
    pub risk: Option<RiskLevel>,
    pub limit: usize,
    pub offset: usize,
    pub include_drafts: bool,
}

struct Row {
    id: String,
    about: String,
    risk: RiskLevel,
    draft: bool,
    haystack: String,
}

fn row_from(id: &str, about: &str, risk: RiskLevel, alias: &[String], draft: bool) -> Row {
    let haystack = format!(
        "{} {} {}",
        id.to_lowercase(),
        about.to_lowercase(),
        alias.join(" ").to_lowercase()
    );
    Row {
        id: id.to_string(),
        about: about.to_string(),
        risk,
        draft,
        haystack,
    }
}

/// The library scan (every folder under `kata/`), gated on the agent ceiling per folder —
/// skipped entirely when `args.folder` names a draft namespace, since those never appear here.
fn library_rows(state: &ServerState, config: &kadou_core::Config, args: &ListArgs) -> Vec<Row> {
    let mut rows = Vec::new();
    if matches!(
        args.folder.as_deref(),
        Some(drafts::PROPOSED_NS) | Some(drafts::MINED_NS)
    ) {
        return rows;
    }
    let Ok(scanned) = kadou_core::scan_kata_dir(&state.paths.kata_dir()) else {
        return rows;
    };
    for (folder, files) in scanned {
        if let Some(want) = &args.folder
            && &folder != want
        {
            continue;
        }
        let ceiling = visibility::agent_ceiling(config, &folder, state.max_risk_flag);
        for file in files {
            let Some(header) = &file.header else { continue };
            if !visibility::is_visible_risk(header.risk, ceiling) {
                continue;
            }
            rows.push(row_from(
                &file.id,
                &header.about,
                header.risk,
                &header.alias,
                false,
            ));
        }
    }
    rows
}

/// The trusted project-local `kata/` folder discovered at server startup (§4.5), if any and
/// if trusted — rewritten back to `./name` ids, exactly like `resolve_visible` does for a
/// single lookup.
fn project_local_rows(
    state: &ServerState,
    config: &kadou_core::Config,
    args: &ListArgs,
) -> Vec<Row> {
    let mut rows = Vec::new();
    if !matches!(args.folder.as_deref(), None | Some(".")) {
        return rows;
    }
    let Some(project_dir) = &state.project_local_kata_dir else {
        return rows;
    };
    if !visibility::is_trusted(config, project_dir) {
        return rows;
    }
    let Some(parent) = project_dir.parent() else {
        return rows;
    };
    let Ok(files) = kadou_core::scan_folder(parent, "kata") else {
        return rows;
    };
    let ceiling = visibility::agent_ceiling(config, PROJECT_LOCAL_FOLDER_KEY, state.max_risk_flag);
    for file in files {
        let Some(header) = &file.header else { continue };
        if !visibility::is_visible_risk(header.risk, ceiling) {
            continue;
        }
        let id = format!("./{}", file.id.strip_prefix("kata/").unwrap_or(&file.id));
        rows.push(row_from(
            &id,
            &header.about,
            header.risk,
            &header.alias,
            false,
        ));
    }
    rows
}

/// Draft kata (`proposed/`, `mined/`) — only scanned when asked for, either via
/// `include_drafts` or by naming a draft namespace directly in `folder` (§5.4).
fn draft_rows(state: &ServerState, args: &ListArgs) -> Vec<Row> {
    let mut rows = Vec::new();
    let wants_drafts = args.include_drafts
        || matches!(
            args.folder.as_deref(),
            Some(drafts::PROPOSED_NS) | Some(drafts::MINED_NS)
        );
    if !wants_drafts {
        return rows;
    }
    for file in drafts::scan_drafts(&state.paths.state_dir) {
        let Some(header) = &file.header else { continue };
        if let Some(want) = &args.folder
            && top_folder(&file.id) != want
        {
            continue;
        }
        rows.push(row_from(
            &file.id,
            &header.about,
            header.risk,
            &header.alias,
            true,
        ));
    }
    rows
}

/// Filters, sorts, dedupes, and pages `rows` into the §5.5 `list_kata` result shape.
fn paginate(mut rows: Vec<Row>, args: &ListArgs) -> Value {
    if let Some(risk) = args.risk {
        rows.retain(|r| r.risk == risk);
    }
    if let Some(query) = &args.query {
        let query = query.to_lowercase();
        rows.retain(|r| r.haystack.contains(&query));
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows.dedup_by(|a, b| a.id == b.id);

    let total = rows.len();
    let page: Vec<&Row> = rows.iter().skip(args.offset).take(args.limit).collect();
    let truncated = args.offset + page.len() < total;

    let kata: Vec<Value> = page
        .iter()
        .map(|r| {
            let mut obj = serde_json::Map::new();
            obj.insert("id".to_string(), json!(r.id));
            obj.insert("about".to_string(), json!(r.about));
            obj.insert("risk".to_string(), json!(r.risk.as_str()));
            if r.draft {
                obj.insert("draft".to_string(), json!(true));
            }
            Value::Object(obj)
        })
        .collect();

    json!({
        "kata": kata,
        "total": total,
        "offset": args.offset,
        "limit": args.limit,
        "truncated": truncated,
    })
}

pub fn list_kata(state: &ServerState, config: &kadou_core::Config, args: ListArgs) -> Value {
    let mut rows = library_rows(state, config, &args);
    rows.extend(project_local_rows(state, config, &args));
    rows.extend(draft_rows(state, &args));
    paginate(rows, &args)
}

// ---------------------------------------------------------------------------
// describe_kata
// ---------------------------------------------------------------------------

pub struct DescribeArgs {
    pub id: String,
    pub include_source: bool,
}

pub fn describe_kata(
    state: &ServerState,
    config: &kadou_core::Config,
    vault: &kadou_core::Vault,
    args: DescribeArgs,
) -> (Value, bool) {
    let id = args.id.clone();

    let (kata, files) = if drafts::is_draft_id(&id) {
        match drafts::find_draft(&state.paths.state_dir, &id)
            .and_then(|f| kadou_core::kata_from_scanned(&f))
        {
            Some(kata) => (kata, Vec::new()),
            None => return no_such_kata(&id),
        }
    } else {
        match resolve_visible(state, config, &id) {
            Some((kata, folder)) => {
                let files = if folder == PROJECT_LOCAL_FOLDER_KEY {
                    Vec::new()
                } else {
                    sibling_files(&state.paths.kata_dir(), &folder)
                };
                (kata, files)
            }
            None => return no_such_kata(&id),
        }
    };

    (
        build_describe_value(&kata, vault, args.include_source, files),
        false,
    )
}

/// Sibling helper files in `folder` — files the scanner found but that have no header of
/// their own (e.g. Sesami's shared `scripts/trigger-pipeline.sh`, §5.3), as paths relative to
/// the folder root.
fn sibling_files(kata_dir: &Path, folder: &str) -> Vec<String> {
    let Ok(files) = kadou_core::scan_folder(kata_dir, folder) else {
        return Vec::new();
    };
    let folder_root = kata_dir.join(folder);
    files
        .into_iter()
        .filter(|f| f.header.is_none() && f.diagnostics.is_empty())
        .filter_map(|f| {
            f.path
                .strip_prefix(&folder_root)
                .ok()
                .map(|p| p.display().to_string())
        })
        .collect()
}

fn build_describe_value(
    kata: &Kata,
    vault: &kadou_core::Vault,
    include_source: bool,
    files: Vec<String>,
) -> Value {
    let sha256 = kadou_core::file_sha256(&kata.path).unwrap_or_default();
    let resolved_needs = kadou_core::resolve_needs(kata, vault);
    let needs: Vec<&str> = kata.needs.iter().map(|n| n.name.as_str()).collect();
    let needs_missing: Vec<&str> = resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.as_str())
        .collect();

    let mut obj = serde_json::Map::new();
    obj.insert("id".to_string(), json!(kata.id));
    obj.insert("folder".to_string(), json!(top_folder(&kata.id)));
    obj.insert("about".to_string(), json!(kata.about));
    obj.insert("risk".to_string(), json!(kata.risk.as_str()));
    obj.insert("file".to_string(), json!(kata.path.display().to_string()));
    obj.insert("sha256".to_string(), json!(sha256));
    obj.insert("needs".to_string(), json!(needs));
    obj.insert("needs_missing".to_string(), json!(needs_missing));
    obj.insert("args".to_string(), build_args_schema(&kata.args));
    if include_source {
        let (source, truncated) = read_source_capped(&kata.path);
        obj.insert("source".to_string(), json!(source));
        obj.insert("source_truncated".to_string(), json!(truncated));
    }
    if !files.is_empty() {
        obj.insert("files".to_string(), json!(files));
    }
    Value::Object(obj)
}

fn read_source_capped(path: &Path) -> (String, bool) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    if text.len() <= SOURCE_CAP_BYTES {
        return (text, false);
    }
    let mut end = SOURCE_CAP_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

/// Builds an `args` JSON Schema from the header's arg list — shared by `describe_kata.args`
/// and `run_kata`'s `invalid_args.expected` (§5.5), so the two can never drift.
fn build_args_schema(args: &[Arg]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for arg in args {
        let mut prop = serde_json::Map::new();
        match &arg.ty {
            ArgType::Text => {
                prop.insert("type".to_string(), json!("string"));
            }
            ArgType::Int => {
                prop.insert("type".to_string(), json!("integer"));
            }
            ArgType::Bool => {
                prop.insert("type".to_string(), json!("boolean"));
            }
            ArgType::Select { options } => {
                prop.insert("type".to_string(), json!("string"));
                prop.insert("enum".to_string(), json!(options));
            }
        }
        if let Some(default) = &arg.default {
            prop.insert("default".to_string(), default_to_json(default));
        }
        if let Some(help) = &arg.help {
            prop.insert("description".to_string(), json!(help));
        }
        properties.insert(arg.name.clone(), Value::Object(prop));
        if arg.is_required() {
            required.push(arg.name.clone());
        }
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": Value::Object(properties),
        "required": required,
    })
}

fn default_to_json(default: &ArgDefault) -> Value {
    match default {
        ArgDefault::Text(s) => json!(s),
        ArgDefault::Int(i) => json!(i),
        ArgDefault::Bool(b) => json!(b),
        ArgDefault::Select(s) => json!(s),
    }
}

// ---------------------------------------------------------------------------
// run_kata
// ---------------------------------------------------------------------------

pub struct RunArgs {
    pub id: String,
    pub args: serde_json::Map<String, Value>,
    pub dry_run: bool,
}

fn json_args_to_strings(
    args: &serde_json::Map<String, Value>,
) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for (key, value) in args {
        let text = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Err(key.clone()),
        };
        out.insert(key.clone(), text);
    }
    Ok(out)
}

fn invalid_args_result(kata: &Kata, message: String) -> (Value, bool) {
    (
        json!({
            "status": "error",
            "error": "invalid_args",
            "isError": true,
            "message": message,
            "expected": build_args_schema(&kata.args),
        }),
        true,
    )
}

fn draft_run_result(id: &str) -> (Value, bool) {
    // `proposed/<folder>/<name>` accepts as `<folder>/<name>` (its own folder is already in
    // the id); `mined/<name>` has no folder of its own, so `kadou accept` needs the `mined/`
    // prefix kept to know which draft namespace to resolve (`docs/design/05-prd.md` §5.5's
    // own worked example: `"kadou accept mined/k8s-pod-logs"`, not `"kadou accept
    // k8s-pod-logs"`).
    let human = id
        .strip_prefix("proposed/")
        .map(|rest| format!("kadou accept {rest}"))
        .or_else(|| {
            if id.starts_with("mined/") {
                Some(format!("kadou accept {id}"))
            } else {
                None
            }
        })
        .unwrap_or_else(|| "kadou accept".to_string());
    (
        json!({
            "status": "error",
            "error": "draft",
            "isError": true,
            "id": id,
            "message": format!("draft; a human must run: {human}"),
        }),
        true,
    )
}

/// A looked-up kata with its args/needs already resolved — everything before the risk/grant
/// gate (§6.3). `Err` is a final result to return as-is: `draft`/`no_such_kata`/
/// `invalid_args`, or `dry_run`'s own success shape (dry_run never reaches the gate at all).
struct ResolvedRequest {
    kata: Kata,
    folder: String,
    resolved_args: Vec<kadou_core::ResolvedVar>,
    resolved_needs: Vec<kadou_core::ResolvedNeed>,
}

fn resolve_request(
    state: &ServerState,
    config: &kadou_core::Config,
    args: &RunArgs,
) -> Result<ResolvedRequest, (Value, bool)> {
    let id = &args.id;
    if drafts::is_draft_id(id) {
        return Err(draft_run_result(id));
    }
    let Some((kata, folder)) = resolve_visible(state, config, id) else {
        return Err(no_such_kata(id));
    };

    let provided = json_args_to_strings(&args.args).map_err(|bad_key| {
        invalid_args_result(
            &kata,
            format!("arg `{bad_key}` must be a string, number, or boolean"),
        )
    })?;
    let resolved_args = kadou_core::resolve_args(&kata, &provided)
        .map_err(|err| invalid_args_result(&kata, err.to_string()))?;

    let vault = state.load_vault();
    let resolved_needs = kadou_core::resolve_needs(&kata, &vault);

    if args.dry_run {
        let dry = kadou_core::runner::dry_run(&resolved_args, &resolved_needs);
        return Err((
            json!({
                "status": "dry_run",
                "id": kata.id,
                "env_names": dry.env_names,
                "env_public": dry.env_public,
                "secret_env_names": dry.secret_env_names,
            }),
            false,
        ));
    }

    Ok(ResolvedRequest {
        kata,
        folder,
        resolved_args,
        resolved_needs,
    })
}

/// The risk/grant gate, the missing-needs check, and the per-id concurrency reservation
/// (§6.3, §6.4, §6.1) — everything a resolved request must clear before it may spawn.
fn gate(
    state: &ServerState,
    config: &kadou_core::Config,
    req: &ResolvedRequest,
    mcp_client: Option<String>,
) -> Result<crate::concurrency::RunGuard, (Value, bool)> {
    // §6.3/§6.4: a *visible* high/critical kata still needs a human grant unless the agent's
    // own `[agent].allow` already covers it (bare id: any version; `id@sha256:...`: pinned to
    // that exact file). `dry_run` never reaches this gate at all — it never executes anything.
    if req.kata.risk >= RiskLevel::High {
        let sha256 = kadou_core::file_sha256(&req.kata.path).unwrap_or_default();
        if !allow_permits(&config.agent.allow, &req.kata.id, &sha256) {
            return Err(pending_grant_result(
                state,
                config,
                &req.kata,
                &req.folder,
                &req.resolved_args,
                mcp_client,
                &sha256,
            ));
        }
    }

    let missing: Vec<String> = req
        .resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.clone())
        .collect();
    if !missing.is_empty() {
        let fix = format!("kadou vault set {}", missing[0]);
        return Err((
            json!({
                "status": "error",
                "error": "missing_needs",
                "isError": true,
                "id": req.kata.id,
                "needs_missing": missing,
                "message": fix,
            }),
            true,
        ));
    }

    state.concurrency.try_start(&req.kata.id).map_err(|()| {
        (
            json!({
                "status": "error",
                "error": "busy",
                "isError": true,
                "id": req.kata.id,
                "message": format!("{} is already running, or the server is at its concurrency limit", req.kata.id),
            }),
            true,
        )
    })
}

/// Builds the env/spec/history record (`kadou_core::runner::begin`), spawns the exec, and
/// races it against cancellation and `mcp.max_wait` (§6.1). `ct` is scoped to this request's
/// own lifetime — it is only meaningful to race here, in the synchronous branch; once we
/// detach (the `max_wait` branch below), the spawned kata keeps running on its own and `ct` is
/// no longer watched (§6.1 "cancel via notifications/cancelled, stdio EOF, or exec.timeout").
/// `cancel_tx` must stay alive for as long as the kata might still be running: dropping a
/// oneshot `Sender` resolves the receiver's `.await` exactly like a real cancel signal, so
/// letting it drop here would SIGTERM every detached run the instant this function returns.
/// Builds the `kata_dir_root`/env/`RunOneRequest` and calls `kadou_core::runner::begin` — the
/// part of `spawn_and_await` that can fail before anything is spawned.
fn begin_mcp_run(
    state: &ServerState,
    config: &kadou_core::Config,
    mcp_client: Option<&str>,
    req: &ResolvedRequest,
) -> Result<kadou_core::runner::PreparedRun, (Value, bool)> {
    let kata_dir_root = if req.kata.id.starts_with("./") {
        state
            .project_local_kata_dir
            .clone()
            .unwrap_or_else(|| state.paths.kata_dir())
    } else {
        state.paths.kata_dir()
    };
    let base_env = env::build_mcp_env(std::env::vars(), &config.exec.pass_env);
    let initiator = history::current_initiator();
    let runner_req = kadou_core::runner::RunOneRequest {
        kata: &req.kata,
        folder: &req.folder,
        resolved_args: &req.resolved_args,
        resolved_needs: &req.resolved_needs,
        kata_dir_root: &kata_dir_root,
        interface: "mcp",
        initiator: &initiator,
        mcp_client,
        base_env,
        // `env` above is already the complete allowlisted MCP environment (§6.1) — the child
        // must not also inherit this server process's own environment.
        env_clear: true,
        config_exec_timeout: config.exec.timeout,
    };

    kadou_core::runner::begin(&state.paths.state_dir, &runner_req).map_err(|err| {
        (
            json!({"status":"error","error":"internal","isError":true,"id":req.kata.id,"message":err.to_string()}),
            true,
        )
    })
}

async fn spawn_and_await(
    state: &ServerState,
    config: &kadou_core::Config,
    mcp_client: Option<String>,
    ct: CancellationToken,
    req: ResolvedRequest,
    guard: crate::concurrency::RunGuard,
) -> (Value, bool) {
    let prepared = match begin_mcp_run(state, config, mcp_client.as_deref(), &req) {
        Ok(p) => p,
        Err(result) => {
            drop(guard);
            return result;
        }
    };
    let history_store = prepared.history_store;
    let record = prepared.record;

    let max_output_lines = (config.mcp.max_output_lines as usize).clamp(1, 200);
    let max_wait = config.mcp.max_wait;

    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel();
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let outcome = kadou_exec::run(prepared.spec, Some(cancel_rx)).await;
        let _ = done_tx.send(outcome);
    });

    tokio::select! {
        result = &mut done_rx => {
            drop(cancel_tx);
            let value = finish_and_shape(&history_store, record, result, max_output_lines);
            drop(guard);
            value
        }
        () = ct.cancelled() => {
            let _ = cancel_tx.send(());
            let result = done_rx.await;
            let value = finish_and_shape(&history_store, record, result, max_output_lines);
            drop(guard);
            value
        }
        () = tokio::time::sleep(max_wait) => {
            let running_id = req.kata.id.clone();
            let history_id = record.history_id.clone();
            let log_path = record.log_path.display().to_string();
            tokio::spawn(async move {
                let _keep_alive = cancel_tx;
                let result = done_rx.await;
                let _ = finish_and_shape(&history_store, record, result, max_output_lines);
                drop(guard);
            });
            (
                json!({
                    "status": "running",
                    "id": running_id,
                    "history_id": history_id,
                    "log_path": log_path,
                }),
                false,
            )
        }
    }
}

/// Executes `run_kata`. `ct` is cancelled by the framework when `notifications/cancelled`
/// arrives for this request (§6.1).
pub async fn run_kata(
    state: &ServerState,
    config: &kadou_core::Config,
    mcp_client: Option<String>,
    ct: CancellationToken,
    args: RunArgs,
) -> (Value, bool) {
    let req = match resolve_request(state, config, &args) {
        Ok(req) => req,
        Err(result) => return result,
    };
    let guard = match gate(state, config, &req, mcp_client.clone()) {
        Ok(guard) => guard,
        Err(result) => return result,
    };
    spawn_and_await(state, config, mcp_client, ct, req, guard).await
}

/// `true` when `id` is covered by an `[agent].allow` entry: a bare id (any version) or
/// `id@sha256:...` pinned to exactly this file's current hash (§6.4 item 5, §7.1 `grant
/// allow`). A pinned entry whose hash no longer matches simply doesn't grant — the kata falls
/// through to `pending_grant` like an unlisted one, per the worker brief's "or whose pinned
/// sha256 differs when the allow entry is pinned".
fn allow_permits(allow: &[String], id: &str, sha256: &str) -> bool {
    let pin_prefix = format!("{id}@");
    allow.iter().any(|entry| {
        entry == id
            || entry
                .strip_prefix(&pin_prefix)
                .is_some_and(|pin| pin == sha256)
    })
}

/// The on-disk directory backing `folder` — used for the git-HEAD pin (§6.4). `None` for a
/// project-local kata whose `kata/` directory this server never discovered.
fn folder_dir(state: &ServerState, kata: &Kata, folder: &str) -> Option<PathBuf> {
    if kata.id.starts_with("./") {
        state.project_local_kata_dir.clone()
    } else {
        Some(state.paths.kata_dir().join(folder))
    }
}

/// Writes (or reuses, per the dedupe rule) a pending grant record and returns the §5.5
/// `pending_grant` result. Posts the best-effort desktop notification (§7.8) only when a new
/// record was actually created — a deduped repeat of the same request doesn't re-notify.
fn pending_grant_result(
    state: &ServerState,
    config: &kadou_core::Config,
    kata: &Kata,
    folder: &str,
    resolved_args: &[kadou_core::ResolvedVar],
    mcp_client: Option<String>,
    sha256: &str,
) -> (Value, bool) {
    let public_args: BTreeMap<String, String> = resolved_args
        .iter()
        .map(|a| (a.name.clone(), a.value.clone()))
        .collect();
    let hash = pending::args_hash(&public_args);
    let store = PendingStore::new(&state.paths.state_dir);

    let record = match store.find_outstanding(&kata.id, &hash) {
        Some(existing) => existing,
        None => {
            let source = std::fs::read_to_string(&kata.path).unwrap_or_default();
            let folder_head = folder_dir(state, kata, folder)
                .as_deref()
                .and_then(pending::git_head);
            match store.create(
                &kata.id,
                folder,
                kata.risk,
                public_args,
                "mcp",
                mcp_client.as_deref(),
                sha256,
                &source,
                folder_head.as_deref(),
            ) {
                Ok(record) => {
                    let body = format!(
                        "{} ({}) from {}\nkadou grant approve {}",
                        record.id,
                        record.risk,
                        mcp_client.as_deref().unwrap_or("an agent"),
                        pending::short_pending_id(&record.pending_id)
                    );
                    notify::notify(config, "kadou · grant wanted", &body);
                    record
                }
                Err(err) => {
                    return (
                        json!({"status":"error","error":"internal","isError":true,"id":kata.id,"message":err.to_string()}),
                        true,
                    );
                }
            }
        }
    };

    (
        json!({
            "status": "pending_grant",
            "id": kata.id,
            "risk": kata.risk.as_str(),
            "pending_id": record.pending_id,
            "pending_path": store.path(&record.pending_id).display().to_string(),
            "approve": format!(
                "kadou grant approve {}",
                pending::short_pending_id(&record.pending_id)
            ),
            "expires": record.expires,
            "reason": format!("{}; not in [agent] allow", kata.risk),
        }),
        false,
    )
}

/// Turns a completed exec result into the §5.5 wire shape: writes the terminal history record
/// via `kadou_core::runner::finish` and applies MCP's own output-view truncation on top (§5.5
/// default 50 lines/8192 bytes) — a CLI caller of the same shared `finish` wants the full
/// output instead, so this shaping stays MCP-specific rather than living in `kadou-core`.
fn finish_and_shape(
    history_store: &HistoryStore,
    record: HistoryRecord,
    result: Result<
        Result<kadou_exec::RunOutcome, kadou_exec::ExecError>,
        tokio::sync::oneshot::error::RecvError,
    >,
    max_output_lines: usize,
) -> (Value, bool) {
    let id = record.id.clone();
    let exec_result = match result {
        Ok(exec_result) => exec_result,
        Err(_) => Err(kadou_exec::ExecError::Wait(std::io::Error::other(
            "the run task ended unexpectedly",
        ))),
    };

    match kadou_core::runner::finish(history_store, record, exec_result) {
        Ok(report) => {
            let (view, output_lines, truncated) = truncate_output(&report.output, max_output_lines);
            let (status_str, is_error) = match report.status {
                kadou_exec::RunStatus::Success => ("success", false),
                kadou_exec::RunStatus::Failed | kadou_exec::RunStatus::TimedOut => ("failed", true),
                kadou_exec::RunStatus::Cancelled => ("cancelled", true),
            };

            let mut obj = serde_json::Map::new();
            obj.insert("status".to_string(), json!(status_str));
            obj.insert("id".to_string(), json!(report.id));
            obj.insert("exit_code".to_string(), json!(report.exit_code));
            obj.insert("duration_ms".to_string(), json!(report.duration_ms));
            obj.insert("output_lines".to_string(), json!(output_lines));
            obj.insert("output".to_string(), json!(view));
            obj.insert("truncated".to_string(), json!(truncated));
            obj.insert("summary".to_string(), json!(report.output_summary));
            obj.insert(
                "log_path".to_string(),
                json!(report.log_path.display().to_string()),
            );
            obj.insert("history_id".to_string(), json!(report.history_id));
            if is_error {
                obj.insert("isError".to_string(), json!(true));
            }
            (Value::Object(obj), is_error)
        }
        Err(err) => (
            json!({"status":"error","error":"internal","isError":true,"id":id,"message":err.to_string()}),
            true,
        ),
    }
}

/// Last **N** lines (§5.5 default 50, `mcp.max_output_lines`), then a hard cap of 8192 UTF-8
/// bytes, cutting whole lines from the front until it fits.
fn truncate_output(lines: &[String], max_lines: usize) -> (String, usize, bool) {
    let total = lines.len();
    let take_from = total.saturating_sub(max_lines.max(1));
    let mut kept: Vec<String> = lines[take_from..].to_vec();
    let mut truncated = take_from > 0;
    let mut joined = kept.join("\n");

    while joined.len() > MAX_OUTPUT_BYTES && kept.len() > 1 {
        kept.remove(0);
        joined = kept.join("\n");
        truncated = true;
    }
    if joined.len() > MAX_OUTPUT_BYTES {
        let mut end = MAX_OUTPUT_BYTES;
        while !joined.is_char_boundary(end) {
            end -= 1;
        }
        joined.truncate(end);
        truncated = true;
    }
    (joined, kept.len(), truncated)
}

// ---------------------------------------------------------------------------
// propose_kata
// ---------------------------------------------------------------------------

pub struct ProposeArgs {
    pub id: String,
    pub source: String,
}

pub fn propose_kata(
    state: &ServerState,
    config: &kadou_core::Config,
    args: ProposeArgs,
) -> (Value, bool) {
    match drafts::propose(
        &state.paths.state_dir,
        &state.paths.kata_dir(),
        &args.id,
        &args.source,
    ) {
        Ok(outcome) => {
            // §7.8: "Same for propose_kata ('draft wanted: kadou accept ops/argocd-sync')".
            let body = format!("draft wanted: {}", outcome.accept_command);
            notify::notify(config, "kadou · draft wanted", &body);
            (
                json!({
                    "status": "proposed",
                    "id": outcome.draft_id,
                    "path": outcome.path.display().to_string(),
                    "diff": outcome.diff,
                    "accept": outcome.accept_command,
                }),
                false,
            )
        }
        Err(err) => (
            json!({
                "status": "error",
                "error": "invalid_args",
                "isError": true,
                "message": err.to_string(),
            }),
            true,
        ),
    }
}

#[cfg(test)]
mod tools_tests {
    use super::*;

    fn lines(n: usize, prefix: &str) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    #[test]
    fn sibling_files_lists_headerless_files_relative_to_the_folder_root() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        let folder_dir = kata_dir.join("sesami");
        std::fs::create_dir_all(&folder_dir).unwrap();
        std::fs::write(
            folder_dir.join("deploy.sh"),
            "#!/bin/sh\n# ---\n# about: Deploy\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();
        std::fs::create_dir_all(folder_dir.join("scripts")).unwrap();
        std::fs::write(
            folder_dir.join("scripts/trigger-pipeline.sh"),
            "#!/bin/sh\necho helper\n",
        )
        .unwrap();

        let files = sibling_files(&kata_dir, "sesami");
        assert_eq!(files, vec!["scripts/trigger-pipeline.sh".to_string()]);
    }

    #[test]
    fn sibling_files_is_empty_for_an_unknown_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(sibling_files(&dir.path().join("kata"), "nope").is_empty());
    }

    #[test]
    fn read_source_capped_returns_untruncated_content_under_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kata.sh");
        std::fs::write(&path, "echo hi\n").unwrap();
        let (source, truncated) = read_source_capped(&path);
        assert_eq!(source, "echo hi\n");
        assert!(!truncated);
    }

    #[test]
    fn read_source_capped_truncates_at_a_char_boundary_over_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kata.sh");
        // Multi-byte characters straddling SOURCE_CAP_BYTES exercise the char-boundary walk-back.
        std::fs::write(&path, "é".repeat(SOURCE_CAP_BYTES)).unwrap();
        let (source, truncated) = read_source_capped(&path);
        assert!(truncated);
        assert!(source.len() <= SOURCE_CAP_BYTES);
        assert!(source.is_char_boundary(source.len()));
    }

    fn text_arg(name: &str, default: Option<ArgDefault>) -> Arg {
        Arg {
            name: name.to_string(),
            ty: ArgType::Text,
            default,
            help: None,
        }
    }

    #[test]
    fn build_args_schema_encodes_int_bool_and_select_defaults() {
        let args = vec![
            Arg {
                name: "retries".to_string(),
                ty: ArgType::Int,
                default: Some(ArgDefault::Int(3)),
                help: None,
            },
            Arg {
                name: "force".to_string(),
                ty: ArgType::Bool,
                default: Some(ArgDefault::Bool(true)),
                help: None,
            },
            Arg {
                name: "env".to_string(),
                ty: ArgType::Select {
                    options: vec!["dev".to_string(), "prod".to_string()],
                },
                default: Some(ArgDefault::Select("dev".to_string())),
                help: None,
            },
            text_arg("required_text", None),
        ];
        let schema = build_args_schema(&args);
        assert_eq!(schema["properties"]["retries"]["type"], "integer");
        assert_eq!(schema["properties"]["retries"]["default"], 3);
        assert_eq!(schema["properties"]["force"]["type"], "boolean");
        assert_eq!(schema["properties"]["force"]["default"], true);
        assert_eq!(schema["properties"]["env"]["enum"][0], "dev");
        assert_eq!(schema["properties"]["env"]["default"], "dev");
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "required_text"));
        assert!(!required.iter().any(|r| r == "retries"));
    }

    fn row(id: &str, risk: RiskLevel) -> Row {
        row_from(id, "about", risk, &[], false)
    }

    fn list_args(limit: usize, offset: usize) -> ListArgs {
        ListArgs {
            query: None,
            folder: None,
            risk: None,
            limit,
            offset,
            include_drafts: false,
        }
    }

    #[test]
    fn paginate_reports_truncated_when_more_rows_remain() {
        let rows = vec![
            row("a/one", RiskLevel::Low),
            row("a/two", RiskLevel::Low),
            row("a/three", RiskLevel::Low),
        ];
        let value = paginate(rows, &list_args(2, 0));
        assert_eq!(value["total"], 3);
        assert_eq!(value["truncated"], true);
        assert_eq!(value["kata"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn paginate_offset_returns_the_remaining_page_untruncated() {
        // paginate() sorts by id first, so the page order is alphabetical: one, three, two.
        let rows = vec![
            row("a/one", RiskLevel::Low),
            row("a/two", RiskLevel::Low),
            row("a/three", RiskLevel::Low),
        ];
        let value = paginate(rows, &list_args(2, 2));
        assert_eq!(value["truncated"], false);
        let ids: Vec<&str> = value["kata"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["a/two"]);
    }

    #[test]
    fn paginate_filters_by_risk() {
        let rows = vec![
            row("a/low", RiskLevel::Low),
            row("a/medium", RiskLevel::Medium),
        ];
        let mut args = list_args(50, 0);
        args.risk = Some(RiskLevel::Medium);
        let value = paginate(rows, &args);
        let ids: Vec<&str> = value["kata"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["a/medium"]);
    }

    #[test]
    fn truncate_output_keeps_only_the_last_max_lines() {
        let input = lines(60, "line");
        let (joined, output_lines, truncated) = truncate_output(&input, 50);
        assert!(truncated);
        assert_eq!(output_lines, 50);
        assert!(joined.starts_with("line10\n"));
        assert!(joined.ends_with("line59"));
    }

    #[test]
    fn truncate_output_under_the_line_limit_is_not_truncated() {
        let input = lines(3, "line");
        let (joined, output_lines, truncated) = truncate_output(&input, 50);
        assert!(!truncated);
        assert_eq!(output_lines, 3);
        assert_eq!(joined, "line0\nline1\nline2");
    }

    #[test]
    fn truncate_output_enforces_the_8192_byte_cap_by_dropping_whole_lines() {
        // Each line is 1000 bytes; 9 of them (9000 bytes) exceeds MAX_OUTPUT_BYTES (8192), so
        // whole lines must be dropped from the front until it fits.
        let input: Vec<String> = (0..9)
            .map(|i| format!("{i}").repeat(1) + &"x".repeat(999))
            .collect();
        let (joined, output_lines, truncated) = truncate_output(&input, 50);
        assert!(truncated);
        assert!(joined.len() <= MAX_OUTPUT_BYTES);
        assert!(output_lines < 9);
    }

    #[test]
    fn truncate_output_cuts_at_a_char_boundary_when_a_single_line_exceeds_the_cap() {
        // One line alone (a multi-byte character repeated) already exceeds MAX_OUTPUT_BYTES,
        // so the byte-cap loop must fall through to the char-boundary truncation branch.
        let huge_line = "é".repeat(MAX_OUTPUT_BYTES); // 2 bytes/char in UTF-8
        let input = vec![huge_line];
        let (joined, output_lines, truncated) = truncate_output(&input, 50);
        assert!(truncated);
        assert_eq!(output_lines, 1);
        assert!(joined.len() <= MAX_OUTPUT_BYTES);
        assert!(joined.is_char_boundary(joined.len()));
    }
}
