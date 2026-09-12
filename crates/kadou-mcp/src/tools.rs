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

pub fn list_kata(state: &ServerState, config: &kadou_core::Config, args: ListArgs) -> Value {
    let mut rows: Vec<Row> = Vec::new();

    if !matches!(
        args.folder.as_deref(),
        Some(drafts::PROPOSED_NS) | Some(drafts::MINED_NS)
    ) && let Ok(scanned) = kadou_core::scan_kata_dir(&state.paths.kata_dir())
    {
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
    }

    if matches!(args.folder.as_deref(), None | Some("."))
        && let Some(project_dir) = &state.project_local_kata_dir
        && visibility::is_trusted(config, project_dir)
        && let Some(parent) = project_dir.parent()
        && let Ok(files) = kadou_core::scan_folder(parent, "kata")
    {
        let ceiling =
            visibility::agent_ceiling(config, PROJECT_LOCAL_FOLDER_KEY, state.max_risk_flag);
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
    }

    if args.include_drafts
        || matches!(
            args.folder.as_deref(),
            Some(drafts::PROPOSED_NS) | Some(drafts::MINED_NS)
        )
    {
        for file in drafts::scan_drafts(&state.paths.state_dir) {
            let Some(header) = &file.header else { continue };
            if let Some(want) = &args.folder {
                let top = top_folder(&file.id);
                if top != want {
                    continue;
                }
            }
            rows.push(row_from(
                &file.id,
                &header.about,
                header.risk,
                &header.alias,
                true,
            ));
        }
    }

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
    let human = id
        .strip_prefix("proposed/")
        .map(|rest| format!("kadou accept {rest}"))
        .or_else(|| {
            id.strip_prefix("mined/")
                .map(|_| "kadou mine review".to_string())
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
async fn spawn_and_await(
    state: &ServerState,
    config: &kadou_core::Config,
    mcp_client: Option<String>,
    ct: CancellationToken,
    req: ResolvedRequest,
    guard: crate::concurrency::RunGuard,
) -> (Value, bool) {
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
        mcp_client: mcp_client.as_deref(),
        base_env,
        // `env` above is already the complete allowlisted MCP environment (§6.1) — the child
        // must not also inherit this server process's own environment.
        env_clear: true,
        config_exec_timeout: config.exec.timeout,
    };

    let prepared = match kadou_core::runner::begin(&state.paths.state_dir, &runner_req) {
        Ok(p) => p,
        Err(err) => {
            drop(guard);
            return (
                json!({"status":"error","error":"internal","isError":true,"id":req.kata.id,"message":err.to_string()}),
                true,
            );
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
                        record.pending_id
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
            "approve": format!("kadou grant approve {}", record.pending_id),
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
