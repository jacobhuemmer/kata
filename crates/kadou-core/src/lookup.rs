//! Resolves a kata id to a [`Kata`] by scanning its folder (`docs/design/05-prd.md` §4.2).
//!
//! This is the first real consumer of [`Kata`] (the scanner and `check` work directly with
//! [`crate::scan::ScannedFile`]/[`crate::header::ParsedHeader`]); `kadou run` and `kadou
//! show` need the assembled shape to hand to `kadou-exec`.

use std::path::Path;

use crate::header::Diagnostic;
use crate::kata::Kata;
use crate::scan::{self, ScanError, ScannedFile};

/// The result of resolving one id under `kata_dir`.
#[derive(Debug, Clone)]
pub enum LookupResult {
    Found(Kata),
    /// No folder, or no file, has this id.
    NotFound,
    /// A file with this id exists but its header failed to parse — distinct from
    /// `NotFound` so the caller can point at `kadou check` instead of `kadou list`.
    Invalid {
        diagnostics: Vec<Diagnostic>,
    },
}

/// Looks up `id` (`folder/name`, arbitrarily nested) by scanning only its top-level folder,
/// not the whole `kata/` tree.
pub fn find_kata(kata_dir: &Path, id: &str) -> Result<LookupResult, ScanError> {
    let folder = id.split('/').next().unwrap_or(id);
    if folder.is_empty() || !kata_dir.join(folder).is_dir() {
        return Ok(LookupResult::NotFound);
    }
    let files = scan::scan_folder(kata_dir, folder)?;
    let Some(file) = files.into_iter().find(|f| f.id == id) else {
        return Ok(LookupResult::NotFound);
    };
    Ok(match kata_from_scanned(&file) {
        Some(kata) => LookupResult::Found(kata),
        None => LookupResult::Invalid {
            diagnostics: file.diagnostics,
        },
    })
}

/// Assembles a [`Kata`] from a scanned file, `None` when its header didn't parse.
pub fn kata_from_scanned(file: &ScannedFile) -> Option<Kata> {
    let header = file.header.as_ref()?;
    Some(Kata {
        id: file.id.clone(),
        path: file.path.clone(),
        about: header.about.clone(),
        risk: header.risk,
        needs: header.needs.clone(),
        args: header.args.clone(),
        alias: header.alias.clone(),
        timeout: header.timeout,
        notes: header.notes.clone(),
        shebang: header.shebang.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    const HELLO: &str = "#!/bin/sh\n# ---\n# about: Say hello\n# risk:  low\n# ---\necho hi\n";

    #[test]
    fn finds_a_kata_by_id() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/starter/hello.sh", HELLO);
        let result = find_kata(&dir.path().join("kata"), "starter/hello").unwrap();
        let LookupResult::Found(kata) = result else {
            panic!("expected Found, got {result:?}");
        };
        assert_eq!(kata.id, "starter/hello");
        assert_eq!(kata.shebang.as_deref(), Some("#!/bin/sh"));
    }

    #[test]
    fn unknown_id_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/starter/hello.sh", HELLO);
        let result = find_kata(&dir.path().join("kata"), "starter/missing").unwrap();
        assert!(matches!(result, LookupResult::NotFound));
    }

    #[test]
    fn unknown_folder_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = find_kata(&dir.path().join("kata"), "nope/missing").unwrap();
        assert!(matches!(result, LookupResult::NotFound));
    }

    #[test]
    fn a_file_with_a_broken_header_is_invalid_not_not_found() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kata/broken/x.sh",
            "#!/bin/sh\n# ---\n# risk:  low\n# ---\necho hi\n",
        );
        let result = find_kata(&dir.path().join("kata"), "broken/x").unwrap();
        let LookupResult::Invalid { diagnostics } = result else {
            panic!("expected Invalid, got {result:?}");
        };
        assert!(!diagnostics.is_empty());
    }
}
