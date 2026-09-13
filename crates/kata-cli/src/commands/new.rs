//! `kata new` (§7.1, §7.3, §9 slice 8).

use std::io::IsTerminal as _;
use std::process::ExitCode;

use super::edit::spawn_editor;
use super::{find_kata_or_report, materialize_starter, resolve_paths};

/// The template `kata new` writes for a fresh kata (`.claude/skills/create-kata/SKILL.md`'s
/// own template, §4.3's six-key grammar): `about`/`risk` only, `set -eu`, a `main` function --
/// the smallest header that passes `kata check` unedited.
fn new_kata_template(name: &str) -> String {
    format!(
        "#!/bin/sh\n# ---\n# about: TODO: describe {name}\n# risk:  low\n# ---\nset -eu\n\nmain() {{\n  echo \"TODO: implement {name}\"\n}}\n\nmain \"$@\"\n"
    )
}

/// `kata new <folder/name> [--from <id>]` (§7.1, §7.3, §9 slice 8): writes the header
/// template (or a copy of `--from`'s source) and opens `$EDITOR`, skipping the editor off a
/// TTY so scripting `kata new` never blocks on an interactive program.
pub fn run_new(id: String, from: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let kata_dir = paths.kata_dir();
    let target = kata_dir.join(format!("{id}.sh"));

    if target.exists() {
        eprintln!("error: {id} already exists");
        eprintln!("  = kata edit {id}, or pick a different name");
        return ExitCode::from(2);
    }

    let source = match from {
        Some(from_id) => match find_kata_or_report(&kata_dir, &from_id) {
            Ok(kata) => match std::fs::read_to_string(&kata.path) {
                Ok(text) => text,
                Err(err) => {
                    eprintln!("error: failed to read {}: {err}", kata.path.display());
                    return ExitCode::FAILURE;
                }
            },
            Err(code) => return code,
        },
        None => {
            let name = id.rsplit('/').next().unwrap_or(&id);
            new_kata_template(name)
        }
    };

    if let Some(parent) = target.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        eprintln!("error: failed to create {}: {err}", parent.display());
        return ExitCode::FAILURE;
    }
    if let Err(err) = std::fs::write(&target, source) {
        eprintln!("error: failed to write {}: {err}", target.display());
        return ExitCode::FAILURE;
    }
    println!("wrote {}", target.display());

    if !std::io::stdin().is_terminal() {
        return ExitCode::SUCCESS;
    }
    spawn_editor(&target)
}
