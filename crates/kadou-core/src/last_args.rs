//! Last-used args store (`docs/design/05-prd.md` §6.6 D5, §9 slice 4).
//!
//! "Interactive-prefill only": `kadou run <id>` on a TTY, missing a required arg, shows the
//! last-used value as the prompt default. That prompting UI is slice 8; this slice ships the
//! write (on a successful CLI run) and a read API, per §9 slice 4's scope note. Values here
//! are always plain and non-secret — a need's value never belongs here (the vault owns
//! that).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::fsutil;

#[derive(Debug, thiserror::Error)]
pub enum LastArgsError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize last-used args: {0}")]
    Serialize(#[source] serde_json::Error),
}

/// `~/.local/state/kadou/last/` (§6.6).
pub struct LastArgsStore {
    dir: PathBuf,
}

impl LastArgsStore {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            dir: state_dir.join("last"),
        }
    }

    /// One kata's last-used args live at `<dir>/<id>.json`; a project-local id's leading
    /// `./` is stripped so it doesn't leave a stray `.` path component on disk.
    fn path(&self, kata_id: &str) -> PathBuf {
        let id = kata_id.strip_prefix("./").unwrap_or(kata_id);
        self.dir.join(format!("{id}.json"))
    }

    /// Reads the last-used args for `kata_id`. A missing or corrupt file reads as empty —
    /// this is a prefill convenience, never load-bearing (§6.6).
    pub fn read(&self, kata_id: &str) -> BTreeMap<String, String> {
        let path = self.path(kata_id);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return BTreeMap::new();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Writes the last-used args for `kata_id`, atomically (§6.6, §3.1 `tempfile`). Called
    /// on a successful CLI run (§9 slice 4 "write on successful CLI run").
    pub fn write(
        &self,
        kata_id: &str,
        args: &BTreeMap<String, String>,
    ) -> Result<(), LastArgsError> {
        let path = self.path(kata_id);
        let text = serde_json::to_string(args).map_err(LastArgsError::Serialize)?;
        fsutil::write_atomic_0600(&path, text.as_bytes())
            .map_err(|source| LastArgsError::Write { path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_args_for_a_kata_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = LastArgsStore::new(dir.path());

        let mut args = BTreeMap::new();
        args.insert("branch".to_string(), "release/1.2".to_string());
        store.write("sesami/cc4-aaa", &args).unwrap();

        assert_eq!(store.read("sesami/cc4-aaa"), args);
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = LastArgsStore::new(dir.path());
        assert!(store.read("starter/hello").is_empty());
    }

    #[test]
    fn project_local_id_does_not_leave_a_stray_dot_component() {
        let dir = tempfile::tempdir().unwrap();
        let store = LastArgsStore::new(dir.path());
        let mut args = BTreeMap::new();
        args.insert("x".to_string(), "y".to_string());
        store.write("./deploy", &args).unwrap();

        assert!(dir.path().join("last/deploy.json").is_file());
        assert_eq!(store.read("./deploy"), args);
    }
}
