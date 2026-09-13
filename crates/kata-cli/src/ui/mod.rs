//! Human-facing output (`docs/design/05-prd.md` §7, §3; `09-tui-decision.md` §3.7): `style`
//! (theme to ANSI, TTY/`NO_COLOR` detection), `frame` (the library, run, and check printers),
//! `picker` (list plus preview, fuzzy scoring), `prompt` (text, int, bool, select, confirm).
//! No `kadou-tui` crate, no ratatui anywhere in this module or its callers.

pub mod frame;
pub mod picker;
pub mod prompt;
pub mod style;
