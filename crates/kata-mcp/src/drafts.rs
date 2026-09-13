//! Drafts: `propose_kata`'s write path and the `proposed`/`mined` draft namespaces
//! (`docs/design/05-prd.md` §6.7, §4.1, §5.3).
//!
//! Drafts live under the state dir (`~/.local/state/kata/{proposed,mined}/`), never under
//! `~/.config/kata/catalog/`, and are listable/describable regardless of an agent's risk
//! ceiling — but `run_kata` on one always fails with `error: draft` (§5.3). `proposed/` and
//! `mined/` are ordinary namespace directories to the existing folder scanner, so listing
//! reuses [`kata_core::scan_folder`] unchanged: `scan_folder(state_dir, "proposed")` yields
//! ids like `proposed/sesami/argocd-sync` exactly the way `scan_folder(kata_dir, "sesami")`
//! yields `sesami/cc4-aaa` for the real library.

use std::path::{Path, PathBuf};

use kata_core::{Diagnostic, FolderReport, ScannedFile};

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
            && let Ok(files) = kata_core::scan_folder(state_dir, ns)
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
/// (`^[a-z0-9][a-z0-9-]*$`, [`kata_core::valid_id_segment`]) and there are at least two
/// segments -- exactly [`schema::PROPOSE_ID_PATTERN`], checked by hand rather than a regex
/// crate.
fn valid_propose_id(id: &str) -> bool {
    if id.len() > schema::PROPOSE_ID_MAX_LEN {
        return false;
    }
    let segments: Vec<&str> = id.split('/').collect();
    segments.len() >= 2 && segments.iter().all(|s| kata_core::valid_id_segment(s))
}

#[derive(Debug)]
pub struct ProposeOutcome {
    pub draft_id: String,
    pub path: PathBuf,
    pub diff: String,
    pub accept_command: String,
}

