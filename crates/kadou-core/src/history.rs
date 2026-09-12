//! History records with the fresh plain-text log tier (`docs/design/05-prd.md` §6.6, §9 slice
//! 5 "History write path with the fresh plain-text tier and stream redaction — every MCP run
//! gets an audit record from the start.").
//!
//! Fresh-tier logs (`~/.local/state/kadou/history/logs/<date>/<uuid>.log`) are plain `0600`
//! text files from the moment a run begins — an agent's own file tools can open `log_path`
//! immediately, even while the run is still in flight. JSON metadata records
//! (`~/.local/state/kadou/history/records/<uuid>.json`) carry the fields §6.6 lists; archival
//! into 7-day-aged gzip-tar tiers and the 90-day/50MB retention sweep are out of scope for
//! this slice (no code path here produces a log old enough to age out yet).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::fsutil;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub history_id: String,
    pub id: String,
    pub folder: String,
    pub args: BTreeMap<String, String>,
    /// `running | success | failed | cancelled | pending_grant` (§6.6).
    pub status: String,
    pub exit_code: Option<i32>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_ms: Option<u64>,
    pub output_lines: Option<usize>,
    pub output_summary: Option<String>,
    pub log_path: PathBuf,
    /// `cli | mcp` (§6.6, no `tui` value).
    pub interface: String,
    pub initiator: String,
    pub mcp_client: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize a history record: {0}")]
    Serialize(#[source] serde_json::Error),
}

fn now_rfc3339() -> String {
    humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string()
}

/// The username running the server, or `"local"` when it can't be determined (§6.6
/// "`initiator` (username if known, else `local`)").
pub fn current_initiator() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "local".to_string())
}

pub struct HistoryStore {
    state_dir: PathBuf,
}

impl HistoryStore {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            state_dir: state_dir.to_path_buf(),
        }
    }

    fn logs_dir(&self) -> PathBuf {
        self.state_dir.join("history/logs")
    }

    fn records_dir(&self) -> PathBuf {
        self.state_dir.join("history/records")
    }

    fn record_path(&self, history_id: &str) -> PathBuf {
        self.records_dir().join(format!("{history_id}.json"))
    }

    /// Starts a new record: allocates a `history_id`, creates an empty (but real, `0600`)
    /// fresh-tier log file, and writes the initial `status: running` record.
    pub fn begin(
        &self,
        id: &str,
        folder: &str,
        args: &BTreeMap<String, String>,
        interface: &str,
        initiator: &str,
        mcp_client: Option<&str>,
    ) -> Result<HistoryRecord, HistoryError> {
        let history_id = Uuid::new_v4().to_string();
        let date = humantime::format_rfc3339_seconds(std::time::SystemTime::now())
            .to_string()
            .split('T')
            .next()
            .unwrap_or("1970-01-01")
            .to_string();
        let log_path = self
            .logs_dir()
            .join(&date)
            .join(format!("{history_id}.log"));

        fsutil::write_atomic_0600(&log_path, b"").map_err(|source| HistoryError::Write {
            path: log_path.clone(),
            source,
        })?;

        let record = HistoryRecord {
            history_id,
            id: id.to_string(),
            folder: folder.to_string(),
            args: args.clone(),
            status: "running".to_string(),
            exit_code: None,
            start_time: now_rfc3339(),
            end_time: None,
            duration_ms: None,
            output_lines: None,
            output_summary: None,
            log_path,
            interface: interface.to_string(),
            initiator: initiator.to_string(),
            mcp_client: mcp_client.map(str::to_string),
        };
        self.write_record(&record)?;
        Ok(record)
    }

    fn write_record(&self, record: &HistoryRecord) -> Result<(), HistoryError> {
        let path = self.record_path(&record.history_id);
        let text = serde_json::to_string(record).map_err(HistoryError::Serialize)?;
        fsutil::write_atomic_0600(&path, text.as_bytes())
            .map_err(|source| HistoryError::Write { path, source })
    }

    /// Finishes a record: writes the full redacted output to the fresh-tier log file, updates
    /// the record's terminal fields, and rewrites the record JSON.
    pub fn finish(
        &self,
        record: &mut HistoryRecord,
        outcome: FinishOutcome<'_>,
    ) -> Result<(), HistoryError> {
        fsutil::write_atomic_0600(&record.log_path, outcome.redacted_output.as_bytes()).map_err(
            |source| HistoryError::Write {
                path: record.log_path.clone(),
                source,
            },
        )?;

        record.status = outcome.status.to_string();
        record.exit_code = outcome.exit_code;
        record.end_time = Some(now_rfc3339());
        record.duration_ms = Some(outcome.duration_ms);
        record.output_lines = Some(outcome.output_lines);
        record.output_summary = Some(outcome.output_summary.to_string());
        self.write_record(record)
    }

    /// The newest `limit` records, newest first (`docs/design/05-prd.md` §7.1 `kadou history
    /// [--limit N]`, §9 slice 8). A record file that fails to read or parse is skipped rather
    /// than aborting the whole listing; a missing `records/` directory (a fresh home) lists as
    /// empty.
    pub fn list_recent(&self, limit: usize) -> Vec<HistoryRecord> {
        todo!()
    }
}

