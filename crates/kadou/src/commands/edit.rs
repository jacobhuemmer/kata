//! `kadou edit` (§7.1, §7.3, §9 slice 8).

use std::path::Path;
use std::process::ExitCode;

use super::picker::{PickOutcome, resolve_id_or_pick};
use super::{find_kata_or_report, materialize_starter, resolve_paths};

/// `kadou edit <id>` opens the kata file in `$EDITOR`; no id opens the picker on a TTY (§7.1,
/// §7.3, §9 slice 8).
pub fn run_edit(id: Option<String>, plain: bool) -> ExitCode {
    let id = match id {
        Some(id) => id,
        None => match resolve_id_or_pick("edit", plain) {
            PickOutcome::Use(id) => id,
            PickOutcome::Done(code) => return code,
        },
    };
    edit_kata_by_id(&id)
}

pub(super) fn edit_kata_by_id(id: &str) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let kata = match find_kata_or_report(&paths.kata_dir(), id) {
        Ok(k) => k,
        Err(code) => return code,
    };
    spawn_editor(&kata.path)
}

/// Opens `path` in `$EDITOR` (`vi` when unset), waiting for it to exit. A non-TTY caller has
/// no business launching an interactive editor -- callers that need that check (`kadou new`)
/// do it themselves before calling this.
pub(super) fn spawn_editor(path: &Path) -> ExitCode {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    match std::process::Command::new(&editor).arg(path).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("error: {editor} exited with {status}");
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!(
                "error: failed to launch {editor} on {}: {err}",
                path.display()
            );
            ExitCode::FAILURE
        }
    }
}
