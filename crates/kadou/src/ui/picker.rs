//! The inline fzf-style picker (`docs/design/05-prd.md` §7.3; `09-tui-decision.md` §3.3).
//!
//! Split the same way `confirm::confirm_protocol` is: [`filter`] and [`apply_key`] are pure
//! functions with no terminal I/O, so the ranking and key-handling logic is unit-testable
//! without a real pty. The actual `crossterm` raw-mode event loop (in `main.rs`/`commands.rs`)
//! is thin glue around these two functions plus [`render_list`]/[`render_preview`], verified
//! manually over a real terminal (`CLAUDE.md` forced verification).

use kadou_core::RiskLevel;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::ui::style;

/// At most this many rows are shown at once; the list scrolls past that (§7.3).
pub const MAX_ROWS: usize = 12;

#[derive(Debug, Clone)]
pub struct PickCandidate {
    pub id: String,
    pub about: String,
    pub alias: Vec<String>,
    pub risk: RiskLevel,
}

/// Fuzzy-filters `candidates` by `query`: id matches are always ranked above about/alias
/// matches (§7.3 "Fuzzy match, id ranked above about"), each group ordered by its own nucleo
/// score. An empty query matches every candidate, in the given order.
pub fn filter<'a>(query: &str, candidates: &'a [PickCandidate]) -> Vec<&'a PickCandidate> {
    if query.is_empty() {
        return candidates.iter().collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();

    let mut id_matches: Vec<(u32, &PickCandidate)> = Vec::new();
    let mut about_matches: Vec<(u32, &PickCandidate)> = Vec::new();

    for candidate in candidates {
        if let Some(score) = pattern.score(Utf32Str::new(&candidate.id, &mut buf), &mut matcher) {
            id_matches.push((score, candidate));
            continue;
        }
        let haystack = format!("{} {}", candidate.about, candidate.alias.join(" "));
        if let Some(score) = pattern.score(Utf32Str::new(&haystack, &mut buf), &mut matcher) {
            about_matches.push((score, candidate));
        }
    }

    id_matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    about_matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    id_matches
        .into_iter()
        .chain(about_matches)
        .map(|(_, c)| c)
        .collect()
}

/// The picker's list block: the filter header, up to [`MAX_ROWS`] rows, or the no-matches
/// line (§7.3 `no kata matches "xyz"   kadou new sesami/xyz`).
pub fn render_list(
    command_label: &str,
    query: &str,
    matches: &[&PickCandidate],
    styled: bool,
) -> String {
    let mut out = format!(
        " {} {command_label} which kata?  {query}\u{2588}\n",
        style::MARKER
    );
    if matches.is_empty() {
        out.push_str(&format!(
            "   no kata matches \"{query}\"   kadou new {query}\n"
        ));
        return out;
    }
    let width = matches
        .iter()
        .take(MAX_ROWS)
        .map(|c| c.id.len())
        .max()
        .unwrap_or(0)
        + 3;
    for candidate in matches.iter().take(MAX_ROWS) {
        out.push_str(&format!(
            "   {:width$}{}   {}\n",
            candidate.id,
            style::risk_badge(candidate.risk, styled),
            candidate.about
        ));
    }
    out
}

#[derive(Debug, Clone)]
pub struct PreviewNeed {
    pub name: String,
    pub satisfied: bool,
}

#[derive(Debug, Clone)]
pub struct PreviewArg {
    pub name: String,
    /// Pre-rendered `type   required` or `type   = "default"` plus help text.
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub id: String,
    pub risk: RiskLevel,
    pub about: String,
    pub needs: Vec<PreviewNeed>,
    pub args: Vec<PreviewArg>,
    pub file: String,
    pub sha_short: String,
}

