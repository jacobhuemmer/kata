//! Real implementations of the slice-2 (`list`, `check`, `import`) and slice-3/4 (`run`,
//! `show`, `vault`) commands — the rest of the command tree in `main.rs` stays a stub until
//! its own slice lands (`docs/design/05-prd.md` §9).

use std::collections::BTreeMap;
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kadou_core::history::current_initiator;
use kadou_core::runner::{RunOneRequest, run_one_blocking};
use kadou_core::{
    Config, GoVaultSource, KadouPaths, Kata, LastArgsStore, LookupResult, RiskLevel, Vault,
    VaultStore,
};
use kadou_mcp::pending::{self, PendingRecord, PendingStore};

use crate::confirm::{self, ConfirmOutcome};
use crate::starter;

/// Resolves [`KadouPaths`] from the real process environment, printing a fatal error and
/// exiting on failure. `KADOU_HOME` isolates every path for tests (§7.6).
fn resolve_paths() -> KadouPaths {
    match kadou_core::discover() {
        Ok(resolved) => {
            if let Some(warning) = resolved.dops_home_warning {
                eprintln!("warning: {warning}");
            }
            resolved.paths
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}

fn load_config(paths: &KadouPaths) -> Config {
    Config::load(&paths.config_file()).unwrap_or_else(|err| {
        eprintln!(
            "warning: failed to load {}: {err}; using defaults",
            paths.config_file().display()
        );
        Config::default()
    })
}

fn vault_store(paths: &KadouPaths) -> VaultStore {
    VaultStore::new(&paths.data_dir)
}

/// Auto-imports a legacy Go `~/.dops` vault into kadou's own, once (§6.5, §7.4 "Go import:
/// ... or auto-import on first vault use" — this worker's interpretation call, noted in the
/// handoff: auto-import beat a separate `kadou vault import` subcommand). A no-op when
/// `$HOME` can't be resolved, the marker already exists, or `~/.dops` is incomplete.
fn auto_import_go_vault(store: &VaultStore) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let source = GoVaultSource::under_home(Path::new(&home));
    match store.import_go_vault_once(&source) {
        Ok(Some(summary)) => {
            eprintln!(
                "imported {} secret(s) from ~/.dops (dropped {} catalog-scoped value(s); see kadou vault list)",
                summary.imported.len(),
                summary.dropped_catalog_keys.len()
            );
        }
        Ok(None) => {}
        Err(err) => {
            eprintln!("warning: failed to auto-import ~/.dops vault: {err}");
        }
    }
}

/// Loads the vault store, first giving the Go auto-import a chance to run (§6.5, §9 slice
/// 4). A load failure (e.g. a corrupted envelope, or the wrong identity) is reported but not
/// fatal for commands that can still make progress without it (`check`, `show`); `run`
/// treats every need as `missing` in that case, same as an empty vault.
fn load_vault(paths: &KadouPaths) -> Vault {
    let store = vault_store(paths);
    auto_import_go_vault(&store);
    store.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    })
}

/// Materializes any missing starter kata (decision D6, §7.4, §9 slice 6): called by every
/// command that scans kata (`list`, `check`, `run`, `show`, `mcp serve`, `grant`, and the bare
/// `kadou` frame in `main.rs`) so a home is never missing the starter kata just because some
/// other folder got there first. A failure here is a warning, not fatal — the command that
/// called it can still make progress against whatever is already on disk.
fn materialize_starter(paths: &KadouPaths) {
    if let Err(err) = starter::materialize_if_needed(&paths.kata_dir()) {
        eprintln!(
            "warning: failed to materialize the starter kata into {}: {err}",
            paths.kata_dir().display()
        );
    }
}

/// The bare `kadou` frame (a stub until slice 8) still scans kata for its preview, so it still
/// needs decision D6's materialize-on-scan behavior; `main.rs` calls this before printing the
/// stub message.
pub fn materialize_starter_for_bare_invocation() {
    materialize_starter(&resolve_paths());
}

/// The human ceiling still applies regardless of the confirm protocol (§6.2, §6.3 "The human
/// ceiling still applies"): a kata above `max_risk`/`[folder.<f>] max_risk` never runs, no
/// matter what `--confirm` or a TTY prompt says. `--dry-run` never spawns, so it bypasses this
/// too (§6.1).
fn check_human_ceiling(kata: &Kata, config: &Config) -> Result<(), ExitCode> {
    let folder = kata.id.split('/').next().unwrap_or(&kata.id);
    let ceiling = kadou_core::visibility::human_ceiling(config, folder);
    if kata.risk > ceiling {
        eprintln!(
            "error: {} is {} risk, above the max_risk ceiling {ceiling} for folder {folder}",
            kata.id, kata.risk
        );
        eprintln!("  = raise it with `max_risk` (or `[folder.{folder}] max_risk`) in kadou.toml");
        return Err(ExitCode::from(2));
    }
    Ok(())
}

