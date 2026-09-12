//! On-disk state: the queue (`~/.local/state/kadou/mine/queue/<fingerprint>/`), the
//! append-only audit log, and the review actions that move a queued draft toward the `mined`
//! staging namespace (`docs/design/05-prd.md` §6.8; `docs/design/06-session-mining.md` §3.3,
//! §4.6). Every write goes through [`kadou_core::fsutil`]'s atomic-write/0600/0700 helpers.

use std::path::{Path, PathBuf};

use kadou_core::sha256_hex_prefixed;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// XDG-rooted paths this crate writes under (`docs/design/05-prd.md` §6.8's path table).
/// `mined_dir` is the *existing* draft namespace `crates/kadou-mcp/src/drafts.rs` already
/// scans (`~/.local/state/kadou/mined/`) -- this crate only ever copies an approved draft's
/// source into it by filename; it never depends on kadou-mcp's types (§9 slice 9 "no MCP
/// types").
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

    pub fn processed_path(&self) -> PathBuf {
        self.mine_dir.join("checkpoints/processed.jsonl")
    }

    pub fn redaction_failures_path(&self) -> PathBuf {
        self.mine_dir.join("redaction-failures.jsonl")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.mine_dir.join("mine.lock")
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

fn io_err(action: &'static str, path: &Path, source: std::io::Error) -> StoreError {
    StoreError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

/// Writes `queue/<fingerprint>/{meta.json,kata.sh}` (`06` §2.8, §3.3). Re-proposing the same
/// fingerprint overwrites the prior draft in place (idempotency rule 3: rank/score may be
/// recomputed without creating a new proposal).
pub fn write_queue_entry(
    home: &MineHome,
    fingerprint: &str,
    kata_source: &str,
    meta_json: &str,
) -> Result<(), StoreError> {
    let dir = home.entry_dir(fingerprint);
    kadou_core::fsutil::ensure_dir_0700(&dir).map_err(|e| io_err("create", &dir, e))?;
    let kata_path = dir.join("kata.sh");
    let meta_path = dir.join("meta.json");
    kadou_core::fsutil::write_atomic_0600(&kata_path, kata_source.as_bytes())
        .map_err(|e| io_err("write", &kata_path, e))?;
    kadou_core::fsutil::write_atomic_0600(&meta_path, meta_json.as_bytes())
        .map_err(|e| io_err("write", &meta_path, e))?;
    Ok(())
}

/// Every fingerprint currently under `queue/`, regardless of review status (`06` §3.4 rule 2:
/// "same cluster fingerprint already in `queue/` in any state ... do not re-queue").
pub fn queued_fingerprints(home: &MineHome) -> Vec<String> {
    std::fs::read_dir(home.queue_dir())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

/// Reads one queue entry back, or `None` if either file is missing/unreadable.
pub fn read_queue_entry(home: &MineHome, fingerprint: &str) -> Option<QueueEntry> {
    let dir = home.entry_dir(fingerprint);
    let kata_source = std::fs::read_to_string(dir.join("kata.sh")).ok()?;
    let meta_json = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    Some(QueueEntry {
        fingerprint: fingerprint.to_string(),
        meta_json,
        kata_source,
    })
}

/// Every queue entry, fingerprint order (`06` §5.1 `dops mine list`).
pub fn list_queue(home: &MineHome) -> Vec<QueueEntry> {
    let mut fingerprints = queued_fingerprints(home);
    fingerprints.sort();
    fingerprints
        .into_iter()
        .filter_map(|fp| read_queue_entry(home, &fp))
        .collect()
}

/// One row of `audit.jsonl` (`06` §4.6): append-only, `0600`, and -- by construction, since
/// this struct has no field for it -- never a command.
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

fn now_rfc3339() -> String {
    humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string()
}

/// Appends one row to `audit.jsonl`, creating the file at mode `0600` if it doesn't exist yet
/// (`06` §4.6).
pub fn append_audit(home: &MineHome, row: &AuditRow) -> Result<(), StoreError> {
    let path = home.audit_log_path();
    let line = serde_json::to_string(row).unwrap_or_default();
    kadou_core::fsutil::append_line_0600(&path, &line).map_err(|e| io_err("write", &path, e))
}

/// Every audit row on disk, oldest first. A missing or unreadable log reads as empty (a fresh
/// home has never audited anything yet).
pub fn read_audit(home: &MineHome) -> Vec<AuditRow> {
    std::fs::read_to_string(home.audit_log_path())
        .into_iter()
        .flat_map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The most recent action recorded for `fingerprint`, or `None` if it has never been audited.
pub fn latest_action(rows: &[AuditRow], fingerprint: &str) -> Option<String> {
    rows.iter()
        .rfind(|r| r.fingerprint == fingerprint)
        .map(|r| r.action.clone())
}

#[derive(Debug, thiserror::Error)]
pub enum ApproveError {
    #[error("no queued draft for fingerprint {0}")]
    NotFound(String),
    #[error("fingerprint {0} was already {1}")]
    AlreadyDecided(String, String),
    #[error(
        "invalid --into name `{0}`; expected one id segment matching ^[a-z0-9][a-z0-9-]*$, not a path"
    )]
    InvalidName(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Copies a queued draft's source into the `mined` staging namespace as `<name>.sh` and
/// records `approved` in the audit log (`06` §2.9, §4.6; `docs/design/05-prd.md` §6.8
/// "Approve copies the draft into the inactive `mined` staging area"). Refuses a fingerprint
/// that was already approved or rejected (idempotency rule 2) rather than silently
/// overwriting a human decision.
///
/// `name` is checked against [`kadou_core::valid_id_segment`] *before* it is ever joined onto
/// `mined_dir` (B4/D16, `docs/design/12-mvp-review.md` §3, §6): `--into` is human-typed input
/// that used to reach `mined_dir().join(format!("{name}.sh"))` verbatim, so
/// `--into ../../../.config/kadou/kata/sesami/pwn` wrote an executable kata straight into the
/// library, skipping `kadou accept`'s diff-and-confirm review gate entirely.
pub fn approve(home: &MineHome, fingerprint: &str, name: &str) -> Result<PathBuf, ApproveError> {
    if !kadou_core::valid_id_segment(name) {
        return Err(ApproveError::InvalidName(name.to_string()));
    }

    let rows = read_audit(home);
    if let Some(action) = latest_action(&rows, fingerprint)
        && (action == "approved" || action == "rejected")
    {
        return Err(ApproveError::AlreadyDecided(
            fingerprint.to_string(),
            action,
        ));
    }
    let entry = read_queue_entry(home, fingerprint)
        .ok_or_else(|| ApproveError::NotFound(fingerprint.to_string()))?;

    let target = home.mined_dir().join(format!("{name}.sh"));
    kadou_core::fsutil::write_atomic_0600(&target, entry.kata_source.as_bytes())
        .map_err(|e| io_err("write", &target, e))?;

    append_audit(
        home,
        &AuditRow {
            when: now_rfc3339(),
            action: "approved".to_string(),
            fingerprint: fingerprint.to_string(),
            actor: "user".to_string(),
            reason: None,
            catalog_id: Some(format!("mined.{name}")),
            script_sha256: Some(sha256_hex_prefixed(entry.kata_source.as_bytes())),
        },
    )?;

    Ok(target)
}

/// Records `skipped` and leaves the draft exactly as queued (`06` §2.9 "skip: leave queued";
/// D11, `docs/design/12-mvp-review.md` §3). Unlike `approve`/`reject`, this is never terminal:
/// it does not check or record any prior decision, so a fingerprint can be skipped any number
/// of times and still be approved or rejected later.
pub fn skip(home: &MineHome, fingerprint: &str) -> Result<(), ApproveError> {
    if read_queue_entry(home, fingerprint).is_none() {
        return Err(ApproveError::NotFound(fingerprint.to_string()));
    }
    append_audit(
        home,
        &AuditRow {
            when: now_rfc3339(),
            action: "skipped".to_string(),
            fingerprint: fingerprint.to_string(),
            actor: "user".to_string(),
            reason: None,
            catalog_id: None,
            script_sha256: None,
        },
    )?;
    Ok(())
}

/// Records `rejected` with a reason; the fingerprint is banned from re-queueing until
/// `--force` is used by a future run (`06` §2.9, §3.4 rule 2).
pub fn reject(home: &MineHome, fingerprint: &str, reason: &str) -> Result<(), ApproveError> {
    let rows = read_audit(home);
    if let Some(action) = latest_action(&rows, fingerprint)
        && (action == "approved" || action == "rejected")
    {
        return Err(ApproveError::AlreadyDecided(
            fingerprint.to_string(),
            action,
        ));
    }
    if read_queue_entry(home, fingerprint).is_none() {
        return Err(ApproveError::NotFound(fingerprint.to_string()));
    }
    append_audit(
        home,
        &AuditRow {
            when: now_rfc3339(),
            action: "rejected".to_string(),
            fingerprint: fingerprint.to_string(),
            actor: "user".to_string(),
            reason: Some(reason.to_string()),
            catalog_id: None,
            script_sha256: None,
        },
    )?;
    Ok(())
}
