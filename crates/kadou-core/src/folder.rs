//! `kadou get`/`update`/`remove`: folder-level git install (`docs/design/05-prd.md` §9 slice
//! 7, §4.1 on-disk layout, §4.2 identity/reserved names). Pure validation plus the real git
//! and filesystem calls; `kadou`'s `commands.rs` is the thin CLI wrapper that prompts and
//! prints these results.

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
