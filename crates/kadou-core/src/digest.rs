//! Kata file digest (`docs/design/05-prd.md` §5.5 `describe_kata.sha256`, §6.4 grant-approval
//! pin). `kadou show` in this slice is the first caller; approval pinning lands with slice 6.

use std::io;
use std::path::Path;

use std::fmt::Write as _;

use sha2::{Digest as _, Sha256};

/// `sha256:<hex>` of `bytes` — the one place this formatting loop lives (C5,
/// `docs/design/11-code-review.md`: previously duplicated verbatim in `kadou-mcp/pending.rs`).
pub fn sha256_hex_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();

    let mut hex = String::with_capacity("sha256:".len() + digest.len() * 2);
    hex.push_str("sha256:");
    for byte in digest.as_slice() {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// `sha256:<hex>` of `path`'s contents, matching the `describe_kata` field spelling (§5.5).
pub fn file_sha256(path: &Path) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(sha256_hex_prefixed(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_file_contents_with_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sh");
        std::fs::write(&path, b"echo hi\n").unwrap();
        let digest = file_sha256(&path).unwrap();
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), "sha256:".len() + 64);
    }

    #[test]
    fn same_content_hashes_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sh");
        let b = dir.path().join("b.sh");
        std::fs::write(&a, b"echo hi\n").unwrap();
        std::fs::write(&b, b"echo hi\n").unwrap();
        assert_eq!(file_sha256(&a).unwrap(), file_sha256(&b).unwrap());
    }
}
