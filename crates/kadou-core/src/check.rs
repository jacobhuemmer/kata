//! `kadou check`: the loader (`docs/design/05-prd.md` §4.7, decision 12).
//!
//! A folder with any header error is reported with a nonzero error count and none of its
//! kata would run; other folders are unaffected. Diagnostics are rendered cargo-shaped: a
//! line per error, a fix line per error, one summary line per folder.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::header::{Diagnostic, Severity};
use crate::scan::{self, ScanError, ScannedFile};
use crate::vault::Vault;

/// One folder's check result.
#[derive(Debug, Clone)]
pub struct FolderReport {
    pub folder: String,
    pub files: Vec<ScannedFile>,
}

impl FolderReport {
    /// Kata that parsed with no errors — the count `checked N kata in <folder>` reports.
    pub fn kata_count(&self) -> usize {
        self.files.iter().filter(|f| f.header.is_some()).count()
    }

    pub fn error_count(&self) -> usize {
        self.files.iter().map(ScannedFile::error_count).sum()
    }

    pub fn warning_count(&self) -> usize {
        self.files.iter().map(ScannedFile::warning_count).sum()
    }

    pub fn is_ok(&self) -> bool {
        self.error_count() == 0
    }
}

#[derive(Debug, Clone)]
pub struct CheckReport {
    pub folders: Vec<FolderReport>,
}

impl CheckReport {
    pub fn error_count(&self) -> usize {
        self.folders.iter().map(FolderReport::error_count).sum()
    }

    pub fn is_ok(&self) -> bool {
        self.error_count() == 0
    }
}

/// Checks one folder (an immediate subdirectory of `kata_dir`), including needs and alias
/// conflicts within that folder alone. `vault` decides which needs are actually missing
/// (§4.7, §9 slice 4 "`kadou check`'s missing-need warning now consults the vault").
pub fn check_folder(
    kata_dir: &Path,
    folder: &str,
    vault: &Vault,
) -> Result<FolderReport, ScanError> {
    let files = scan::scan_folder(kata_dir, folder)?;
    let wrapped = vec![(folder.to_string(), files)];
    let extra = cross_kata_diagnostics(&wrapped);
    // `wrapped` is the one-element vec built just above; `unwrap_or_default` (an empty file
    // list) rather than `expect` keeps this fn panic-free even if that ever changes (R6).
    let mut files = wrapped
        .into_iter()
        .next()
        .map(|(_, f)| f)
        .unwrap_or_default();
    files.extend(extra.into_iter().map(|(_, d)| d));
    add_missing_vault_warnings(&mut files, vault);
    Ok(FolderReport {
        folder: folder.to_string(),
        files,
    })
}

/// Checks every folder under `kata_dir`, plus needs and alias conflicts across folder
/// boundaries (§4.4 "kadou check errors when two folders declare the same need with
/// different defaults"; §4.2 "Aliases ... unique across all folders, checked"). `vault`
/// decides which needs are actually missing (§4.7).
pub fn check_all(kata_dir: &Path, vault: &Vault) -> Result<CheckReport, ScanError> {
    let scanned = scan::scan_kata_dir(kata_dir)?;
    let mut extra_by_folder: BTreeMap<String, Vec<ScannedFile>> = BTreeMap::new();
    for (folder, diag_file) in cross_kata_diagnostics(&scanned) {
        extra_by_folder.entry(folder).or_default().push(diag_file);
    }

    let folders = scanned
        .into_iter()
        .map(|(folder, mut files)| {
            if let Some(extra) = extra_by_folder.remove(&folder) {
                files.extend(extra);
            }
            add_missing_vault_warnings(&mut files, vault);
            FolderReport { folder, files }
        })
        .collect();

    Ok(CheckReport { folders })
}

/// A need with no header default and no vault entry has nowhere to resolve from until a
/// human runs `kadou vault set` (§4.7 "a need with no vault entry and no default"; §9 slice
/// 4 replaces the slice-3 stopgap, which warned on every default-less need unconditionally
/// because there was no vault yet to ask).
fn add_missing_vault_warnings(files: &mut [ScannedFile], vault: &Vault) {
    for file in files.iter_mut() {
        let Some(header) = &file.header else {
            continue;
        };
        let missing: Vec<String> = header
            .needs
            .iter()
            .filter(|n| n.default.is_none() && vault.get(&n.name).is_none())
            .map(|n| n.name.clone())
            .collect();
        for name in missing {
            file.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                message: format!("need `{name}` has no vault entry and no default"),
                fix: Some(format!("kadou vault set {name}")),
                line: 1,
                col: 1,
                len: 1,
            });
        }
    }
}

