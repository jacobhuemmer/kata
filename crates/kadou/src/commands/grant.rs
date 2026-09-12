//! `kadou grant *` (§6.4, §7.1, §9 slice 6).

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use kadou_core::{Config, KadouPaths, Kata, LookupResult, Vault};
use kadou_mcp::pending::{self, PendingRecord, PendingStore};

use super::{
    auto_import_go_vault, cli_confirm, find_kata_or_report, load_config, materialize_starter,
    print_run_report_and_exit_code, resolve_paths, run_frame_header, run_one_cli,
    styled_for_stdout, vault_store,
};

fn render_args(args: &BTreeMap<String, String>) -> String {
    args.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolves `pending_id` (a full id or an unambiguous R11 prefix) to its record, printing a
/// clean error and returning `Err(exit code)` on `NotFound`/`Ambiguous` so every `grant *`
/// subcommand handles both the same way.
fn resolve_pending_or_report(
    store: &PendingStore,
    pending_id: &str,
) -> Result<PendingRecord, ExitCode> {
    match store.resolve(pending_id) {
        pending::PendingLookup::Found(record) => Ok(*record),
        pending::PendingLookup::NotFound => {
            eprintln!("error: no pending grant `{pending_id}`");
            Err(ExitCode::from(2))
        }
        pending::PendingLookup::Ambiguous(matches) => {
            eprintln!("error: `{pending_id}` matches more than one pending grant:");
            for candidate in &matches {
                eprintln!("  {candidate}");
            }
            eprintln!("  = use more characters to disambiguate");
            Err(ExitCode::from(2))
        }
    }
}

/// The status column `kadou grant list`/`show` prints: the run's own terminal status once
/// approved, else `expired` (§6.4 item 2 "shows expired records as expired") or `pending`.
fn grant_status_label(record: &PendingRecord) -> String {
    match &record.status {
        Some(status) => status.clone(),
        None if record.is_expired() => "expired".to_string(),
        None => "pending".to_string(),
    }
}

/// `kadou grant list` (§7.1, §6.4 item 4).
pub fn run_grant_list() -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let store = PendingStore::new(&paths.state_dir);
    let mut records = store.list();
    records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    if records.is_empty() {
        println!("no pending grants");
        return ExitCode::SUCCESS;
    }

    for record in &records {
        println!(
            "{:<36} {:<8} {:<28} {:<9} {}",
            record.pending_id,
            record.risk,
            record.id,
            grant_status_label(record),
            render_args(&record.args),
        );
    }
    ExitCode::SUCCESS
}

/// `kadou grant show <pending_id>` (§7.1, §6.4 item 4: "the record and a kata diff since
/// request").
pub fn run_grant_show(pending_id: String) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let store = PendingStore::new(&paths.state_dir);

    let record = match resolve_pending_or_report(&store, &pending_id) {
        Ok(r) => r,
        Err(code) => return code,
    };

    println!(
        "{}   {}   {}",
        record.id,
        record.risk,
        grant_status_label(&record)
    );
    println!();
    println!("pending_id  {}", record.pending_id);
    println!(
        "requester   {} ({})",
        record.requester,
        record.mcp_client.as_deref().unwrap_or("-")
    );
    println!("requested   {}", record.timestamp);
    println!("expires     {}", record.expires);
    println!("sha256      {}", record.sha256);
    if let Some(head) = &record.folder_head {
        println!("folder_head {head}");
    }
    if !record.args.is_empty() {
        println!();
        println!("args");
        for (name, value) in &record.args {
            println!("  {name} = {value}");
        }
    }
    if let Some(history_id) = &record.history_id {
        println!();
        println!("history_id  {history_id}");
        println!("status      {}", record.status.as_deref().unwrap_or("?"));
        if let Some(log_path) = &record.log_path {
            println!("log_path    {}", log_path.display());
        }
    }

    println!();
    match kadou_core::find_kata(&paths.kata_dir(), &record.id) {
        Ok(LookupResult::Found(kata)) => {
            let current = std::fs::read_to_string(&kata.path).unwrap_or_default();
            if current == record.source {
                println!("no changes to the kata since the request");
            } else {
                let diff = similar::TextDiff::from_lines(&record.source, &current)
                    .unified_diff()
                    .header("requested", "current")
                    .to_string();
                print!("{diff}");
            }
        }
        _ => println!("(the kata no longer exists on disk)"),
    }

    ExitCode::SUCCESS
}

