//! Shared git subprocess helpers (`docs/design/05-prd.md` §9 slice 7): one place that shells
//! out to the real `git` binary, wiring `git rev-parse --show-toplevel` detection into a
//! single helper used by `kadou get`/`update`/`remove`, `kadou accept`'s git-checkout
//! refusal, and the pending-record folder-HEAD pin (`crates/kadou-mcp/src/pending.rs`).
//! `--show-toplevel` (not a bare `.git` existence check) is what lets a `--root`-selected
//! subdirectory of a checkout, exposed through a symlink, still be recognized as git-backed
//! even though no `.git` entry sits directly inside it.

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
}