/// The real CLI half of the unified confirm protocol (§6.3, decision 4): wraps
/// [`confirm::confirm_protocol`] with the real terminal and real `inquire` prompts, and turns
/// its outcome into the exact user-facing messages and exit codes the PRD specifies.
/// `replay_line` is the full non-interactive command line to print on `NeedsConfirm` — the
/// caller's own invocation with `--confirm <id>` appended (§6.3's worked example).
fn cli_confirm(kata: &Kata, confirm_flag: Option<&str>, replay_line: &str) -> Result<(), ExitCode> {
    let is_tty = std::io::stdin().is_terminal();
    let outcome = confirm::confirm_protocol(
        kata.risk,
        &kata.id,
        confirm_flag,
        is_tty,
        || {
            inquire::Confirm::new(&format!("run {}?", kata.id))
                .with_default(false)
                .prompt()
                .unwrap_or(false)
        },
        || {
            inquire::Text::new(&format!("type {} to confirm", kata.id))
                .prompt()
                .ok()
        },
    );

    match outcome {
        ConfirmOutcome::Proceed => Ok(()),
        ConfirmOutcome::Declined => {
            eprintln!("cancelled");
            Err(ExitCode::from(1))
        }
        ConfirmOutcome::ConfirmMismatch => {
            eprintln!(
                "error: --confirm {} does not match {}",
                confirm_flag.unwrap_or(""),
                kata.id
            );
            Err(ExitCode::from(2))
        }
        ConfirmOutcome::NeedsConfirm => {
            eprintln!("error: {} is {} and needs confirmation", kata.id, kata.risk);
            eprintln!("  = {replay_line}");
            Err(ExitCode::from(2))
        }
    }
}

/// Reads a vault value from a TTY prompt (masked unless `plain`) or, off a TTY, from stdin
/// to EOF — never argv (§6.5 "reads the value from a TTY prompt or stdin, never argv").
fn read_vault_value(name: &str, plain: bool) -> Result<String, String> {
    if std::io::stdin().is_terminal() {
        let message = format!("value for {name}");
        let result = if plain {
            inquire::Text::new(&message).prompt()
        } else {
            inquire::Password::new(&message)
                .without_confirmation()
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .prompt()
        };
        return result.map_err(|err| err.to_string());
    }

    use std::io::Read as _;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|err| err.to_string())?;
    Ok(buf.trim_end_matches(['\n', '\r']).to_string())
}

/// `kadou list [--folder F] [--risk R] [query]` — the CLI projection of `list_kata` (§7.1).
/// A human's own ceiling applies (§6.2 `visible(k,f) = rank(k.risk) ≤ human_ceiling(f)
/// [CLI]`); untrusted project-local folders are out of scope for this slice.
type ScannedFolders = Vec<(String, Vec<kadou_core::ScannedFile>)>;

/// One folder's visible-and-matching rows: gated on the human ceiling (§6.2 [CLI]), then the
/// `--risk` and free-text query filters.
fn list_rows_for_folder(
    files: &[kadou_core::ScannedFile],
    ceiling: RiskLevel,
    risk_filter: Option<RiskLevel>,
    query_lower: Option<&str>,
) -> Vec<(String, RiskLevel, String)> {
    let mut rows = Vec::new();
    for file in files {
        let Some(header) = &file.header else {
            continue;
        };
        if header.risk > ceiling {
            continue;
        }
        if let Some(want) = risk_filter
            && header.risk != want
        {
            continue;
        }
        if let Some(q) = query_lower {
            let haystack = format!(
                "{} {} {}",
                file.id.to_lowercase(),
                header.about.to_lowercase(),
                header.alias.join(" ").to_lowercase()
            );
            if !haystack.contains(q) {
                continue;
            }
        }
        rows.push((file.id.clone(), header.risk, header.about.clone()));
    }
    rows
}

fn list_rows(
    scanned: &ScannedFolders,
    folder: Option<&str>,
    risk_filter: Option<RiskLevel>,
    query_lower: Option<&str>,
    config: &Config,
) -> Vec<(String, RiskLevel, String)> {
    let mut rows = Vec::new();
    for (name, files) in scanned {
        if let Some(want) = folder
            && name != want
        {
            continue;
        }
        let ceiling = kadou_core::visibility::human_ceiling(config, name);
        rows.extend(list_rows_for_folder(
            files,
            ceiling,
            risk_filter,
            query_lower,
        ));
    }
    rows.sort();
    rows
}

