//! The folder scanner (`docs/design/05-prd.md` §4.1, `08-shape-review.md` §3.7).
//!
//! A folder under `kata/` is walked for kata candidates: a directory containing `kata.sh`
//! is a multi-file kata leaf (nothing inside it is scanned further); a directory without
//! one is a namespace and is recursed into; a file with a `# ---` header within its first 3
//! lines is a single-file kata candidate; anything else (a helper script, `README.md`,
//! dotfiles, `_`-prefixed names) is silently ignored, never an error.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::header::{self, Diagnostic, ParsedHeader, Severity};

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("no such folder: {0}")]
    NoSuchFolder(PathBuf),
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// One scanned kata candidate: either a real file that was parsed (successfully or not),
/// or a synthetic entry describing a structural problem (a naming collision) that is not
/// tied to a single file's content.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Path under `kata/`, no extension, `/`-separated (§4.2).
    pub id: String,
    /// The kata file itself (`<name>.sh` or `<name>/kata.sh`), or — for a structural
    /// diagnostic with no single owning file — the folder directory.
    pub path: PathBuf,
    pub source: String,
    pub header: Option<ParsedHeader>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ScannedFile {
    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics.len() - self.error_count()
    }
}

/// Scans one folder (an immediate subdirectory of `kata_dir`), returning every kata
/// candidate found, sorted by id.
pub fn scan_folder(kata_dir: &Path, folder: &str) -> Result<Vec<ScannedFile>, ScanError> {
    let dir = kata_dir.join(folder);
    if !dir.is_dir() {
        return Err(ScanError::NoSuchFolder(dir));
    }
    let mut out = Vec::new();
    scan_dir(&dir, &format!("{folder}/"), &mut out)?;
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Scans every immediate subdirectory of `kata_dir` as a folder. Returns `(folder_name,
/// files)` pairs sorted by folder name. A missing `kata_dir` scans as empty, not an error
/// (a fresh install before the starter kata are materialized).
pub fn scan_kata_dir(kata_dir: &Path) -> Result<Vec<(String, Vec<ScannedFile>)>, ScanError> {
    if !kata_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut folders = Vec::new();
    for entry in read_dir_sorted(kata_dir)? {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('_') || name.starts_with('.') {
            continue;
        }
        let files = scan_folder(kata_dir, &name)?;
        folders.push((name, files));
    }
    Ok(folders)
}

/// One directory's immediate entries, sorted into single-file kata candidates, multi-file
/// (`kata.sh`) kata directories, and namespace subdirectories to recurse into — a helper file
/// with no header marker is pushed straight into `out` here rather than tracked, since it
/// never participates in the collision/dedupe logic below.
struct Entries {
    file_candidates: BTreeMap<String, PathBuf>,
    dir_kata: BTreeMap<String, PathBuf>,
    namespace_dirs: Vec<PathBuf>,
}

fn classify_entries(
    dir: &Path,
    id_prefix: &str,
    out: &mut Vec<ScannedFile>,
) -> Result<Entries, ScanError> {
    let mut file_candidates: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut dir_kata: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut namespace_dirs: Vec<PathBuf> = Vec::new();

    for entry in read_dir_sorted(dir)? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('_') || name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            if path.join("kata.sh").is_file() {
                dir_kata.insert(name, path);
            } else {
                namespace_dirs.push(path);
            }
        } else if path.is_file() {
            match fs::read_to_string(&path) {
                Ok(text) if header::looks_like_kata_candidate(&text) => {
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or(name);
                    file_candidates.insert(stem, path);
                }
                _ => {
                    // No `# ---` marker (or unreadable as UTF-8): a helper file, not an
                    // error. Recorded with no header and no diagnostics so `kadou check -v`
                    // can still name it (§3.7 "a forgotten header is findable").
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or(name);
                    out.push(ScannedFile {
                        id: format!("{id_prefix}{stem}"),
                        path,
                        source: String::new(),
                        header: None,
                        diagnostics: Vec::new(),
                    });
                }
            }
        }
    }

    Ok(Entries {
        file_candidates,
        dir_kata,
        namespace_dirs,
    })
}

