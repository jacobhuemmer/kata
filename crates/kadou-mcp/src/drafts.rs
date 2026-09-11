//! Drafts: `propose_kata`'s write path and the `proposed`/`mined` draft namespaces
//! (`docs/design/05-prd.md` §6.7, §4.1, §5.3).
//!
//! Drafts live under the state dir (`~/.local/state/kadou/{proposed,mined}/`), never under
//! `~/.config/kadou/kata/`, and are listable/describable regardless of an agent's risk
//! ceiling — but `run_kata` on one always fails with `error: draft` (§5.3). `proposed/` and
//! `mined/` are ordinary namespace directories to the existing folder scanner, so listing
//! reuses [`kadou_core::scan_folder`] unchanged: `scan_folder(state_dir, "proposed")` yields
//! ids like `proposed/sesami/argocd-sync` exactly the way `scan_folder(kata_dir, "sesami")`
//! yields `sesami/cc4-aaa` for the real library.

use std::path::{Path, PathBuf};

use kadou_core::{Diagnostic, ScannedFile};

pub const PROPOSED_NS: &str = "proposed";
pub const MINED_NS: &str = "mined";

/// `true` when `id` names a draft namespace (`proposed/...` or `mined/...`) — such an id is
/// never runnable, even if a file happens to exist there (§5.3).
pub fn is_draft_id(id: &str) -> bool {
    let top = id.split('/').next().unwrap_or(id);
    top == PROPOSED_NS || top == MINED_NS
}

/// Every draft under `state_dir`, from both namespaces that exist. Missing namespace
/// directories scan as empty, not an error (mirrors `scan_kata_dir`'s "a missing kata_dir
/// scans as empty").
pub fn scan_drafts(state_dir: &Path) -> Vec<ScannedFile> {
    let mut out = Vec::new();
    for ns in [PROPOSED_NS, MINED_NS] {
        if state_dir.join(ns).is_dir()
            && let Ok(files) = kadou_core::scan_folder(state_dir, ns)
        {
            out.extend(files);
        }
    }
    out
}

/// Looks up one draft by its full id (`proposed/sesami/argocd-sync`, `mined/k8s-pod-logs`).
pub fn find_draft(state_dir: &Path, id: &str) -> Option<ScannedFile> {
    scan_drafts(state_dir).into_iter().find(|f| f.id == id)
}

#[derive(Debug, thiserror::Error)]
pub enum ProposeError {
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the resolved draft path escapes the proposed/ root")]
    PathEscape,
    #[error("header error: {0}")]
    BadHeader(String),
}

#[derive(Debug)]
pub struct ProposeOutcome {
    pub draft_id: String,
    pub path: PathBuf,
    pub diff: String,
    pub accept_command: String,
}

