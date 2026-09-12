// ---------------------------------------------------------------------------
// The inline picker glue (§7.3, `09` §3.3, §9 slice 8): the interactive raw-mode loop lives
// here, over `ui::picker`'s pure filter/key/render functions -- see `ui::picker`'s own doc
// comment for the split and why the loop itself is thin. Shared by `run`, `show`, and `edit`.
// ---------------------------------------------------------------------------

use std::io::IsTerminal as _;
use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use kadou_core::{Config, KadouPaths, LookupResult, Vault};

use crate::ui;

use super::edit::edit_kata_by_id;
use super::show::show_kata_by_id;
use super::{load_config, load_vault, materialize_starter, resolve_paths, styled_for_stdout};

/// What opening the picker (or failing to) leaves the calling command to do.
pub(super) enum PickOutcome {
    /// Continue the calling command with this id.
    Use(String),
    /// The picker already finished the whole command itself (showed, edited, or cancelled).
    Done(ExitCode),
}

/// Every visible kata (§6.2 [CLI] human ceiling), the picker's candidate list (§7.3).
fn build_pick_candidates(paths: &KadouPaths, config: &Config) -> Vec<ui::picker::PickCandidate> {
    let scanned = kadou_core::scan_kata_dir(&paths.kata_dir()).unwrap_or_default();
    let mut out = Vec::new();
    for (name, files) in &scanned {
        let ceiling = kadou_core::visibility::human_ceiling(config, name);
        for file in files {
            let Some(header) = &file.header else {
                continue;
            };
            if header.risk > ceiling {
                continue;
            }
            out.push(ui::picker::PickCandidate {
                id: file.id.clone(),
                about: header.about.clone(),
                alias: header.alias.clone(),
                risk: header.risk,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// `no id given and no terminal to pick one` (§7.3, `09` §3.6): the exact non-interactive form
/// shared by `run`, `show`, and `edit`.
fn no_terminal_to_pick(command: &str) -> ExitCode {
    eprintln!("error: no kata id given and no terminal to pick one");
    eprintln!("  = kadou {command} <id>, or kadou list");
    ExitCode::from(2)
}

/// Opens the picker when both stdin and stdout are a real TTY, else the non-interactive error
/// (§7.3). `command` names the calling command for the fix line and the picker's own header
/// (`run which kata?`, `show which kata?`, `edit which kata?`).
pub(super) fn resolve_id_or_pick(command: &str, plain: bool) -> PickOutcome {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return PickOutcome::Done(no_terminal_to_pick(command));
    }
    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);
    let candidates = build_pick_candidates(&paths, &config);
    let styled = styled_for_stdout(plain);

    match run_picker_interactive(&paths, &candidates, command, styled) {
        PickerOutcomeReal::Cancelled => {
            eprintln!("cancelled");
            PickOutcome::Done(ExitCode::from(130))
        }
        PickerOutcomeReal::Selected(id) => PickOutcome::Use(id),
        PickerOutcomeReal::ShowFull(id) => PickOutcome::Done(show_kata_by_id(&id)),
        PickerOutcomeReal::EditAndReturn(id) => PickOutcome::Done(edit_kata_by_id(&id)),
    }
}

/// The real terminal outcome of one picker session -- [`ui::picker::PickerAction`] resolved
/// against the candidate list it fired on.
enum PickerOutcomeReal {
    Cancelled,
    Selected(String),
    ShowFull(String),
    EditAndReturn(String),
}

/// Maps one raw terminal key event to [`ui::picker::PickerKey`], `None` for a key the picker
/// doesn't handle (function keys, mouse events, a key-release on a platform that reports one).
fn map_key(key: crossterm::event::KeyEvent) -> Option<ui::picker::PickerKey> {
    use crossterm::event::{KeyCode, KeyModifiers};
    if key.kind != crossterm::event::KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(ui::picker::PickerKey::Escape),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(ui::picker::PickerKey::Escape)
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(ui::picker::PickerKey::Up)
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(ui::picker::PickerKey::Down)
        }
        // Bound to Ctrl+e, not bare `e` -- see `ui::picker::PickerKey::Edit`'s doc comment for
        // why bare `e` must stay a normal filter character.
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(ui::picker::PickerKey::Edit)
        }
        KeyCode::Up => Some(ui::picker::PickerKey::Up),
        KeyCode::Down => Some(ui::picker::PickerKey::Down),
        KeyCode::Enter => Some(ui::picker::PickerKey::Enter),
        KeyCode::Tab => Some(ui::picker::PickerKey::Tab),
        KeyCode::Backspace => Some(ui::picker::PickerKey::Backspace),
        KeyCode::Char(c) => Some(ui::picker::PickerKey::Char(c)),
        _ => None,
    }
}