/// The header preview block below the picker list: the highlighted kata's own `kadou show`
/// summary (§7.3 "the highlighted kata's header shows below the list").
pub fn render_preview(preview: &Preview, styled: bool) -> String {
    let mut out = format!("{}\n", "─".repeat(72));
    out.push_str(&format!(
        " {}   {}   {}\n",
        preview.id,
        style::risk_badge(preview.risk, styled),
        preview.about
    ));
    if !preview.needs.is_empty() {
        let names: Vec<&str> = preview.needs.iter().map(|n| n.name.as_str()).collect();
        let mark = if preview.needs.iter().all(|n| n.satisfied) {
            format!("{} vault", style::ok_mark(styled))
        } else {
            format!("{} missing", style::err_mark(styled))
        };
        out.push_str(&format!(" needs  {}   {mark}\n", names.join(" ")));
    }
    for arg in &preview.args {
        out.push_str(&format!(" args   {:<12} {}\n", arg.name, arg.summary));
    }
    out.push_str(&format!(
        " file   {}   sha {}\n",
        preview.file, preview.sha_short
    ));
    out.push_str(" ↑↓ move   ↵ run   tab show   e edit   esc cancel\n");
    out
}

// ---------------------------------------------------------------------------
// Key handling (§7.3 "Picker and prompt keys" table)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKey {
    /// A printable character. `'e'` is the reserved edit hotkey (§9 slice 8 interpretation
    /// call, in the handoff: the key table lists both "printable characters filter" and a
    /// bare `e` action, so a literal `e` cannot be typed into the query in this
    /// implementation).
    Char(char),
    Backspace,
    Up,
    Down,
    Enter,
    Tab,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerAction {
    Continue,
    /// Select the candidate at this index into the *current filtered* list, and continue the
    /// command (§7.3 `↵`).
    Select(usize),
    /// Print the highlighted kata's full `kadou show` and return (§7.3 `tab`).
    ShowFull(usize),
    /// Open the highlighted kata in `$EDITOR` and return (§7.3 `e`).
    EditAndReturn(usize),
    /// Cancel, exit 130 (§7.3 `esc`/`Ctrl+c`).
    Cancel,
}

#[derive(Debug, Clone, Default)]
pub struct PickerState {
    pub query: String,
    pub selected: usize,
}

