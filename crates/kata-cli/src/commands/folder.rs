//! `kata get` / `update` / `remove` / `accept` (§6.7, §7.1, §9 slice 7).

use std::io::IsTerminal as _;
use std::process::ExitCode;

use kata_core::KataPaths;

use crate::confirm;

use super::{load_vault, resolve_paths};

/// `kata get <url> [--as F] [--ref R] [--root SUB]` (§7.1, §4.1, §4.2).
pub fn run_get(
    url: String,
    as_folder: Option<String>,
    git_ref: Option<String>,
    root: Option<String>,
) -> ExitCode {
    let paths = resolve_paths();
    match kata_core::folder::get_folder(
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

/// One `kata update` outcome, rendered to a human-facing line (§7.1, §9 slice 7).
fn print_update_outcome(outcome: &kata_core::folder::UpdateOutcome) -> bool {
    use kata_core::folder::UpdateOutcome;
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

/// `kata update [<folder>]` (§7.1): every folder when none is named.
pub fn run_update(folder: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    let kata_dir = paths.kata_dir();
    let outcomes = match folder {
        Some(f) => vec![kata_core::folder::update_folder(&kata_dir, &f)],
        None => kata_core::folder::update_all(&kata_dir),
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

/// `kata remove <folder> [--yes] [--force]` (§7.1): a `y/N` prompt unless `--yes`, then
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

    match kata_core::folder::remove_folder(&paths.kata_dir(), &folder, force) {
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

/// Runs `kata check`'s real loader (`check_folder`, not just the one accepted file) on the
/// target folder and prints any diagnostics, the same cargo-shaped text `kata check <folder>`
/// itself would print (§6.7 "and running `kata check` on the result"). Checking the whole
/// folder, not just the copied file, is what can actually fail here: the draft's own header
/// already passed `prepare_accept`'s strict load, but a cross-kata conflict (a duplicate alias,
/// say) with another kata already in the target folder is a check-time-only failure.
fn check_accepted_kata(paths: &KataPaths, folder: &str) -> bool {
    let vault = load_vault(paths);
    match kata_core::check_folder(&paths.kata_dir(), folder, &vault) {
        Ok(report) => {
            print!(
                "{}",
                kata_core::render_report(&report, &paths.config_dir, false)
            );
            report.is_ok()
        }
        Err(err) => {
            eprintln!("error: {err}");
            false
        }
    }
}

/// `kata accept <id> [--into F] [--yes]` (§6.7): prints the diff, prompts `y/N` unless
/// `--yes`, copies the draft into its target folder, removes the draft, then runs `kadou
/// check` on the result.
pub fn run_accept(id: String, into: Option<String>, yes: bool) -> ExitCode {
    let paths = resolve_paths();

    let prep =
        match kata_mcp::prepare_accept(&paths.state_dir, &paths.kata_dir(), &id, into.as_deref()) {
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
    if let Err(err) = kata_mcp::apply_accept(&prep) {
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
