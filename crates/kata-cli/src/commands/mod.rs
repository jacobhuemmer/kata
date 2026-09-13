//! Real implementations of the slice-2 (`list`, `check`, `import`) and slice-3/4 (`run`,
//! `show`, `vault`) commands — the rest of the command tree in `main.rs` stays a stub until
//! its own slice lands (`docs/design/05-prd.md` §9).
//!
//! One module per command family (`docs/design/12-mvp-review.md` §6 Later-6/L5): this file
//! holds the infra every family shares (path/config/vault loading, the human-ceiling and
//! confirm gates, the shared run-and-report tail `run`/`grant approve` both call into) plus
//! the bare-frame family itself, since it composites rows from the folder/grant/draft
//! families below rather than belonging to any single one of them.

use std::collections::BTreeMap;
use std::io::IsTerminal as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use kata_core::history::current_initiator;
use kata_core::runner::{RunOneRequest, run_one_blocking};
use kata_core::{
    Config, GoVaultSource, Kata, KataPaths, LastArgsStore, LookupResult, RiskLevel, Vault,
    VaultStore,
};
use kata_mcp::pending::{self, PendingRecord, PendingStore};

use crate::confirm::{self, ConfirmOutcome};
use crate::starter;
use crate::ui;

mod check;
mod completion;
mod edit;
mod folder;
mod grant;
mod history;
mod import;
mod list;
mod mcp;
mod mine;
mod new;
mod picker;
mod run;
mod show;
mod vault;

pub use check::run_check;
pub use completion::run_completion;
pub use edit::run_edit;
pub use folder::{run_accept, run_get, run_remove, run_update};
pub use grant::{
    run_grant_allow, run_grant_approve, run_grant_deny, run_grant_list, run_grant_show,
};
pub use history::run_history;
pub use import::run_import;
pub use list::run_list;
pub use mcp::{run_mcp_schema, run_mcp_serve};
pub use mine::{
    run_mine_approve, run_mine_install_schedule, run_mine_list, run_mine_reject, run_mine_review,
    run_mine_run, run_mine_show, run_mine_skip, run_mine_status,
};
pub use new::run_new;
pub use run::run_run;
pub use show::run_show;
pub use vault::{run_vault_list, run_vault_rm, run_vault_set};