/// The terminal fields [`HistoryStore::finish`] needs, bundled to keep the call site (and
/// clippy's `too_many_arguments`) sane.
pub struct FinishOutcome<'a> {
    pub status: &'a str,
    pub exit_code: Option<i32>,
    pub redacted_output: &'a str,
    pub output_lines: usize,
    pub output_summary: &'a str,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_creates_a_real_plain_text_log_file_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path());
        let record = store
            .begin(
                "starter/hello",
                "starter",
                &BTreeMap::new(),
                "mcp",
                "local",
                Some("claude-code"),
            )
            .unwrap();

        assert!(record.log_path.is_file());
        assert_eq!(record.status, "running");
        assert_eq!(std::fs::read_to_string(&record.log_path).unwrap(), "");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&record.log_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn finish_writes_the_log_and_updates_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path());
        let mut record = store
            .begin(
                "starter/hello",
                "starter",
                &BTreeMap::new(),
                "mcp",
                "local",
                None,
            )
            .unwrap();

        store
            .finish(
                &mut record,
                FinishOutcome {
                    status: "success",
                    exit_code: Some(0),
                    redacted_output: "hello, world",
                    output_lines: 1,
                    output_summary: "hello, world",
                    duration_ms: 42,
                },
            )
            .unwrap();

        assert_eq!(record.status, "success");
        assert_eq!(record.exit_code, Some(0));
        assert_eq!(
            std::fs::read_to_string(&record.log_path).unwrap(),
            "hello, world"
        );

        let record_path = dir
            .path()
            .join("history/records")
            .join(format!("{}.json", record.history_id));
        let saved: HistoryRecord =
            serde_json::from_str(&std::fs::read_to_string(&record_path).unwrap()).unwrap();
        assert_eq!(saved.status, "success");
        assert_eq!(saved.duration_ms, Some(42));
    }

    #[test]
    fn list_recent_returns_newest_first_and_respects_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path());
        for i in 0..3 {
            let mut record = store
                .begin(
                    &format!("starter/k{i}"),
                    "starter",
                    &BTreeMap::new(),
                    "cli",
                    "local",
                    None,
                )
                .unwrap();
            // Distinct start_time values, strictly increasing, so newest-first ordering is
            // unambiguous regardless of how fast this loop runs.
            record.start_time = format!("2026-01-01T00:00:0{i}Z");
            store.write_record(&record).unwrap();
        }

        let recent = store.list_recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, "starter/k2");
        assert_eq!(recent[1].id, "starter/k1");
    }

    #[test]
    fn list_recent_on_a_fresh_home_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path());
        assert!(store.list_recent(20).is_empty());
    }
}
