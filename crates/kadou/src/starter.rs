//! The embedded starter kata (`docs/design/05-prd.md` §7.4, §7.7, §9 slice 3/6).
//!
//! Embedded inside the `kadou` bin crate — not `kadou-core` — so `cargo package` works for
//! `rust-embed` (§3 crate layout note).
//!
//! **Decision D6 (revised, I-2):** the whole embedded set is materialized on a scan **only if
//! `kata/starter/` does not exist yet** — not per file, and not gated on `kata/` itself. Once
//! `kata/starter/` exists at all, it is never touched again: a deleted file inside it stays
//! deleted (`kadou get starter` is what restores it, slice 7). Gating on `kata/starter/`
//! specifically (rather than `kata/`) is what still fixes the slice-5 defect this replaces: a
//! home that ran `kadou import` (or only `kadou mcp serve`) before ever touching `starter`
//! still gets the starter kata, because `kata/team/` existing does not make `kata/starter/`
//! exist.
use std::path::Path;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "starter/"]
struct StarterKata;

/// Materializes the whole embedded starter set under `kata_dir/starter/` the first time that
/// directory doesn't exist yet (§7.4 item 2, decision D6). A no-op once it exists, by design —
/// safe to call on every command that scans kata.
pub fn materialize_if_needed(kata_dir: &Path) -> std::io::Result<()> {
    let starter_dir = kata_dir.join("starter");
    if starter_dir.exists() {
        return Ok(());
    }
    for name in StarterKata::iter() {
        let dest = starter_dir.join(name.as_ref());
        let Some(file) = StarterKata::get(&name) else {
            // `iter()` just yielded this exact name, so `get()` cannot really miss it — but
            // failing this one file closed rather than panicking the whole command is still
            // strictly safer (R6).
            return Err(std::io::Error::other(format!(
                "embedded starter file `{name}` listed by iter() but missing from get()"
            )));
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, file.data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materializes_hello_into_a_fresh_kata_dir() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        materialize_if_needed(&kata_dir).unwrap();
        assert!(kata_dir.join("starter/hello.sh").is_file());
    }

    #[test]
    fn materializes_missing_files_even_when_kata_dir_already_has_other_folders() {
        // Reproduces the slice-5 defect (D6): a home that imported a team folder first (so
        // `kata_dir` already existed) must still get the starter kata on the next scan.
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        std::fs::create_dir_all(kata_dir.join("team")).unwrap();
        std::fs::write(kata_dir.join("team/other.sh"), "#!/bin/sh\necho hi\n").unwrap();

        materialize_if_needed(&kata_dir).unwrap();

        assert!(kata_dir.join("starter/hello.sh").is_file());
        assert!(kata_dir.join("starter/disk-usage.sh").is_file());
        assert!(kata_dir.join("starter/git-status.sh").is_file());
        assert!(kata_dir.join("starter/health.sh").is_file());
        assert!(kata_dir.join("starter/list-path.sh").is_file());
        // The pre-existing folder is untouched.
        assert!(kata_dir.join("team/other.sh").is_file());
    }

    #[test]
    fn never_overwrites_an_existing_starter_file() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        materialize_if_needed(&kata_dir).unwrap();

        let hello = kata_dir.join("starter/hello.sh");
        std::fs::write(&hello, "#!/bin/sh\necho customized\n").unwrap();

        materialize_if_needed(&kata_dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(&hello).unwrap(),
            "#!/bin/sh\necho customized\n"
        );
    }

    #[test]
    fn a_deleted_starter_file_stays_deleted() {
        // Decision D6 (revised, I-2): "materialized from the embed only if kata/starter/
        // does not exist. If deleted, it stays deleted; kadou get starter restores it." Once
        // the starter/ directory exists at all, materialize_if_needed never touches it again
        // -- a single deleted file inside it is not recreated on the next scan.
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        materialize_if_needed(&kata_dir).unwrap();
        std::fs::remove_file(kata_dir.join("starter/hello.sh")).unwrap();

        materialize_if_needed(&kata_dir).unwrap();
        assert!(!kata_dir.join("starter/hello.sh").exists());
    }
}