/// The pure key-handling decision: given the current state and how many candidates the
/// *current* query matches, what does this keypress do. Selection wraps; typing or
/// backspacing always resets it to the top match, the same way fzf and friends behave.
pub fn apply_key(state: &mut PickerState, key: PickerKey, match_count: usize) -> PickerAction {
    match key {
        PickerKey::Char('e') => {
            if match_count == 0 {
                PickerAction::Continue
            } else {
                PickerAction::EditAndReturn(state.selected)
            }
        }
        PickerKey::Char(c) => {
            state.query.push(c);
            state.selected = 0;
            PickerAction::Continue
        }
        PickerKey::Backspace => {
            state.query.pop();
            state.selected = 0;
            PickerAction::Continue
        }
        PickerKey::Down if match_count > 0 => {
            state.selected = (state.selected + 1) % match_count;
            PickerAction::Continue
        }
        PickerKey::Up if match_count > 0 => {
            state.selected = (state.selected + match_count - 1) % match_count;
            PickerAction::Continue
        }
        PickerKey::Down | PickerKey::Up => PickerAction::Continue,
        PickerKey::Enter if match_count > 0 => PickerAction::Select(state.selected),
        PickerKey::Tab if match_count > 0 => PickerAction::ShowFull(state.selected),
        PickerKey::Enter | PickerKey::Tab => PickerAction::Continue,
        PickerKey::Escape => PickerAction::Cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, about: &str) -> PickCandidate {
        PickCandidate {
            id: id.to_string(),
            about: about.to_string(),
            alias: Vec::new(),
            risk: RiskLevel::Low,
        }
    }

    #[test]
    fn empty_query_matches_everything_in_order() {
        let candidates = vec![candidate("a/x", "one"), candidate("b/y", "two")];
        let matches = filter("", &candidates);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].id, "a/x");
    }

    #[test]
    fn id_matches_rank_above_about_matches() {
        // "deploy" appears in one kata's about text and another kata's id -- the id match
        // must be ranked first regardless of nucleo's own raw score for either.
        let candidates = vec![
            candidate("sesami/ses-release-build", "Run the deploy pipeline"),
            candidate("sesami/ses-deploy", "Trigger the SES Deploy pipeline"),
        ];
        let matches = filter("deploy", &candidates);
        assert_eq!(matches.len(), 2);
        assert_eq!(
            matches[0].id, "sesami/ses-deploy",
            "id match must rank first"
        );
        assert_eq!(matches[1].id, "sesami/ses-release-build");
    }

    #[test]
    fn alias_also_matches_like_about() {
        let mut c = candidate("starter/hello", "Print a greeting");
        c.alias = vec!["hi".to_string()];
        let matches = filter("hi", std::slice::from_ref(&c));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn no_match_returns_an_empty_list() {
        let candidates = vec![candidate("starter/hello", "Print a greeting")];
        assert!(filter("zzz-nope", &candidates).is_empty());
    }

    #[test]
    fn render_list_shows_no_matches_hint_with_the_new_command() {
        let text = render_list("run", "xyz", &[], false);
        assert!(text.contains("no kata matches \"xyz\""));
        assert!(text.contains("kadou new xyz"));
    }

    #[test]
    fn render_list_caps_at_max_rows() {
        let candidates: Vec<PickCandidate> = (0..20)
            .map(|i| candidate(&format!("f/k{i}"), "x"))
            .collect();
        let refs: Vec<&PickCandidate> = candidates.iter().collect();
        let text = render_list("run", "", &refs, false);
        assert_eq!(text.lines().count() - 1, MAX_ROWS);
    }

    #[test]
    fn typing_appends_to_the_query_and_resets_selection() {
        let mut state = PickerState {
            query: String::new(),
            selected: 2,
        };
        let action = apply_key(&mut state, PickerKey::Char('s'), 5);
        assert_eq!(action, PickerAction::Continue);
        assert_eq!(state.query, "s");
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn backspace_pops_the_last_character() {
        let mut state = PickerState {
            query: "ab".to_string(),
            selected: 0,
        };
        apply_key(&mut state, PickerKey::Backspace, 3);
        assert_eq!(state.query, "a");
    }

    #[test]
    fn arrow_keys_wrap_the_selection() {
        let mut state = PickerState::default();
        assert_eq!(
            apply_key(&mut state, PickerKey::Up, 3),
            PickerAction::Continue
        );
        assert_eq!(state.selected, 2, "up from 0 wraps to the last row");
        apply_key(&mut state, PickerKey::Down, 3);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn enter_selects_the_highlighted_row() {
        let mut state = PickerState {
            query: String::new(),
            selected: 1,
        };
        assert_eq!(
            apply_key(&mut state, PickerKey::Enter, 3),
            PickerAction::Select(1)
        );
    }

    #[test]
    fn enter_on_an_empty_list_does_nothing() {
        let mut state = PickerState::default();
        assert_eq!(
            apply_key(&mut state, PickerKey::Enter, 0),
            PickerAction::Continue
        );
    }

    #[test]
    fn tab_shows_the_full_kata_and_e_opens_the_editor() {
        let mut state = PickerState {
            query: String::new(),
            selected: 0,
        };
        assert_eq!(
            apply_key(&mut state, PickerKey::Tab, 1),
            PickerAction::ShowFull(0)
        );
        assert_eq!(
            apply_key(&mut state, PickerKey::Char('e'), 1),
            PickerAction::EditAndReturn(0)
        );
    }

    #[test]
    fn escape_always_cancels() {
        let mut state = PickerState::default();
        assert_eq!(
            apply_key(&mut state, PickerKey::Escape, 0),
            PickerAction::Cancel
        );
    }
}
