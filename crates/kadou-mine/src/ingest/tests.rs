//! Ingest tests (`docs/design/06-session-mining.md` §2.1, §3.3-3.4). Synthetic index/markdown
//! fixtures only -- the real fixture files used by the end-to-end pipeline test live under
//! `tests/fixtures/sessions/`.

use super::*;

const EVENT_MD: &str = r#"# Agent session archive — session_end

- **When:** 2026-09-01T00:00:00Z
- **Agent:** grok
- **Event:** session_end
- **Session:** `s1`
- **Project:** demo
- **CWD:** `/repo`
- **Transcript:** `transcripts/s1.jsonl`

## Summary
(empty)

## Local sources
- `transcripts/s1.jsonl`
"#;

#[test]
fn reads_valid_index_rows_and_skips_malformed_lines() {
    let dir = tempfile::tempdir().unwrap();
    let index_path = dir.path().join("index.jsonl");
    std::fs::write(
        &index_path,
        concat!(
            r#"{"when":"2026-09-01T00:00:00Z","agent":"grok","event":"session_end","sessionId":"s1","path":"a.md","bytes":100}"#,
            "\n",
            "not json\n",
            r#"{"when":"2026-09-04T00:00:00Z","agent":"codex","event":"session_end","sessionId":"s2","path":"b.md","bytes":200}"#,
            "\n",
        ),
    )
    .unwrap();

    let rows = read_index_rows(&index_path);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].session_id, "s1");
    assert_eq!(rows[1].agent, "codex");
}

#[test]
fn a_missing_index_file_reads_as_empty() {
    let rows = read_index_rows(Path::new("/nonexistent/index.jsonl"));
    assert!(rows.is_empty());
}

#[test]
fn extracts_the_transcript_path_from_event_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let md_path = dir.path().join("event.md");
    std::fs::write(&md_path, EVENT_MD).unwrap();

    let transcript = read_transcript_path(&md_path).unwrap();
    assert_eq!(transcript, "transcripts/s1.jsonl");
}

#[test]
fn a_markdown_file_without_a_transcript_line_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let md_path = dir.path().join("event.md");
    std::fs::write(&md_path, "# Agent session archive\n\n- **When:** 2026-09-01T00:00:00Z\n").unwrap();

    assert!(read_transcript_path(&md_path).is_none());
}

#[test]
fn processed_records_round_trip_and_detect_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let home = MineHome::new(dir.path());

    assert!(load_processed(&home).is_empty());
    append_processed(
        &home,
        &ProcessedRecord {
            event_path: "a.md".to_string(),
            bytes: 100,
            transcript_sha256: None,
            status: "ok".to_string(),
        },
    )
    .unwrap();

    let processed = load_processed(&home);
    assert_eq!(processed.len(), 1);
    assert!(already_processed(&processed, "a.md", 100));
    assert!(!already_processed(&processed, "a.md", 101));
    assert!(!already_processed(&processed, "b.md", 100));
}

#[test]
fn resolve_relative_keeps_an_absolute_path_and_joins_a_relative_one() {
    let base = Path::new("/home/x/Sessions");
    assert_eq!(
        resolve_relative(base, "events/a.md"),
        PathBuf::from("/home/x/Sessions/events/a.md")
    );
    assert_eq!(
        resolve_relative(base, "/already/absolute.md"),
        PathBuf::from("/already/absolute.md")
    );
}
