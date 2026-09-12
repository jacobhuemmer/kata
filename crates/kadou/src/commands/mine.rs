//! `kadou mine` (§6.8, §9 slice 9; docs/design/06-session-mining.md).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kadou_core::KadouPaths;

use super::resolve_paths;

/// `~/Documents/Sessions/index.jsonl`, or `AGENT_SESSION_LEDGER_DIR/index.jsonl` when set
/// (`06` §2.1), or the CLI's own `--index` override.
fn default_sessions_index_path(override_path: Option<String>) -> PathBuf {
    if let Some(path) = override_path {
        return PathBuf::from(path);
    }
    if let Some(dir) = std::env::var_os("AGENT_SESSION_LEDGER_DIR") {
        return PathBuf::from(dir).join("index.jsonl");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Documents/Sessions/index.jsonl")
}

/// `~/.config/kadou/mine/redact-extra.txt`: one denylist term per line, blank lines and `#`
/// comments ignored (`docs/design/05-prd.md` §6.8's path table; `06` §4.2 R9).
fn load_redact_extra(config_dir: &Path) -> Vec<String> {
    let path = config_dir.join("mine/redact-extra.txt");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn mine_config(
    paths: &KadouPaths,
    index_override: Option<String>,
) -> kadou_mine::orchestrate::MineConfig {
    let mut config = kadou_mine::orchestrate::MineConfig::new(
        default_sessions_index_path(index_override),
        paths.state_dir.clone(),
    );
    config.redact_extra = load_redact_extra(&paths.config_dir);
    config.home_prefix = std::env::var("HOME").ok();
    config
}

/// `kadou mine run [--once] [--since ISO] [--index PATH]` (`06` §2-3, §5.1). `--watch` is
/// explicitly optional in `06` §3.2 ("Optional: tail index.jsonl") and is not implemented this
/// slice.
pub fn run_mine_run(
    _once: bool,
    watch: bool,
    since: Option<String>,
    index: Option<String>,
) -> ExitCode {
    if watch {
        eprintln!(
            "error: --watch is not implemented; run with --once (06 §3.2 marks --watch optional)"
        );
        return ExitCode::from(2);
    }
    if since.is_some() {
        // `--since` (06 §5.1 backfill trigger) is accepted for CLI-shape parity but not yet
        // wired to an ingest-layer filter this slice -- every unprocessed row is read
        // regardless (the processed-record checkpoint is what actually bounds repeat work).
        // See the handoff's interpretation calls.
        eprintln!(
            "warning: --since is accepted but not yet a real filter; running the full unprocessed backlog"
        );
    }
    let paths = resolve_paths();
    let config = mine_config(&paths, index);
    let summary = kadou_mine::orchestrate::run_once(&config);
    if summary.already_running {
        println!("already running");
        return ExitCode::SUCCESS;
    }
    println!(
        "events: {} transcripts: {} (missing {}) candidates: {} clusters: {} queued: {}{}",
        summary.events_seen,
        summary.transcripts_seen,
        summary.transcripts_missing,
        summary.candidates_emitted,
        summary.clusters,
        summary.queued,
        if summary.bounded_stop {
            " (bounded stop)"
        } else {
            ""
        },
    );
    ExitCode::SUCCESS
}

/// `kadou mine status` (`06` §5.1): counts only, no command text.
pub fn run_mine_status() -> ExitCode {
    let paths = resolve_paths();
    let home = kadou_mine::store::MineHome::new(&paths.state_dir);
    let queue = kadou_mine::store::list_queue(&home);
    let audit = kadou_mine::store::read_audit(&home);
    let approved = audit.iter().filter(|r| r.action == "approved").count();
    let rejected = audit.iter().filter(|r| r.action == "rejected").count();
    println!(
        "queued: {} approved: {approved} rejected: {rejected}",
        queue.len()
    );
    ExitCode::SUCCESS
}

fn print_queue_row(entry: &kadou_mine::store::QueueEntry) {
    let meta: serde_json::Value = serde_json::from_str(&entry.meta_json).unwrap_or_default();
    let (header, _) = kadou_core::parse_header(&entry.kata_source);
    let about = header.as_ref().map(|h| h.about.as_str()).unwrap_or("");
    let risk = header.as_ref().map(|h| h.risk.as_str()).unwrap_or("?");
    println!(
        "{}  risk={risk}  score={:.2}  sessions={}  {about}",
        entry.fingerprint,
        meta.get("score")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        meta.get("unique_sessions")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    );
}

/// `kadou mine list` (`06` §5.1).
pub fn run_mine_list() -> ExitCode {
    let paths = resolve_paths();
    let home = kadou_mine::store::MineHome::new(&paths.state_dir);
    let queue = kadou_mine::store::list_queue(&home);
    if queue.is_empty() {
        println!("no queued drafts");
        return ExitCode::SUCCESS;
    }
    for entry in &queue {
        print_queue_row(entry);
    }
    ExitCode::SUCCESS
}

/// `kadou mine show <fingerprint>` (`06` §5.1): the full redacted draft and its meta.
pub fn run_mine_show(fingerprint: String) -> ExitCode {
    let paths = resolve_paths();
    let home = kadou_mine::store::MineHome::new(&paths.state_dir);
    match kadou_mine::store::read_queue_entry(&home, &fingerprint) {
        Some(entry) => {
            println!("{}", entry.meta_json);
            println!("---");
            print!("{}", entry.kata_source);
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("error: no queued draft for fingerprint {fingerprint}");
            ExitCode::from(2)
        }
    }
}

/// `kadou mine review [--dump <fp> [--redacted]]` (`06` §2.9, §4.5). Approve is human-CLI-only
/// via the dedicated `mine approve`/`mine reject` subcommands, never from here directly
/// (`06` §4.5 "the design default is approve is human-only").
pub fn run_mine_review(dump: Option<String>, _redacted: bool) -> ExitCode {
    if let Some(fingerprint) = dump {
        return run_mine_show(fingerprint);
    }
    let paths = resolve_paths();
    let home = kadou_mine::store::MineHome::new(&paths.state_dir);
    let queue = kadou_mine::store::list_queue(&home);
    if queue.is_empty() {
        println!("no queued drafts");
        return ExitCode::SUCCESS;
    }
    for entry in &queue {
        print_queue_row(entry);
    }
    println!();
    println!("kadou mine show <fingerprint>              -- see the full redacted draft");
    println!("kadou mine approve <fingerprint>           -- copy into mined/");
    println!("kadou mine reject <fingerprint> --reason …  -- ban this fingerprint");
    ExitCode::SUCCESS
}

/// `kadou mine approve <fingerprint> [--into NAME]` (`06` §2.9; `docs/design/05-prd.md` §6.8
/// "Approve copies the draft into the inactive `mined` staging area").
pub fn run_mine_approve(fingerprint: String, into: Option<String>) -> ExitCode {
    let paths = resolve_paths();
    let home = kadou_mine::store::MineHome::new(&paths.state_dir);
    let name = match into {
        Some(name) => name,
        None => match kadou_mine::store::read_queue_entry(&home, &fingerprint) {
            Some(entry) => {
                let meta: serde_json::Value =
                    serde_json::from_str(&entry.meta_json).unwrap_or_default();
                meta.get("slug")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(&fingerprint)
                    .to_string()
            }
            None => {
                eprintln!("error: no queued draft for fingerprint {fingerprint}");
                return ExitCode::from(2);
            }
        },
    };

    match kadou_mine::store::approve(&home, &fingerprint, &name) {
        Ok(path) => {
            println!("approved -> {}", path.display());
            println!("kadou accept mined/{name} --into <folder>  to make it executable");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// `kadou mine reject <fingerprint> --reason …` (`06` §2.9).
pub fn run_mine_reject(fingerprint: String, reason: String) -> ExitCode {
    let paths = resolve_paths();
    let home = kadou_mine::store::MineHome::new(&paths.state_dir);
    match kadou_mine::store::reject(&home, &fingerprint, &reason) {
        Ok(()) => {
            println!("rejected {fingerprint}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// `kadou mine install-schedule [--load]` (`06` §3.1). Writes the LaunchAgent plist on macOS
/// (loading it only when `--load` is given) and always prints the Linux crontab fallback line
/// so the command's own output stays useful on every platform.
pub fn run_mine_install_schedule(load: bool) -> ExitCode {
    let paths = resolve_paths();
    let Ok(binary) = std::env::current_exe() else {
        eprintln!("error: could not determine the kadou binary's own path");
        return ExitCode::FAILURE;
    };
    let log_path = paths.state_dir.join("mine/logs/launchd.out.log");

    if kadou_mine::schedule::use_launch_agent() {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let launch_agents_dir = home.join("Library/LaunchAgents");
        match kadou_mine::schedule::write_plist(&launch_agents_dir, &binary, &log_path) {
            Ok(path) => {
                println!("wrote {}", path.display());
                if load {
                    let status = std::process::Command::new("launchctl")
                        .arg("load")
                        .arg(&path)
                        .status();
                    match status {
                        Ok(s) if s.success() => println!("loaded {}", kadou_mine::schedule::LABEL),
                        Ok(s) => {
                            eprintln!("error: launchctl load exited with {s}");
                            return ExitCode::FAILURE;
                        }
                        Err(err) => {
                            eprintln!("error: failed to run launchctl: {err}");
                            return ExitCode::FAILURE;
                        }
                    }
                } else {
                    println!("run with --load to launchctl load it, or:");
                    println!("  launchctl load {}", path.display());
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        }
    } else {
        println!("add this to your crontab (crontab -e):");
        println!("{}", kadou_mine::schedule::render_crontab_line(&binary));
        ExitCode::SUCCESS
    }
}
