//! `kadou get`/`update`/`remove`: folder-level git install (`docs/design/05-prd.md` §9 slice
//! 7, §4.1 on-disk layout, §4.2 identity/reserved names). Pure validation plus the real git
//! and filesystem calls; `kadou`'s `commands.rs` is the thin CLI wrapper that prompts and
//! prints these results.

use std::path::{Path, PathBuf};

use crate::git::{self, GitError};

/// Top-level names under `kata/` that `kadou get`/`kadou remove` never touch (§4.2).
pub const RESERVED_FOLDER_NAMES: [&str; 3] = ["starter", "proposed", "mined"];

#[derive(Debug, thiserror::Error)]
pub enum FolderError {
    #[error("`{0}` is a reserved name (starter, proposed, mined); pass --as with another name")]
    ReservedName(String),
    #[error("`{0}` is not a valid folder name; folder names match ^[a-z0-9][a-z0-9-]*$")]
    InvalidName(String),
    #[error("{} already exists; kadou get refuses to overwrite an existing folder", .0.display())]
    AlreadyExists(PathBuf),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("failed to {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// `^[a-z0-9][a-z0-9-]*$` — the same segment shape a folder name's own id segment must match
/// (§4.2).
fn is_valid_folder_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The folder name `kadou get` defaults to when `--as` is absent: the URL's last path
/// segment (scp-like `host:path` included), minus a trailing `.git`.
fn folder_name_from_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or(trimmed);
    last.strip_suffix(".git").unwrap_or(last).to_string()
}

fn validate_folder_name(name: &str) -> Result<(), FolderError> {
    if RESERVED_FOLDER_NAMES.contains(&name) {
        return Err(FolderError::ReservedName(name.to_string()));
    }
    if !is_valid_folder_name(name) {
        return Err(FolderError::InvalidName(name.to_string()));
    }
    Ok(())
}

fn io_err(action: &'static str, path: &Path, source: std::io::Error) -> FolderError {
    FolderError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

/// `kadou get <url> [--as <folder>] [--ref <git_ref>] [--root <sub>]` (§7.1, §6.7, §4.1).
/// Clones straight into `kata_dir/<folder>` when `root` is absent; `root` support (a symlink
/// into a subdirectory of a hidden checkout) lands alongside its own tests later this slice.
pub fn get_folder(
    kata_dir: &Path,
    url: &str,
    as_folder: Option<&str>,
    git_ref: Option<&str>,
    root: Option<&str>,
) -> Result<PathBuf, FolderError> {
    let folder = as_folder
        .map(str::to_string)
        .unwrap_or_else(|| folder_name_from_url(url));
    validate_folder_name(&folder)?;
    let _ = root;

    let target = kata_dir.join(&folder);
    if target.exists() {
        return Err(FolderError::AlreadyExists(target));
    }
    std::fs::create_dir_all(kata_dir).map_err(|source| io_err("create", kata_dir, source))?;

    git::clone(url, &target, git_ref)?;
    Ok(target)
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

    /// A local bare fixture repo with one commit at its root (D4: "kata sit at the repo
    /// root"), no network -- §9 slice 7's test list.
    fn bare_fixture_with_one_commit(root: &Path) -> std::path::PathBuf {
        let bare = root.join("origin.git");
        std::fs::create_dir_all(&bare).unwrap();
        run(&bare, &["init", "-q", "--bare", "-b", "main"]);

        let seed = root.join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        run(&seed, &["init", "-q", "-b", "main"]);
        run(&seed, &["config", "user.email", "test@example.com"]);
        run(&seed, &["config", "user.name", "test"]);
        std::fs::write(
            seed.join("hello.sh"),
            "#!/bin/sh\n# ---\n# about: Say hello\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();
        run(&seed, &["add", "-A"]);
        run(&seed, &["commit", "-q", "-m", "init"]);
        run(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
        run(&seed, &["push", "-q", "origin", "main"]);
        bare
    }

    fn bare_fixture_url(root: &Path) -> String {
        format!("file://{}", bare_fixture_with_one_commit(root).display())
    }

    #[test]
    fn get_folder_clones_the_fixture_with_kata_at_its_root() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());

        let target = super::get_folder(kata_dir.path(), &url, Some("team"), None, None).unwrap();

        assert_eq!(target, kata_dir.path().join("team"));
        assert!(target.join("hello.sh").is_file());
        let files = kadou_core_scan_folder_for_test(kata_dir.path(), "team");
        assert_eq!(files, vec!["team/hello".to_string()]);
    }

    /// A thin wrapper so this test file doesn't need `kadou_core::` self-qualification --
    /// `folder.rs` lives inside the `kadou_core` crate itself.
    fn kadou_core_scan_folder_for_test(kata_dir: &Path, folder: &str) -> Vec<String> {
        crate::scan_folder(kata_dir, folder)
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect()
    }

    #[test]
    fn get_folder_refuses_an_as_reserved_for_mined() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());

        let err = super::get_folder(kata_dir.path(), &url, Some("mined"), None, None).unwrap_err();
        assert!(matches!(err, super::FolderError::ReservedName(_)), "{err:?}");
        assert!(!kata_dir.path().join("mined").exists());
    }
}
