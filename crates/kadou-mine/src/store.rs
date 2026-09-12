//! On-disk state: the queue (`~/.local/state/kadou/mine/queue/<fingerprint>/`), the
//! append-only audit log, and the review actions that move a queued draft toward the `mined`
//! staging namespace (`docs/design/05-prd.md` §6.8; `docs/design/06-session-mining.md` §3.3,
//! §4.6). Every write goes through [`kadou_core::fsutil`]'s atomic-write/0600/0700 helpers.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

pub struct MineHome {
    mine_dir: PathBuf,
    mined_dir: PathBuf,
}

impl MineHome {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            mine_dir: state_dir.join("mine"),
            mined_dir: state_dir.join("mined"),
        }
    }

    pub fn queue_dir(&self) -> PathBuf {
        self.mine_dir.join("queue")
    }

    pub fn entry_dir(&self, fingerprint: &str) -> PathBuf {
        self.queue_dir().join(fingerprint)
    }

    pub fn audit_log_path(&self) -> PathBuf {
        self.mine_dir.join("audit.jsonl")
    }

    pub fn mined_dir(&self) -> &Path {
        &self.mined_dir
    }
}

#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub fingerprint: String,
    pub meta_json: String,
    pub kata_source: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("failed to {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn write_queue_entry(
    _home: &MineHome,
    fingerprint: &str,
    kata_source: &str,
    meta_json: &str,
) -> Result<(), StoreError> {
    todo!("{fingerprint} {kata_source} {meta_json}")
}

pub fn read_queue_entry(_home: &MineHome, fingerprint: &str) -> Option<QueueEntry> {
    todo!("{fingerprint}")
}

pub fn list_queue(_home: &MineHome) -> Vec<QueueEntry> {
    todo!()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRow {
    pub when: String,
    pub action: String,
    pub fingerprint: String,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_sha256: Option<String>,
}

pub fn append_audit(_home: &MineHome, row: &AuditRow) -> Result<(), StoreError> {
    todo!("{row:?}")
}

pub fn read_audit(_home: &MineHome) -> Vec<AuditRow> {
    todo!()
}

pub fn latest_action(rows: &[AuditRow], fingerprint: &str) -> Option<String> {
    todo!("{rows:?} {fingerprint}")
}

#[derive(Debug, thiserror::Error)]
pub enum ApproveError {
    #[error("no queued draft for fingerprint {0}")]
    NotFound(String),
    #[error("fingerprint {0} was already {1}")]
    AlreadyDecided(String, String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub fn approve(_home: &MineHome, fingerprint: &str, name: &str) -> Result<PathBuf, ApproveError> {
    todo!("{fingerprint} {name}")
}

pub fn reject(_home: &MineHome, fingerprint: &str, reason: &str) -> Result<(), ApproveError> {
    todo!("{fingerprint} {reason}")
}
