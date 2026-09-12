//! End-to-end pipeline test against the synthetic fixture cluster under
//! `tests/fixtures/sessions/` (mirrors `docs/design/06-session-mining.md` §6's own worked
//! example: three sessions, two agents, one repeated `kubectl` automation, plus a fourth
//! Claude session whose transcript is missing on disk). Exercises ingest -> parse -> extract
//! -> normalize -> cluster -> rank -> redact -> propose -> queue in one call, then re-runs to
//! prove idempotency (`06` §3.4), and asserts the §4.1 never-written list holds on every file
//! this run produced.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use kadou_mine::orchestrate::{MineConfig, run_once};
use kadou_mine::store::MineHome;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sessions")
}

/// Strings that must never appear in anything this crate writes to disk (`06` §4.1): the
/// synthetic-but-realistic-shaped values from the fixture transcripts, and the raw JSON/tool
/// envelope shapes those transcripts are made of.
const NEVER_WRITTEN: [&str; 8] = [
    "eks-dev",
    "eks-prod",
    "payments",
    "billing",
    "tool_calls",
    "run_terminal_command",
    "CommandExecution",
    "parsed_cmd",
];

fn assert_never_written(root: &std::path::Path) {
    for entry in walk(root) {
        if !entry.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&entry) else {
            continue;
        };
        for banned in NEVER_WRITTEN {
            assert!(
                !text.contains(banned),
                "{} contains forbidden text `{banned}`:\n{text}",
                entry.display()
            );
        }
    }
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn assert_summary_matches_the_worked_example(summary: &kadou_mine::orchestrate::RunSummary) {
    assert!(!summary.already_running);
    assert!(!summary.bounded_stop);
    assert_eq!(summary.events_seen, 4, "{summary:?}");
    assert_eq!(summary.transcripts_seen, 3, "{summary:?}");
    assert_eq!(summary.transcripts_missing, 1, "{summary:?}");
    assert_eq!(summary.candidates_emitted, 3, "{summary:?}");
    assert_eq!(summary.clusters, 1, "{summary:?}");
    assert_eq!(summary.queued, 1, "{summary:?}");
}

fn assert_queued_draft_is_valid(home: &MineHome) {
    let queue = kadou_mine::store::list_queue(home);
    assert_eq!(queue.len(), 1);
    let entry = &queue[0];

    let (header, diagnostics) = kadou_core::parse_header(&entry.kata_source);
    assert!(header.is_some(), "invalid header: {diagnostics:?}");
    let header = header.unwrap();
    assert_eq!(header.risk, kadou_core::RiskLevel::Medium);
    assert!(!header.args.is_empty());
    let context_arg = header.args.iter().find(|a| a.name == "context").unwrap();
    assert!(context_arg.is_required());

    let meta: serde_json::Value = serde_json::from_str(&entry.meta_json).unwrap();
    assert_eq!(meta["unique_sessions"], 3);
    assert_eq!(meta["unique_agents"], 2);
}

#[test]
fn fixture_cluster_of_three_synthetic_sessions_proposes_one_valid_draft() {
    let state_dir = tempfile::tempdir().unwrap();
    let config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );

    let summary = run_once(&config);

    assert_summary_matches_the_worked_example(&summary);
    assert_queued_draft_is_valid(&MineHome::new(state_dir.path()));
    assert_never_written(state_dir.path());
}

#[test]
fn a_second_run_against_unchanged_input_queues_nothing_new() {
    let state_dir = tempfile::tempdir().unwrap();
    let config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );

    let first = run_once(&config);
    assert_eq!(first.queued, 1);

    let second = run_once(&config);
    assert_eq!(second.events_seen, 0, "{second:?}");
    assert_eq!(second.candidates_emitted, 0, "{second:?}");
    assert_eq!(second.queued, 0, "{second:?}");

    let home = MineHome::new(state_dir.path());
    assert_eq!(kadou_mine::store::list_queue(&home).len(), 1);
}