/// Renders every error diagnostic the same way `kadou check` would (§5.4 "a bad header is
/// `invalid_args` with the same diagnostic text `kadou check` prints").
fn render_diagnostics(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Writes `source` as a draft at `<state_dir>/proposed/<input_id>.sh`, strict-loading the
/// header first (§6.7 "strict-loads the header at propose time"). `input_id` is the tool's
/// `id` argument (already schema-validated: lowercase/digits/hyphens and `/` only, so a
/// `..` path-traversal segment is structurally impossible — the canonicalization check below
/// is defense in depth, §5.4).
pub fn propose(
    state_dir: &Path,
    kata_dir: &Path,
    input_id: &str,
    source: &str,
) -> Result<ProposeOutcome, ProposeError> {
    let (header, diagnostics) = kadou_core::parse_header(source);
    if header.is_none() {
        return Err(ProposeError::BadHeader(render_diagnostics(&diagnostics)));
    }

    let proposed_root = state_dir.join(PROPOSED_NS);
    std::fs::create_dir_all(&proposed_root).map_err(|source| ProposeError::Write {
        path: proposed_root.clone(),
        source,
    })?;
    let canonical_root = proposed_root
        .canonicalize()
        .map_err(|source| ProposeError::Write {
            path: proposed_root.clone(),
            source,
        })?;

    let target = proposed_root.join(format!("{input_id}.sh"));
    if target
        .parent()
        .is_some_and(|p| !p.starts_with(&proposed_root))
    {
        return Err(ProposeError::PathEscape);
    }

    kadou_core::fsutil::write_atomic_0600(&target, source.as_bytes()).map_err(|source_err| {
        ProposeError::Write {
            path: target.clone(),
            source: source_err,
        }
    })?;

    // Defense in depth (§5.4): confirm the file we just wrote actually landed under the
    // canonical proposed/ root.
    let canonical_target = target
        .canonicalize()
        .map_err(|source| ProposeError::Write {
            path: target.clone(),
            source,
        })?;
    if !canonical_target.starts_with(&canonical_root) {
        let _ = std::fs::remove_file(&target);
        return Err(ProposeError::PathEscape);
    }

    let existing = existing_kata_source(kata_dir, input_id);
    let label = format!("{input_id}.sh");
    let old_label = if existing.is_some() {
        label.as_str()
    } else {
        "/dev/null"
    };
    let diff = similar::TextDiff::from_lines(existing.as_deref().unwrap_or(""), source)
        .unified_diff()
        .header(old_label, &label)
        .to_string();

    Ok(ProposeOutcome {
        draft_id: format!("{PROPOSED_NS}/{input_id}"),
        path: target,
        diff,
        accept_command: format!("kadou accept {input_id}"),
    })
}

/// The currently-accepted kata's source, if `input_id` already names one under `kata_dir`
/// (§6.7 "the diff is computed against the currently accepted kata when one exists").
fn existing_kata_source(kata_dir: &Path, input_id: &str) -> Option<String> {
    match kadou_core::find_kata(kata_dir, input_id).ok()? {
        kadou_core::LookupResult::Found(kata) => std::fs::read_to_string(&kata.path).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = "#!/bin/sh\n# ---\n# about: Say hello\n# risk:  low\n# ---\necho hi\n";

    #[test]
    fn is_draft_id_recognizes_both_namespaces() {
        assert!(is_draft_id("proposed/sesami/argocd-sync"));
        assert!(is_draft_id("mined/k8s-pod-logs"));
        assert!(!is_draft_id("sesami/cc4-aaa"));
    }

    #[test]
    fn propose_writes_a_draft_and_diffs_against_dev_null_when_new() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();

        let outcome = propose(
            state_dir.path(),
            kata_dir.path(),
            "sesami/argocd-sync",
            HELLO,
        )
        .unwrap();
        assert_eq!(outcome.draft_id, "proposed/sesami/argocd-sync");
        assert!(outcome.path.is_file());
        assert!(outcome.diff.contains("/dev/null"));
        assert_eq!(outcome.accept_command, "kadou accept sesami/argocd-sync");

        let drafts = scan_drafts(state_dir.path());
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].id, "proposed/sesami/argocd-sync");
    }

    #[test]
    fn propose_rejects_a_bad_header() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        let err = propose(
            state_dir.path(),
            kata_dir.path(),
            "sesami/bad",
            "no header here\n",
        )
        .unwrap_err();
        assert!(matches!(err, ProposeError::BadHeader(_)));
    }

    #[test]
    fn propose_diffs_against_the_existing_accepted_kata() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("sesami")).unwrap();
        std::fs::write(kata_dir.path().join("sesami/cc4-aaa.sh"), HELLO).unwrap();

        let changed = "#!/bin/sh\n# ---\n# about: Say hello loudly\n# risk:  low\n# ---\necho HI\n";
        let outcome =
            propose(state_dir.path(), kata_dir.path(), "sesami/cc4-aaa", changed).unwrap();
        assert!(!outcome.diff.contains("/dev/null"));
        assert!(outcome.diff.contains("-echo hi"));
        assert!(outcome.diff.contains("+echo HI"));
    }

    #[test]
    fn re_proposing_the_same_id_overwrites_the_prior_draft() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        propose(state_dir.path(), kata_dir.path(), "sesami/x", HELLO).unwrap();
        let second = "#!/bin/sh\n# ---\n# about: Different\n# risk:  low\n# ---\necho bye\n";
        propose(state_dir.path(), kata_dir.path(), "sesami/x", second).unwrap();

        let drafts = scan_drafts(state_dir.path());
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].header.as_ref().unwrap().about, "Different");
    }
}
