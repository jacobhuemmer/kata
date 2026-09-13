//! The unified confirm protocol (`docs/design/05-prd.md` §6.3, decision 4): one pure decision
//! function shared by `kata run` and `kata grant approve` — "There is no TUI face; the TTY
//! prompt *is* the interactive confirm, and `--confirm <id>` is the non-interactive form for
//! both — one protocol, two faces instead of three."
//!
//! [`confirm_protocol`] takes `is_tty` and the two prompt closures as parameters (rather than
//! reading the real terminal itself) precisely so it stays unit-testable without a real pty:
//! callers inject `std::io::stdin().is_terminal()` and real `inquire` prompts; tests inject a
//! fixed `bool`/prompt result.

use kata_core::RiskLevel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// No confirmation needed (low/medium), or confirmation was given and matched.
    Proceed,
    /// A TTY prompt was declined: `y/N` answered no, or the typed id didn't match.
    Declined,
    /// `--confirm <flag>` was given but doesn't name this kata.
    ConfirmMismatch,
    /// Non-TTY, high/critical, no `--confirm` given at all.
    NeedsConfirm,
}

/// The §6.3 decision table, as one pure function:
///
/// | Level | CLI | MCP |
/// |---|---|---|
/// | low/medium | none | none |
/// | high | TTY: `run? [y/N]`, default No. Else: `--confirm <id>`. | (not this module) |
/// | critical | TTY: type the kata id. Else: `--confirm <id>`. | (not this module) |
///
/// `prompt_high` is called only for a high-risk kata on a TTY with no `--confirm`; it returns
/// whether the human accepted. `prompt_critical` is likewise TTY-only and returns the text the
/// human typed (compared against `kata_id`).
pub fn confirm_protocol(
    risk: RiskLevel,
    kata_id: &str,
    confirm_flag: Option<&str>,
    is_tty: bool,
    prompt_high: impl FnOnce() -> bool,
    prompt_critical: impl FnOnce() -> Option<String>,
) -> ConfirmOutcome {
    if risk <= RiskLevel::Medium {
        return ConfirmOutcome::Proceed;
    }

    if let Some(flag) = confirm_flag {
        return if flag == kata_id {
            ConfirmOutcome::Proceed
        } else {
            ConfirmOutcome::ConfirmMismatch
        };
    }

    if !is_tty {
        return ConfirmOutcome::NeedsConfirm;
    }

    match risk {
        RiskLevel::High => {
            if prompt_high() {
                ConfirmOutcome::Proceed
            } else {
                ConfirmOutcome::Declined
            }
        }
        RiskLevel::Critical => match prompt_critical() {
            Some(typed) if typed == kata_id => ConfirmOutcome::Proceed,
            _ => ConfirmOutcome::Declined,
        },
        RiskLevel::Low | RiskLevel::Medium => unreachable!("checked above: risk > Medium"),
    }
}

/// The plain `y/N` prompt `kata remove` and `kata accept` share (§7.1, §9 slice 7): never
/// prompts off a TTY, otherwise returns whatever the caller's own prompt closure answers.
/// Pulled out as its own pure function, the same way [`confirm_protocol`] is, so the
/// TTY-short-circuit is unit-testable without a real pty or a subprocess.
pub fn yes_no(is_tty: bool, prompt: impl FnOnce() -> bool) -> bool {
    is_tty && prompt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_and_medium_never_need_confirmation() {
        for risk in [RiskLevel::Low, RiskLevel::Medium] {
            let outcome = confirm_protocol(
                risk,
                "team/x",
                None,
                false,
                || panic!("not called"),
                || panic!("not called"),
            );
            assert_eq!(outcome, ConfirmOutcome::Proceed);
        }
    }

    #[test]
    fn non_tty_high_or_critical_without_confirm_needs_confirm() {
        for risk in [RiskLevel::High, RiskLevel::Critical] {
            let outcome = confirm_protocol(
                risk,
                "sesami/ses-deploy",
                None,
                false,
                || panic!(),
                || panic!(),
            );
            assert_eq!(outcome, ConfirmOutcome::NeedsConfirm);
        }
    }

    #[test]
    fn matching_confirm_flag_proceeds_regardless_of_tty() {
        for is_tty in [false, true] {
            let outcome = confirm_protocol(
                RiskLevel::Critical,
                "sesami/ses-deploy",
                Some("sesami/ses-deploy"),
                is_tty,
                || panic!(),
                || panic!(),
            );
            assert_eq!(outcome, ConfirmOutcome::Proceed);
        }
    }

    #[test]
    fn mismatched_confirm_flag_is_rejected() {
        let outcome = confirm_protocol(
            RiskLevel::High,
            "sesami/ses-deploy",
            Some("sesami/other"),
            false,
            || panic!(),
            || panic!(),
        );
        assert_eq!(outcome, ConfirmOutcome::ConfirmMismatch);
    }

    #[test]
    fn tty_high_prompt_accept_and_decline() {
        let accepted =
            confirm_protocol(RiskLevel::High, "team/x", None, true, || true, || panic!());
        assert_eq!(accepted, ConfirmOutcome::Proceed);

        let declined =
            confirm_protocol(RiskLevel::High, "team/x", None, true, || false, || panic!());
        assert_eq!(declined, ConfirmOutcome::Declined);
    }

    #[test]
    fn tty_critical_prompt_requires_the_typed_id_to_match() {
        let accepted = confirm_protocol(
            RiskLevel::Critical,
            "sesami/ses-deploy",
            None,
            true,
            || panic!(),
            || Some("sesami/ses-deploy".to_string()),
        );
        assert_eq!(accepted, ConfirmOutcome::Proceed);

        let wrong_text = confirm_protocol(
            RiskLevel::Critical,
            "sesami/ses-deploy",
            None,
            true,
            || panic!(),
            || Some("sesami/other".to_string()),
        );
        assert_eq!(wrong_text, ConfirmOutcome::Declined);

        let cancelled = confirm_protocol(
            RiskLevel::Critical,
            "sesami/ses-deploy",
            None,
            true,
            || panic!(),
            || None,
        );
        assert_eq!(cancelled, ConfirmOutcome::Declined);
    }

    #[test]
    fn yes_no_never_calls_the_prompt_off_a_tty() {
        assert!(!super::yes_no(false, || panic!(
            "must not prompt off a tty"
        )));
    }

    #[test]
    fn yes_no_returns_the_prompts_own_answer_on_a_tty() {
        assert!(super::yes_no(true, || true));
        assert!(!super::yes_no(true, || false));
    }
}
