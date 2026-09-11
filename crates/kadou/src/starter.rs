//! The embedded starter kata (`docs/design/05-prd.md` §7.4, §7.7, §9 slice 3).
//!
//! Embedded inside the `kadou` bin crate — not `kadou-core` — so `cargo package` works for
//! `rust-embed` (§3 crate layout note). This slice ships `starter/hello` only; §7.7's other
//! four starter kata are first-run-experience polish for a later slice.

use std::path::Path;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "starter/"]
struct StarterKata;

/// Materializes the embedded starter kata into `kata_dir/starter/`, but only when `kata_dir`
/// does not exist yet (§7.4 item 2: "written on first run; yours after that. If deleted, it
/// stays deleted.").
pub fn materialize_if_needed(kata_dir: &Path) -> std::io::Result<()> {
    if kata_dir.exists() {
        return Ok(());
    }
    for name in StarterKata::iter() {
        let file = StarterKata::get(&name).expect("embedded file listed by iter() must exist");
        let dest = kata_dir.join("starter").join(name.as_ref());
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
    fn does_not_touch_an_already_existing_kata_dir() {
        let dir = tempfile::tempdir().unwrap();
        let kata_dir = dir.path().join("kata");
        std::fs::create_dir_all(&kata_dir).unwrap();
        materialize_if_needed(&kata_dir).unwrap();
        assert!(!kata_dir.join("starter").exists());
    }
}
