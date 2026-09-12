//! The embedded starter kata (`docs/design/05-prd.md` §7.4, §7.7, §9 slice 3/6).
//!
//! Embedded inside the `kadou` bin crate — not `kadou-core` — so `cargo package` works for
//! `rust-embed` (§3 crate layout note).
//!
//! **Decision D6 (slice 6):** every embedded file under `starter/` is materialized
//! independently, on **every** command that scans kata (`list`, `check`, `run`, `show`, `mcp
//! serve`, `grant`, and the bare `kadou` frame) — never only "when `kata/` doesn't exist yet".
//! The slice-5 version checked that instead, so a home that ran `kadou import` (or only ever
//! `kadou mcp serve`) before touching `starter` directly never got the starter kata at all,
//! since `kata/` already existed once any other folder had been written into it. A file that
//! already exists (materialized before, or a human's own edit) is never overwritten or
//! recreated once it exists; only files genuinely missing get written.
use std::path::Path;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "starter/"]
struct StarterKata;

/// Materializes every embedded starter file whose destination under `kata_dir/starter/` does
/// not yet exist (§7.4 item 2, decision D6). Safe to call on every command that scans kata —
/// a fully-materialized starter folder does no I/O beyond the existence checks.
pub fn materialize_if_needed(kata_dir: &Path) -> std::io::Result<()> {
    for name in StarterKata::iter() {
        let dest = kata_dir.join("starter").join(name.as_ref());
        if dest.exists() {
            continue;
        }
        let file = StarterKata::get(&name).expect("embedded file listed by iter() must exist");
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