fn scan_dir(dir: &Path, id_prefix: &str, out: &mut Vec<ScannedFile>) -> Result<(), ScanError> {
    let Entries {
        file_candidates,
        dir_kata,
        namespace_dirs,
    } = classify_entries(dir, id_prefix, out)?;

    for (stem, file_path) in &file_candidates {
        if let Some(dir_path) = dir_kata.get(stem) {
            out.push(collision(id_prefix, stem, file_path, dir_path));
        }
    }

    for (stem, file_path) in &file_candidates {
        if dir_kata.contains_key(stem) {
            continue;
        }
        out.push(validated_scan_file(id_prefix, stem, file_path.clone()));
    }

    for (stem, dir_path) in &dir_kata {
        if file_candidates.contains_key(stem) {
            continue;
        }
        out.push(validated_scan_file(
            id_prefix,
            stem,
            dir_path.join("kata.sh"),
        ));
    }

    for nd in namespace_dirs {
        let name = nd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        scan_dir(&nd, &format!("{id_prefix}{name}/"), out)?;
    }

    Ok(())
}

/// `scan_file`, but first checks `stem` against the PRD §4.2 id-segment grammar (I-11): an
/// uppercase letter or underscore in a filename is a check error with a kebab-case rename
/// suggestion, not a silently-accepted kata.
fn validated_scan_file(id_prefix: &str, stem: &str, path: PathBuf) -> ScannedFile {
    if header::is_valid_id_segment(stem) {
        return scan_file(format!("{id_prefix}{stem}"), path);
    }
    ScannedFile {
        id: format!("{id_prefix}{stem}"),
        path,
        source: String::new(),
        header: None,
        diagnostics: vec![Diagnostic {
            severity: Severity::Error,
            message: format!("`{stem}` is not a valid kata name"),
            fix: Some(format!(
                "rename to `{}` (id segments match ^[a-z0-9][a-z0-9-]*$)",
                kebab_case_suggestion(stem)
            )),
            line: 1,
            col: 1,
            len: stem.chars().count().max(1),
        }],
    }
}

