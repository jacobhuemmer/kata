//! Real implementations of the slice-2 (`list`, `check`, `import`) and slice-3/4 (`run`,
//! `show`, `vault`) commands — the rest of the command tree in `main.rs` stays a stub until
//! its own slice lands (`docs/design/05-prd.md` §9).

use std::collections::BTreeMap;
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kadou_core::{
    Config, GoVaultSource, KadouPaths, Kata, LastArgsStore, LookupResult, RiskLevel, Vault,
    VaultStore,
};
use kadou_mcp::history::{FinishOutcome, HistoryStore, current_initiator};
use kadou_mcp::pending::{self, PendingRecord, PendingStore};
use kadou_mcp::redact;

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

/// The human ceiling for `folder`, per §6.2's formula: `folder[f].max_risk ?? max_risk`.
fn human_ceiling(config: &Config, folder: &str) -> RiskLevel {
    config
        .folder
        .get(folder)
        .and_then(|f| f.max_risk)
        .unwrap_or(config.max_risk)
}

/// The human ceiling still applies regardless of the confirm protocol (§6.2, §6.3 "The human
/// ceiling still applies"): a kata above `max_risk`/`[folder.<f>] max_risk` never runs, no
/// matter what `--confirm` or a TTY prompt says. `--dry-run` never spawns, so it bypasses this
/// too (§6.1).
fn check_human_ceiling(kata: &Kata, config: &Config) -> Result<(), ExitCode> {
    let folder = kata.id.split('/').next().unwrap_or(&kata.id);
    let ceiling = human_ceiling(config, folder);
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

fn parse_risk(s: &str) -> Option<RiskLevel> {
    match s {
        "low" => Some(RiskLevel::Low),
        "medium" => Some(RiskLevel::Medium),
        "high" => Some(RiskLevel::High),
        "critical" => Some(RiskLevel::Critical),
        _ => None,
    }
}

/// `kadou list [--folder F] [--risk R] [query]` — the CLI projection of `list_kata` (§7.1).
/// A human's own ceiling applies (§6.2 `visible(k,f) = rank(k.risk) ≤ human_ceiling(f)
/// [CLI]`); untrusted project-local folders are out of scope for this slice.
pub fn run_list(query: Option<String>, folder: Option<String>, risk: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let risk_filter = match risk.as_deref() {
        None => None,
        Some(s) => match parse_risk(s) {
            Some(r) => Some(r),
            None => {
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
    let mut rows: Vec<(String, RiskLevel, String)> = Vec::new();

    for (name, files) in &scanned {
        if let Some(want) = &folder
            && name != want
        {
            continue;
        }
        let ceiling = config
            .folder
            .get(name)
            .and_then(|f| f.max_risk)
            .unwrap_or(config.max_risk);

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
            if let Some(q) = &query_lower {
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
    }

    rows.sort();

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
            folder: arg.clone(),
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
pub fn run_run(
    id: Option<String>,
    kv: Vec<String>,
    dry_run: bool,
    confirm_flag: Option<String>,
) -> ExitCode {
    let Some(id) = id else {
        eprintln!("error: 'run' with no id is not yet implemented (slice 8)");
        eprintln!("  = kadou run <id>, or kadou list");
        return ExitCode::from(2);
    };

    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let provided = match parse_kv(&kv) {
        Ok(m) => m,
        Err(bad) => {
            eprintln!("error: invalid arg `{bad}`, expected key=value");
            return ExitCode::from(2);
        }
    };

    let kata = match find_kata_or_report(&paths.kata_dir(), &id) {
        Ok(k) => k,
        Err(code) => return code,
    };

    let resolved_args = match kadou_core::resolve_args(&kata, &provided) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    let vault_store_handle = vault_store(&paths);
    auto_import_go_vault(&vault_store_handle);
    let mut vault = vault_store_handle.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    });
    let mut resolved_needs = kadou_core::resolve_needs(&kata, &vault);

    if dry_run {
        let result = kadou_exec::dry_run(&resolved_args, &resolved_needs);
        println!("{}", kata.id);
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
        return ExitCode::SUCCESS;
    }

    if let Err(code) = check_human_ceiling(&kata, &config) {
        return code;
    }

    let mut replay_line = format!("kadou run {id}");
    for pair in &kv {
        replay_line.push(' ');
        replay_line.push_str(pair);
    }
    replay_line.push_str(&format!(" --confirm {id}"));
    if let Err(code) = cli_confirm(&kata, confirm_flag.as_deref(), &replay_line) {
        return code;
    }

    let missing_needs: Vec<String> = resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.clone())
        .collect();
    if !missing_needs.is_empty() {
        if std::io::stdin().is_terminal() {
            for name in &missing_needs {
                let value = match read_vault_value(name, false) {
                    Ok(v) => v,
                    Err(err) => {
                        eprintln!("error: failed to read a value for `{name}`: {err}");
                        return ExitCode::from(2);
                    }
                };
                vault.set(name.clone(), value, true);
            }
            if let Err(err) = vault_store_handle.save(&vault) {
                eprintln!("error: failed to save the vault: {err}");
                return ExitCode::FAILURE;
            }
            resolved_needs = kadou_core::resolve_needs(&kata, &vault);
        } else {
            eprintln!(
                "error: {} needs {} but the vault isn't set up yet",
                kata.id,
                missing_needs.join(", ")
            );
            for name in &missing_needs {
                eprintln!("  = kadou vault set {name}");
            }
            return ExitCode::from(2);
        }
    }

    let ctx = kadou_exec::kata_context(&paths.kata_dir(), &kata);
    let mut env = kadou_exec::context_env(&ctx);
    for arg in &resolved_args {
        env.push((arg.env_name.clone(), arg.value.clone()));
    }
    for need in &resolved_needs {
        env.push((
            need.env_name.clone(),
            need.value.clone().expect("checked above"),
        ));
    }

    let timeout = kadou_exec::effective_timeout(kata.timeout, &config.exec.timeout);
    let spec = kadou_exec::RunSpec {
        shebang: kata.shebang.clone(),
        file: kata.path.clone(),
        cwd: ctx.dir.clone(),
        env,
        timeout,
        // CLI keeps the full parent environment — a human's own shell context (§6.1).
        env_clear: false,
    };

    match kadou_exec::run_blocking(spec) {
        Ok(outcome) => {
            for line in &outcome.output {
                println!("{line}");
            }
            match outcome.status {
                kadou_exec::RunStatus::Success => {
                    // Last-used args are a prefill convenience (§6.6 D5): args are never
                    // secret by construction, so writing them plainly is safe.
                    let mut last = BTreeMap::new();
                    for arg in &resolved_args {
                        last.insert(arg.name.clone(), arg.value.clone());
                    }
                    if !last.is_empty()
                        && let Err(err) =
                            LastArgsStore::new(&paths.state_dir).write(&kata.id, &last)
                    {
                        eprintln!("warning: failed to save last-used args: {err}");
                    }
                    ExitCode::SUCCESS
                }
                kadou_exec::RunStatus::Failed => ExitCode::FAILURE,
                kadou_exec::RunStatus::TimedOut => {
                    eprintln!("error: {} timed out after {timeout:?}", kata.id);
                    ExitCode::FAILURE
                }
                kadou_exec::RunStatus::Cancelled => {
                    eprintln!("cancelled");
                    ExitCode::from(130)
                }
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
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

    let max_risk_flag = match max_risk.as_deref() {
        None => None,
        Some(s) => match parse_risk(s) {
            Some(r) => Some(r),
            None => {
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
    let json_bytes = kadou_mcp::schema::tools_list_bytes();
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

    let Some(record) = store.get(&pending_id) else {
        eprintln!("error: no pending grant `{pending_id}`");
        return ExitCode::from(2);
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
    if store.get(&pending_id).is_none() {
        eprintln!("error: no pending grant `{pending_id}`");
        return ExitCode::from(2);
    }
    match store.delete(&pending_id) {
        Ok(()) => {
            println!("denied and removed {pending_id}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: failed to remove {pending_id}: {err}");
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

    let mut config = load_config(&paths);
    let pin_prefix = format!("{id}@");
    config
        .agent
        .allow
        .retain(|existing| existing != &id && !existing.starts_with(&pin_prefix));
    config.agent.allow.push(entry.clone());

    match config.save(&paths.config_file()) {
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
pub fn run_grant_approve(pending_id: String, confirm_flag: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let store = PendingStore::new(&paths.state_dir);

    let Some(mut record) = store.get(&pending_id) else {
        eprintln!("error: no pending grant `{pending_id}`");
        return ExitCode::from(2);
    };

    if record.history_id.is_some() {
        eprintln!(
            "error: {pending_id} was already approved (history_id {})",
            record.history_id.as_deref().unwrap_or("?")
        );
        return ExitCode::from(2);
    }
    if record.is_expired() {
        eprintln!("error: {pending_id} expired at {}", record.expires);
        eprintln!("  = ask the agent to request {} again", record.id);
        return ExitCode::from(2);
    }

    let kata = match find_kata_or_report(&paths.kata_dir(), &record.id) {
        Ok(k) => k,
        Err(code) => return code,
    };

    let current_sha256 = match kadou_core::file_sha256(&kata.path) {
        Ok(sha256) => sha256,
        Err(err) => {
            eprintln!("error: failed to hash {}: {err}", kata.path.display());
            return ExitCode::FAILURE;
        }
    };
    if current_sha256 != record.sha256 {
        eprintln!(
            "error: {} changed since the request (sha256 no longer matches)",
            record.id
        );
        eprintln!("  = kadou grant show {pending_id} to see the diff, then ask for a fresh grant");
        return ExitCode::from(2);
    }
    if let Some(pinned_head) = &record.folder_head {
        let folder_dir = paths.kata_dir().join(&record.folder);
        if pending::git_head(&folder_dir).as_deref() != Some(pinned_head.as_str()) {
            eprintln!(
                "error: {}'s folder git HEAD changed since the request",
                record.id
            );
            eprintln!(
                "  = kadou grant show {pending_id} to see the diff, then ask for a fresh grant"
            );
            return ExitCode::from(2);
        }
    }

    let replay_line = format!("kadou grant approve {pending_id} --confirm {}", record.id);
    if let Err(code) = cli_confirm(&kata, confirm_flag.as_deref(), &replay_line) {
        return code;
    }

    let resolved_args = match kadou_core::resolve_args(&kata, &record.args) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error: the pinned args no longer resolve against the current header: {err}");
            return ExitCode::from(2);
        }
    };

    let vault_store_handle = vault_store(&paths);
    auto_import_go_vault(&vault_store_handle);
    let vault = vault_store_handle.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    });
    let resolved_needs = kadou_core::resolve_needs(&kata, &vault);
    let missing_needs: Vec<String> = resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.clone())
        .collect();
    if !missing_needs.is_empty() {
        eprintln!(
            "error: {} needs {} but the vault isn't set up yet",
            kata.id,
            missing_needs.join(", ")
        );
        for name in &missing_needs {
            eprintln!("  = kadou vault set {name}");
        }
        return ExitCode::from(2);
    }

    // Approval runs in the CLI's own full environment (§6.4 item 5 "Approval runs in the
    // human CLI's environment (full parent env, not the MCP server's allowlisted one)").
    let config = load_config(&paths);
    let ctx = kadou_exec::kata_context(&paths.kata_dir(), &kata);
    let mut env = kadou_exec::context_env(&ctx);
    let secret_values: Vec<String> = resolved_needs
        .iter()
        .filter(|n| n.secret)
        .filter_map(|n| n.value.clone())
        .collect();
    for arg in &resolved_args {
        env.push((arg.env_name.clone(), arg.value.clone()));
    }
    for need in &resolved_needs {
        env.push((
            need.env_name.clone(),
            need.value.clone().expect("checked above"),
        ));
    }

    let timeout = kadou_exec::effective_timeout(kata.timeout, &config.exec.timeout);
    let spec = kadou_exec::RunSpec {
        shebang: kata.shebang.clone(),
        file: kata.path.clone(),
        cwd: ctx.dir.clone(),
        env,
        timeout,
        env_clear: false,
    };

    let history_store = HistoryStore::new(&paths.state_dir);
    let public_args: BTreeMap<String, String> = resolved_args
        .iter()
        .map(|a| (a.name.clone(), a.value.clone()))
        .collect();
    let mut hist_record = match history_store.begin(
        &kata.id,
        &record.folder,
        &public_args,
        "cli",
        &current_initiator(),
        None,
    ) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("error: failed to start a history record: {err}");
            return ExitCode::FAILURE;
        }
    };

    match kadou_exec::run_blocking(spec) {
        Ok(outcome) => {
            for line in &outcome.output {
                println!("{line}");
            }
            let full_output = outcome.output.join("\n");
            let redacted = redact::redact_all(&full_output, &secret_values);
            let output_lines = if redacted.is_empty() {
                0
            } else {
                redacted.split('\n').count()
            };
            let summary = redacted
                .split('\n')
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .to_string();
            let duration_ms = u64::try_from(outcome.duration.as_millis()).unwrap_or(u64::MAX);
            let status_str = match outcome.status {
                kadou_exec::RunStatus::Success => "success",
                kadou_exec::RunStatus::Failed | kadou_exec::RunStatus::TimedOut => "failed",
                kadou_exec::RunStatus::Cancelled => "cancelled",
            };

            let _ = history_store.finish(
                &mut hist_record,
                FinishOutcome {
                    status: status_str,
                    exit_code: outcome.exit_code,
                    redacted_output: &redacted,
                    output_lines,
                    output_summary: &summary,
                    duration_ms,
                },
            );

            // §6.4 item 5: "On success the pending record gains history_id, status, and
            // log_path, all readable via pending_path" — written back regardless of the run's
            // own outcome, so a failed approved run is still visible via `grant show`.
            record.history_id = Some(hist_record.history_id.clone());
            record.status = Some(status_str.to_string());
            record.log_path = Some(hist_record.log_path.clone());
            if let Err(err) = store.save(&record) {
                eprintln!("warning: failed to update the pending record: {err}");
            }

            match outcome.status {
                kadou_exec::RunStatus::Success => ExitCode::SUCCESS,
                kadou_exec::RunStatus::Failed => ExitCode::FAILURE,
                kadou_exec::RunStatus::TimedOut => {
                    eprintln!("error: {} timed out after {timeout:?}", kata.id);
                    ExitCode::FAILURE
                }
                kadou_exec::RunStatus::Cancelled => {
                    eprintln!("cancelled");
                    ExitCode::from(130)
                }
            }
        }
        Err(err) => {
            let _ = history_store.finish(
                &mut hist_record,
                FinishOutcome {
                    status: "failed",
                    exit_code: None,
                    redacted_output: "",
                    output_lines: 0,
                    output_summary: "",
                    duration_ms: 0,
                },
            );
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