/// `kadou grant deny <pending_id>` (§6.4 item 6: "deletes the record").
pub fn run_grant_deny(pending_id: String) -> ExitCode {
    let paths = resolve_paths();
    let store = PendingStore::new(&paths.state_dir);
    let record = match resolve_pending_or_report(&store, &pending_id) {
        Ok(r) => r,
        Err(code) => return code,
    };
    match store.delete(&record.pending_id) {
        Ok(()) => {
            println!(
                "denied and removed {}",
                pending::short_pending_id(&record.pending_id)
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: failed to remove {}: {err}", record.pending_id);
            ExitCode::FAILURE
        }
    }
}

/// `kadou grant allow <id> [--any-version]` (§6.4 item 5, §7.1): a config edit appending to
/// `[agent].allow`, pinned to the current sha256 unless `--any-version`. Idempotent: a repeat
/// call replaces this id's prior entry (bare or pinned) rather than accumulating stale ones.
pub fn run_grant_allow(id: String, any_version: bool) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);

    let kata = match find_kata_or_report(&paths.kata_dir(), &id) {
        Ok(k) => k,
        Err(code) => return code,
    };

    let entry = if any_version {
        id.clone()
    } else {
        match kadou_core::file_sha256(&kata.path) {
            Ok(sha256) => format!("{id}@{sha256}"),
            Err(err) => {
                eprintln!("error: failed to hash {}: {err}", kata.path.display());
                return ExitCode::FAILURE;
            }
        }
    };

    let config = load_config(&paths);
    let pin_prefix = format!("{id}@");
    let mut allow: Vec<String> = config
        .agent
        .allow
        .into_iter()
        .filter(|existing| existing != &id && !existing.starts_with(&pin_prefix))
        .collect();
    allow.push(entry.clone());

    let result = Config::edit(&paths.config_file(), |doc| {
        let mut array = toml_edit::Array::new();
        for item in &allow {
            array.push(item.as_str());
        }
        doc["agent"]["allow"] = toml_edit::Item::Value(toml_edit::Value::Array(array));
    });

    match result {
        Ok(()) => {
            println!("allowed {entry}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!(
                "error: failed to save {}: {err}",
                paths.config_file().display()
            );
            ExitCode::FAILURE
        }
    }
}

/// `kadou grant approve <pending_id> [--confirm <id>]` (§6.4 item 5): one-shot executes the
/// pinned kata in the CLI's own full environment (not MCP's allowlisted one), subject to the
/// same confirm protocol as `kadou run` for high/critical, refusing on a sha256/folder-HEAD
/// mismatch or expiry.
/// A missing need is a hard stop with the `kadou vault set` fix line, never a guessed/prompted
/// value (§4.4) — shared between `run_grant_approve` and (in spirit) `run_run`'s own
/// non-interactive branch.
fn missing_needs_error(
    kata: &Kata,
    resolved_needs: &[kadou_core::ResolvedNeed],
) -> Result<(), ExitCode> {
    let missing_needs: Vec<String> = resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.clone())
        .collect();
    if missing_needs.is_empty() {
        return Ok(());
    }
    eprintln!(
        "error: {} needs {} but the vault isn't set up yet",
        kata.id,
        missing_needs.join(", ")
    );
    for name in &missing_needs {
        eprintln!("  = kadou vault set {name}");
    }
    Err(ExitCode::from(2))
}

/// Resolves the pinned args against the kata's current header and the needs against the
/// vault — the setup `run_grant_approve` needs before it can spawn.
#[allow(clippy::type_complexity)]
fn resolve_grant_args_and_needs(
    paths: &KadouPaths,
    kata: &Kata,
    record: &PendingRecord,
) -> Result<(Vec<kadou_core::ResolvedVar>, Vec<kadou_core::ResolvedNeed>), ExitCode> {
    let resolved_args = kadou_core::resolve_args(kata, &record.args).map_err(|err| {
        eprintln!("error: the pinned args no longer resolve against the current header: {err}");
        ExitCode::from(2)
    })?;

    let vault_store_handle = vault_store(paths);
    auto_import_go_vault(&vault_store_handle);
    let vault = vault_store_handle.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    });
    let resolved_needs = kadou_core::resolve_needs(kata, &vault);
    missing_needs_error(kata, &resolved_needs)?;
    Ok((resolved_args, resolved_needs))
}