/// A best-effort kebab-case rename suggestion for an invalid id segment: lowercase, and any
/// run of characters outside `[a-z0-9]` becomes a single `-`.
fn kebab_case_suggestion(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            out.push(lower);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn scan_file(id: String, path: PathBuf) -> ScannedFile {
    match fs::read_to_string(&path) {
        Ok(source) => {
            let (header, diagnostics) = header::parse_header(&source);
            ScannedFile {
                id,
                path,
                source,
                header,
                diagnostics,
            }
        }
        Err(source) => ScannedFile {
            id,
            path: path.clone(),
            source: String::new(),
            header: None,
            diagnostics: vec![Diagnostic {
                severity: Severity::Error,
                message: format!("failed to read {}: {source}", path.display()),
                fix: None,
                line: 1,
                col: 1,
                len: 1,
            }],
        },
    }
}

fn collision(id_prefix: &str, stem: &str, file_path: &Path, dir_path: &Path) -> ScannedFile {
    ScannedFile {
        id: format!("{id_prefix}{stem}"),
        path: file_path.to_path_buf(),
        source: String::new(),
        header: None,
        diagnostics: vec![Diagnostic {
            severity: Severity::Error,
            message: format!(
                "`{}` and `{}` collide: a kata may be one file or one directory, not both",
                file_path.display(),
                dir_path.join("kata.sh").display()
            ),
            fix: Some("keep only one of the two forms".to_string()),
            line: 1,
            col: 1,
            len: 1,
        }],
    }
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<fs::DirEntry>, ScanError> {
    let map_err = |source: io::Error| ScanError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
        .map_err(map_err)?
        .collect::<Result<_, io::Error>>()
        .map_err(map_err)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    const HELLO: &str = "#!/bin/sh\n# ---\n# about: Say hello\n# risk:  low\n# ---\necho hi\n";
    const BROKEN: &str = "#!/bin/sh\n# ---\n# risk:  low\n# ---\necho hi\n";

    #[test]
    fn single_file_kata_is_found() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/starter/hello.sh", HELLO);
        let files = scan_folder(&dir.path().join("kata"), "starter").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "starter/hello");
        assert!(files[0].header.is_some());
    }

    #[test]
    fn multi_file_kata_is_a_leaf() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/sesami/device/kata.sh", HELLO);
        write(dir.path(), "kata/sesami/device/lib/helper.sh", "not a kata");
        let files = scan_folder(&dir.path().join("kata"), "sesami").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "sesami/device");
        assert!(files[0].path.ends_with("device/kata.sh"));
    }

    #[test]
    fn helper_file_without_header_is_ignored_but_findable_with_v() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/sesami/hello.sh", HELLO);
        write(
            dir.path(),
            "kata/sesami/scripts/trigger.sh",
            "#!/usr/bin/env bash\n# a helper\n#\necho hi\n",
        );
        let files = scan_folder(&dir.path().join("kata"), "sesami").unwrap();
        let kata: Vec<_> = files.iter().filter(|f| f.header.is_some()).collect();
        assert_eq!(kata.len(), 1);
        assert_eq!(kata[0].id, "sesami/hello");

        let ignored = files
            .iter()
            .find(|f| f.id == "sesami/scripts/trigger")
            .expect("helper file should still be recorded, unparsed");
        assert!(ignored.header.is_none());
        assert!(ignored.diagnostics.is_empty());
    }

    #[test]
    fn nested_namespace_kata_gets_a_path_id() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/sesami/db/migrate.sh", HELLO);
        let files = scan_folder(&dir.path().join("kata"), "sesami").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "sesami/db/migrate");
    }

    #[test]
    fn x_sh_and_x_kata_sh_collide() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/sesami/x.sh", HELLO);
        write(dir.path(), "kata/sesami/x/kata.sh", HELLO);
        let files = scan_folder(&dir.path().join("kata"), "sesami").unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].error_count() > 0);
        assert!(files[0].diagnostics[0].message.contains("collide"));
    }

    #[test]
    fn a_bad_file_in_one_folder_does_not_affect_another() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/broken/x.sh", BROKEN);
        write(dir.path(), "kata/starter/hello.sh", HELLO);
        let folders = scan_kata_dir(&dir.path().join("kata")).unwrap();
        let broken = folders.iter().find(|(n, _)| n == "broken").unwrap();
        let starter = folders.iter().find(|(n, _)| n == "starter").unwrap();
        assert!(broken.1[0].error_count() > 0);
        assert_eq!(starter.1[0].error_count(), 0);
    }

    #[test]
    fn an_uppercase_or_underscored_file_name_is_a_check_error_with_a_kebab_case_fix() {
        // PRD §4.2: id segments match ^[a-z0-9][a-z0-9-]*$; "Uppercase or underscore in a
        // filename is a check error with a rename suggestion." (I-11, H3.)
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/sesami/My_Kata.sh", HELLO);
        let files = scan_folder(&dir.path().join("kata"), "sesami").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "sesami/My_Kata");
        assert!(files[0].error_count() > 0, "{:?}", files[0].diagnostics);
        let diag = &files[0].diagnostics[0];
        assert!(diag.message.contains("My_Kata"), "{diag:?}");
        assert!(diag.fix.as_deref().unwrap().contains("my-kata"), "{diag:?}");
    }

    #[test]
    fn missing_kata_dir_scans_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let folders = scan_kata_dir(&dir.path().join("kata")).unwrap();
        assert!(folders.is_empty());
    }
}
