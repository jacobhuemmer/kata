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
            std::fs::create_dir_all(parent)?;
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

/// Creates `dir` (and its parents) if missing, then sets mode `0700` on `dir` itself.
pub fn ensure_dir_0700(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
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
}
