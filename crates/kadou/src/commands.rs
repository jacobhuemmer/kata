//! Real implementations of the slice-2 (`list`, `check`, `import`) and slice-3 (`run`,
//! `show`) commands — the rest of the command tree in `main.rs` stays a stub until its own
//! slice lands (`docs/design/05-prd.md` §9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kadou_core::{Config, KadouPaths, Kata, LookupResult, RiskLevel};

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
    let kata_dir = paths.kata_dir();
    let display_root = paths.config_dir.clone();

    let Some(arg) = folder_or_path else {
        return check_all(&kata_dir, &display_root, verbose);
    };

    if kata_dir.join(&arg).is_dir() {
        return check_one_folder(&kata_dir, &arg, &display_root, verbose);
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

fn check_all(kata_dir: &Path, display_root: &Path, verbose: bool) -> ExitCode {
    match kadou_core::check_all(kata_dir) {
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

fn check_one_folder(kata_dir: &Path, folder: &str, display_root: &Path, verbose: bool) -> ExitCode {
    match kadou_core::check_folder(kata_dir, folder) {
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
pub fn run_run(id: Option<String>, kv: Vec<String>, dry_run: bool) -> ExitCode {
    let Some(id) = id else {
        eprintln!("error: 'run' with no id is not yet implemented (slice 8)");
        eprintln!("  = kadou run <id>, or kadou list");
        return ExitCode::from(2);
    };

    let paths = resolve_paths();
    if let Err(err) = starter::materialize_if_needed(&paths.kata_dir()) {
        eprintln!(
            "warning: failed to materialize the starter kata into {}: {err}",
            paths.kata_dir().display()
        );
    }
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
    let resolved_needs = kadou_core::resolve_needs(&kata);

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

    let missing_needs: Vec<&str> = resolved_needs
        .iter()
        .filter(|n| n.value.is_none())
        .map(|n| n.name.as_str())
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
    };

    match kadou_exec::run_blocking(spec) {
        Ok(outcome) => {
            for line in &outcome.output {
                println!("{line}");
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
    if let Err(err) = starter::materialize_if_needed(&paths.kata_dir()) {
        eprintln!(
            "warning: failed to materialize the starter kata into {}: {err}",
            paths.kata_dir().display()
        );
    }

    let kata = match find_kata_or_report(&paths.kata_dir(), &id) {
        Ok(k) => k,
        Err(code) => return code,
    };

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
    let resolved_needs = kadou_core::resolve_needs(&kata);

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
                _ => println!("  {} (secret; kadou vault set {})", need.name, need.name),
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