/// Every check a pending record must clear before it may be approved (§6.4 item 5): not
/// already approved, not expired, the kata unchanged since the request (sha256, and the
/// folder's git HEAD when the request pinned one).
fn validate_pending_record(
    record: &PendingRecord,
    pending_id: &str,
    kata: &Kata,
    kata_dir: &Path,
) -> Result<(), ExitCode> {
    if record.history_id.is_some() {
        eprintln!(
            "error: {pending_id} was already approved (history_id {})",
            record.history_id.as_deref().unwrap_or("?")
        );
        return Err(ExitCode::from(2));
    }
    if record.is_expired() {
        eprintln!("error: {pending_id} expired at {}", record.expires);
        eprintln!("  = ask the agent to request {} again", record.id);
        return Err(ExitCode::from(2));
    }

    let current_sha256 = kadou_core::file_sha256(&kata.path).map_err(|err| {
        eprintln!("error: failed to hash {}: {err}", kata.path.display());
        ExitCode::FAILURE
    })?;
    if current_sha256 != record.sha256 {
        eprintln!(
            "error: {} changed since the request (sha256 no longer matches)",
            record.id
        );
        eprintln!("  = kadou grant show {pending_id} to see the diff, then ask for a fresh grant");
        return Err(ExitCode::from(2));
    }
    if let Some(pinned_head) = &record.folder_head {
        let folder_dir = kata_dir.join(&record.folder);
        if pending::git_head(&folder_dir).as_deref() != Some(pinned_head.as_str()) {
            eprintln!(
                "error: {}'s folder git HEAD changed since the request",
                record.id
            );
            eprintln!(
                "  = kadou grant show {pending_id} to see the diff, then ask for a fresh grant"
            );
            return Err(ExitCode::from(2));
        }
    }
    Ok(())
}

/// §6.4 item 5: "On success the pending record gains history_id, status, and log_path, all
/// readable via pending_path" — written back regardless of the run's own outcome, so a failed
/// approved run is still visible via `grant show`.
fn record_grant_outcome(
    store: &PendingStore,
    record: &mut PendingRecord,
    report: &kadou_core::runner::RunReport,
) {
    let status_str = match report.status {
        kadou_exec::RunStatus::Success => "success",
        kadou_exec::RunStatus::Failed | kadou_exec::RunStatus::TimedOut => "failed",
        kadou_exec::RunStatus::Cancelled => "cancelled",
    };
    record.history_id = Some(report.history_id.clone());
    record.status = Some(status_str.to_string());
    record.log_path = Some(report.log_path.clone());
    if let Err(err) = store.save(record) {
        eprintln!("warning: failed to update the pending record: {err}");
    }
}

pub fn run_grant_approve(pending_id: String, confirm_flag: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let store = PendingStore::new(&paths.state_dir);

    let mut record = match resolve_pending_or_report(&store, &pending_id) {
        Ok(r) => r,
        Err(code) => return code,
    };

    let kata = match find_kata_or_report(&paths.kata_dir(), &record.id) {
        Ok(k) => k,
        Err(code) => return code,
    };
    if let Err(code) =
        validate_pending_record(&record, &record.pending_id, &kata, &paths.kata_dir())
    {
        return code;
    }

    let replay_line = format!(
        "kadou grant approve {} --confirm {}",
        pending::short_pending_id(&record.pending_id),
        record.id
    );
    if let Err(code) = cli_confirm(&kata, confirm_flag.as_deref(), &replay_line) {
        return code;
    }

    let (resolved_args, resolved_needs) = match resolve_grant_args_and_needs(&paths, &kata, &record)
    {
        Ok(v) => v,
        Err(code) => return code,
    };

    // Approval runs in the CLI's own full environment (§6.4 item 5 "Approval runs in the
    // human CLI's environment (full parent env, not the MCP server's allowlisted one)").
    let config = load_config(&paths);
    let styled = styled_for_stdout(false);
    print!(
        "{}",
        run_frame_header(&kata, &resolved_args, &resolved_needs, styled)
    );
    match run_one_cli(
        &paths,
        &config,
        &kata,
        &record.folder,
        &resolved_args,
        &resolved_needs,
        None,
    ) {
        Ok(report) => {
            record_grant_outcome(&store, &mut record, &report);
            print_run_report_and_exit_code(
                &report,
                &kata,
                &resolved_args,
                &config,
                &paths.state_dir,
                styled,
            )
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
