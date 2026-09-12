//! `kadou check` (§4.7, decision 12, §9 slice 2).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kadou_core::Vault;

use crate::ui;

use super::{load_vault, materialize_starter, resolve_paths, styled_for_stdout};

/// `kadou check [folder|path] [-v]` — the loader (§4.7, decision 12). No argument checks
/// every folder under `kata/`; a folder name checks just that folder; a filesystem path to
/// a single kata file checks just that file.
pub fn run_check(folder_or_path: Option<String>, verbose: bool, plain: bool) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let kata_dir = paths.kata_dir();
    let display_root = paths.config_dir.clone();
    let vault = load_vault(&paths);
    let styled = styled_for_stdout(plain);

    let Some(arg) = folder_or_path else {
        return check_all(&kata_dir, &display_root, verbose, &vault, styled);
    };

    if kata_dir.join(&arg).is_dir() {
        return check_one_folder(&kata_dir, &arg, &display_root, verbose, &vault, styled);
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
            colorized_report(&report, &display_root, verbose, styled)
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

/// `kadou check`'s diagnostics colors (§9 slice 8, `09` §3.5): `kadou_core::render_report`'s
/// plain cargo-shaped text, recolored by severity when `styled` (never otherwise -- a script
/// piping `kadou check` sees the exact bytes the loader produced).
fn colorized_report(
    report: &kadou_core::FolderReport,
    display_root: &Path,
    verbose: bool,
    styled: bool,
) -> String {
    ui::frame::colorize_check_report(
        &kadou_core::render_report(report, display_root, verbose),
        styled,
    )
}

fn check_all(
    kata_dir: &Path,
    display_root: &Path,
    verbose: bool,
    vault: &Vault,
    styled: bool,
) -> ExitCode {
    match kadou_core::check_all(kata_dir, vault) {
        Ok(mut report) => {
            for folder in &mut report.folders {
                add_interpreter_warnings(folder);
                print!(
                    "{}",
                    colorized_report(folder, display_root, verbose, styled)
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
    styled: bool,
) -> ExitCode {
    match kadou_core::check_folder(kata_dir, folder, vault) {
        Ok(mut report) => {
            add_interpreter_warnings(&mut report);
            print!(
                "{}",
                colorized_report(&report, display_root, verbose, styled)
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
