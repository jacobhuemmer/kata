//! The library, run, and check frame printers (`docs/design/05-prd.md` §7.2, §7.3;
//! `09-tui-decision.md` §3.2, §3.4, §3.5). Every function here is pure: it takes already-
//! gathered data and a `styled` bool and returns a `String` -- no filesystem or terminal I/O,
//! so every frame is an `insta` snapshot test without a real TTY (`09` §3.7).

use std::time::Duration;

use kadou_core::RiskLevel;

use crate::ui::style;

// ---------------------------------------------------------------------------
// The bare `kadou` frame (§7.2, `09` §3.2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct KataRow {
    /// The id segment shown under its folder, not the full id (`hello`, not `starter/hello`).
    pub name: String,
    pub risk: RiskLevel,
    pub about: String,
}

#[derive(Debug, Clone)]
pub struct FolderRow {
    pub name: String,
    pub git_backed: bool,
    /// From `kadou check`: `0` prints `git ✓` (or nothing, when not git-backed); nonzero
    /// prints `✗ N errors` regardless of `git_backed` (§9 slice 8 "folder health").
    pub error_count: usize,
    pub kata: Vec<KataRow>,
}

impl FolderRow {
    fn health(&self, styled: bool) -> Option<String> {
        if self.error_count > 0 {
            let word = if self.error_count == 1 {
                "error"
            } else {
                "errors"
            };
            return Some(format!(
                "{} {} {word}",
                style::err_mark(styled),
                self.error_count
            ));
        }
        if self.git_backed {
            return Some(format!("git {}", style::ok_mark(styled)));
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct GrantRow {
    pub short_id: String,
    pub kata_id: String,
    /// Pre-rendered `k=v k=v` (never a need -- needs are never included, §6.4).
    pub args: String,
    pub client: String,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub struct DraftRow {
    pub id: String,
    /// The command line printed to act on it: `kadou accept <id>` for a proposed draft,
    /// `kadou mine review` for a mined one (it has no folder of its own to accept into yet).
    pub action: String,
}

#[derive(Debug, Clone, Default)]
pub struct NeedsYou {
    pub grants: Vec<GrantRow>,
    pub drafts: Vec<DraftRow>,
}

impl NeedsYou {
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty() && self.drafts.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct BareFrame {
    pub folders: Vec<FolderRow>,
    pub needs_you: NeedsYou,
}

impl BareFrame {
    fn kata_count(&self) -> usize {
        self.folders.iter().map(|f| f.kata.len()).sum()
    }
}

/// "4m ago" / "2h ago" / "3d ago" / "just now" (§7.2's `claude-code · 4m ago`). Takes an
/// already-computed [`Duration`] rather than two `SystemTime`s so it is a pure, deterministic
/// function of its input.
pub fn humanize_ago(elapsed: Duration) -> String {
    todo!()
}

fn needs_you_header(needs_you: &NeedsYou) -> Option<String> {
    if needs_you.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !needs_you.grants.is_empty() {
        let word = if needs_you.grants.len() == 1 {
            "grant"
        } else {
            "grants"
        };
        parts.push(format!("{} {word}", needs_you.grants.len()));
    }
    if !needs_you.drafts.is_empty() {
        let word = if needs_you.drafts.len() == 1 {
            "draft"
        } else {
            "drafts"
        };
        parts.push(format!("{} {word}", needs_you.drafts.len()));
    }
    Some(format!("needs you: {}", parts.join(" · ")))
}

fn push_folder_block(out: &mut String, folder: &FolderRow, styled: bool) {
    match folder.health(styled) {
        Some(health) => out.push_str(&format!(" {}                {health}\n", folder.name)),
        None => out.push_str(&format!(" {}\n", folder.name)),
    }
    let width = folder.kata.iter().map(|k| k.name.len()).max().unwrap_or(0) + 2;
    for kata in &folder.kata {
        let row = format!(
            "   {:width$}{}   {}\n",
            kata.name,
            style::risk_badge(kata.risk, styled),
            kata.about
        );
        out.push_str(&if folder.error_count > 0 {
            style::muted(&row, styled)
        } else {
            row
        });
    }
}

fn push_needs_you_block(out: &mut String, needs_you: &NeedsYou) {
    if needs_you.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str(" needs you\n");
    for grant in &needs_you.grants {
        out.push_str(&format!(
            "   grant  {}  {}   {} · {}\n",
            grant.kata_id,
            grant.args,
            grant.client,
            humanize_ago(grant.elapsed)
        ));
        out.push_str(&format!(
            "          kadou grant approve {}   ·   kadou grant deny {}\n",
            grant.short_id, grant.short_id
        ));
    }
    for draft in &needs_you.drafts {
        out.push_str(&format!("   draft  {}                {}\n", draft.id, draft.action));
    }
}

/// The onboarding footer (§7.2 first-run example) shown only while the library is still just
/// the starter set; a grown library gets the compact `run · help` line instead (§9 slice 8
/// interpretation call, noted in the handoff: the PRD shows both without stating the rule
/// that picks between them).
fn is_starter_only(frame: &BareFrame) -> bool {
    frame.folders.len() == 1 && frame.folders[0].name == "starter"
}

fn push_footer(out: &mut String, frame: &BareFrame) {
    out.push('\n');
    if is_starter_only(frame) {
        out.push_str(" run     kadou run                 pick one, or:  kadou run starter/hello\n");
        out.push_str(" new     kadou new <folder/name>   write a kata and open it\n");
        out.push_str(" team    kadou get <git-url>       add your team's kata as a folder\n");
        out.push_str(" agents  kadou mcp serve\n");
    } else {
        out.push_str(" run  kadou run   ·   help  kadou --help\n");
    }
}

/// The bare `kadou` frame on a TTY (§7.2, `09` §3.2).
pub fn render_bare(frame: &BareFrame, styled: bool) -> String {
    todo!()
}

/// Piped `kadou`: one kata per line, tab-separated, full id (§7.2 "Piped, `kadou` prints one
/// kata per line, tab-separated").
pub fn render_bare_piped(frame: &BareFrame) -> String {
    todo!()
}

// ---------------------------------------------------------------------------
// `kadou run` header and footer (`09` §3.4)
// ---------------------------------------------------------------------------

/// The frame printed just before a kata spawns: `▶ id  risk`, the resolved args on one line,
/// and (when the kata has any) a `needs` line with a vault-satisfied checkmark or a missing
/// cross, followed by a divider before the script's own output starts.
pub fn run_header(
    kata_id: &str,
    risk: RiskLevel,
    args: &[(String, String)],
    needs_satisfied: Option<bool>,
    styled: bool,
) -> String {
    todo!()
}

/// The final status line after a run completes (§9 `08` §2.7's frame, carried unchanged).
pub fn run_footer(
    kata_id: &str,
    success: bool,
    exit_code: i32,
    duration: Duration,
    history_short: &str,
    styled: bool,
) -> String {
    todo!()
}

// ---------------------------------------------------------------------------
// `kadou check` diagnostics colors (`09` §3.5)
// ---------------------------------------------------------------------------

/// Colorizes `kadou_core::render_report`'s plain cargo-shaped text: `error:`/`warning:`
/// severity words, the `-->` location line (muted), the caret line (the same color as its
/// severity), the `= fix` line (muted), and the summary line (bold, red if any error). Never
/// changes the text itself -- `styled = false` returns the input unchanged, so scripts piping
/// `kadou check` see the exact bytes `kadou_core` produced.
pub fn colorize_check_report(text: &str, styled: bool) -> String {
    todo!()
}

fn colorize_check_line(line: &str) -> String {
    use anstyle::{AnsiColor, Color, Style};

    let red_bold = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red))).bold();
    let yellow_bold = Style::new()
        .fg_color(Some(Color::Ansi(AnsiColor::Yellow)))
        .bold();
    let green_bold = Style::new()
        .fg_color(Some(Color::Ansi(AnsiColor::Green)))
        .bold();

    if let Some(rest) = line.strip_prefix("error: ") {
        return format!(
            "{}error{}: {rest}",
            red_bold.render(),
            red_bold.render_reset()
        );
    }
    if let Some(rest) = line.strip_prefix("warning: ") {
        return format!(
            "{}warning{}: {rest}",
            yellow_bold.render(),
            yellow_bold.render_reset()
        );
    }
    if line.trim_start().starts_with("-->") {
        return style::muted(line, true);
    }
    if is_caret_line(line) {
        return format!(
            "{}{line}{}",
            red_bold.render(),
            red_bold.render_reset()
        );
    }
    if let Some(fix) = line.strip_prefix("  = ") {
        return format!("  {}", style::muted(&format!("= {fix}"), true));
    }
    if line.starts_with("checked ") {
        let style = if line.contains(" 0 errors") {
            green_bold
        } else {
            red_bold
        };
        return format!("{}{line}{}", style.render(), style.render_reset());
    }
    line.to_string()
}

/// A line whose only non-whitespace content after the gutter `|` is `^` carets.
fn is_caret_line(line: &str) -> bool {
    let Some((_, after_pipe)) = line.rsplit_once('|') else {
        return false;
    };
    let trimmed = after_pipe.trim_start();
    !trimmed.is_empty() && trimmed.chars().all(|c| c == '^')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_folder() -> FolderRow {
        FolderRow {
            name: "starter".to_string(),
            git_backed: false,
            error_count: 0,
            kata: vec![
                KataRow {
                    name: "hello".to_string(),
                    risk: RiskLevel::Low,
                    about: "Print a greeting".to_string(),
                },
                KataRow {
                    name: "disk-usage".to_string(),
                    risk: RiskLevel::Low,
                    about: "Disk usage of a directory".to_string(),
                },
            ],
        }
    }

    #[test]
    fn first_run_frame_shows_the_starter_folder_and_onboarding_footer() {
        let frame = BareFrame {
            folders: vec![sample_folder()],
            needs_you: NeedsYou::default(),
        };
        let text = render_bare(&frame, false);
        insta::assert_snapshot!("bare_frame_first_run_plain", text);
    }

    #[test]
    fn first_run_frame_styled_snapshot() {
        let frame = BareFrame {
            folders: vec![sample_folder()],
            needs_you: NeedsYou::default(),
        };
        let text = render_bare(&frame, true);
        insta::assert_snapshot!("bare_frame_first_run_styled", text);
    }

    #[test]
    fn a_grown_library_with_needs_you_uses_the_compact_footer() {
        let mut sesami = FolderRow {
            name: "sesami".to_string(),
            git_backed: true,
            error_count: 0,
            kata: vec![KataRow {
                name: "ses-deploy".to_string(),
                risk: RiskLevel::Critical,
                about: "Trigger the SES Deploy pipeline".to_string(),
            }],
        };
        sesami.kata.sort_by(|a, b| a.name.cmp(&b.name));
        let needs_you = NeedsYou {
            grants: vec![GrantRow {
                short_id: "7c1e".to_string(),
                kata_id: "sesami/ses-deploy".to_string(),
                args: "version=25.6.1.2 oke_cluster=uat".to_string(),
                client: "claude-code".to_string(),
                elapsed: Duration::from_secs(4 * 60),
            }],
            drafts: vec![
                DraftRow {
                    id: "proposed/ops/argocd-sync".to_string(),
                    action: "kadou accept ops/argocd-sync".to_string(),
                },
                DraftRow {
                    id: "mined/k8s-pod-logs".to_string(),
                    action: "kadou mine review".to_string(),
                },
            ],
        };
        let frame = BareFrame {
            folders: vec![sample_folder(), sesami],
            needs_you,
        };
        let text = render_bare(&frame, false);
        assert!(text.contains("needs you: 1 grant · 2 drafts"));
        assert!(text.contains("kadou grant approve 7c1e"));
        assert!(text.contains("kadou grant deny 7c1e"));
        assert!(text.contains("kadou accept ops/argocd-sync"));
        assert!(text.contains("kadou mine review"));
        assert!(text.contains(" run  kadou run   ·   help  kadou --help"));
        assert!(!text.contains("agents  kadou mcp serve"));
        insta::assert_snapshot!("bare_frame_needs_you_plain", text);
    }

    #[test]
    fn a_folder_that_failed_check_shows_error_count_not_git_ok() {
        let folder = FolderRow {
            name: "broken".to_string(),
            git_backed: true,
            error_count: 2,
            kata: vec![KataRow {
                name: "x".to_string(),
                risk: RiskLevel::Low,
                about: "X".to_string(),
            }],
        };
        let frame = BareFrame {
            folders: vec![folder],
            needs_you: NeedsYou::default(),
        };
        let text = render_bare(&frame, false);
        assert!(text.contains("✗ 2 errors"));
        assert!(!text.contains("git ✓"));
    }

    #[test]
    fn piped_output_is_one_tab_separated_line_per_kata_with_the_full_id() {
        let frame = BareFrame {
            folders: vec![sample_folder()],
            needs_you: NeedsYou::default(),
        };
        let text = render_bare_piped(&frame);
        assert_eq!(
            text,
            "starter/hello\tlow\tPrint a greeting\nstarter/disk-usage\tlow\tDisk usage of a directory\n"
        );
    }

    #[test]
    fn humanize_ago_buckets_by_magnitude() {
        assert_eq!(humanize_ago(Duration::from_secs(30)), "just now");
        assert_eq!(humanize_ago(Duration::from_secs(4 * 60)), "4m ago");
        assert_eq!(humanize_ago(Duration::from_secs(2 * 60 * 60)), "2h ago");
        assert_eq!(humanize_ago(Duration::from_secs(3 * 24 * 60 * 60)), "3d ago");
    }

    #[test]
    fn run_header_snapshot_plain_and_styled() {
        let args = vec![
            ("version".to_string(), "25.6.1.2".to_string()),
            ("oke_cluster".to_string(), "uat".to_string()),
        ];
        let plain = run_header("sesami/ses-deploy", RiskLevel::Critical, &args, Some(true), false);
        insta::assert_snapshot!("run_header_plain", plain);
        let styled = run_header("sesami/ses-deploy", RiskLevel::Critical, &args, Some(true), true);
        insta::assert_snapshot!("run_header_styled", styled);
    }

    #[test]
    fn run_footer_reports_success_and_failure() {
        let ok = run_footer("starter/hello", true, 0, Duration::from_secs(1), "91aa", false);
        assert!(ok.contains("✓ starter/hello  exit 0"));
        let failed = run_footer("starter/hello", false, 1, Duration::from_secs(1), "91aa", false);
        assert!(failed.contains("✗ starter/hello  exit 1"));
    }

    const CHECK_TEXT: &str = "error: unknown arg type `string`\n  --> kata/sesami/x.sh:8:14\n 8 | #   app_name: string\n   |               ^^^^^^\n   = arg types are text, int, bool, select\n\nchecked 32 kata in sesami   1 error  0 warnings\n";

    #[test]
    fn check_colorizer_is_a_no_op_when_not_styled() {
        assert_eq!(colorize_check_report(CHECK_TEXT, false), CHECK_TEXT);
    }

    #[test]
    fn check_colorizer_snapshot_when_styled() {
        let text = colorize_check_report(CHECK_TEXT, true);
        assert_ne!(text, CHECK_TEXT);
        assert!(text.contains('\x1b'));
        insta::assert_snapshot!("check_report_styled", text);
    }
}