#[test]
fn a_low_byte_bound_stops_reading_mid_transcript_and_the_next_run_resumes() {
    // 06 §2.1/§3.5: the miner must stream a transcript and stop mid-file once the run's
    // scanned-bytes bound is hit, rather than slurping the whole file first (D2/D7, B2). A
    // bound this low (well under the fixture's first transcript line) proves the stop happens
    // before even one line is fully read, without needing a real multi-gigabyte transcript.
    let state_dir = tempfile::tempdir().unwrap();
    let mut bounded_config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );
    bounded_config.max_bytes_scanned = 100;

    let first = run_once(&bounded_config);
    assert!(!first.already_running);
    assert!(first.bounded_stop, "{first:?}");
    assert_eq!(first.transcripts_seen, 1, "{first:?}");
    assert_eq!(first.candidates_emitted, 0, "{first:?}");
    assert_eq!(first.queued, 0, "{first:?}");

    // The bounded row was never checkpointed as processed, so a second run under the normal
    // (unbounded) config re-reads it from the start and resumes exactly where the first left
    // off -- reaching the same end state a single unbounded run would.
    let unbounded_config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );
    let second = run_once(&unbounded_config);
    assert_summary_matches_the_worked_example(&second);
}

#[test]
fn missing_claude_transcript_is_recorded_as_a_skip_not_a_failure() {
    let state_dir = tempfile::tempdir().unwrap();
    let config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );

    let summary = run_once(&config);
    assert_eq!(summary.transcripts_missing, 1);

    let home = MineHome::new(state_dir.path());
    let processed = kadou_mine::ingest::load_processed(&home);
    let claude_row = processed
        .iter()
        .find(|r| r.event_path.contains("claude__demo__s4"))
        .unwrap();
    assert_eq!(claude_row.status, "skip: transcript_missing");
}

#[test]
fn approve_then_accept_is_the_only_path_to_executable() {
    let state_dir = tempfile::tempdir().unwrap();
    let config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );
    run_once(&config);

    let home = MineHome::new(state_dir.path());
    let entry = &kadou_mine::store::list_queue(&home)[0];
    let fingerprint = entry.fingerprint.clone();

    let mined_path = kadou_mine::store::approve(&home, &fingerprint, "k8s-pod-logs").unwrap();
    assert!(mined_path.is_file());
    assert_eq!(mined_path, state_dir.path().join("mined/k8s-pod-logs.sh"));

    // The draft is now describable/listable as `mined/k8s-pod-logs` by kadou-mcp's existing,
    // unmodified `drafts.rs` (this crate has no MCP types -- §9 slice 9): it scans
    // `state_dir/mined/` exactly like `state_dir/proposed/`.
    let kata_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(kata_dir.path().join("ops")).unwrap();
    // `kadou accept mined/<name> --into ops` is `kadou-mcp::drafts::prepare_accept` +
    // `apply_accept`; exercised end-to-end in `crates/kadou-mcp`'s own tests. Here we only
    // confirm the file approve() produced is exactly what that path expects to find.
    let source = std::fs::read_to_string(&mined_path).unwrap();
    assert!(source.starts_with("#!/bin/sh"));

    assert_never_written(state_dir.path());
}

#[test]
fn a_held_lock_makes_run_once_report_already_running_and_process_nothing() {
    // 06 §3.4 rule 5: a concurrent run must exit immediately rather than double-process the
    // same input. A fresh lock file at MineHome::lock_path() is indistinguishable from one a
    // live run just wrote.
    let state_dir = tempfile::tempdir().unwrap();
    let config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );
    let home = MineHome::new(state_dir.path());
    std::fs::create_dir_all(home.lock_path().parent().unwrap()).unwrap();
    std::fs::write(home.lock_path(), b"").unwrap();

    let summary = run_once(&config);
    assert!(summary.already_running, "{summary:?}");
    assert_eq!(
        summary,
        kadou_mine::orchestrate::RunSummary {
            already_running: true,
            ..Default::default()
        }
    );
}