/// Checks a single kata file directly by filesystem path (the `kadou check <path>` form),
/// independent of any folder under `kata_dir`.
pub fn check_path(path: &Path) -> ScannedFile {
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    match std::fs::read_to_string(path) {
        Ok(source) => {
            let (header, diagnostics) = crate::header::parse_header(&source);
            ScannedFile {
                id,
                path: path.to_path_buf(),
                source,
                header,
                diagnostics,
            }
        }
        Err(source) => ScannedFile {
            id,
            path: path.to_path_buf(),
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

/// Finds needs declared with conflicting defaults, and aliases declared more than once,
/// across every kata in `folders`. Each conflict is reported as a synthetic diagnostic
/// entry attached to every folder involved, so `checked N kata in F` still counts real kata
/// while the conflict still fails that folder's check.
/// (folder, kata_id, need default), grouped by need name.
type NeedsSeen<'a> = BTreeMap<&'a str, Vec<(&'a str, &'a str, &'a Option<String>)>>;
/// (folder, kata_id), grouped by alias name.
type AliasSeen<'a> = BTreeMap<&'a str, Vec<(&'a str, &'a str)>>;

fn collect_needs_and_aliases<'a>(
    folders: &'a [(String, Vec<ScannedFile>)],
) -> (NeedsSeen<'a>, AliasSeen<'a>) {
    let mut needs_seen: NeedsSeen = BTreeMap::new();
    let mut alias_seen: AliasSeen = BTreeMap::new();

    for (folder, files) in folders {
        for file in files.iter() {
            let Some(header) = &file.header else {
                continue;
            };
            for need in &header.needs {
                needs_seen.entry(need.name.as_str()).or_default().push((
                    folder.as_str(),
                    file.id.as_str(),
                    &need.default,
                ));
            }
            for alias in &header.alias {
                alias_seen
                    .entry(alias.as_str())
                    .or_default()
                    .push((folder.as_str(), file.id.as_str()));
            }
        }
    }
    (needs_seen, alias_seen)
}

/// One diagnostic per folder touched by a need declared with conflicting defaults (§4.4).
fn conflicting_need_diagnostics(needs_seen: &NeedsSeen<'_>) -> Vec<(String, ScannedFile)> {
    let mut out = Vec::new();
    for (name, entries) in needs_seen {
        let defaults: Vec<&Option<String>> = entries.iter().map(|(_, _, d)| *d).collect();
        let conflicting = defaults.windows(2).any(|w| w[0] != w[1]);
        if !conflicting {
            continue;
        }
        let where_ = entries
            .iter()
            .map(|(f, id, d)| match d {
                Some(v) => format!("{id} ({f}) = {v}"),
                None => format!("{id} ({f}) has no default"),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let touched: std::collections::BTreeSet<&str> =
            entries.iter().map(|(f, _, _)| *f).collect();
        for folder in touched {
            out.push((
                folder.to_string(),
                pseudo_entry(
                    format!("need/{name}"),
                    format!("need `{name}` is declared with conflicting defaults: {where_}"),
                    Some("give every declaration of this need the same default".to_string()),
                ),
            ));
        }
    }
    out
}

/// One diagnostic per folder touched by an alias declared more than once (§4.2).
fn duplicate_alias_diagnostics(alias_seen: &AliasSeen<'_>) -> Vec<(String, ScannedFile)> {
    let mut out = Vec::new();
    for (name, entries) in alias_seen {
        if entries.len() < 2 {
            continue;
        }
        let where_ = entries
            .iter()
            .map(|(f, id)| format!("{id} ({f})"))
            .collect::<Vec<_>>()
            .join(", ");
        let touched: std::collections::BTreeSet<&str> = entries.iter().map(|(f, _)| *f).collect();
        for folder in touched {
            out.push((
                folder.to_string(),
                pseudo_entry(
                    format!("alias/{name}"),
                    format!("alias `{name}` is declared by more than one kata: {where_}"),
                    Some("aliases must be unique across all folders".to_string()),
                ),
            ));
        }
    }
    out
}

fn cross_kata_diagnostics(folders: &[(String, Vec<ScannedFile>)]) -> Vec<(String, ScannedFile)> {
    let (needs_seen, alias_seen) = collect_needs_and_aliases(folders);
    let mut out = conflicting_need_diagnostics(&needs_seen);
    out.extend(duplicate_alias_diagnostics(&alias_seen));
    out
}

fn pseudo_entry(id: String, message: String, fix: Option<String>) -> ScannedFile {
    ScannedFile {
        id,
        path: PathBuf::new(),
        source: String::new(),
        header: None,
        diagnostics: vec![Diagnostic {
            severity: Severity::Error,
            message,
            fix,
            line: 1,
            col: 1,
            len: 1,
        }],
    }
}

/// Renders one folder's report cargo-shaped (§4.7). `display_root` is stripped from each
/// file's path so the location reads `kata/<folder>/<name>.sh:L:C` instead of an absolute
/// path; pass the parent of `kata_dir` (typically the config dir).
pub fn render_report(report: &FolderReport, display_root: &Path, verbose: bool) -> String {
    let mut out = String::new();

    for file in &report.files {
        if file.diagnostics.is_empty() {
            if verbose && file.header.is_none() {
                let _ = writeln!(out, "ignored: {}", display_path(&file.path, display_root));
            }
            continue;
        }
        for diag in &file.diagnostics {
            render_diagnostic(&mut out, diag, &file.path, display_root, &file.source);
            out.push('\n');
        }
    }

    let error_word = if report.error_count() == 1 {
        "error"
    } else {
        "errors"
    };
    let warning_word = if report.warning_count() == 1 {
        "warning"
    } else {
        "warnings"
    };
    let _ = writeln!(
        out,
        "checked {} kata in {}   {} {error_word}  {} {warning_word}",
        report.kata_count(),
        report.folder,
        report.error_count(),
        report.warning_count(),
    );

    out
}

fn render_diagnostic(
    out: &mut String,
    diag: &Diagnostic,
    path: &Path,
    display_root: &Path,
    source: &str,
) {
    let level = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let _ = writeln!(out, "{level}: {}", diag.message);

    let location = display_path(path, display_root);
    let source_line = source.lines().nth(diag.line.saturating_sub(1));

    if let Some(line_text) = source_line {
        let _ = writeln!(out, "  --> {location}:{}:{}", diag.line, diag.col);
        let gutter_width = diag.line.to_string().len().max(1);
        let _ = writeln!(out, "{:gutter_width$} |", "");
        let _ = writeln!(out, "{:gutter_width$} | {line_text}", diag.line);
        let caret_indent = diag.col.saturating_sub(1);
        let carets = "^".repeat(diag.len.max(1));
        let _ = writeln!(out, "{:gutter_width$} | {:caret_indent$}{carets}", "", "");
    } else {
        let _ = writeln!(out, "  --> {location}");
    }

    if let Some(fix) = &diag.fix {
        let _ = writeln!(out, "  = {fix}");
    }
}

fn display_path(path: &Path, display_root: &Path) -> String {
    path.strip_prefix(display_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
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
    fn clean_folder_reports_zero_errors() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/starter/hello.sh", HELLO);
        let report = check_folder(&dir.path().join("kata"), "starter", &Vault::default()).unwrap();
        assert_eq!(report.kata_count(), 1);
        assert_eq!(report.error_count(), 0);
        assert!(report.is_ok());
    }

    #[test]
    fn folder_with_a_bad_header_is_not_ok_but_other_folders_are() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kata/broken/x.sh",
            "#!/bin/sh\n# ---\n# risk:  low\n# ---\necho hi\n",
        );
        write(dir.path(), "kata/starter/hello.sh", HELLO);
        let report = check_all(&dir.path().join("kata"), &Vault::default()).unwrap();
        let broken = report
            .folders
            .iter()
            .find(|f| f.folder == "broken")
            .unwrap();
        let starter = report
            .folders
            .iter()
            .find(|f| f.folder == "starter")
            .unwrap();
        assert!(!broken.is_ok());
        assert!(starter.is_ok());
        assert!(!report.is_ok());
    }

    #[test]
    fn conflicting_need_defaults_across_folders_fail_both() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kata/a/x.sh",
            "#!/bin/sh\n# ---\n# about: A\n# risk:  low\n# needs: token=one\n# ---\necho hi\n",
        );
        write(
            dir.path(),
            "kata/b/y.sh",
            "#!/bin/sh\n# ---\n# about: B\n# risk:  low\n# needs: token=two\n# ---\necho hi\n",
        );
        let report = check_all(&dir.path().join("kata"), &Vault::default()).unwrap();
        for name in ["a", "b"] {
            let folder = report.folders.iter().find(|f| f.folder == name).unwrap();
            assert!(!folder.is_ok(), "{name} should have a conflict error");
        }
    }

    #[test]
    fn same_need_default_across_folders_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kata/a/x.sh",
            "#!/bin/sh\n# ---\n# about: A\n# risk:  low\n# needs: token=one\n# ---\necho hi\n",
        );
        write(
            dir.path(),
            "kata/b/y.sh",
            "#!/bin/sh\n# ---\n# about: B\n# risk:  low\n# needs: token=one\n# ---\necho hi\n",
        );
        let report = check_all(&dir.path().join("kata"), &Vault::default()).unwrap();
        assert!(report.is_ok());
    }

    #[test]
    fn duplicate_alias_across_folders_fails() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kata/a/x.sh",
            "#!/bin/sh\n# ---\n# about: A\n# risk:  low\n# alias: deploy\n# ---\necho hi\n",
        );
        write(
            dir.path(),
            "kata/b/y.sh",
            "#!/bin/sh\n# ---\n# about: B\n# risk:  low\n# alias: deploy\n# ---\necho hi\n",
        );
        let report = check_all(&dir.path().join("kata"), &Vault::default()).unwrap();
        assert!(!report.is_ok());
    }

    #[test]
    fn render_report_includes_summary_line() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kata/starter/hello.sh", HELLO);
        let report = check_folder(&dir.path().join("kata"), "starter", &Vault::default()).unwrap();
        let text = render_report(&report, dir.path(), false);
        assert!(text.contains("checked 1 kata in starter   0 errors  0 warnings"));
    }

    #[test]
    fn multiple_conflicts_render_in_a_deterministic_sorted_order() {
        // PRD §4.7 requires insta snapshots of this output; a HashMap-ordered rendering would
        // flake across process runs once there is more than one conflict in a folder (G6).
        // Five names, deliberately not inserted in sorted order, make a HashMap's
        // (effectively random, but insertion-order-correlated) iteration order land on this
        // exact sequence by chance vanishingly unlikely.
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kata/a/x.sh",
            "#!/bin/sh\n# ---\n# about: A\n# risk:  low\n# needs: zulu=1 yankee=1 xray=1 whiskey=1 victor=1\n# ---\necho hi\n",
        );
        write(
            dir.path(),
            "kata/b/y.sh",
            "#!/bin/sh\n# ---\n# about: B\n# risk:  low\n# needs: zulu=2 yankee=2 xray=2 whiskey=2 victor=2\n# ---\necho hi\n",
        );
        let report = check_all(&dir.path().join("kata"), &Vault::default()).unwrap();
        let folder_a = report.folders.iter().find(|f| f.folder == "a").unwrap();
        let text = render_report(folder_a, dir.path(), false);

        let positions: Vec<(usize, &str)> = ["victor", "whiskey", "xray", "yankee", "zulu"]
            .iter()
            .map(|name| {
                (
                    text.find(&format!("need `{name}`")).unwrap_or_else(|| {
                        panic!("expected a conflict diagnostic for `{name}`: {text}")
                    }),
                    *name,
                )
            })
            .collect();
        let mut sorted = positions.clone();
        sorted.sort_by_key(|(pos, _)| *pos);
        assert_eq!(
            positions, sorted,
            "conflict diagnostics must render in sorted-by-name order: {text}"
        );
    }

    #[test]
    fn missing_need_warns_without_a_vault_entry_but_not_with_one() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kata/team/secret-task.sh",
            "#!/bin/sh\n# ---\n# about: Needs a secret\n# risk:  low\n# needs: api_token\n# ---\necho hi\n",
        );

        let without_entry =
            check_folder(&dir.path().join("kata"), "team", &Vault::default()).unwrap();
        assert_eq!(without_entry.warning_count(), 1);

        let mut vault = Vault::default();
        vault.set("api_token", "not-a-real-token", true);
        let with_entry = check_folder(&dir.path().join("kata"), "team", &vault).unwrap();
        assert_eq!(with_entry.warning_count(), 0);
    }
}
