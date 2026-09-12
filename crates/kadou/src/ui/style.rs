//! Style tokens (`docs/design/05-prd.md` §7 rules 1, 3, 4; `09-tui-decision.md` §3.1, §3.7):
//! the on/off decision ([`use_color`]) is a pure function of injected state, the same pattern
//! `confirm::confirm_protocol` uses, so it is unit-testable without a real terminal or
//! environment. Every token function below takes that decision as a plain `bool` rather than
//! re-reading `NO_COLOR`/TTY itself.

use anstyle::{AnsiColor, Color, Style};
use kadou_core::RiskLevel;

pub const MARKER: char = '▸';
pub const CHECK: char = '✓';
pub const CROSS: char = '✗';
pub const DOT: char = '\u{25cf}';

/// Rule 3 (§7 "Styled on a TTY only. `NO_COLOR` and `--plain` force plain on a TTY.").
pub fn use_color(_is_tty: bool, _no_color_set: bool, _plain_flag: bool) -> bool {
    todo!()
}

fn paint(_text: &str, _style: Style, _styled: bool) -> String {
    todo!()
}

/// One palette (§7 rule 4): the color a risk level's dot renders in. Never the only carrier
/// of meaning -- [`risk_badge`] always prints the word too (rule 1).
pub fn risk_color(_risk: RiskLevel) -> AnsiColor {
    todo!()
}

/// Rule 1: "Risk is a colored dot plus a word. Never only a color."
pub fn risk_badge(_risk: RiskLevel, _styled: bool) -> String {
    todo!()
}

pub fn muted(_text: &str, _styled: bool) -> String {
    todo!()
}

pub fn bold(_text: &str, _styled: bool) -> String {
    todo!()
}

pub fn ok_mark(_styled: bool) -> String {
    todo!()
}

pub fn err_mark(_styled: bool) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_on_a_pipe_styled_on_a_real_tty() {
        assert!(!use_color(false, false, false), "not a tty");
        assert!(use_color(true, false, false), "a tty, nothing forcing plain");
    }

    #[test]
    fn no_color_forces_plain_even_on_a_tty() {
        assert!(!use_color(true, true, false));
    }

    #[test]
    fn plain_flag_forces_plain_even_on_a_tty() {
        assert!(!use_color(true, false, true));
    }

    #[test]
    fn risk_badge_always_prints_the_word_styled_or_not() {
        for risk in RiskLevel::ALL {
            assert!(risk_badge(risk, true).contains(risk.as_str()));
            assert!(risk_badge(risk, false).contains(risk.as_str()));
        }
    }

    #[test]
    fn plain_tokens_carry_no_escape_bytes() {
        assert!(!risk_badge(RiskLevel::Critical, false).contains('\x1b'));
        assert!(!muted("x", false).contains('\x1b'));
        assert!(!bold("x", false).contains('\x1b'));
        assert!(!ok_mark(false).contains('\x1b'));
        assert!(!err_mark(false).contains('\x1b'));
    }

    #[test]
    fn styled_tokens_carry_escape_bytes_and_round_trip_the_text() {
        assert!(risk_badge(RiskLevel::High, true).contains('\x1b'));
        let styled_muted = muted("x", true);
        assert!(styled_muted.contains('\x1b'));
        assert!(styled_muted.contains('x'));
        assert!(bold("x", true).contains('\x1b'));
        assert!(ok_mark(true).contains('\x1b'));
        assert!(err_mark(true).contains('\x1b'));
    }

    #[test]
    fn different_risk_levels_get_different_colors() {
        assert_ne!(risk_color(RiskLevel::Low), risk_color(RiskLevel::Critical));
        assert_ne!(risk_color(RiskLevel::Medium), risk_color(RiskLevel::High));
    }
}