/// Resolves [`KataPaths`] from the real process environment, printing a fatal error and
/// exiting on failure. `KATA_HOME` isolates every path for tests (§7.6).
fn resolve_paths() -> KataPaths {
    match kata_core::discover() {
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

fn load_config(paths: &KataPaths) -> Config {
    Config::load(&paths.config_file()).unwrap_or_else(|err| {
        eprintln!(
            "warning: failed to load {}: {err}; using defaults",
            paths.config_file().display()
        );
        Config::default()
    })
}

fn vault_store(paths: &KataPaths) -> VaultStore {
    VaultStore::new(&paths.data_dir)
}

/// Auto-imports a legacy Go `~/.dops` vault into kadou's own, once (§6.5, §7.4 "Go import:
/// ... or auto-import on first vault use" — this worker's interpretation call, noted in the
/// handoff: auto-import beat a separate `kata vault import` subcommand). A no-op when
/// `$HOME` can't be resolved, the marker already exists, or `~/.dops` is incomplete.
fn auto_import_go_vault(store: &VaultStore) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let source = GoVaultSource::under_home(Path::new(&home));
    match store.import_go_vault_once(&source) {
        Ok(Some(summary)) => {
            eprintln!(
                "imported {} secret(s) from ~/.dops (dropped {} catalog-scoped value(s); see kata vault list)",
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
fn load_vault(paths: &KataPaths) -> Vault {
    let store = vault_store(paths);
    auto_import_go_vault(&store);
    store.load().unwrap_or_else(|err| {
        eprintln!("warning: failed to load the vault: {err}");
        Vault::default()
    })
}

/// Materializes any missing starter kata (decision D6, §7.4, §9 slice 6): called by every
/// command that scans kata (`list`, `check`, `run`, `show`, `mcp serve`, `grant`, and the bare
/// `kata` frame in `main.rs`) so a home is never missing the starter kata just because some
/// other folder got there first. A failure here is a warning, not fatal — the command that
/// called it can still make progress against whatever is already on disk.
fn materialize_starter(paths: &KataPaths) {
    if let Err(err) = starter::materialize_if_needed(&paths.kata_dir()) {
        eprintln!(
            "warning: failed to materialize the starter kata into {}: {err}",
            paths.kata_dir().display()
        );
    }
}

/// `styled` from injected TTY/`NO_COLOR`/`--plain` state -- the one call site every command
/// that prints a frame goes through (`ui::style::use_color`'s own doc comment).
fn styled_for_stdout(plain: bool) -> bool {
    ui::style::use_color(
        std::io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").is_some(),
        plain,
    )
}

/// "4m ago"'s input: how long ago `rfc3339` was, saturating to zero on a clock skew or a
/// malformed timestamp rather than erroring -- this is a display convenience (§7.2), never
/// load-bearing.
fn elapsed_since(rfc3339: &str) -> Duration {
    humantime::parse_rfc3339(rfc3339)
        .ok()
        .and_then(|then| SystemTime::now().duration_since(then).ok())
        .unwrap_or_default()
}

/// One folder's row for the bare frame: kata filtered to the human ceiling (§6.2 [CLI]), and
/// health from `kata check`'s own error count plus whether the folder is a git checkout
/// (§9 slice 8 "folder health (git ✓ or ✗ N errors)").
fn build_folder_row(
    kata_dir: &Path,
    name: &str,
    ceiling: RiskLevel,
    vault: &Vault,
) -> ui::frame::FolderRow {
    let error_count = kata_core::check_folder(kata_dir, name, vault)
        .map(|report| report.error_count())
        .unwrap_or(0);
    let git_backed = kata_core::git::is_git_backed(&kata_dir.join(name));
    let prefix = format!("{name}/");
    let kata = match kata_core::scan_folder(kata_dir, name) {
        Ok(files) => files
            .iter()
            .filter_map(|file| {
                let header = file.header.as_ref()?;
                if header.risk > ceiling {
                    return None;
                }
                Some(ui::frame::KataRow {
                    name: file
                        .id
                        .strip_prefix(&prefix)
                        .unwrap_or(&file.id)
                        .to_string(),
                    risk: header.risk,
                    about: header.about.clone(),
                })
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    ui::frame::FolderRow {
        name: name.to_string(),
        git_backed,
        error_count,
        kata,
    }
}

/// The "needs you" grant rows: every still-outstanding pending record (§6.4 item 2), oldest
/// first, the same order `kata grant list` uses.
fn build_grant_rows(paths: &KataPaths) -> Vec<ui::frame::GrantRow> {
    let store = PendingStore::new(&paths.state_dir);
    let mut records: Vec<PendingRecord> = store
        .list()
        .into_iter()
        .filter(PendingRecord::is_outstanding)
        .collect();
    records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    records
        .iter()
        .map(|record| ui::frame::GrantRow {
            short_id: pending::short_pending_id(&record.pending_id).to_string(),
            kata_id: record.id.clone(),
            args: render_args(&record.args),
            client: record
                .mcp_client
                .clone()
                .unwrap_or_else(|| "agent".to_string()),
            elapsed: elapsed_since(&record.timestamp),
        })
        .collect()
}

/// The "needs you" draft rows: every waiting `proposed/`/`mined/` draft (§6.7). A proposed
/// draft's fix line is `kata accept <id>`; a mined draft has no folder of its own yet, so its
/// fix line sends a human to `kata mine review` instead (§7.2's own worked example).
fn build_draft_rows(paths: &KataPaths) -> Vec<ui::frame::DraftRow> {
    kata_mcp::scan_drafts(&paths.state_dir)
        .into_iter()
        .map(|draft| {
            let action = match draft.id.strip_prefix("proposed/") {
                Some(rest) => format!("kata accept {rest}"),
                None => "kata mine review".to_string(),
            };
            ui::frame::DraftRow {
                id: draft.id,
                action,
            }
        })
        .collect()
}

/// `kata` with no arguments (§7.2, `09` §3.2, §9 slice 8): the library frame with folder
/// health, the `needs you` block only when a grant or draft is waiting, and one kata per line
/// tab-separated when piped.
pub fn run_bare(plain: bool) -> ExitCode {
    let paths = resolve_paths();
    materialize_starter(&paths);
    let config = load_config(&paths);
    let vault = load_vault(&paths);
    let kata_dir = paths.kata_dir();

    let scanned = match kata_core::scan_kata_dir(&kata_dir) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    let folders = scanned
        .iter()
        .map(|(name, _)| {
            let ceiling = kata_core::visibility::human_ceiling(&config, name);
            build_folder_row(&kata_dir, name, ceiling, &vault)
        })
        .collect();
    let frame = ui::frame::BareFrame {
        folders,
        needs_you: ui::frame::NeedsYou {
            grants: build_grant_rows(&paths),
            drafts: build_draft_rows(&paths),
        },
    };

    if !std::io::stdout().is_terminal() {
        print!("{}", ui::frame::render_bare_piped(&frame));
        return ExitCode::SUCCESS;
    }
    print!(
        "{}",
        ui::frame::render_bare(&frame, styled_for_stdout(plain))
    );
    ExitCode::SUCCESS
}

/// The human ceiling still applies regardless of the confirm protocol (§6.2, §6.3 "The human
/// ceiling still applies"): a kata above `max_risk`/`[folder.<f>] max_risk` never runs, no
/// matter what `--confirm` or a TTY prompt says. `--dry-run` never spawns, so it bypasses this
/// too (§6.1).
fn check_human_ceiling(kata: &Kata, config: &Config) -> Result<(), ExitCode> {
    let folder = kata.id.split('/').next().unwrap_or(&kata.id);
    let ceiling = kata_core::visibility::human_ceiling(config, folder);
    if kata.risk > ceiling {
        eprintln!(
            "error: {} is {} risk, above the max_risk ceiling {ceiling} for folder {folder}",
            kata.id, kata.risk
        );
        eprintln!("  = raise it with `max_risk` (or `[folder.{folder}] max_risk`) in kata.toml");
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

/// Looks up `id` under `kata_dir`, printing a fatal CLI error and returning `Err(exit code)`
/// on any failure (no such kata, or a kata whose header failed to load) so callers can just
/// `return` the code on `Err`.
fn find_kata_or_report(kata_dir: &Path, id: &str) -> Result<Kata, ExitCode> {
    match kata_core::find_kata(kata_dir, id) {
        Ok(LookupResult::Found(kata)) => Ok(kata),
        Ok(LookupResult::NotFound) => {
            eprintln!("error: no such kata `{id}`");
            eprintln!("  = kata list, or kata new {id}");
            Err(ExitCode::from(2))
        }
        Ok(LookupResult::Invalid { diagnostics }) => {
            eprintln!("error: `{id}` has a header error and cannot run");
            for diag in &diagnostics {
                eprintln!("  {}", diag.message);
            }
            eprintln!("  = kata check {}", id.split('/').next().unwrap_or(id));
            Err(ExitCode::from(2))
        }
        Err(err) => {
            eprintln!("error: {err}");
            Err(ExitCode::FAILURE)
        }
    }
}

fn render_args(args: &BTreeMap<String, String>) -> String {
    args.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds the `RunOneRequest` and runs it to completion — the one shared shape `kata run` and
/// `kata grant approve` both call into (§6.4 item 5 "Approval runs in the human CLI's
/// environment (full parent env, not the MCP server's allowlisted one)" applies to both).
fn run_one_cli(
    paths: &KataPaths,
    config: &Config,
    kata: &Kata,
    folder: &str,
    resolved_args: &[kata_core::ResolvedVar],
    resolved_needs: &[kata_core::ResolvedNeed],
    mcp_client: Option<&str>,
) -> Result<kata_core::runner::RunReport, kata_core::runner::RunOneError> {
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
/// last-used args on success (§6.6 D5). Shared shape between `kata run` and `kata grant
/// approve`'s own report handling.
fn print_run_report_and_exit_code(
    report: &kata_core::runner::RunReport,
    kata: &Kata,
    resolved_args: &[kata_core::ResolvedVar],
    config: &Config,
    state_dir: &Path,
    styled: bool,
) -> ExitCode {
    for line in &report.output {
        println!("{line}");
    }
    let history_short = pending::short_pending_id(&report.history_id);
    let duration = Duration::from_millis(report.duration_ms);
    match report.status {
        kata_exec::RunStatus::Success => {
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
            print!(
                "{}",
                ui::frame::run_footer(&kata.id, true, 0, duration, history_short, styled)
            );
            ExitCode::SUCCESS
        }
        kata_exec::RunStatus::Failed => {
            print!(
                "{}",
                ui::frame::run_footer(
                    &kata.id,
                    false,
                    report.exit_code.unwrap_or(1),
                    duration,
                    history_short,
                    styled
                )
            );
            ExitCode::FAILURE
        }
        kata_exec::RunStatus::TimedOut => {
            let timeout = kata_exec::effective_timeout(kata.timeout, config.exec.timeout);
            eprintln!("error: {} timed out after {timeout:?}", kata.id);
            ExitCode::FAILURE
        }
        kata_exec::RunStatus::Cancelled => {
            eprintln!("cancelled");
            ExitCode::from(130)
        }
    }
}

/// The frame printed just before spawning (§9 slice 8, `09` §3.4): the resolved args and
/// whether every need is satisfied, from the same data `run_one_cli` is about to use. Shared
/// between `kata run` and `kata grant approve`, which prints the identical frame before its
/// own one-shot spawn.
fn run_frame_header(
    kata: &Kata,
    resolved_args: &[kata_core::ResolvedVar],
    resolved_needs: &[kata_core::ResolvedNeed],
    styled: bool,
) -> String {
    let args: Vec<(String, String)> = resolved_args
        .iter()
        .map(|arg| (arg.name.clone(), arg.value.clone()))
        .collect();
    let needs_satisfied = (!resolved_needs.is_empty())
        .then(|| resolved_needs.iter().all(|need| need.value.is_some()));
    ui::frame::run_header(&kata.id, kata.risk, &args, needs_satisfied, styled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_under(dir: &Path) -> KataPaths {
        KataPaths {
            config_dir: dir.join(".config/kata"),
            data_dir: dir.join(".local/share/kata"),
            state_dir: dir.join(".local/state/kata"),
        }
    }

    #[test]
    fn elapsed_since_parses_rfc3339_and_saturates_on_garbage() {
        let now = humantime::format_rfc3339_seconds(SystemTime::now()).to_string();
        assert!(elapsed_since(&now) < Duration::from_secs(5));

        let ten_seconds_ago =
            humantime::format_rfc3339_seconds(SystemTime::now() - Duration::from_secs(10))
                .to_string();
        let elapsed = elapsed_since(&ten_seconds_ago);
        assert!(
            elapsed >= Duration::from_secs(9) && elapsed <= Duration::from_secs(15),
            "expected roughly 10s, got {elapsed:?}"
        );

        assert_eq!(elapsed_since("not a timestamp"), Duration::default());
    }

    #[test]
    fn styled_for_stdout_is_false_off_a_tty_regardless_of_plain() {
        // assert_cmd/cargo test never give a unit test a real TTY either, so both arguments
        // must agree with `ui::style::use_color(false, _, _)`: always false.
        assert!(!styled_for_stdout(false));
        assert!(!styled_for_stdout(true));
    }

    #[test]
    fn build_folder_row_filters_kata_above_the_ceiling_and_names_them_without_the_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
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

        let row = build_folder_row(&kata_dir, "ops", RiskLevel::Medium, &Vault::default());
        assert_eq!(row.kata.len(), 1);
        assert_eq!(row.kata[0].name, "low");
        assert!(!row.git_backed);
        assert_eq!(row.error_count, 0);
    }

    #[test]
    fn build_folder_row_at_the_ceiling_boundary_is_still_visible() {
        // Pins the `>` (not `>=`) boundary: a kata exactly at the ceiling is visible (§6.2).
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        std::fs::create_dir_all(kata_dir.join("ops")).unwrap();
        std::fs::write(
            kata_dir.join("ops/at-ceiling.sh"),
            "#!/bin/sh\n# ---\n# about: At ceiling\n# risk:  high\n# ---\necho hi\n",
        )
        .unwrap();

        let row = build_folder_row(&kata_dir, "ops", RiskLevel::High, &Vault::default());
        assert_eq!(row.kata.len(), 1);
    }

    #[test]
    fn build_folder_row_reports_check_errors() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        std::fs::create_dir_all(kata_dir.join("broken")).unwrap();
        std::fs::write(
            kata_dir.join("broken/x.sh"),
            "#!/bin/sh\n# ---\n# risk:  mediun\n# ---\necho hi\n",
        )
        .unwrap();

        let row = build_folder_row(&kata_dir, "broken", RiskLevel::Critical, &Vault::default());
        assert!(row.error_count > 0);
    }

    fn sample_kata(id: &str, risk: RiskLevel) -> Kata {
        Kata {
            id: id.to_string(),
            path: std::path::PathBuf::from(format!("{id}.sh")),
            about: "About".to_string(),
            risk,
            needs: Vec::new(),
            args: Vec::new(),
            alias: Vec::new(),
            timeout: None,
            notes: None,
            shebang: None,
        }
    }

    #[test]
    fn run_frame_header_reports_needs_satisfaction_only_when_the_kata_has_any() {
        let kata = sample_kata("ops/x", RiskLevel::Low);
        let args = vec![kata_core::ResolvedVar {
            name: "branch".to_string(),
            env_name: "BRANCH".to_string(),
            value: "main".to_string(),
        }];

        let no_needs = run_frame_header(&kata, &args, &[], false);
        assert!(!no_needs.contains("needs"));
        assert!(no_needs.contains("branch=main"));

        let satisfied = kata_core::ResolvedNeed {
            name: "token".to_string(),
            env_name: "TOKEN".to_string(),
            value: Some("x".to_string()),
            secret: true,
        };
        let with_satisfied_need =
            run_frame_header(&kata, &args, std::slice::from_ref(&satisfied), false);
        assert!(with_satisfied_need.contains("needs  ✓ vault"));

        let missing = kata_core::ResolvedNeed {
            name: "token".to_string(),
            env_name: "TOKEN".to_string(),
            value: None,
            secret: false,
        };
        let with_missing_need = run_frame_header(&kata, &args, &[missing], false);
        assert!(with_missing_need.contains("needs  ✗ missing"));
    }

    #[test]
    fn build_grant_rows_includes_only_outstanding_records_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_under(dir.path());
        let store = PendingStore::new(&paths.state_dir);

        let first = store
            .create(
                "sesami/a",
                "sesami",
                RiskLevel::Critical,
                BTreeMap::new(),
                "mcp",
                Some("claude-code"),
                "sha256:aaa",
                "source-a",
                None,
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        let _second = store
            .create(
                "sesami/b",
                "sesami",
                RiskLevel::High,
                BTreeMap::new(),
                "mcp",
                None,
                "sha256:bbb",
                "source-b",
                None,
            )
            .unwrap();
        // A third, already-approved record must not appear (`is_outstanding` is false once a
        // history_id is set).
        let mut approved = store
            .create(
                "sesami/c",
                "sesami",
                RiskLevel::High,
                BTreeMap::new(),
                "mcp",
                None,
                "sha256:ccc",
                "source-c",
                None,
            )
            .unwrap();
        approved.history_id = Some("done".to_string());
        store.save(&approved).unwrap();

        let rows = build_grant_rows(&paths);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kata_id, "sesami/a");
        assert_eq!(rows[1].kata_id, "sesami/b");
        assert_eq!(
            rows[1].client, "agent",
            "no mcp_client falls back to 'agent'"
        );
        assert_eq!(
            rows[0].short_id,
            pending::short_pending_id(&first.pending_id)
        );
    }

    #[test]
    fn build_draft_rows_maps_proposed_to_accept_and_mined_to_mine_review() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_under(dir.path());
        std::fs::create_dir_all(paths.state_dir.join("proposed/ops")).unwrap();
        std::fs::write(
            paths.state_dir.join("proposed/ops/argocd-sync.sh"),
            "#!/bin/sh\n# ---\n# about: Sync\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();
        std::fs::create_dir_all(paths.state_dir.join("mined")).unwrap();
        std::fs::write(
            paths.state_dir.join("mined/k8s-pod-logs.sh"),
            "#!/bin/sh\n# ---\n# about: Logs\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();

        let mut rows = build_draft_rows(&paths);
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "mined/k8s-pod-logs");
        assert_eq!(rows[0].action, "kata mine review");
        assert_eq!(rows[1].id, "proposed/ops/argocd-sync");
        assert_eq!(rows[1].action, "kata accept ops/argocd-sync");
    }
}
