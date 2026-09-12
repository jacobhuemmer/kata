//! Reads `index.jsonl` and the event markdown pointers it names, and tracks per-event
//! idempotency (`docs/design/06-session-mining.md` §2.1, §3.3-3.4). Never reads anything
//! under a session directory other than the event markdown `path` names (§4.1).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::MineHome;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Deserialize)]
pub struct IndexRow {
    pub when: String,
    pub agent: String,
    pub event: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub path: String,
    #[serde(default)]
    pub bytes: u64,
}

pub fn read_index_rows(index_path: &Path) -> Vec<IndexRow> {
    todo!("{index_path:?}")
}

pub fn read_transcript_path(event_markdown_path: &Path) -> Option<String> {
    todo!("{event_markdown_path:?}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedRecord {
    pub event_path: String,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_sha256: Option<String>,
    pub status: String,
}

pub fn load_processed(_home: &MineHome) -> Vec<ProcessedRecord> {
    todo!()
}

pub fn append_processed(_home: &MineHome, record: &ProcessedRecord) -> std::io::Result<()> {
    todo!("{record:?}")
}

pub fn already_processed(processed: &[ProcessedRecord], event_path: &str, bytes: u64) -> bool {
    todo!("{processed:?} {event_path} {bytes}")
}

pub fn resolve_relative(base: &Path, raw: &str) -> PathBuf {
    todo!("{base:?} {raw}")
}
