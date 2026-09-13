//! End-to-end probe for D10 (`docs/design/12-mvp-review.md` §3): a synthetic session
//! transcript containing a *written* `.sh` file -- not a shell invocation -- must still reach
//! the same cluster/rank/redact/propose pipeline and come out as a queued draft. Three Claude
//! sessions each write an identical `scripts/pod-logs.sh` body via a `Write` tool call; no
//! `Bash` tool call appears anywhere in these fixtures.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use kata_mine::orchestrate::{MineConfig, run_once};
use kata_mine::store::MineHome;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/script_file_sessions")
}

#[test]
fn a_written_sh_file_across_three_sessions_is_mined_into_one_queued_draft() {
    let state_dir = tempfile::tempdir().unwrap();
    let config = MineConfig::new(
        fixtures_dir().join("index.jsonl"),
        state_dir.path().to_path_buf(),
    );

    let summary = run_once(&config);

    assert!(!summary.already_running);
    assert!(!summary.bounded_stop);
    assert_eq!(summary.events_seen, 3, "{summary:?}");
    assert_eq!(summary.transcripts_seen, 3, "{summary:?}");
    assert_eq!(summary.candidates_emitted, 3, "{summary:?}");
    assert_eq!(summary.clusters, 1, "{summary:?}");
    assert_eq!(summary.queued, 1, "{summary:?}");

    let home = MineHome::new(state_dir.path());
    let queue = kata_mine::store::list_queue(&home);
    assert_eq!(queue.len(), 1);
    let entry = &queue[0];

    // The draft's two steps came from the written script's body, split into lines by
    // extract::script_command_lines -- proving the ScriptFile kind actually reached propose,
    // not just extract/cluster.
    assert!(entry.kata_source.contains("kubectl"));
    assert!(entry.kata_source.contains("get pods"));
    assert!(entry.kata_source.contains("logs"));

    let (header, diagnostics) = kata_core::parse_header(&entry.kata_source);
    assert!(header.is_some(), "invalid header: {diagnostics:?}");

    let meta: serde_json::Value = serde_json::from_str(&entry.meta_json).unwrap();
    assert_eq!(meta["unique_sessions"], 3);
}
