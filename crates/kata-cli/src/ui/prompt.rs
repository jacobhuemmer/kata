//! Inline prompts for `kata run`'s missing/asked args (`docs/design/05-prd.md` §7.3;
//! `09-tui-decision.md` §3.4). The pure decisions here (what default to show, whether typed
//! text is a valid `int`/`bool`) are unit-tested directly; the real `inquire` widgets that use
//! them live in `commands.rs`, the same split `confirm.rs` and `read_vault_value` already use.

use kata_core::{Arg, ArgDefault};

/// D5 (§6.6, §7.3 "showing the header default (or last-used value when one exists)"): the
/// value shown as a prompt's default. A header default always wins over a last-used value --
/// D5 holds that an optional arg's header default is never shadowed by history.
pub fn prefill(arg: &Arg, last_used: Option<&str>) -> Option<String> {
    match &arg.default {
        Some(ArgDefault::Text(s)) => Some(s.clone()),
        Some(ArgDefault::Int(i)) => Some(i.to_string()),
        Some(ArgDefault::Bool(b)) => Some(b.to_string()),
        Some(ArgDefault::Select(s)) => Some(s.clone()),
        None => last_used.map(str::to_string),
    }
}

/// `int` prompts reject non-digit input before `↵` (§7.3): an optional leading `-`, at least
/// one digit, nothing else.
pub fn is_valid_int(input: &str) -> bool {
    let digits = input.strip_prefix('-').unwrap_or(input);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg_with_default(default: Option<ArgDefault>) -> Arg {
        Arg {
            name: "x".to_string(),
            ty: kata_core::ArgType::Text,
            default,
            help: None,
        }
    }

    #[test]
    fn header_default_wins_over_a_last_used_value() {
        let arg = arg_with_default(Some(ArgDefault::Text("main".to_string())));
        assert_eq!(prefill(&arg, Some("release/1.2")).as_deref(), Some("main"));
    }

    #[test]
    fn a_required_arg_falls_back_to_the_last_used_value() {
        let arg = arg_with_default(None);
        assert_eq!(prefill(&arg, Some("25.6.1.2")).as_deref(), Some("25.6.1.2"));
    }

    #[test]
    fn a_required_arg_with_no_history_has_no_prefill() {
        let arg = arg_with_default(None);
        assert_eq!(prefill(&arg, None), None);
    }

    #[test]
    fn int_default_renders_as_plain_digits() {
        let arg = arg_with_default(Some(ArgDefault::Int(-3)));
        assert_eq!(prefill(&arg, None).as_deref(), Some("-3"));
    }

    #[test]
    fn valid_ints_accept_digits_and_an_optional_leading_minus() {
        assert!(is_valid_int("42"));
        assert!(is_valid_int("-7"));
        assert!(is_valid_int("0"));
    }

    #[test]
    fn invalid_ints_are_rejected() {
        assert!(!is_valid_int(""));
        assert!(!is_valid_int("-"));
        assert!(!is_valid_int("4.2"));
        assert!(!is_valid_int("4x"));
        assert!(!is_valid_int(" 4"));
    }
}