pub fn run_list(query: Option<String>, folder: Option<String>, risk: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let risk_filter = match risk.as_deref() {
        None => None,
        Some(s) => match s.parse::<RiskLevel>() {
            Ok(r) => Some(r),
            Err(_) => {
                eprintln!("error: unknown risk level `{s}`");
                eprintln!("  = risk is one of low, medium, high, critical");
                return ExitCode::from(2);
            }
        },
    };

    let scanned = match kadou_core::scan_kata_dir(&paths.kata_dir()) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    let query_lower = query.map(|q| q.to_lowercase());
    let rows = list_rows(
        &scanned,
        folder.as_deref(),
        risk_filter,
        query_lower.as_deref(),
        &config,
    );

    if rows.is_empty() {
        println!("no kata found");
    } else {
        for (id, risk, about) in &rows {
            println!("{id:<28} \u{25cf} {risk:<8} {about}");
        }
    }

    ExitCode::SUCCESS
}

/// `kadou check [folder|path] [-v]` — the loader (§4.7, decision 12). No argument checks
/// every folder under `kata/`; a folder name checks just that folder; a filesystem path to
/// a single kata file checks just that file.
pub fn run_check(folder_or_path: Option<String>, verbose: bool) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let kata_dir = paths.kata_dir();
    let display_root = paths.config_dir.clone();
    let vault = load_vault(&paths);

    let Some(arg) = folder_or_path else {
        return check_all(&kata_dir, &display_root, verbose, &vault);
    };

    if kata_dir.join(&arg).is_dir() {
        return check_one_folder(&kata_dir, &arg, &display_root, verbose, &vault);
    }

    let path = PathBuf::from(&arg);
    if path.is_file() {
        let file = kadou_core::check_path(&path);
        let report = kadou_core::FolderReport {
            folder: arg,
            files: vec![file],
        };
        let ok = report.is_ok();
        print!(
            "{}",
            kadou_core::render_report(&report, &display_root, verbose)
        );
        return if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    eprintln!("error: no such folder or file: {arg}");
    eprintln!("  = kadou check <folder>, or kadou check <path-to-a-kata-file>");
    ExitCode::from(2)
}

