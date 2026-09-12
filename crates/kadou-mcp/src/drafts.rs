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

use kadou_core::{Diagnostic, FolderReport, ScannedFile};

use crate::schema;

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
    #[error("invalid id `{id}`; expected folder/name segments matching {pattern}", pattern = schema::PROPOSE_ID_PATTERN)]
    InvalidId { id: String },
    #[error("source is {len} bytes, over the {cap} byte cap", cap = schema::PROPOSE_SOURCE_MAX_BYTES)]
    SourceTooLarge { len: usize },
}

/// `true` when every `/`-separated segment of `id` matches the PRD §4.2 segment rule
/// (`^[a-z0-9][a-z0-9-]*$`) and there are at least two segments -- exactly
/// [`schema::PROPOSE_ID_PATTERN`], checked by hand rather than a regex crate.
fn valid_propose_id(id: &str) -> bool {
    if id.len() > schema::PROPOSE_ID_MAX_LEN {
        return false;
    }
    let segments: Vec<&str> = id.split('/').collect();
    segments.len() >= 2 && segments.iter().all(|s| valid_id_segment(s))
}

fn valid_id_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Debug)]
pub struct ProposeOutcome {
    pub draft_id: String,
    pub path: PathBuf,
    pub diff: String,
    pub accept_command: String,
}

/// Renders every diagnostic exactly the way `kadou check` would (§5.4 "a bad header is
/// `invalid_args` with the same diagnostic text `kadou check` prints") -- location, source
/// snippet, carets, and fix line, not just the bare message (I-23). Wraps the parsed draft in
/// a one-file `FolderReport`, the same shape `check_path` builds for `kadou check <path>`, so
/// `render_report` produces identical text for free.
fn render_diagnostics(input_id: &str, source: &str, diagnostics: Vec<Diagnostic>) -> String {
    let scanned = ScannedFile {
        id: input_id.to_string(),
        path: PathBuf::from(format!("{input_id}.sh")),
        source: source.to_string(),
        header: None,
        diagnostics,
    };
    let report = FolderReport {
        folder: PROPOSED_NS.to_string(),
        files: vec![scanned],
    };
    kadou_core::render_report(&report, Path::new(""), false)
}

