//! Header fixture corpus (`docs/design/05-prd.md` §4.7, `08-shape-review.md` §6.2 risk 1):
//! the bespoke parser's error messages are its user interface, so `tests/fixtures/headers/
//! {good,bad}/` is snapshot-tested the way `tools/list` is.

use std::path::PathBuf;

fn fixtures_dir(sub: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/headers"
    ))
    .join(sub)
}

fn sorted_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    entries
}

#[test]
fn good_headers_parse_cleanly_and_match_their_snapshot() {
    for path in sorted_files(&fixtures_dir("good")) {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        let (header, diags) = kadou_core::parse_header(&source);
        assert!(
            !diags.iter().any(kadou_core::Diagnostic::is_error),
            "{name} should parse with no errors, got {diags:?}"
        );
        let header = header.unwrap_or_else(|| panic!("{name}: no header returned"));
        insta::assert_debug_snapshot!(format!("good_{name}"), header);
    }
}

#[test]
fn bad_headers_fail_and_match_their_diagnostic_snapshot() {
    for path in sorted_files(&fixtures_dir("bad")) {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        let (header, diags) = kadou_core::parse_header(&source);
        assert!(header.is_none(), "{name} should fail to parse");
        assert!(
            diags.iter().any(kadou_core::Diagnostic::is_error),
            "{name} should carry at least one error diagnostic"
        );
        insta::assert_debug_snapshot!(format!("bad_{name}"), diags);
    }
}