fn check_all(kata_dir: &Path, display_root: &Path, verbose: bool, vault: &Vault) -> ExitCode {
    match kadou_core::check_all(kata_dir, vault) {
        Ok(mut report) => {
            for folder in &mut report.folders {
                add_interpreter_warnings(folder);
                print!(
                    "{}",
                    kadou_core::render_report(folder, display_root, verbose)
                );
            }
            if report.is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn check_one_folder(
    kata_dir: &Path,
    folder: &str,
    display_root: &Path,
    verbose: bool,
    vault: &Vault,
) -> ExitCode {
    match kadou_core::check_folder(kata_dir, folder, vault) {
        Ok(mut report) => {
            add_interpreter_warnings(&mut report);
            print!(
                "{}",
                kadou_core::render_report(&report, display_root, verbose)
            );
            if report.is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Warns (not errors — `kadou check` still passes) when a kata's declared interpreter is
/// missing from disk/`PATH` (`docs/design/05-prd.md` §6.1, §8.2 risk row "Shebang-as-runtime
/// widens what a kata can declare as its interpreter"). `error_count()`/`is_ok()` only count
/// `Severity::Error`, so this never flips a folder from checked to failing.
fn add_interpreter_warnings(report: &mut kadou_core::FolderReport) {
    for file in &mut report.files {
        let Some(header) = &file.header else {
            continue;
        };
        if let Some(missing) = kadou_exec::interpreter_missing(header.shebang.as_deref()) {
            file.diagnostics.push(kadou_core::Diagnostic {
                severity: kadou_core::Severity::Warning,
                message: format!("declared interpreter `{missing}` is not on PATH"),
                fix: Some(format!("install `{missing}`, or fix the shebang")),
                line: 1,
                col: 1,
                len: 1,
            });
        }
    }
}

/// `kadou import <dir> --as <folder>` (§4.6). Converts an old dops catalog once; refuses to
/// overwrite an existing folder.
pub fn run_import(dir: String, as_folder: String) -> ExitCode {
    let paths = resolve_paths();
    match kadou_core::import_catalog(Path::new(&dir), &paths.kata_dir(), &as_folder) {
        Ok(summary) => {
            for diff in &summary.diffs {
                print!("{diff}");
            }
            println!(
                "imported {} kata into {} ({} booleans coerced, {} integers coerced)",
                summary.kata_written, as_folder, summary.booleans_coerced, summary.integers_coerced
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Parses `k=v` positional args into a map, per the shape `kadou run <id> [k=v…]` already
/// declares in `main.rs`'s `RunArgs` (§7.1).
fn parse_kv(pairs: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for pair in pairs {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(pair.clone());
        };
        out.insert(key.to_string(), value.to_string());
    }
    Ok(out)
}

/// Looks up `id` under `kata_dir`, printing a fatal CLI error and returning `Err(exit code)`
/// on any failure (no such kata, or a kata whose header failed to load) so callers can just
/// `return` the code on `Err`.
fn find_kata_or_report(kata_dir: &Path, id: &str) -> Result<Kata, ExitCode> {
    match kadou_core::find_kata(kata_dir, id) {
        Ok(LookupResult::Found(kata)) => Ok(kata),
        Ok(LookupResult::NotFound) => {
            eprintln!("error: no such kata `{id}`");
            eprintln!("  = kadou list, or kadou new {id}");
            Err(ExitCode::from(2))
        }
        Ok(LookupResult::Invalid { diagnostics }) => {
            eprintln!("error: `{id}` has a header error and cannot run");
            for diag in &diagnostics {
                eprintln!("  {}", diag.message);
            }
            eprintln!("  = kadou check {}", id.split('/').next().unwrap_or(id));
            Err(ExitCode::from(2))
        }
        Err(err) => {
            eprintln!("error: {err}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// `kadou run <id> [k=v…] [--dry-run]` (§7.1, §6.1, §9 slice 3). No picker: a missing id is a
/// stub for slice 8, same as the bare `kadou` frame.
/// Prints `--dry-run`'s env-name report (§6.1 "`dry_run` does not spawn. It returns env
/// names, not a command line.").
fn print_dry_run(kata_id: &str, result: &kadou_core::runner::DryRunResult) {
    println!("{kata_id}");
    println!("env_names: {}", result.env_names.join(", "));
    if result.env_public.is_empty() {
        println!("env_public: (none)");
    } else {
        for (name, value) in &result.env_public {
            println!("env_public: {name}={value}");
        }
    }
    if result.secret_env_names.is_empty() {
        println!("secret_env_names: (none)");
    } else {
        println!("secret_env_names: {}", result.secret_env_names.join(", "));
    }
}

/// Prompts for every missing need on a TTY and saves them to the vault before returning the
/// freshly re-resolved needs; off a TTY, reports the fix line and stops (§4.4 "Missing
/// needs"). Re-resolving after the save (rather than patching the in-memory list) keeps this
/// the same code path `kadou vault set` itself goes through.
fn resolve_missing_needs_interactively(
    kata: &Kata,
    vault_store_handle: &VaultStore,
    vault: &mut Vault,
    missing_needs: &[String],
) -> Result<Vec<kadou_core::ResolvedNeed>, ExitCode> {
    if std::io::stdin().is_terminal() {
        for name in missing_needs {
            let value = read_vault_value(name, false).map_err(|err| {
                eprintln!("error: failed to read a value for `{name}`: {err}");
                ExitCode::from(2)
            })?;
            vault.set(name.clone(), value, true);
        }
        vault_store_handle.save(vault).map_err(|err| {
            eprintln!("error: failed to save the vault: {err}");
            ExitCode::FAILURE
        })?;
        Ok(kadou_core::resolve_needs(kata, vault))
    } else {
        eprintln!(
            "error: {} needs {} but the vault isn't set up yet",
            kata.id,
            missing_needs.join(", ")
        );
        for name in missing_needs {
            eprintln!("  = kadou vault set {name}");
        }
        Err(ExitCode::from(2))
    }
}

/// Builds the `RunOneRequest` and runs it to completion — the one shared shape `kadou run` and
/// `kadou grant approve` both call into (§6.4 item 5 "Approval runs in the human CLI's
/// environment (full parent env, not the MCP server's allowlisted one)" applies to both).
fn run_one_cli(
    paths: &KadouPaths,
    config: &Config,
    kata: &Kata,
    folder: &str,
    resolved_args: &[kadou_core::ResolvedVar],
    resolved_needs: &[kadou_core::ResolvedNeed],
    mcp_client: Option<&str>,
) -> Result<kadou_core::runner::RunReport, kadou_core::runner::RunOneError> {
    let initiator = current_initiator();
    let req = RunOneRequest {
        kata,
        folder,
        resolved_args,
        resolved_needs,
        kata_dir_root: &paths.kata_dir(),
        interface: "cli",
        initiator: &initiator,
        mcp_client,
        // CLI keeps the full parent environment (§6.1) — nothing extra to layer on top.
        base_env: Vec::new(),
        env_clear: false,
        config_exec_timeout: config.exec.timeout,
    };
    run_one_blocking(&paths.state_dir, &req)
}

/// Prints a completed run's output and maps its terminal status to an exit code, writing
/// last-used args on success (§6.6 D5). Shared shape between `kadou run` and `kadou grant
/// approve`'s own report handling.
fn print_run_report_and_exit_code(
    report: &kadou_core::runner::RunReport,
    kata: &Kata,
    resolved_args: &[kadou_core::ResolvedVar],
    config: &Config,
    state_dir: &Path,
) -> ExitCode {
    for line in &report.output {
        println!("{line}");
    }
    match report.status {
        kadou_exec::RunStatus::Success => {
            // Last-used args are a prefill convenience (§6.6 D5): args are never secret by
            // construction, so writing them plainly is safe.
            let mut last = BTreeMap::new();
            for arg in resolved_args {
                last.insert(arg.name.clone(), arg.value.clone());
            }
            if !last.is_empty()
                && let Err(err) = LastArgsStore::new(state_dir).write(&kata.id, &last)
            {
                eprintln!("warning: failed to save last-used args: {err}");
            }
            ExitCode::SUCCESS
        }
        kadou_exec::RunStatus::Failed => ExitCode::FAILURE,
        kadou_exec::RunStatus::TimedOut => {
            let timeout = kadou_exec::effective_timeout(kata.timeout, config.exec.timeout);
            eprintln!("error: {} timed out after {timeout:?}", kata.id);
            ExitCode::FAILURE
        }
        kadou_exec::RunStatus::Cancelled => {
            eprintln!("cancelled");
            ExitCode::from(130)
        }
    }
}

/// Everything `run_run` needs before it can gate/spawn: the looked-up kata, its resolved
/// args/needs, and the vault handles the missing-needs prompt (if any) reuses.
struct RunPreparation {
    kata: Kata,
    resolved_args: Vec<kadou_core::ResolvedVar>,
    resolved_needs: Vec<kadou_core::ResolvedNeed>,
    vault_store_handle: VaultStore,
    vault: Vault,
}

fn prepare_run(paths: &KadouPaths, id: &str, kv: &[String]) -> Result<RunPreparation, ExitCode> {
    let provided = parse_kv(kv).map_err(|bad| {
        eprintln!("error: invalid arg `{bad}`, expected key=value");
        ExitCode::from(2)
    })?;
    let kata = find_kata_or_report(&paths.kata_dir(), id)?;
    let resolved_args = kadou_core::resolve_args(&kata, &provided).map_err(|err| {
        eprintln!("error: {err}");
        ExitCode::from(2)
    })?;
    let vault_store_handle = vault_store(paths);
    auto_import_go_vault(&vault_store_handle);
    let vault = vault_store_handle.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    });
    let resolved_needs = kadou_core::resolve_needs(&kata, &vault);
    Ok(RunPreparation {
        kata,
        resolved_args,
        resolved_needs,
        vault_store_handle,
        vault,
    })
}

/// The non-interactive replay line §6.3's confirm protocol prints on `NeedsConfirm`: the same
/// invocation with `--confirm <id>` appended.
fn build_replay_line(id: &str, kv: &[String]) -> String {
    let mut replay_line = format!("kadou run {id}");
    for pair in kv {
        replay_line.push(' ');
        replay_line.push_str(pair);
    }
    replay_line.push_str(&format!(" --confirm {id}"));
    replay_line
}

pub fn run_run(
    id: Option<String>,
    kv: Vec<String>,
    dry_run: bool,
    confirm_flag: Option<String>,
    ask: bool,
) -> ExitCode {
    if ask {
        eprintln!("error: 'run --ask' is not yet implemented (slice 8)");
        eprintln!("  = kadou run <id> [k=v...] [--dry-run]");
        return ExitCode::from(2);
    }

    let Some(id) = id else {
        eprintln!("error: 'run' with no id is not yet implemented (slice 8)");
        eprintln!("  = kadou run <id>, or kadou list");
        return ExitCode::from(2);
    };

    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let mut prep = match prepare_run(&paths, &id, &kv) {
        Ok(p) => p,
        Err(code) => return code,
    };

    if dry_run {
        let result = kadou_core::runner::dry_run(&prep.resolved_args, &prep.resolved_needs);
        print_dry_run(&prep.kata.id, &result);
        return ExitCode::SUCCESS;
    }

    if let Err(code) = check_human_ceiling(&prep.kata, &config) {
        return code;
    }
    let replay_line = build_replay_line(&id, &kv);
    if let Err(code) = cli_confirm(&prep.kata, confirm_flag.as_deref(), &replay_line) {
        return code;
    }

    if let Err(code) = fill_missing_needs_interactively(&mut prep) {
        return code;
    }

    let folder = prep
        .kata
        .id
        .split('/')
        .next()
        .unwrap_or(&prep.kata.id)
        .to_string();
    match run_one_cli(
        &paths,
        &config,
        &prep.kata,
        &folder,
        &prep.resolved_args,
        &prep.resolved_needs,
        None,
    ) {
        Ok(report) => print_run_report_and_exit_code(
            &report,
            &prep.kata,
            &prep.resolved_args,
            &config,
            &paths.state_dir,
        ),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Re-resolves `prep.resolved_needs` after prompting for any missing ones, if there are any
/// (a no-op otherwise) — the shared tail of `run_run`'s missing-needs handling.
fn fill_missing_needs_interactively(prep: &mut RunPreparation) -> Result<(), ExitCode> {
    let missing_needs: Vec<String> = prep
        .resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.clone())
        .collect();
    if missing_needs.is_empty() {
        return Ok(());
    }
    prep.resolved_needs = resolve_missing_needs_interactively(
        &prep.kata,
        &prep.vault_store_handle,
        &mut prep.vault,
        &missing_needs,
    )?;
    Ok(())
}

/// `kadou show <id>` (§9 slice 3 "`kadou show` printing the header fields, resolved args, env
/// names, file path and sha256"). No picker for a missing id in this slice, same as `run`.
pub fn run_show(id: Option<String>) -> ExitCode {
    let Some(id) = id else {
        eprintln!("error: 'show' with no id is not yet implemented (slice 8)");
        eprintln!("  = kadou show <id>, or kadou list");
        return ExitCode::from(2);
    };

    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let kata = match find_kata_or_report(&paths.kata_dir(), &id) {
        Ok(k) => k,
        Err(code) => return code,
    };

    if let Err(code) = check_human_ceiling(&kata, &config) {
        return code;
    }

    let sha256 = match kadou_core::file_sha256(&kata.path) {
        Ok(digest) => digest,
        Err(err) => {
            eprintln!("error: failed to hash {}: {err}", kata.path.display());
            return ExitCode::FAILURE;
        }
    };

    // Missing-required-arg is not a `show`-time failure: the point of `show` is to tell a
    // human which args they still need to pass, not to refuse to describe the kata.
    let resolved_args = kadou_core::resolve_args(&kata, &BTreeMap::new()).unwrap_or_default();
    let vault = load_vault(&paths);
    let resolved_needs = kadou_core::resolve_needs(&kata, &vault);

    println!("{}   {}   {}", kata.id, kata.risk, kata.about);
    println!();
    println!("file    {}", kata.path.display());
    println!("sha256  {sha256}");

    if !kata.needs.is_empty() {
        println!();
        println!("needs");
        for need in &resolved_needs {
            match &need.value {
                Some(value) if !need.secret => println!("  {} = {value}", need.name),
                Some(_) => println!("  {} (secret)", need.name),
                None => println!("  {} (missing; kadou vault set {})", need.name, need.name),
            }
        }
    }

    if !kata.args.is_empty() {
        println!();
        println!("args");
        for arg in &kata.args {
            let resolved = resolved_args
                .iter()
                .find(|r| r.name == arg.name)
                .map(|r| r.value.clone());
            match resolved {
                Some(value) => println!("  {} = {value}", arg.name),
                None => println!("  {} (required)", arg.name),
            }
        }
    }

    let mut env_names: Vec<String> = resolved_needs.iter().map(|n| n.env_name.clone()).collect();
    env_names.extend(kata.args.iter().map(|a| a.env_name()));
    println!();
    println!("env     {}", env_names.join(" "));

    ExitCode::SUCCESS
}

/// `kadou vault set [--plain] <name>` (§6.5, §7.1, §9 slice 4). Reads the value from a TTY
/// prompt or stdin, never argv — `main.rs`'s `VaultAction::Set` has no `value` field, so
/// there is nowhere on the command line a value could even go.
pub fn run_vault_set(name: String, plain: bool) -> ExitCode {
    let paths = resolve_paths();
    let store = vault_store(&paths);
    auto_import_go_vault(&store);

    let value = match read_vault_value(&name, plain) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error: failed to read a value for `{name}`: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut vault = store.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the existing vault: {err}; starting empty");
        Vault::default()
    });
    vault.set(name.clone(), value, !plain);

    match store.save(&vault) {
        Ok(()) => {
            println!("saved {name}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: failed to save the vault: {err}");
            ExitCode::FAILURE
        }
    }
}

/// `kadou vault list` (§6.5, §7.1): names and the secret bit, never values.
pub fn run_vault_list() -> ExitCode {
    let paths = resolve_paths();
    let vault = load_vault(&paths);

    if vault.is_empty() {
        println!("no vault entries");
        return ExitCode::SUCCESS;
    }

    let mut names: Vec<(&str, bool)> = vault.names().collect();
    names.sort_by(|a, b| a.0.cmp(b.0));
    for (name, secret) in names {
        let kind = if secret { "secret" } else { "plain" };
        println!("{name:<28} {kind}");
    }
    ExitCode::SUCCESS
}

/// `kadou mcp serve [--transport stdio|http] [--bind ...] [--max-risk LEVEL]` (§5.1, §7.1, §9
/// slice 5). HTTP stays behind a cargo feature this slice does not enable (§5.1 "HTTP stays
/// behind a build-time cargo feature"), so any transport other than `stdio` is a clean
/// refusal rather than a silent fallback.
pub fn run_mcp_serve(
    transport: String,
    _bind: Option<String>,
    max_risk: Option<String>,
) -> ExitCode {
    if transport != "stdio" {
        eprintln!("error: --transport {transport} is not available in this build");
        eprintln!(
            "  = HTTP transport stays behind a cargo feature not yet shipped (docs/design/05-prd.md §5.1); use --transport stdio"
        );
        return ExitCode::from(2);
    }

    // Structured logs to stderr, never stdout (the stdio transport reserves stdout for the
    // MCP protocol itself, §3.1) — the only channel a swallowed config/vault load failure
    // (A6) has to reach a human.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let max_risk_flag = match max_risk.as_deref() {
        None => None,
        Some(s) => match s.parse::<RiskLevel>() {
            Ok(r) => Some(r),
            Err(_) => {
                eprintln!("error: unknown risk level `{s}`");
                eprintln!("  = risk is one of low, medium, high, critical");
                return ExitCode::from(2);
            }
        },
    };

    let paths = resolve_paths();
    materialize_starter(&paths);

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // Default concurrency limit of 2 (§6.1 "A per-server concurrency limit (default 2)").
    let state = kadou_mcp::ServerState::new(paths, max_risk_flag, 2, &cwd);
    let server = kadou_mcp::KadouMcpServer::new(state);

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("error: failed to start the async runtime: {err}");
            return ExitCode::FAILURE;
        }
    };

    match rt.block_on(server.serve_stdio()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: MCP server failed: {err}");
            ExitCode::FAILURE
        }
    }
}

/// `kadou mcp schema [--bytes]` (§7.1): prints the served `tools/list` JSON, and — with
/// `--bytes` — its byte size, the same number the CI byte gate checks (§1.3).
pub fn run_mcp_schema(bytes: bool) -> ExitCode {
    let json_bytes = match kadou_mcp::schema::tools_list_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", String::from_utf8_lossy(&json_bytes));
    if bytes {
        println!("bytes: {}", json_bytes.len());
    }
    ExitCode::SUCCESS
}

/// `kadou vault rm <name>` (§7.1).
pub fn run_vault_rm(name: String) -> ExitCode {
    let paths = resolve_paths();
    let store = vault_store(&paths);
    let mut vault = store.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    });

    if !vault.remove(&name) {
        eprintln!("error: no vault entry named `{name}`");
        return ExitCode::from(2);
    }

    match store.save(&vault) {
        Ok(()) => {
            println!("removed {name}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: failed to save the vault: {err}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// kadou grant * (§6.4, §7.1, §9 slice 6)
// ---------------------------------------------------------------------------

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
            )
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// kadou get / update / remove / accept (§6.7, §7.1, §9 slice 7)
// ---------------------------------------------------------------------------

/// `kadou get <url> [--as F] [--ref R] [--root SUB]` (§7.1, §4.1, §4.2).
pub fn run_get(
    url: String,
    as_folder: Option<String>,
    git_ref: Option<String>,
    root: Option<String>,
) -> ExitCode {
    let paths = resolve_paths();
    match kadou_core::folder::get_folder(
        &paths.kata_dir(),
        &url,
        as_folder.as_deref(),
        git_ref.as_deref(),
        root.as_deref(),
    ) {
        Ok(target) => {
            println!("cloned {url} into {}", target.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}

/// One `kadou update` outcome, rendered to a human-facing line (§7.1, §9 slice 7).
fn print_update_outcome(outcome: &kadou_core::folder::UpdateOutcome) -> bool {
    use kadou_core::folder::UpdateOutcome;
    match outcome {
        UpdateOutcome::Pulled { folder } => {
            println!("updated {folder}");
            true
        }
        UpdateOutcome::NotGitBacked { folder } => {
            println!("{folder} is not a git checkout; skipped");
            true
        }
        UpdateOutcome::Dirty { folder } => {
            println!("{folder} has local changes; left alone");
            true
        }
        UpdateOutcome::Failed { folder, error } => {
            eprintln!("error: {folder}: {error}");
            false
        }
    }
}

/// `kadou update [<folder>]` (§7.1): every folder when none is named.
pub fn run_update(folder: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    let kata_dir = paths.kata_dir();
    let outcomes = match folder {
        Some(f) => vec![kadou_core::folder::update_folder(&kata_dir, &f)],
        None => kadou_core::folder::update_all(&kata_dir),
    };
    // Every folder is reported, even after one fails (§7.1) -- collect first, then fold, so
    // a `.all()`/`.any()` short-circuit can never skip printing a later folder's outcome.
    let results: Vec<bool> = outcomes.iter().map(print_update_outcome).collect();
    let ok = results.into_iter().all(|ok| ok);
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `kadou remove <folder> [--yes] [--force]` (§7.1): a `y/N` prompt unless `--yes`, then
/// `folder::remove_folder`'s own reserved-name/dirty-without-force refusals (checked again
/// there regardless of what this prompt already confirmed).
pub fn run_remove(folder: String, yes: bool, force: bool) -> ExitCode {
    let paths = resolve_paths();

    if !yes {
        let confirmed = confirm::yes_no(std::io::stdin().is_terminal(), || {
            inquire::Confirm::new(&format!("remove {folder}?"))
                .with_default(false)
                .prompt()
                .unwrap_or(false)
        });
        if !confirmed {
            println!("cancelled");
            return ExitCode::from(1);
        }
    }

    match kadou_core::folder::remove_folder(&paths.kata_dir(), &folder, force) {
        Ok(()) => {
            println!("removed {folder}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}

/// Runs `kadou check`'s real loader (`check_folder`, not just the one accepted file) on the
/// target folder and prints any diagnostics, the same cargo-shaped text `kadou check <folder>`
/// itself would print (§6.7 "and running `kadou check` on the result"). Checking the whole
/// folder, not just the copied file, is what can actually fail here: the draft's own header
/// already passed `prepare_accept`'s strict load, but a cross-kata conflict (a duplicate alias,
/// say) with another kata already in the target folder is a check-time-only failure.
fn check_accepted_kata(paths: &KadouPaths, folder: &str) -> bool {
    let vault = load_vault(paths);
    match kadou_core::check_folder(&paths.kata_dir(), folder, &vault) {
        Ok(report) => {
            print!(
                "{}",
                kadou_core::render_report(&report, &paths.config_dir, false)
            );
            report.is_ok()
        }
        Err(err) => {
            eprintln!("error: {err}");
            false
        }
    }
}

/// `kadou accept <id> [--into F] [--yes]` (§6.7): prints the diff, prompts `y/N` unless
/// `--yes`, copies the draft into its target folder, removes the draft, then runs `kadou
/// check` on the result.
pub fn run_accept(id: String, into: Option<String>, yes: bool) -> ExitCode {
    let paths = resolve_paths();

    let prep = match kadou_mcp::prepare_accept(
        &paths.state_dir,
        &paths.kata_dir(),
        &id,
        into.as_deref(),
    ) {
        Ok(prep) => prep,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };

    print!("{}", prep.diff);

    if !yes {
        let confirmed = confirm::yes_no(std::io::stdin().is_terminal(), || {
            inquire::Confirm::new(&format!("accept into {}?", prep.new_id))
                .with_default(false)
                .prompt()
                .unwrap_or(false)
        });
        if !confirmed {
            println!("cancelled");
            return ExitCode::from(1);
        }
    }

    let target_folder_existed = prep.target_folder_exists;
    if let Err(err) = kadou_mcp::apply_accept(&prep) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
    if !target_folder_existed {
        let folder = prep.new_id.split('/').next().unwrap_or(&prep.new_id);
        println!("created folder {folder}");
    }
    println!("accepted {}", prep.new_id);

    let folder = prep.new_id.split('/').next().unwrap_or(&prep.new_id);
    if check_accepted_kata(&paths, folder) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
