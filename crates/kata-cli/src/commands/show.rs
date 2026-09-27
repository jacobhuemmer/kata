//! `kata show` (§9 slice 3).

use std::process::ExitCode;

use super::picker::{PickOutcome, resolve_id_or_pick};
use super::{
    check_human_ceiling, find_kata_or_report, load_config, load_vault, materialize_starter,
    resolve_paths,
};
use crate::ui::prompt::prefill;

/// `kata show <id>` (§9 slice 3 "`kata show` printing the header fields, resolved args, env
/// names, file path and sha256"); no id opens the picker on a TTY (§7.3, §9 slice 8).
pub fn run_show(id: Option<String>, plain: bool) -> ExitCode {
    let id = match id {
        Some(id) => id,
        None => match resolve_id_or_pick("show", plain) {
            PickOutcome::Use(id) => id,
            PickOutcome::Done(code) => return code,
        },
    };
    show_kata_by_id(&id)
}

pub(super) fn show_kata_by_id(id: &str) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);

    let kata = match find_kata_or_report(&paths.kata_dir(), id) {
        Ok(k) => k,
        Err(code) => return code,
    };

    if let Err(code) = check_human_ceiling(&kata, &config) {
        return code;
    }

    let sha256 = match kata_core::file_sha256(&kata.path) {
        Ok(digest) => digest,
        Err(err) => {
            eprintln!("error: failed to hash {}: {err}", kata.path.display());
            return ExitCode::FAILURE;
        }
    };

    let vault = load_vault(&paths);
    let resolved_needs = kata_core::resolve_needs(&kata, &vault);

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
                None => println!("  {} (missing; kata vault set {})", need.name, need.name),
            }
        }
    }

    if !kata.args.is_empty() {
        println!();
        println!("args");
        // Each arg on its own: one missing required arg must not hide the others' defaults.
        for arg in &kata.args {
            match prefill(arg, None) {
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
