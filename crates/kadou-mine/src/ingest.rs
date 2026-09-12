//! Reads `index.jsonl` and the event markdown pointers it names, and tracks per-event
//! idempotency (`docs/design/06-session-mining.md` §2.1, §3.3-3.4). Never reads anything
//! under a session directory other than the event markdown `path` names (§4.1).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::MineHome;

#[cfg(test)]
mod tests;

/// One row of `index.jsonl` (`06` §1.1).
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

/// Parses `index.jsonl`: one JSON object per line. A malformed line is skipped, not an error
/// -- the file is append-only and a partially-written last line can be in flight.
pub fn read_index_rows(index_path: &Path) -> Vec<IndexRow> {
    let Ok(text) = std::fs::read_to_string(index_path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Recovers `transcriptPath` from one event markdown file's `- **Transcript:** \`<path>\``
/// line (`06` §1.3, §2.1 step 3). The other fields the doc lists (`agent`, `event`,
/// `sessionId`, `when`) are already on the [`IndexRow`] this markdown file's own index row
/// carries, so this crate reads the markdown only for the one pointer index.jsonl doesn't
/// have.
pub fn read_transcript_path(event_markdown_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(event_markdown_path).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("- **Transcript:**") {
            let inner = rest.trim().trim_matches('`');
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    None
}

/// One row of `checkpoints/processed.jsonl` (`06` §3.3): the idempotent unit is the event
/// file's `(path, bytes)` (a mtime check would need a second stat syscall for no real gain
/// here, since the archive never rewrites a finished event file in place).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedRecord {
    pub event_path: String,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_sha256: Option<String>,
    /// `"ok"` or `"skip: transcript_missing"` (`06` §2.1 step 4).
    pub status: String,
}

/// Every processed record on disk; a missing/unreadable file reads as empty (a fresh home has
/// processed nothing yet).
pub fn load_processed(home: &MineHome) -> Vec<ProcessedRecord> {
    std::fs::read_to_string(home.processed_path())
        .into_iter()
        .flat_map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Appends one record to `checkpoints/processed.jsonl`, creating its directory as needed.
pub fn append_processed(home: &MineHome, record: &ProcessedRecord) -> std::io::Result<()> {
    let path = home.processed_path();
    if let Some(parent) = path.parent() {
        kadou_core::fsutil::ensure_dir_0700(parent)?;
    }
    let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
    existing.push_str(&serde_json::to_string(record).unwrap_or_default());
    existing.push('\n');
    kadou_core::fsutil::write_atomic_0600(&path, existing.as_bytes())
}

/// `06` §3.4 rule 1: same event path + bytes -> skip.
pub fn already_processed(processed: &[ProcessedRecord], event_path: &str, bytes: u64) -> bool {
    processed
        .iter()
        .any(|r| r.event_path == event_path && r.bytes == bytes)
}

/// Resolves `raw` (as it appeared in `index.jsonl`'s `path`, or an event markdown's
/// `Transcript:` line) against `base` when it isn't already absolute -- fixtures and a real
/// `~/Documents/Sessions` both use paths relative to the ledger root in practice.
pub fn resolve_relative(base: &Path, raw: &str) -> PathBuf {
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    }
}
