//! Real implementations of the slice-2 commands (`list`, `check`, `import`) — the rest of
//! the command tree in `main.rs` stays a stub until its own slice lands
//! (`docs/design/05-prd.md` §9).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kadou_core::{Config, KadouPaths, RiskLevel};

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
        Ok(report) => {
            for folder in &report.folders {
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
        Ok(report) => {
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
