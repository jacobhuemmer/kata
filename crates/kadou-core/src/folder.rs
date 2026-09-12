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
    #[error("--root `{0}` escapes the checkout")]
    RootEscapes(String),
    #[error("--root `{0}` is not a directory in the checkout")]
    RootNotFound(String),
    #[error("no such folder: {0}")]
    NoSuchFolder(String),
    #[error("refusing to remove the reserved folder `{0}`")]
    ReservedRemove(String),
    #[error("{0} has uncommitted local changes; pass --force to remove it anyway")]
    Dirty(String),
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

/// The `..`-free, non-absolute half of the `--root` escape check, done before any clone is
/// attempted (§9 slice 7's test list: "`--root` with `..` is refused"). [`resolve_root`] is
/// the post-clone half, which also catches an in-repo symlink pointing outside the checkout.
fn validate_root_syntax(root: &str) -> Result<(), FolderError> {
    let path = Path::new(root);
    let escapes = path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
    if escapes {
        return Err(FolderError::RootEscapes(root.to_string()));
    }
    Ok(())
}

/// Confirms `checkout_dir.join(root)` is a directory that stays inside `checkout_dir` even
/// after symlinks resolve, then returns it (§9 slice 7 "cannot escape the checkout").
fn resolve_root(checkout_dir: &Path, root: &str) -> Result<PathBuf, FolderError> {
    let canonical_root = checkout_dir
        .canonicalize()
        .map_err(|source| io_err("read", checkout_dir, source))?;
    let candidate = checkout_dir.join(root);
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|_| FolderError::RootNotFound(root.to_string()))?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(FolderError::RootEscapes(root.to_string()));
    }
    Ok(candidate)
}

fn hidden_checkout_dir(kata_dir: &Path, folder: &str) -> PathBuf {
    kata_dir.join(".checkouts").join(folder)
}

#[cfg(unix)]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

/// Clones into a hidden checkout under `kata_dir/.checkouts/<folder>` (never scanned: the
/// folder scanner skips dot-prefixed names) and symlinks `target` to `root`'s resolved
/// subdirectory of it. Cleans up the hidden checkout on any failure so a bad `--root` never
/// leaves a half-installed folder behind.
fn get_folder_with_root(
    kata_dir: &Path,
    folder: &str,
    url: &str,
    git_ref: Option<&str>,
    root: &str,
    target: &Path,
) -> Result<(), FolderError> {
    let checkout_dir = hidden_checkout_dir(kata_dir, folder);
    if let Some(parent) = checkout_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|source| io_err("create", parent, source))?;
    }
    git::clone(url, &checkout_dir, git_ref)?;

    let outcome = resolve_root(&checkout_dir, root).and_then(|subdir| {
        symlink_dir(&subdir, target).map_err(|source| io_err("symlink", target, source))
    });
    if outcome.is_err() {
        let _ = std::fs::remove_dir_all(&checkout_dir);
    }
    outcome
}

/// `kadou get <url> [--as <folder>] [--ref <git_ref>] [--root <sub>]` (§7.1, §6.7, §4.1).
/// Without `root`, clones straight into `kata_dir/<folder>`. With `root`, clones into a
/// hidden checkout and exposes only the named subdirectory through a symlink at `target` —
/// git commands against `target` still find the real checkout by walking up through the
/// symlink target (`git::is_git_backed`).
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
    if let Some(root) = root {
        validate_root_syntax(root)?;
    }

    let target = kata_dir.join(&folder);
    if target.exists() {
        return Err(FolderError::AlreadyExists(target));
    }
    std::fs::create_dir_all(kata_dir).map_err(|source| io_err("create", kata_dir, source))?;

    match root {
        None => git::clone(url, &target, git_ref)?,
        Some(root) => get_folder_with_root(kata_dir, &folder, url, git_ref, root, &target)?,
    }
    Ok(target)
}

/// The per-folder result of `kadou update` (§7.1, §9 slice 7): every folder is reported, none
/// silently ignored, whether it was pulled, skipped (not git-backed), left alone (dirty), or
/// failed outright.
#[derive(Debug)]
pub enum UpdateOutcome {
    Pulled { folder: String },
    NotGitBacked { folder: String },
    Dirty { folder: String },
    Failed { folder: String, error: String },
}