/// Renders every diagnostic exactly the way `kata check` would (§5.4 "a bad header is
/// `invalid_args` with the same diagnostic text `kata check` prints") -- location, source
/// snippet, carets, and fix line, not just the bare message (I-23). Wraps the parsed draft in
/// a one-file `FolderReport`, the same shape `check_path` builds for `kata check <path>`, so
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
    kata_core::render_report(&report, Path::new(""), false)
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
    kata_core::fsutil::ensure_dir_0700(&proposed_root).map_err(|source| ProposeError::Write {
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
    kata_core::fsutil::ensure_dir_0700(target_parent).map_err(|source| ProposeError::Write {
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

    kata_core::fsutil::write_atomic_0600(&target, source.as_bytes()).map_err(|source_err| {
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

    let (header, diagnostics) = kata_core::parse_header(source);
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
        accept_command: format!("kata accept {input_id}"),
    })
}

/// The currently-accepted kata's source, if `input_id` already names one under `kata_dir`
/// (§6.7 "the diff is computed against the currently accepted kata when one exists").
fn existing_kata_source(kata_dir: &Path, input_id: &str) -> Option<String> {
    match kata_core::find_kata(kata_dir, input_id).ok()? {
        kata_core::LookupResult::Found(kata) => std::fs::read_to_string(&kata.path).ok(),
        _ => None,
    }
}

// -----------------------------------------------------------------------
// kata accept (§6.7, §9 slice 7)
// -----------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AcceptError {
    #[error("no such draft `{0}`")]
    DraftNotFound(String),
    #[error("`kata accept {0}` needs --into <folder>; a mined draft has no folder of its own")]
    IntoRequired(String),
    #[error("{0}")]
    InvalidHeader(String),
    #[error(
        "{folder} is a git-backed folder; kata accept only copies into a user-owned folder, never a git checkout"
    )]
    GitBackedTarget { folder: String },
    #[error("failed to {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The parts of an `accept` id: which draft namespace it names, the path (no extension) under
/// that namespace's directory, the folder to default `--into` to (mined drafts have none), and
/// the name the kata keeps once copied into its target folder.
struct AcceptId {
    namespace: &'static str,
    draft_relpath: String,
    default_folder: Option<String>,
    own_name: String,
}

fn parse_accept_id(id: &str) -> Option<AcceptId> {
    if !id.split('/').all(kata_core::valid_id_segment) {
        return None;
    }
    let (first, rest) = id.split_once('/')?;
    if first == MINED_NS {
        Some(AcceptId {
            namespace: MINED_NS,
            draft_relpath: rest.to_string(),
            default_folder: None,
            own_name: rest.to_string(),
        })
    } else {
        Some(AcceptId {
            namespace: PROPOSED_NS,
            draft_relpath: id.to_string(),
            default_folder: Some(first.to_string()),
            own_name: rest.to_string(),
        })
    }
}

/// A prepared `kata accept`: everything validated and diffed, nothing written yet — the CLI
/// prints [`Self::diff`] and prompts `y/N` before calling [`apply_accept`].
#[derive(Debug)]
pub struct AcceptPreparation {
    source: String,
    pub draft_path: PathBuf,
    pub target_path: PathBuf,
    pub new_id: String,
    pub diff: String,
    /// `false` when the target folder does not exist yet -- [`apply_accept`] creates it, and
    /// the CLI prints `created folder <name>` (§6.7, carried over from §9 slice 7: a
    /// nonexistent folder cannot be a git checkout, so it is user-owned).
    pub target_folder_exists: bool,
}

/// `kata accept <id> [--into <folder>]` (§6.7): resolves `id` to a draft under `proposed/`
/// (the default, `id` itself being `folder/name`) or `mined/` (`id` is `mined/name`, which has
/// no folder of its own and requires `into`), strict-loads its header, and refuses a target
/// folder that is a git checkout. An existing or new user-owned folder is accepted; a missing
/// one is created by [`apply_accept`]. Nothing is written yet.
pub fn prepare_accept(
    state_dir: &Path,
    kata_dir: &Path,
    id: &str,
    into: Option<&str>,
) -> Result<AcceptPreparation, AcceptError> {
    let parsed = parse_accept_id(id).ok_or_else(|| AcceptError::DraftNotFound(id.to_string()))?;
    let target_folder = into
        .map(str::to_string)
        .or(parsed.default_folder)
        .ok_or_else(|| AcceptError::IntoRequired(id.to_string()))?;

    let draft_path = state_dir
        .join(parsed.namespace)
        .join(format!("{}.sh", parsed.draft_relpath));
    let source = std::fs::read_to_string(&draft_path)
        .map_err(|_| AcceptError::DraftNotFound(id.to_string()))?;

    let (header, diagnostics) = kata_core::parse_header(&source);
    if header.is_none() {
        return Err(AcceptError::InvalidHeader(render_diagnostics(
            id,
            &source,
            diagnostics,
        )));
    }

    let target_dir = kata_dir.join(&target_folder);
    // A nonexistent folder cannot be a git checkout, so `is_git_backed` naturally returns
    // `false` for it (its `canonicalize()` fails first) -- no separate existence check needed
    // before this one.
    if kata_core::git::is_git_backed(&target_dir) {
        return Err(AcceptError::GitBackedTarget {
            folder: target_folder,
        });
    }
    let target_folder_exists = target_dir.is_dir();

    let new_id = format!("{target_folder}/{}", parsed.own_name);
    let target_path = kata_dir.join(format!("{new_id}.sh"));
    let diff = render_diff(kata_dir, &new_id, &source);

    Ok(AcceptPreparation {
        source,
        draft_path,
        target_path,
        new_id,
        diff,
        target_folder_exists,
    })
}

/// Copies the draft's source into its prepared target path and removes the draft (§6.7
/// "after accept the draft file is removed").
pub fn apply_accept(prep: &AcceptPreparation) -> Result<(), AcceptError> {
    if let Some(parent) = prep.target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| AcceptError::Io {
            action: "create",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(&prep.target_path, &prep.source).map_err(|source| AcceptError::Io {
        action: "write",
        path: prep.target_path.clone(),
        source,
    })?;
    std::fs::remove_file(&prep.draft_path).map_err(|source| AcceptError::Io {
        action: "remove",
        path: prep.draft_path.clone(),
        source,
    })
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
        assert_eq!(outcome.accept_command, "kata accept sesami/argocd-sync");

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
        // §5.4: "a bad header is invalid_args with the same diagnostic text kata check
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
    fn source_at_the_byte_cap_is_accepted() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        let prefix = "#!/bin/sh\n# ---\n# about: pad\n# risk:  low\n# ---\necho hi\n# ";
        let padding = schema::PROPOSE_SOURCE_MAX_BYTES - prefix.len();
        let source = format!("{prefix}{}", "x".repeat(padding));
        assert_eq!(source.len(), schema::PROPOSE_SOURCE_MAX_BYTES);
        propose(state_dir.path(), kata_dir.path(), "sesami/at-cap", &source).unwrap();
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
    fn propose_id_at_the_length_cap_is_accepted_one_over_is_rejected() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();

        let at_cap = format!("{}/b", "a".repeat(schema::PROPOSE_ID_MAX_LEN - 2));
        assert_eq!(at_cap.len(), schema::PROPOSE_ID_MAX_LEN);
        propose(state_dir.path(), kata_dir.path(), &at_cap, HELLO).unwrap();

        let over_cap = format!("{}/b", "a".repeat(schema::PROPOSE_ID_MAX_LEN - 1));
        assert_eq!(over_cap.len(), schema::PROPOSE_ID_MAX_LEN + 1);
        let err = propose(state_dir.path(), kata_dir.path(), &over_cap, HELLO).unwrap_err();
        assert!(matches!(err, ProposeError::InvalidId { .. }), "{err:?}");
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

    // -----------------------------------------------------------------------
    // kata accept (§6.7, §9 slice 7)
    // -----------------------------------------------------------------------

    #[test]
    fn accept_prepares_a_diff_against_dev_null_for_a_new_kata_in_its_own_folder() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("ops")).unwrap();
        propose(state_dir.path(), kata_dir.path(), "ops/hello-team", HELLO).unwrap();

        let prep =
            prepare_accept(state_dir.path(), kata_dir.path(), "ops/hello-team", None).unwrap();

        assert_eq!(prep.new_id, "ops/hello-team");
        assert_eq!(prep.target_path, kata_dir.path().join("ops/hello-team.sh"));
        assert!(prep.diff.contains("/dev/null"));
        assert!(!prep.target_path.exists(), "apply hasn't run yet");
    }

    #[test]
    fn apply_accept_writes_the_kata_and_removes_the_draft() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("ops")).unwrap();
        propose(state_dir.path(), kata_dir.path(), "ops/hello-team", HELLO).unwrap();
        let prep =
            prepare_accept(state_dir.path(), kata_dir.path(), "ops/hello-team", None).unwrap();

        apply_accept(&prep).unwrap();

        assert_eq!(std::fs::read_to_string(&prep.target_path).unwrap(), HELLO);
        assert!(
            !prep.draft_path.exists(),
            "the draft must be gone after accept"
        );
    }

    #[test]
    fn accept_into_a_different_folder_uses_only_the_drafts_own_name() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("sesami")).unwrap();
        std::fs::create_dir_all(kata_dir.path().join("ops")).unwrap();
        propose(
            state_dir.path(),
            kata_dir.path(),
            "sesami/argocd-sync",
            HELLO,
        )
        .unwrap();

        let prep = prepare_accept(
            state_dir.path(),
            kata_dir.path(),
            "sesami/argocd-sync",
            Some("ops"),
        )
        .unwrap();

        assert_eq!(prep.new_id, "ops/argocd-sync");
        assert_eq!(prep.target_path, kata_dir.path().join("ops/argocd-sync.sh"));
    }

    #[test]
    fn accept_diffs_against_the_existing_kata_at_the_target_id() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("ops")).unwrap();
        std::fs::write(kata_dir.path().join("ops/hello-team.sh"), HELLO).unwrap();
        let changed = "#!/bin/sh\n# ---\n# about: Say hello loudly\n# risk:  low\n# ---\necho HI\n";
        propose(state_dir.path(), kata_dir.path(), "ops/hello-team", changed).unwrap();

        let prep =
            prepare_accept(state_dir.path(), kata_dir.path(), "ops/hello-team", None).unwrap();

        assert!(!prep.diff.contains("/dev/null"));
        assert!(prep.diff.contains("-echo hi"));
        assert!(prep.diff.contains("+echo HI"));
    }

    #[test]
    fn accept_of_an_unknown_draft_is_a_clean_error() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();

        let err =
            prepare_accept(state_dir.path(), kata_dir.path(), "sesami/nope", None).unwrap_err();
        assert!(matches!(err, AcceptError::DraftNotFound(_)), "{err:?}");
    }

    #[test]
    fn accept_of_a_mined_draft_without_into_is_refused() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(state_dir.path().join("mined")).unwrap();
        std::fs::write(state_dir.path().join("mined/k8s-pod-logs.sh"), HELLO).unwrap();

        let err = prepare_accept(
            state_dir.path(),
            kata_dir.path(),
            "mined/k8s-pod-logs",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AcceptError::IntoRequired(_)), "{err:?}");
    }

    #[test]
    fn accept_of_a_mined_draft_with_into_copies_into_that_folder() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("ops")).unwrap();
        std::fs::create_dir_all(state_dir.path().join("mined")).unwrap();
        std::fs::write(state_dir.path().join("mined/k8s-pod-logs.sh"), HELLO).unwrap();

        let prep = prepare_accept(
            state_dir.path(),
            kata_dir.path(),
            "mined/k8s-pod-logs",
            Some("ops"),
        )
        .unwrap();
        assert_eq!(prep.new_id, "ops/k8s-pod-logs");
    }

    #[test]
    fn accept_creates_a_missing_target_folder() {
        // §6.7 (carried over from slice 7): a nonexistent folder cannot be a git checkout, so
        // it is user-owned -- accept creates it rather than refusing.
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        propose(state_dir.path(), kata_dir.path(), "ops/hello-team", HELLO).unwrap();

        let prep =
            prepare_accept(state_dir.path(), kata_dir.path(), "ops/hello-team", None).unwrap();
        assert!(!prep.target_folder_exists);
        assert!(!kata_dir.path().join("ops").exists(), "not created yet");

        apply_accept(&prep).unwrap();

        assert!(kata_dir.path().join("ops").is_dir());
        assert_eq!(
            std::fs::read_to_string(kata_dir.path().join("ops/hello-team.sh")).unwrap(),
            HELLO
        );
    }

    #[test]
    fn accept_refuses_a_git_backed_target_folder() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        let team_dir = kata_dir.path().join("team");
        std::fs::create_dir_all(&team_dir).unwrap();
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&team_dir)
            .args(["init", "-q"])
            .status()
            .unwrap();
        assert!(status.success());
        propose(state_dir.path(), kata_dir.path(), "team/hello-team", HELLO).unwrap();

        let err =
            prepare_accept(state_dir.path(), kata_dir.path(), "team/hello-team", None).unwrap_err();
        assert!(
            matches!(err, AcceptError::GitBackedTarget { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn accept_of_a_bad_header_draft_fails_with_check_style_diagnostics() {
        let state_dir = tempfile::tempdir().unwrap();
        let kata_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(kata_dir.path().join("ops")).unwrap();
        std::fs::create_dir_all(state_dir.path().join("proposed/ops")).unwrap();
        // Written directly (bypassing propose's own strict-load), the way a mined draft's
        // pipeline could produce a malformed file: `kata accept` must catch it too.
        std::fs::write(
            state_dir.path().join("proposed/ops/bad.sh"),
            "#!/bin/sh\n# ---\n# risk:  mediun\n# ---\necho hi\n",
        )
        .unwrap();

        let err = prepare_accept(state_dir.path(), kata_dir.path(), "ops/bad", None).unwrap_err();
        let AcceptError::InvalidHeader(text) = err else {
            panic!("expected InvalidHeader, got {err:?}");
        };
        assert!(text.contains("-->"), "expected a --> location line: {text}");
        assert!(
            text.contains("= risk is one of low, medium, high, critical"),
            "{text}"
        );
    }
}