/// The preview block for one candidate: needs/args summaries plus the file and its short
/// sha256, the same fields `kadou show` prints (§7.3 "the highlighted kata's header shows
/// below the list").
fn build_preview(
    kata_dir: &Path,
    candidate: &ui::picker::PickCandidate,
    vault: &Vault,
) -> Option<ui::picker::Preview> {
    let LookupResult::Found(kata) = kadou_core::find_kata(kata_dir, &candidate.id).ok()? else {
        return None;
    };
    let needs = kadou_core::resolve_needs(&kata, vault)
        .into_iter()
        .map(|n| ui::picker::PreviewNeed {
            name: n.name,
            satisfied: n.value.is_some(),
        })
        .collect();
    let args = kata
        .args
        .iter()
        .map(|arg| {
            let summary = if arg.is_required() {
                "required".to_string()
            } else {
                "optional".to_string()
            };
            ui::picker::PreviewArg {
                name: arg.name.clone(),
                summary,
            }
        })
        .collect();
    let sha_short = kadou_core::file_sha256(&kata.path)
        .map(|s| s.chars().take(12).collect())
        .unwrap_or_default();
    Some(ui::picker::Preview {
        id: kata.id.clone(),
        risk: kata.risk,
        about: kata.about.clone(),
        needs,
        args,
        file: kata.path.display().to_string(),
        sha_short,
    })
}

/// Renders one frame of the picker (list plus preview) into `out`, replacing `\n` with `\r\n`
/// so it draws correctly in raw mode.
fn render_picker_frame(
    kata_dir: &Path,
    vault: &Vault,
    command: &str,
    state: &ui::picker::PickerState,
    matches: &[&ui::picker::PickCandidate],
    styled: bool,
) -> String {
    let mut block = ui::picker::render_list(command, &state.query, matches, styled);
    if let Some(top) = matches.first()
        && let Some(preview) = build_preview(kata_dir, top, vault)
    {
        block.push_str(&ui::picker::render_preview(&preview, styled));
    }
    block.replace('\n', "\r\n")
}

/// The raw-mode event loop: draw, read one key, apply it, repeat. Deliberately thin -- every
/// decision (filtering, ranking, key handling, rendering) is a pure, already-tested function
/// in `ui::picker`; this function is real-terminal glue only, verified manually over a pty
/// (`CLAUDE.md` forced verification), not by `cargo test`.
fn run_picker_interactive(
    paths: &KadouPaths,
    candidates: &[ui::picker::PickCandidate],
    command: &str,
    styled: bool,
) -> PickerOutcomeReal {
    let vault = load_vault(paths);
    let mut state = ui::picker::PickerState::default();
    let mut stdout = std::io::stdout();
    let _ = crossterm::terminal::enable_raw_mode();
    let mut last_lines: u16 = 0;

    let outcome = loop {
        let matches = ui::picker::filter(&state.query, candidates);
        clear_picker_frame(&mut stdout, last_lines);
        let frame =
            render_picker_frame(&paths.kata_dir(), &vault, command, &state, &matches, styled);
        last_lines = u16::try_from(frame.lines().count()).unwrap_or(u16::MAX);
        let _ = write!(stdout, "\r{frame}");
        let _ = stdout.flush();

        let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() else {
            continue;
        };
        let Some(picker_key) = map_key(key) else {
            continue;
        };
        match ui::picker::apply_key(&mut state, picker_key, matches.len()) {
            ui::picker::PickerAction::Continue => {}
            ui::picker::PickerAction::Select(i) => {
                break PickerOutcomeReal::Selected(matches[i].id.clone());
            }
            ui::picker::PickerAction::ShowFull(i) => {
                break PickerOutcomeReal::ShowFull(matches[i].id.clone());
            }
            ui::picker::PickerAction::EditAndReturn(i) => {
                break PickerOutcomeReal::EditAndReturn(matches[i].id.clone());
            }
            ui::picker::PickerAction::Cancel => break PickerOutcomeReal::Cancelled,
        }
    };

    clear_picker_frame(&mut stdout, last_lines);
    let _ = stdout.flush();
    let _ = crossterm::terminal::disable_raw_mode();
    outcome
}

