//! Pending grant records (`docs/design/05-prd.md` §6.4, §5.5): written by `run_kata` when a
//! **visible** high/critical kata is not allow-listed for this agent, read and mutated by
//! `kadou grant *` (the CLI, in `crates/kadou`). One JSON file per record, at
//! `<state_dir>/pending/<pending_id>.json` — plain `0600` text like every other state-dir
//! record, so a human's own tools can read `pending_path` directly (§5.5).
//!
//! `kadou-mcp` owns this store (like [`crate::history`]) even though the CLI is the only
//! *approver*: the MCP server is the only writer, and keeping both sides of the file format in
//! one place is what keeps them from drifting. The `kadou` bin crate depends on `kadou-mcp`
//! already and reuses [`PendingStore`], [`git_head`], and [`crate::history::HistoryStore`]
//! directly for `grant approve`/`grant list`/`grant show`/`grant deny`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use kadou_core::fsutil;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// TTL for a pending grant (§6.4 "the record expires after 24 hours").
pub const TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRecord {
    pub pending_id: String,
    pub id: String,
    pub folder: String,
    pub risk: kadou_core::RiskLevel,
    /// The requested args, never needs (§6.4 "args (needs never included)").
    pub args: BTreeMap<String, String>,
    pub args_hash: String,
    /// `"mcp"` — the only requester kind this slice writes (§6.4 item 1).
    pub requester: String,
    /// Self-reported `clientInfo.name` — a label, not an identity (§6.4).
    pub mcp_client: Option<String>,
    pub timestamp: String,
    pub expires: String,
    /// The kata file's `sha256:...` digest, pinned at request time (§6.4).
    pub sha256: String,
    /// The kata's full source, pinned at request time, so `kadou grant show` can diff it
    /// against the current on-disk source (§6.4 item 4 "a kata diff since request").
    pub source: String,
    /// The folder's git HEAD, pinned at request time, when the folder is git-backed (§6.4).
    pub folder_head: Option<String>,
    /// Set by `kadou grant approve` on success: the run's `history_id`, terminal status
    /// (`success`/`failed`/`cancelled`), and `log_path` (§6.4 item 5).
    pub history_id: Option<String>,
    pub status: Option<String>,
    pub log_path: Option<PathBuf>,
}

/// The short form a human is meant to copy (§5.5/§7.2's own examples show `7c1e`, not a full
/// UUID) — the first 8 characters, unambiguous in practice for how many pending grants exist
/// at once; [`PendingStore::resolve`] (R11) accepts any prefix of at least 4.
pub fn short_pending_id(pending_id: &str) -> &str {
    let end = pending_id
        .char_indices()
        .nth(8)
        .map_or(pending_id.len(), |(i, _)| i);
    &pending_id[..end]
}

/// The result of [`PendingStore::resolve`] (R11): a `pending_id` prefix resolves to exactly
/// one record, no matches, or more than one — an ambiguous prefix names every full id it
/// could mean, so the caller can show them rather than guessing.
#[derive(Debug)]
pub enum PendingLookup {
    Found(Box<PendingRecord>),
    NotFound,
    Ambiguous(Vec<String>),
}

impl PendingRecord {
    pub fn is_expired(&self) -> bool {
        match humantime::parse_rfc3339(&self.expires) {
            Ok(expires) => SystemTime::now() > expires,
            Err(_) => true,
        }
    }

    /// `true` while this record still needs a human decision: not yet approved, and not
    /// expired. Used both for the dedupe rule (§6.4 item 3) and for `grant list`'s status
    /// column.
    pub fn is_outstanding(&self) -> bool {
        self.history_id.is_none() && !self.is_expired()
    }
}

