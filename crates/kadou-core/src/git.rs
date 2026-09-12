//! Shared git subprocess helpers (`docs/design/05-prd.md` §9 slice 7): one place that shells
//! out to the real `git` binary, wiring `git rev-parse --show-toplevel` detection into a
//! single helper used by `kadou get`/`update`/`remove`, `kadou accept`'s git-checkout
//! refusal, and the pending-record folder-HEAD pin (`crates/kadou-mcp/src/pending.rs`).
//! `--show-toplevel` (not a bare `.git` existence check) is what lets a `--root`-selected
//! subdirectory of a checkout, exposed through a symlink, still be recognized as git-backed
//! even though no `.git` entry sits directly inside it.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("failed to run git: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("git {args} failed: {stderr}")]
    Command { args: String, stderr: String },
}

fn run(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(GitError::Spawn)?;
    if !output.status.success() {
        return Err(GitError::Command {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// `git -C <dir> rev-parse --show-toplevel`, `None` when `dir` is not inside a git working
/// tree at all.
pub fn toplevel(dir: &Path) -> Option<PathBuf> {
    let text = run(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// `true` when `dir` is itself, or lies under, the root of a git working tree — the one check
/// `kadou accept`, `kadou remove`, and [`head`] share to tell a `kadou get` checkout (whose
/// exposed folder may be a `--root` symlink into a subdirectory of the real checkout) from an
/// ordinary user-owned folder.
pub fn is_git_backed(dir: &Path) -> bool {
    let Ok(canonical_dir) = dir.canonicalize() else {
        return false;
    };
    match toplevel(dir).and_then(|top| top.canonicalize().ok()) {
        Some(top) => canonical_dir.starts_with(&top),
        None => false,
    }
}

/// The current commit at `dir`'s `HEAD`, `None` when `dir` is not git-backed.
pub fn head(dir: &Path) -> Option<String> {
    if !is_git_backed(dir) {
        return None;
    }
    run(dir, &["rev-parse", "HEAD"])
        .ok()
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn run(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .expect("git must be on PATH for this test");
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    fn init_repo(dir: &Path) {
        run(dir, &["init", "-q"]);
        run(dir, &["config", "user.email", "test@example.com"]);
        run(dir, &["config", "user.name", "test"]);
    }

    fn commit_all(dir: &Path, message: &str) {
        run(dir, &["add", "-A"]);
        run(dir, &["commit", "-q", "-m", message]);
    }

    #[test]
    fn is_git_backed_is_false_for_a_plain_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!super::is_git_backed(dir.path()));
    }

    #[test]
    fn is_git_backed_is_true_for_a_git_init_directory() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        assert!(super::is_git_backed(dir.path()));
    }

    #[test]
    fn head_is_none_for_a_non_git_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(super::head(dir.path()).is_none());
    }

    #[test]
    fn head_reads_the_real_head_of_a_git_checkout() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("x.txt"), "x").unwrap();
        commit_all(dir.path(), "init");

        let head = super::head(dir.path()).expect("git-backed dir has a HEAD");
        assert_eq!(head.len(), 40, "a full git sha is 40 hex chars: {head}");
    }

    #[test]
    fn has_local_changes_is_false_on_a_freshly_committed_checkout() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("x.txt"), "x").unwrap();
        commit_all(dir.path(), "init");

        assert!(!super::has_local_changes(dir.path()));
    }

    #[test]
    fn has_local_changes_is_true_with_an_untracked_file() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("x.txt"), "x").unwrap();
        commit_all(dir.path(), "init");

        std::fs::write(dir.path().join("untracked.txt"), "y").unwrap();
        assert!(super::has_local_changes(dir.path()));
    }

    #[test]
    fn has_local_changes_is_true_with_a_modified_tracked_file() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("x.txt"), "x").unwrap();
        commit_all(dir.path(), "init");

        std::fs::write(dir.path().join("x.txt"), "changed").unwrap();
        assert!(super::has_local_changes(dir.path()));
    }

    #[test]
    fn has_local_changes_is_false_for_a_non_git_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!super::has_local_changes(dir.path()));
    }
}
