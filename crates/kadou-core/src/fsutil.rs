//! Shared atomic-write and permission helpers for the vault and last-used args stores
//! (`docs/design/05-prd.md` §6.5 "Directory modes: vault and keys directories 0700, files
//! 0600", §6.6 "History and pending directories 0700, files 0600"). `config.rs` predates
//! this module and keeps its own copy of the same write-to-temp/fsync/rename pattern.

use std::io::{self, Write as _};
use std::path::Path;

/// Atomically writes `contents` to `path` at mode `0600` (write-to-temp, fsync, rename —
/// `tempfile`, §3.1), creating parent directories as needed.
pub fn write_atomic_0600(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            ensure_dir_0700(parent)?;
            parent
        }
        _ => Path::new("."),
    };

    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(contents)?;
    tmp.as_file().sync_all()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }

    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Creates `dir` (and its parents) if missing, then sets mode `0700` on `dir` itself *and*
/// every intermediate `create_dir_all` created along the way (§6.6 "History and pending
/// directories 0700" means every directory on that path -- `create_dir_all` makes
/// intermediates the caller never names directly, e.g. `history/` and `history/logs/` on the
/// way to `history/logs/<date>`). `dir` is always chmodded, whether or not it already
/// existed -- the original, still-relied-on contract (`VaultStore`'s data directory is
/// sometimes `dir` itself with no subdirectory to create). Only a pre-existing *ancestor* of
/// `dir` -- not `dir` itself -- is left untouched: this call owns `dir` and whatever it had to
/// create to reach it, nothing further up.
pub fn ensure_dir_0700(dir: &Path) -> io::Result<()> {
    let mut existing_ancestor = dir.parent();
    while let Some(candidate) = existing_ancestor {
        if candidate.exists() {
            break;
        }
        existing_ancestor = candidate.parent();
    }
    let existing_ancestor = existing_ancestor.map(Path::to_path_buf);

    std::fs::create_dir_all(dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut created = Some(dir);
        while let Some(current) = created {
            if Some(current) == existing_ancestor.as_deref() {
                break;
            }
            std::fs::set_permissions(current, std::fs::Permissions::from_mode(0o700))?;
            created = current.parent();
        }
    }
    Ok(())
}

/// Appends `line` (plus a trailing newline) to `path`, creating the file at mode `0600` and
/// its parent directory at `0700` if either is missing (§6.6/§6.8's append-only
/// checkpoint/audit logs). A true append -- O(1) in the file's existing size, unlike the
/// read-whole-file/push-one-line/rewrite-whole-file cycle every append-only log in
/// `kadou-mine` used before (12-mvp-review L8: `processed.jsonl` is the hot path, one append
/// per index row, and Mason's real ledger has thousands).
pub fn append_line_0600(path: &Path, line: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir_0700(parent)?;
    }

    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt as _;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)?
    };
    #[cfg(not(unix))]
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_0600_creates_parents_and_sets_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deep/file.txt");
        write_atomic_0600(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        assert_eq!(mode(&path), 0o600);
        // §6.6: history/pending/proposed/last directories are 0700, not the process umask.
        assert_eq!(mode(path.parent().unwrap()), 0o700);
    }

    #[test]
    #[cfg(unix)]
    fn ensure_dir_0700_sets_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/c");
        ensure_dir_0700(&path).unwrap();
        assert!(path.is_dir());
        assert_eq!(mode(&path), 0o700);
    }

    #[test]
    #[cfg(unix)]
    fn ensure_dir_0700_sets_mode_on_every_intermediate_it_creates() {
        // B3/P1 (`docs/design/12-mvp-review.md` §2, §6): create_dir_all makes every
        // intermediate component, but only the leaf was ever chmodded -- a and a/b were left
        // at the process umask. history/ and history/logs/ are exactly this shape in
        // `HistoryStore`: intermediates on the way to history/records and
        // history/logs/<date>.
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("a/b/c");
        ensure_dir_0700(&path).unwrap();
        assert_eq!(mode(&path), 0o700, "leaf");
        assert_eq!(mode(&root.path().join("a/b")), 0o700, "intermediate b");
        assert_eq!(mode(&root.path().join("a")), 0o700, "intermediate a");
    }

    #[test]
    #[cfg(unix)]
    fn append_line_0600_creates_the_file_0600_and_its_parent_0700() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/log.jsonl");
        append_line_0600(&path, "first").unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
    }

    #[test]
    fn append_line_0600_is_a_true_append_not_a_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.jsonl");
        append_line_0600(&path, "one").unwrap();
        append_line_0600(&path, "two").unwrap();
        append_line_0600(&path, "three").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\nthree\n");
    }

    #[test]
    #[cfg(unix)]
    fn ensure_dir_0700_never_touches_an_ancestor_it_did_not_create() {
        use std::os::unix::fs::PermissionsExt as _;

        // The pre-existing tempdir root is not this call's to chmod -- only components it
        // actually creates should change mode.
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_dir_0700(&root.path().join("a")).unwrap();
        assert_eq!(mode(root.path()), 0o755, "pre-existing ancestor untouched");
        assert_eq!(mode(&root.path().join("a")), 0o700);
    }
}