/// `kadou update <folder>` for one named folder.
pub fn update_folder(kata_dir: &Path, folder: &str) -> UpdateOutcome {
    let target = kata_dir.join(folder);
    if !target.is_dir() {
        return UpdateOutcome::Failed {
            folder: folder.to_string(),
            error: "no such folder".to_string(),
        };
    }
    if !git::is_git_backed(&target) {
        return UpdateOutcome::NotGitBacked {
            folder: folder.to_string(),
        };
    }
    if git::has_local_changes(&target) {
        return UpdateOutcome::Dirty {
            folder: folder.to_string(),
        };
    }
    match git::pull(&target) {
        Ok(()) => UpdateOutcome::Pulled {
            folder: folder.to_string(),
        },
        Err(err) => UpdateOutcome::Failed {
            folder: folder.to_string(),
            error: err.to_string(),
        },
    }
}

/// Every immediate subdirectory of `kata_dir`, dot/underscore-prefixed names excluded (the
/// same rule `scan_kata_dir` uses), sorted, each pulled independently -- `kadou update` with
/// no folder argument (§7.1).
pub fn update_all(kata_dir: &Path) -> Vec<UpdateOutcome> {
    let Ok(entries) = std::fs::read_dir(kata_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.') && !n.starts_with('_'))
        .collect();
    names.sort();
    names
        .iter()
        .map(|name| update_folder(kata_dir, name))
        .collect()
}

/// `kadou remove <folder> [--force]` (§7.1, §4.2): never touches a reserved name (starter,
/// proposed, mined -- though the latter two never live under `kata_dir` in the first place),
/// never removes a git-backed folder with uncommitted local changes unless `force` is set,
/// and removes any hidden `.checkouts/<folder>` a `--root` install left behind. The `y/N`
/// prompt (or `--yes`) is the CLI's own concern, not this function's.
pub fn remove_folder(kata_dir: &Path, folder: &str, force: bool) -> Result<(), FolderError> {
    if RESERVED_FOLDER_NAMES.contains(&folder) {
        return Err(FolderError::ReservedRemove(folder.to_string()));
    }
    let target = kata_dir.join(folder);
    if !target.is_dir() {
        return Err(FolderError::NoSuchFolder(folder.to_string()));
    }
    if git::has_local_changes(&target) && !force {
        return Err(FolderError::Dirty(folder.to_string()));
    }

    std::fs::remove_dir_all(&target).map_err(|source| io_err("remove", &target, source))?;
    let hidden = hidden_checkout_dir(kata_dir, folder);
    if hidden.exists() {
        let _ = std::fs::remove_dir_all(&hidden);
    }
    Ok(())
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
        assert!(
            matches!(err, super::FolderError::ReservedName(_)),
            "{err:?}"
        );
        assert!(!kata_dir.path().join("mined").exists());
    }

    #[test]
    fn get_folder_refuses_an_existing_folder() {
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("team")).unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());

        let err = super::get_folder(kata_dir.path(), &url, Some("team"), None, None).unwrap_err();
        assert!(
            matches!(err, super::FolderError::AlreadyExists(_)),
            "{err:?}"
        );
    }

    #[test]
    fn get_folder_refuses_an_invalid_name() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());

        let err =
            super::get_folder(kata_dir.path(), &url, Some("Team_Name"), None, None).unwrap_err();
        assert!(matches!(err, super::FolderError::InvalidName(_)), "{err:?}");
    }

    #[test]
    fn get_folder_defaults_the_folder_name_from_the_url() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let bare = bare_fixture_with_one_commit(fixture_root.path());
        let url = format!("file://{}", bare.display());

        let target = super::get_folder(kata_dir.path(), &url, None, None, None).unwrap();
        assert_eq!(target, kata_dir.path().join("origin"));
    }

    #[test]
    fn get_folder_with_a_ref_checks_out_the_named_branch() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());

        let target =
            super::get_folder(kata_dir.path(), &url, Some("team"), Some("main"), None).unwrap();

        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&target)
            .args(["branch", "--show-current"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "main");
    }

    /// A local bare fixture whose kata sit under a subdirectory, not the repo root -- the
    /// shape `--root` exists for.
    fn bare_monorepo_fixture(root: &Path) -> std::path::PathBuf {
        let bare = root.join("origin.git");
        std::fs::create_dir_all(&bare).unwrap();
        run(&bare, &["init", "-q", "--bare", "-b", "main"]);

        let seed = root.join("seed");
        std::fs::create_dir_all(seed.join("ops/scripts")).unwrap();
        run(&seed, &["init", "-q", "-b", "main"]);
        run(&seed, &["config", "user.email", "test@example.com"]);
        run(&seed, &["config", "user.name", "test"]);
        std::fs::write(seed.join("README.md"), "unrelated\n").unwrap();
        std::fs::write(
            seed.join("ops/scripts/hello.sh"),
            "#!/bin/sh\n# ---\n# about: Say hello\n# risk:  low\n# ---\necho hi\n",
        )
        .unwrap();
        run(&seed, &["add", "-A"]);
        run(&seed, &["commit", "-q", "-m", "init"]);
        run(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
        run(&seed, &["push", "-q", "origin", "main"]);
        bare
    }

    #[test]
    fn get_folder_root_with_dotdot_is_refused_before_any_clone() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());

        let err = super::get_folder(kata_dir.path(), &url, Some("team"), None, Some("../etc"))
            .unwrap_err();
        assert!(matches!(err, super::FolderError::RootEscapes(_)), "{err:?}");
        assert!(!kata_dir.path().join("team").exists());
    }

    #[test]
    fn get_folder_root_selects_a_subdirectory_via_a_symlink() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let bare = bare_monorepo_fixture(fixture_root.path());
        let url = format!("file://{}", bare.display());

        let target = super::get_folder(
            kata_dir.path(),
            &url,
            Some("ops"),
            None,
            Some("ops/scripts"),
        )
        .unwrap();

        assert!(target.join("hello.sh").is_file());
        assert!(super::git::is_git_backed(&target));
        let files = kadou_core_scan_folder_for_test(kata_dir.path(), "ops");
        assert_eq!(files, vec!["ops/hello".to_string()]);
    }

    #[test]
    fn get_folder_root_pointing_outside_the_checkout_is_refused() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());

        let err = super::get_folder(
            kata_dir.path(),
            &url,
            Some("team"),
            None,
            Some("no/such/sub"),
        )
        .unwrap_err();
        assert!(
            matches!(err, super::FolderError::RootNotFound(_)),
            "{err:?}"
        );
        assert!(!kata_dir.path().join("team").exists());
    }

    #[test]
    fn update_folder_reports_and_skips_a_non_git_folder() {
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("starter")).unwrap();

        let outcome = super::update_folder(kata_dir.path(), "starter");
        assert!(matches!(outcome, super::UpdateOutcome::NotGitBacked { .. }));
    }

    #[test]
    fn update_folder_pulls_a_new_commit_from_the_fixture_remote() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let bare = bare_fixture_with_one_commit(fixture_root.path());
        let url = format!("file://{}", bare.display());
        super::get_folder(kata_dir.path(), &url, Some("team"), None, None).unwrap();

        // Another contributor pushes a new commit to the same fixture remote.
        let other = fixture_root.path().join("other-checkout");
        super_clone(&bare, &other);
        run(&other, &["config", "user.email", "test@example.com"]);
        run(&other, &["config", "user.name", "test"]);
        std::fs::write(other.join("second.txt"), "second").unwrap();
        run(&other, &["add", "-A"]);
        run(&other, &["commit", "-q", "-m", "second"]);
        run(&other, &["push", "-q", "origin", "main"]);

        let outcome = super::update_folder(kata_dir.path(), "team");
        assert!(matches!(outcome, super::UpdateOutcome::Pulled { .. }));
        assert!(kata_dir.path().join("team/second.txt").is_file());
    }

    fn super_clone(bare: &Path, dest: &Path) {
        let status = std::process::Command::new("git")
            .args([
                "clone",
                "-q",
                bare.to_str().unwrap(),
                dest.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn update_folder_leaves_a_dirty_checkout_alone() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());
        let target = super::get_folder(kata_dir.path(), &url, Some("team"), None, None).unwrap();
        std::fs::write(target.join("uncommitted.txt"), "x").unwrap();

        let outcome = super::update_folder(kata_dir.path(), "team");
        assert!(matches!(outcome, super::UpdateOutcome::Dirty { .. }));
    }

    #[test]
    fn update_folder_fails_cleanly_for_an_unknown_folder() {
        let kata_dir = tempfile::tempdir().unwrap();
        let outcome = super::update_folder(kata_dir.path(), "nope");
        assert!(matches!(outcome, super::UpdateOutcome::Failed { .. }));
    }

    #[test]
    fn update_all_covers_every_folder_and_skips_dot_directories() {
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("starter")).unwrap();
        std::fs::create_dir_all(kata_dir.path().join(".checkouts/team")).unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());
        super::get_folder(kata_dir.path(), &url, Some("team"), None, None).unwrap();

        let outcomes = super::update_all(kata_dir.path());
        let folders: Vec<&str> = outcomes
            .iter()
            .map(|o| match o {
                super::UpdateOutcome::Pulled { folder }
                | super::UpdateOutcome::NotGitBacked { folder }
                | super::UpdateOutcome::Dirty { folder }
                | super::UpdateOutcome::Failed { folder, .. } => folder.as_str(),
            })
            .collect();
        assert_eq!(folders, vec!["starter", "team"]);
    }

    #[test]
    fn remove_folder_refuses_starter() {
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("starter")).unwrap();

        let err = super::remove_folder(kata_dir.path(), "starter", false).unwrap_err();
        assert!(
            matches!(err, super::FolderError::ReservedRemove(_)),
            "{err:?}"
        );
        assert!(kata_dir.path().join("starter").is_dir());
    }

    #[test]
    fn remove_folder_refuses_a_dirty_checkout_without_force() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());
        let target = super::get_folder(kata_dir.path(), &url, Some("team"), None, None).unwrap();
        std::fs::write(target.join("uncommitted.txt"), "x").unwrap();

        let err = super::remove_folder(kata_dir.path(), "team", false).unwrap_err();
        assert!(matches!(err, super::FolderError::Dirty(_)), "{err:?}");
        assert!(kata_dir.path().join("team").is_dir());

        super::remove_folder(kata_dir.path(), "team", true).unwrap();
        assert!(!kata_dir.path().join("team").exists());
    }

    #[test]
    fn remove_folder_removes_a_clean_folder() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let url = bare_fixture_url(fixture_root.path());
        super::get_folder(kata_dir.path(), &url, Some("team"), None, None).unwrap();

        super::remove_folder(kata_dir.path(), "team", false).unwrap();
        assert!(!kata_dir.path().join("team").exists());
    }

    #[test]
    fn remove_folder_fails_cleanly_for_an_unknown_folder() {
        let kata_dir = tempfile::tempdir().unwrap();
        let err = super::remove_folder(kata_dir.path(), "nope", false).unwrap_err();
        assert!(
            matches!(err, super::FolderError::NoSuchFolder(_)),
            "{err:?}"
        );
    }

    #[test]
    fn remove_folder_also_removes_its_hidden_root_checkout() {
        let kata_dir = tempfile::tempdir().unwrap();
        let fixture_root = tempfile::tempdir().unwrap();
        let bare = bare_monorepo_fixture(fixture_root.path());
        let url = format!("file://{}", bare.display());
        super::get_folder(
            kata_dir.path(),
            &url,
            Some("ops"),
            None,
            Some("ops/scripts"),
        )
        .unwrap();
        assert!(kata_dir.path().join(".checkouts/ops").is_dir());

        super::remove_folder(kata_dir.path(), "ops", false).unwrap();
        assert!(!kata_dir.path().join("ops").exists());
        assert!(!kata_dir.path().join(".checkouts/ops").exists());
    }
}