/// Moves the cursor back up over the picker's last drawn frame and clears each line, so the
/// next draw (or the shell prompt, on exit) starts clean -- no alternate screen, no lost
/// scrollback (§7 rule 6).
fn clear_picker_frame(stdout: &mut std::io::Stdout, lines: u16) {
    use crossterm::QueueableCommand as _;
    for _ in 0..lines {
        let _ = stdout.queue(crossterm::cursor::MoveUp(1));
        let _ = stdout.queue(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::CurrentLine,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadou_core::RiskLevel;

    fn paths_under(dir: &Path) -> KadouPaths {
        KadouPaths {
            config_dir: dir.join(".config/kadou"),
            data_dir: dir.join(".local/share/kadou"),
            state_dir: dir.join(".local/state/kadou"),
        }
    }

    #[test]
    fn build_pick_candidates_respects_the_human_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_under(dir.path());
        let kata_dir = paths.kata_dir();
        std::fs::create_dir_all(kata_dir.join("ops")).unwrap();
        std::fs::write(
            kata_dir.join("ops/low.sh"),
            "#!/bin/sh\n# ---\n# about: Low\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();
        std::fs::write(
            kata_dir.join("ops/crit.sh"),
            "#!/bin/sh\n# ---\n# about: Crit\n# risk:  critical\n# ---\necho hi\n",
        )
        .unwrap();

        let candidates = build_pick_candidates(&paths, &Config::default());
        assert_eq!(candidates.len(), 1, "default ceiling is medium");
        assert_eq!(candidates[0].id, "ops/low");
    }

    #[test]
    fn build_pick_candidates_includes_a_kata_exactly_at_the_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_under(dir.path());
        let kata_dir = paths.kata_dir();
        std::fs::create_dir_all(kata_dir.join("ops")).unwrap();
        std::fs::write(
            kata_dir.join("ops/at-ceiling.sh"),
            "#!/bin/sh\n# ---\n# about: At ceiling\n# risk:  medium\n# ---\necho hi\n",
        )
        .unwrap();

        let candidates = build_pick_candidates(&paths, &Config::default());
        assert_eq!(
            candidates.len(),
            1,
            "medium is visible at the default medium ceiling"
        );
    }

    #[test]
    fn build_preview_returns_none_for_an_unknown_id() {
        let dir = tempfile::tempdir().unwrap();
        let candidate = ui::picker::PickCandidate {
            id: "ops/nope".to_string(),
            about: "x".to_string(),
            alias: Vec::new(),
            risk: RiskLevel::Low,
        };
        assert!(build_preview(dir.path(), &candidate, &Vault::default()).is_none());
    }

    #[test]
    fn build_preview_reports_needs_and_args_for_a_known_kata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ops")).unwrap();
        std::fs::write(
            dir.path().join("ops/x.sh"),
            "#!/bin/sh\n# ---\n# about: X\n# risk:  low\n# needs: token\n# args:\n#   name: text\n# ---\necho hi\n",
        )
        .unwrap();
        let candidate = ui::picker::PickCandidate {
            id: "ops/x".to_string(),
            about: "X".to_string(),
            alias: Vec::new(),
            risk: RiskLevel::Low,
        };

        let preview = build_preview(dir.path(), &candidate, &Vault::default()).unwrap();
        assert_eq!(preview.id, "ops/x");
        assert_eq!(preview.needs.len(), 1);
        assert!(!preview.needs[0].satisfied, "no vault entry for 'token'");
        assert_eq!(preview.args.len(), 1);
        assert_eq!(preview.args[0].summary, "required");
    }

    #[test]
    fn render_picker_frame_appends_a_preview_only_when_there_is_a_top_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ops")).unwrap();
        std::fs::write(
            dir.path().join("ops/x.sh"),
            "#!/bin/sh\n# ---\n# about: X\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();
        let candidate = ui::picker::PickCandidate {
            id: "ops/x".to_string(),
            about: "X".to_string(),
            alias: Vec::new(),
            risk: RiskLevel::Low,
        };
        let state = ui::picker::PickerState::default();
        let matches: Vec<&ui::picker::PickCandidate> = vec![&candidate];

        let with_match = render_picker_frame(
            dir.path(),
            &Vault::default(),
            "run",
            &state,
            &matches,
            false,
        );
        assert!(with_match.contains("file"), "{with_match}");

        let without_match =
            render_picker_frame(dir.path(), &Vault::default(), "run", &state, &[], false);
        assert!(!without_match.contains("file"));
    }
}