/// A deterministic `sha256:<hex>` of the args map (sorted by `BTreeMap`'s own iteration
/// order), used for the `(id, args_hash)` dedupe rule (§6.4 item 3). Hashes a length-prefixed
/// encoding rather than round-tripping through `serde_json` — this is an opaque dedupe key,
/// not a wire format, and a length prefix on every field means no separator character could
/// ever make two different maps collide (R6: no serialization failure to handle at all).
pub fn args_hash(args: &BTreeMap<String, String>) -> String {
    let mut bytes = Vec::new();
    for (key, value) in args {
        bytes.extend_from_slice(&key.len().to_le_bytes());
        bytes.extend_from_slice(key.as_bytes());
        bytes.extend_from_slice(&value.len().to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    kadou_core::sha256_hex_prefixed(&bytes)
}

pub struct PendingStore {
    state_dir: PathBuf,
}

impl PendingStore {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            state_dir: state_dir.to_path_buf(),
        }
    }

    fn dir(&self) -> PathBuf {
        self.state_dir.join("pending")
    }

    pub fn path(&self, pending_id: &str) -> PathBuf {
        self.dir().join(format!("{pending_id}.json"))
    }

    /// Every pending record on disk, in no particular order. A missing `pending/` directory
    /// scans as empty, not an error (mirrors `scan_kata_dir`'s "a missing kata_dir scans as
    /// empty").
    pub fn list(&self) -> Vec<PendingRecord> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(self.dir()) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(record) = serde_json::from_str(&text)
            {
                out.push(record);
            }
        }
        out
    }

    pub fn get(&self, pending_id: &str) -> Option<PendingRecord> {
        let text = std::fs::read_to_string(self.path(pending_id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Resolves `input` to exactly one record: an exact `pending_id` match first, else an
    /// unambiguous prefix of at least 4 characters (R11, the `git` convention — §7.2's own
    /// examples show a short id like `7c1e`). A prefix under 4 characters is never resolved,
    /// even if it happens to be unambiguous today: as more records accumulate it would start
    /// silently picking whichever one still matches.
    pub fn resolve(&self, input: &str) -> PendingLookup {
        if let Some(record) = self.get(input) {
            return PendingLookup::Found(Box::new(record));
        }
        if input.len() < 4 {
            return PendingLookup::NotFound;
        }
        let matches: Vec<String> = self
            .list()
            .into_iter()
            .filter(|r| r.pending_id.starts_with(input))
            .map(|r| r.pending_id)
            .collect();
        match matches.len() {
            0 => PendingLookup::NotFound,
            1 => self
                .get(&matches[0])
                .map(|r| PendingLookup::Found(Box::new(r)))
                .unwrap_or(PendingLookup::NotFound),
            _ => PendingLookup::Ambiguous(matches),
        }
    }

    /// An outstanding record for the same `(id, args_hash)`, if one already exists — the
    /// dedupe rule (§6.4 item 3: "a second `run_kata` call with the same `(id, args_hash)`
    /// while a pending record is outstanding returns the existing `pending_id`").
    pub fn find_outstanding(&self, id: &str, args_hash: &str) -> Option<PendingRecord> {
        self.list()
            .into_iter()
            .find(|r| r.id == id && r.args_hash == args_hash && r.is_outstanding())
    }

    pub fn save(&self, record: &PendingRecord) -> std::io::Result<()> {
        let text = serde_json::to_string(record).map_err(std::io::Error::other)?;
        fsutil::write_atomic_0600(&self.path(&record.pending_id), text.as_bytes())
    }

    /// `kadou grant deny <pending_id>` (§6.4 item 6: "deletes the record"). Deleting an
    /// already-absent record is not an error.
    pub fn delete(&self, pending_id: &str) -> std::io::Result<()> {
        match std::fs::remove_file(self.path(pending_id)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// Creates and saves a fresh pending record (§6.4 item 1). Callers should check
    /// [`Self::find_outstanding`] first — this always writes a new record.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        id: &str,
        folder: &str,
        risk: kadou_core::RiskLevel,
        args: BTreeMap<String, String>,
        requester: &str,
        mcp_client: Option<&str>,
        sha256: &str,
        source: &str,
        folder_head: Option<&str>,
    ) -> std::io::Result<PendingRecord> {
        let now = SystemTime::now();
        let hash = args_hash(&args);
        let record = PendingRecord {
            pending_id: Uuid::new_v4().to_string(),
            id: id.to_string(),
            folder: folder.to_string(),
            risk,
            args,
            args_hash: hash,
            requester: requester.to_string(),
            mcp_client: mcp_client.map(str::to_string),
            timestamp: humantime::format_rfc3339_seconds(now).to_string(),
            expires: humantime::format_rfc3339_seconds(now + TTL).to_string(),
            sha256: sha256.to_string(),
            source: source.to_string(),
            folder_head: folder_head.map(str::to_string),
            history_id: None,
            status: None,
            log_path: None,
        };
        self.save(&record)?;
        Ok(record)
    }
}

/// The folder's git HEAD (§6.4 "the folder's git HEAD... pinned at request time"), `None` when
/// `folder_dir` is not a git checkout — an imported (non-`kadou get`) folder, for instance.
/// Shells out to the real `git` binary rather than adding a `git2` dependency for one
/// read-only `rev-parse`.
pub fn git_head(folder_dir: &Path) -> Option<String> {
    if !folder_dir.join(".git").exists() {
        return None;
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(folder_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let head = String::from_utf8(output.stdout).ok()?;
    let head = head.trim();
    (!head.is_empty()).then(|| head.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn args_hash_is_stable_for_the_same_args() {
        let a = args(&[("version", "1"), ("cluster", "uat")]);
        let b = args(&[("cluster", "uat"), ("version", "1")]);
        assert_eq!(args_hash(&a), args_hash(&b));
    }

    #[test]
    fn args_hash_differs_for_different_args() {
        let a = args(&[("version", "1")]);
        let b = args(&[("version", "2")]);
        assert_ne!(args_hash(&a), args_hash(&b));
    }

    #[test]
    fn create_then_get_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        let record = store
            .create(
                "sesami/ses-deploy",
                "sesami",
                kadou_core::RiskLevel::Critical,
                args(&[("version", "1")]),
                "mcp",
                Some("claude-code"),
                "sha256:abc",
                "#!/bin/sh\necho hi\n",
                None,
            )
            .unwrap();

        let loaded = store.get(&record.pending_id).unwrap();
        assert_eq!(loaded.id, "sesami/ses-deploy");
        assert_eq!(loaded.sha256, "sha256:abc");
        assert!(loaded.is_outstanding());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(store.path(&record.pending_id))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn resolve_accepts_an_unambiguous_four_char_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        let record = store
            .create(
                "sesami/ses-deploy",
                "sesami",
                kadou_core::RiskLevel::Critical,
                args(&[]),
                "mcp",
                None,
                "sha256:abc",
                "#!/bin/sh\necho hi\n",
                None,
            )
            .unwrap();

        let prefix = &record.pending_id[..4];
        match store.resolve(prefix) {
            PendingLookup::Found(found) => assert_eq!(found.pending_id, record.pending_id),
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn resolve_rejects_an_ambiguous_prefix() {
        // Two records constructed to share the same first four hex characters, by editing
        // the on-disk filename directly (the id itself is a real uuid, but the collision is
        // what resolve() must detect regardless of how it arose).
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        let a = store
            .create(
                "sesami/a",
                "sesami",
                kadou_core::RiskLevel::Critical,
                args(&[]),
                "mcp",
                None,
                "sha256:a",
                "echo a\n",
                None,
            )
            .unwrap();
        let mut b = a.clone();
        b.pending_id = format!("{}extra", a.pending_id);
        b.id = "sesami/b".to_string();
        store.save(&b).unwrap();

        let prefix = &a.pending_id[..8];
        match store.resolve(prefix) {
            PendingLookup::Ambiguous(mut matches) => {
                matches.sort();
                let mut expected = vec![a.pending_id.clone(), b.pending_id];
                expected.sort();
                assert_eq!(matches, expected);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn resolve_requires_at_least_four_characters() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        assert!(matches!(store.resolve("abc"), PendingLookup::NotFound));
    }

    #[test]
    fn resolve_not_found_for_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        assert!(matches!(store.resolve("nope1234"), PendingLookup::NotFound));
    }

    #[test]
    fn find_outstanding_matches_same_id_and_args_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        let created = store
            .create(
                "sesami/ses-deploy",
                "sesami",
                kadou_core::RiskLevel::Critical,
                args(&[("version", "1")]),
                "mcp",
                None,
                "sha256:abc",
                "src",
                None,
            )
            .unwrap();

        let hash = args_hash(&args(&[("version", "1")]));
        let found = store.find_outstanding("sesami/ses-deploy", &hash).unwrap();
        assert_eq!(found.pending_id, created.pending_id);

        let different_hash = args_hash(&args(&[("version", "2")]));
        assert!(
            store
                .find_outstanding("sesami/ses-deploy", &different_hash)
                .is_none()
        );
    }

    #[test]
    fn approved_record_is_no_longer_outstanding() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        let mut record = store
            .create(
                "sesami/ses-deploy",
                "sesami",
                kadou_core::RiskLevel::Critical,
                BTreeMap::new(),
                "mcp",
                None,
                "sha256:abc",
                "src",
                None,
            )
            .unwrap();
        record.history_id = Some("h1".to_string());
        store.save(&record).unwrap();

        assert!(!store.get(&record.pending_id).unwrap().is_outstanding());
        let hash = args_hash(&BTreeMap::new());
        assert!(store.find_outstanding("sesami/ses-deploy", &hash).is_none());
    }

    #[test]
    fn expired_record_is_not_outstanding() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        let mut record = store
            .create(
                "sesami/ses-deploy",
                "sesami",
                kadou_core::RiskLevel::Critical,
                BTreeMap::new(),
                "mcp",
                None,
                "sha256:abc",
                "src",
                None,
            )
            .unwrap();
        record.expires =
            humantime::format_rfc3339_seconds(SystemTime::now() - Duration::from_secs(60))
                .to_string();
        store.save(&record).unwrap();

        let loaded = store.get(&record.pending_id).unwrap();
        assert!(loaded.is_expired());
        assert!(!loaded.is_outstanding());
    }

    #[test]
    fn deny_deletes_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = PendingStore::new(dir.path());
        let record = store
            .create(
                "sesami/ses-deploy",
                "sesami",
                kadou_core::RiskLevel::Critical,
                BTreeMap::new(),
                "mcp",
                None,
                "sha256:abc",
                "src",
                None,
            )
            .unwrap();
        store.delete(&record.pending_id).unwrap();
        assert!(store.get(&record.pending_id).is_none());
        // Deleting again is not an error.
        store.delete(&record.pending_id).unwrap();
    }

    #[test]
    fn git_head_is_none_for_a_non_git_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(git_head(dir.path()).is_none());
    }

    #[test]
    fn git_head_reads_the_real_head_of_a_git_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .expect("git must be on PATH for this test");
            assert!(status.success(), "git {args:?} failed");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "test"]);
        std::fs::write(dir.path().join("x.txt"), "x").unwrap();
        run(&["add", "x.txt"]);
        run(&["commit", "-q", "-m", "init"]);

        let head = git_head(dir.path()).expect("git-backed folder has a HEAD");
        assert_eq!(head.len(), 40, "a full git sha is 40 hex chars: {head}");
    }
}