/// Canonicalizes the resolved *parent* before writing (not the target after, I-13): verifies
/// containment under `state_dir`'s `proposed/` root, then writes `source` at `<proposed_root>/
/// <input_id>.sh` and returns that path.
fn write_draft_contained(
    state_dir: &Path,
    input_id: &str,
    source: &str,
) -> Result<PathBuf, ProposeError> {
    let proposed_root = state_dir.join(PROPOSED_NS);
    kadou_core::fsutil::ensure_dir_0700(&proposed_root).map_err(|source| ProposeError::Write {
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
    // `target` is always `proposed_root` joined with a non-empty relative path, so it always
    // has a parent in practice; failing closed here instead of an `expect` keeps this fn
    // panic-free even if that construction ever changes (R6).
    let Some(target_parent) = target.parent() else {
        return Err(ProposeError::Write {
            path: target,
            source: std::io::Error::other("target path has no parent directory"),
        });
    };
    kadou_core::fsutil::ensure_dir_0700(target_parent).map_err(|source| ProposeError::Write {
        path: target_parent.to_path_buf(),
        source,
    })?;
    let canonical_parent = target_parent
        .canonicalize()
        .map_err(|source| ProposeError::Write {
            path: target_parent.to_path_buf(),
            source,
        })?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(ProposeError::PathEscape);
    }

    kadou_core::fsutil::write_atomic_0600(&target, source.as_bytes()).map_err(|source_err| {
        ProposeError::Write {
            path: target.clone(),
            source: source_err,
        }
    })?;
    Ok(target)
}

/// A unified diff against the currently-accepted kata's source, or against `/dev/null` when
/// `input_id` names no existing kata (§6.7).
fn render_diff(kata_dir: &Path, input_id: &str, source: &str) -> String {
    let existing = existing_kata_source(kata_dir, input_id);
    let label = format!("{input_id}.sh");
    let old_label = if existing.is_some() {
        label.as_str()
    } else {
        "/dev/null"
    };
    similar::TextDiff::from_lines(existing.as_deref().unwrap_or(""), source)
        .unified_diff()
        .header(old_label, &label)
        .to_string()
}

/// Writes `source` as a draft at `<state_dir>/proposed/<input_id>.sh`, strict-loading the
/// header first (§6.7 "strict-loads the header at propose time"). `rmcp` does not validate
/// `inputSchema` server-side (I-13), so `input_id` and `source` are validated here against
/// the exact same shape and size cap the schema declares (`schema::PROPOSE_ID_PATTERN`,
/// `schema::PROPOSE_SOURCE_MAX_BYTES`) before anything is written — the schema alone is
/// advisory to a client, not enforcement. The resolved parent directory is canonicalized and
/// checked for containment under `proposed/` *before* the write, not after (§5.4).
pub fn propose(
    state_dir: &Path,
    kata_dir: &Path,
    input_id: &str,
    source: &str,
) -> Result<ProposeOutcome, ProposeError> {
    if source.len() > schema::PROPOSE_SOURCE_MAX_BYTES {
        return Err(ProposeError::SourceTooLarge { len: source.len() });
    }
    if !valid_propose_id(input_id) {
        return Err(ProposeError::InvalidId {
            id: input_id.to_string(),
        });
    }

    let (header, diagnostics) = kadou_core::parse_header(source);
    if header.is_none() {
        return Err(ProposeError::BadHeader(render_diagnostics(
            input_id,
            source,
            diagnostics,
        )));
    }

    let target = write_draft_contained(state_dir, input_id, source)?;
    let diff = render_diff(kata_dir, input_id, source);

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
    fn propose_with_a_bad_risk_word_returns_the_same_text_kadou_check_prints() {
        // §5.4: "a bad header is invalid_args with the same diagnostic text kadou check
        // prints" -- not just the bare message, but the fix line and the location/caret
        // rendering render_diagnostic produces (I-23).
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        let source = "#!/bin/sh\n# ---\n# about: Bad risk\n# risk:  mediun\n# ---\necho hi\n";
        let err = propose(state_dir.path(), kata_dir.path(), "sesami/bad", source).unwrap_err();
        let ProposeError::BadHeader(text) = err else {
            panic!("expected BadHeader, got {err:?}");
        };
        assert!(
            text.contains("= risk is one of low, medium, high, critical"),
            "{text}"
        );
        assert!(text.contains("-->"), "expected a --> location line: {text}");
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
    fn propose_rejects_an_id_that_escapes_the_proposed_root() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        let err = propose(state_dir.path(), kata_dir.path(), "a/../../x", HELLO).unwrap_err();
        assert!(matches!(err, ProposeError::InvalidId { .. }), "{err:?}");
        assert!(!state_dir.path().join("x.sh").exists());
        assert!(!state_dir.path().parent().unwrap().join("x.sh").exists());
    }

    #[test]
    fn propose_rejects_an_oversized_source() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        let huge = format!(
            "#!/bin/sh\n# ---\n# about: {}\n# risk:  low\n# ---\necho hi\n",
            "a".repeat(schema::PROPOSE_SOURCE_MAX_BYTES)
        );
        let err = propose(state_dir.path(), kata_dir.path(), "sesami/big", &huge).unwrap_err();
        assert!(
            matches!(err, ProposeError::SourceTooLarge { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn propose_rejects_an_uppercase_id() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        let err = propose(state_dir.path(), kata_dir.path(), "Sesami/Bad", HELLO).unwrap_err();
        assert!(matches!(err, ProposeError::InvalidId { .. }), "{err:?}");
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
