//! The header parser: kadou's own ~200-line parser for the closed six-key grammar
//! (`docs/design/05-prd.md` §4.3). It looks like YAML so eyes and models parse it for free;
//! it is small enough that a purpose-built parser is safer than a general one.
//!
//! This module never touches the filesystem — [`parse_header`] takes source text and
//! returns either a parsed header or a list of [`Diagnostic`]s. The folder scanner
//! (`crate::scan`) is the filesystem-facing caller.

use std::time::Duration;

use crate::kata::{Arg, ArgDefault, ArgType, Need};
use crate::risk::RiskLevel;

/// The six keys a header may declare, in the order the PRD lists them (§4.3).
const KNOWN_KEYS: [&str; 6] = ["about", "risk", "needs", "args", "alias", "timeout"];

/// Names an arg or need may never use because uppercasing them shadows a shell or libc
/// variable (§4.3 "reserved env names").
const RESERVED_NAMES: [&str; 9] = [
    "path", "home", "pwd", "ifs", "shell", "oldpwd", "cdpath", "bash_env", "ps4",
];

const MAX_HEADER_LINES: usize = 64;
const MAX_ABOUT_CHARS: usize = 120;
const MAX_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// One cargo-shaped diagnostic: a message, an optional one-line fix, and a location the
/// renderer in `crate::check` uses to print a source snippet with a caret (§4.7).
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub fix: Option<String>,
    /// 1-based line number in the source file.
    pub line: usize,
    /// 1-based column (character, not byte) where the offending token starts.
    pub col: usize,
    /// Number of characters to underline, at least 1.
    pub len: usize,
}

impl Diagnostic {
    fn error(line: usize, col: usize, len: usize, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            fix: None,
            line,
            col,
            len: len.max(1),
        }
    }

    fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// A successfully parsed header. The scanner attaches `id` and `path` to build a full
/// [`crate::kata::Kata`] (§4.2 identity is a scanner concern, not a header concern).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedHeader {
    pub about: String,
    pub risk: RiskLevel,
    pub needs: Vec<Need>,
    pub args: Vec<Arg>,
    pub alias: Vec<String>,
    pub timeout: Option<Duration>,
    pub notes: Option<String>,
    /// Present when line 1 is a shebang (`#!...`); absent means the exec contract falls
    /// back to `/bin/sh` (§6.1).
    pub shebang: Option<String>,
}

/// Cheap check used by the folder scanner to decide whether a file is a kata candidate at
/// all: a trimmed `# ---` line within the first 3 lines of the file (§3.7 "A file with a
/// `# ---` header is a kata. A file without one is a helper and is ignored.").
///
/// This does not validate anything else — a candidate can still fail [`parse_header`].
pub fn looks_like_kata_candidate(source: &str) -> bool {
    source
        .lines()
        .take(3)
        .any(|line| line.trim_end() == "# ---")
}

/// Parses `source` per the closed six-key grammar. Returns `(Some(header), diagnostics)`
/// when the header is valid (diagnostics may still hold warnings), or `(None,
/// diagnostics)` when at least one error makes the header unusable (§4.7).
pub fn parse_header(source: &str) -> (Option<ParsedHeader>, Vec<Diagnostic>) {
    let mut diags = Vec::new();

    if source.contains("\r\n") {
        diags.push(
            Diagnostic::error(1, 1, 1, "CRLF line endings in header").with_fix("convert to LF"),
        );
        return (None, diags);
    }

    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        diags.push(
            Diagnostic::error(1, 1, 1, "empty file: no header").with_fix(
                "start the file with `# ---` within the first 3 lines, a closing `# ---`, and \
             `about:`/`risk:` in between",
            ),
        );
        return (None, diags);
    }

    let shebang = if lines[0].starts_with("#!") {
        Some(lines[0].to_string())
    } else {
        None
    };

    let open_idx = lines
        .iter()
        .take(3)
        .position(|line| line.trim_end() == "# ---");
    let Some(open_idx) = open_idx else {
        diags.push(Diagnostic::error(1, 1, 1, "missing header").with_fix(
            "open a header with a line containing only `# ---` within the first 3 lines",
        ));
        return (None, diags);
    };

    // Structural scan: every line from the opener to the closer must start with `#`, must
    // not contain a tab, and the whole block must fit in MAX_HEADER_LINES.
    let mut close_idx = None;
    let mut i = open_idx + 1;
    while i < lines.len() && i < open_idx + MAX_HEADER_LINES {
        let line = lines[i];
        if line.trim_end() == "# ---" {
            close_idx = Some(i);
            break;
        }
        if !line.starts_with('#') {
            diags.push(
                Diagnostic::error(i + 1, 1, line.chars().count().max(1), "header not closed")
                    .with_fix("every line until the closing `# ---` must start with `#`"),
            );
            return (None, diags);
        }
        if let Some(col) = line.find('\t') {
            diags.push(
                Diagnostic::error(i + 1, col + 1, 1, "tabs are not allowed in the header")
                    .with_fix("use spaces"),
            );
            return (None, diags);
        }
        i += 1;
    }
    let Some(close_idx) = close_idx else {
        diags.push(
            Diagnostic::error(
                open_idx + 1,
                1,
                5,
                "header exceeds 64 lines or is never closed",
            )
            .with_fix("close the header with a line containing only `# ---`"),
        );
        return (None, diags);
    };

    let mut about: Option<(String, usize)> = None;
    let mut risk: Option<(RiskLevel, usize)> = None;
    let mut needs: Vec<Need> = Vec::new();
    let mut args: Vec<Arg> = Vec::new();
    let mut alias: Vec<String> = Vec::new();
    let mut timeout: Option<Duration> = None;
    let mut seen_keys: Vec<&str> = Vec::new();
    let mut active_block: Option<&str> = None;

    for (idx, &raw_line) in lines.iter().enumerate().take(close_idx).skip(open_idx + 1) {
        let line_no = idx + 1;
        let content = strip_comment_prefix(raw_line);

        if content.is_empty() {
            active_block = None;
            continue;
        }

        if let Some(rest) = content.strip_prefix("  ") {
            match active_block {
                Some("args") => {
                    // Column of `rest` in the raw line: 1 (for '#') + 1 (space) + 2 (indent).
                    let col_offset = 5;
                    match parse_arg_line(rest, line_no, col_offset) {
                        Ok(arg) if args.iter().any(|a: &Arg| a.name == arg.name) => {
                            diags.push(Diagnostic::error(
                                line_no,
                                col_offset,
                                arg.name.len(),
                                format!("duplicate arg `{}`", arg.name),
                            ));
                        }
                        Ok(arg) => args.push(arg),
                        Err(diag) => diags.push(diag),
                    }
                }
                _ => {
                    diags.push(
                        Diagnostic::error(line_no, 3, content.len(), "unexpected indented line")
                            .with_fix("only an `args:` block takes indented continuation lines"),
                    );
                }
            }
            continue;
        }

        // Top-level `key: value` line.
        active_block = None;
        let Some(colon) = content.find(':') else {
            diags.push(Diagnostic::error(
                line_no,
                3,
                content.len(),
                "expected `key: value`",
            ));
            continue;
        };
        let key = content[..colon].trim();
        let value = content[colon + 1..].trim();
        // Top-level lines never carry the two-space indent (that's the block-continuation
        // branch above), so the key always starts right after the `# ` comment prefix.
        let key_col = 3;
        let value_col =
            3 + colon + 1 + (content[colon + 1..].len() - content[colon + 1..].trim_start().len());

        if !KNOWN_KEYS.contains(&key) {
            diags.push(
                Diagnostic::error(line_no, key_col, key.len(), format!("unknown key `{key}`"))
                    .with_fix(format!("known keys are {}", KNOWN_KEYS.join(", "))),
            );
            continue;
        }
        if seen_keys.contains(&key) {
            diags.push(Diagnostic::error(
                line_no,
                key_col,
                key.len(),
                format!("duplicate key `{key}`"),
            ));
            continue;
        }
        seen_keys.push(key);

        match key {
            "about" => {
                if value.is_empty() {
                    diags.push(
                        Diagnostic::error(line_no, key_col, 5, "about must not be empty")
                            .with_fix("add a one-line description, 1-120 characters"),
                    );
                } else {
                    about = Some((value.to_string(), line_no));
                }
            }
            "risk" => match value {
                "low" => risk = Some((RiskLevel::Low, line_no)),
                "medium" => risk = Some((RiskLevel::Medium, line_no)),
                "high" => risk = Some((RiskLevel::High, line_no)),
                "critical" => risk = Some((RiskLevel::Critical, line_no)),
                other => {
                    diags.push(
                        Diagnostic::error(
                            line_no,
                            value_col,
                            other.len().max(1),
                            format!("unknown risk level `{other}`"),
                        )
                        .with_fix("risk is one of low, medium, high, critical"),
                    );
                }
            },
            "needs" => {
                for token in value.split_whitespace() {
                    let (name, default) = match token.split_once('=') {
                        Some((n, d)) => (n, Some(d.to_string())),
                        None => (token, None),
                    };
                    if let Err(msg) = validate_name(name) {
                        diags.push(Diagnostic::error(
                            line_no,
                            key_col,
                            token.len(),
                            format!("invalid need name `{name}`: {msg}"),
                        ));
                        continue;
                    }
                    needs.push(Need {
                        name: name.to_string(),
                        default,
                    });
                }
            }
            "args" => {
                if !value.is_empty() {
                    diags.push(
                        Diagnostic::error(
                            line_no,
                            key_col,
                            value.len(),
                            "`args:` takes no value on its own line",
                        )
                        .with_fix("put each arg on its own indented line below `args:`"),
                    );
                }
                active_block = Some("args");
            }
            "alias" => {
                for token in value.split_whitespace() {
                    if !is_valid_id_segment(token) {
                        diags.push(
                            Diagnostic::error(
                                line_no,
                                key_col,
                                token.len(),
                                format!("invalid alias `{token}`"),
                            )
                            .with_fix("aliases match ^[a-z0-9][a-z0-9-]*$, the same shape as a folder/name segment"),
                        );
                        continue;
                    }
                    alias.push(token.to_string());
                }
            }
            "timeout" => match humantime::parse_duration(value) {
                Ok(d) if d > MAX_TIMEOUT => {
                    diags.push(Diagnostic::error(
                        line_no,
                        key_col,
                        value.len(),
                        "timeout exceeds the 24h maximum",
                    ));
                }
                Ok(d) => timeout = Some(d),
                Err(_) => {
                    diags.push(
                        Diagnostic::error(
                            line_no,
                            key_col,
                            value.len(),
                            format!("invalid timeout `{value}`"),
                        )
                        .with_fix("use a duration like 30s, 10m, or 2h"),
                    );
                }
            },
            _ => unreachable!("filtered by KNOWN_KEYS above"),
        }
    }

    let about = match about {
        Some((text, line_no)) if text.chars().count() > MAX_ABOUT_CHARS => {
            diags.push(Diagnostic::error(
                line_no,
                1,
                1,
                format!(
                    "about is {} characters, over the 120-character maximum",
                    text.chars().count()
                ),
            ));
            None
        }
        Some((text, _)) => Some(text),
        None => {
            diags.push(
                Diagnostic::error(open_idx + 1, 1, 5, "about is required")
                    .with_fix("add `# about: <one line, 1-120 chars>`"),
            );
            None
        }
    };

    let risk = match risk {
        Some((r, _)) => Some(r),
        None => {
            diags.push(
                Diagnostic::error(open_idx + 1, 1, 5, "risk is required")
                    .with_fix("add `# risk:  low`, `medium`, `high`, or `critical`"),
            );
            None
        }
    };

    if diags.iter().any(Diagnostic::is_error) {
        return (None, diags);
    }

    // `about`/`risk` being `None` always pushed an error diagnostic above, so the check just
    // above means both are `Some` here — but failing closed (no header) rather than an
    // `expect` keeps this fn panic-free even if that invariant is ever weakened by mistake
    // (R6).
    let (Some(about), Some(risk)) = (about, risk) else {
        return (None, diags);
    };

    let notes = extract_notes(&lines, close_idx);

    (
        Some(ParsedHeader {
            about,
            risk,
            needs,
            args,
            alias,
            timeout,
            notes,
            shebang,
        }),
        diags,
    )
}

/// Strips exactly one leading `#` and, if present, one following space. A block
/// continuation line's two-space indent survives this strip on purpose (§4.3).
fn strip_comment_prefix(line: &str) -> &str {
    let rest = &line[1..];
    rest.strip_prefix(' ').unwrap_or(rest)
}

/// An arg/need name: `^[a-z][a-z0-9_]*$` (it becomes an env var), plus rejection of names that
/// would shadow a shell/env variable.
fn validate_name(name: &str) -> Result<(), &'static str> {
    let mut chars = name.chars();
    let ok = matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        return Err("names match ^[a-z][a-z0-9_]*$");
    }
    let upper = name.to_ascii_uppercase();
    if RESERVED_NAMES.contains(&name)
        || upper.starts_with("LD_")
        || upper.starts_with("DYLD_")
        || upper.starts_with("KADOU_")
    {
        return Err("this name is reserved because it would shadow a shell/env variable");
    }
    Ok(())
}

/// The id-segment grammar (PRD §4.2: `^[a-z0-9][a-z0-9-]*$`), shared by aliases (which are not
/// env vars and use the same kebab-case shape as a folder/name segment, e.g. `quick-deploy`)
/// and by [`crate::scan`]'s id-segment check (I-11).
pub(crate) fn is_valid_id_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn parse_arg_line(content: &str, line_no: usize, col_offset: usize) -> Result<Arg, Diagnostic> {
    let err = |col: usize, len: usize, msg: String| Diagnostic::error(line_no, col, len, msg);

    let (definition, help) = match content.find("  #") {
        Some(idx) => (
            content[..idx].trim_end(),
            Some(content[idx + 3..].trim().to_string()),
        ),
        None => (content.trim_end(), None),
    };

    let Some(colon) = definition.find(':') else {
        return Err(err(
            col_offset,
            definition.len().max(1),
            "expected `name: type`".to_string(),
        ));
    };
    let name = definition[..colon].trim();
    let name_col = col_offset;
    if let Err(msg) = validate_name(name) {
        return Err(err(
            name_col,
            name.len().max(1),
            format!("invalid arg name `{name}`: {msg}"),
        ));
    }

    let rest = definition[colon + 1..].trim_start();
    let rest_col = col_offset
        + definition[..colon + 1].chars().count()
        + (definition[colon + 1..].len() - rest.len());

    let (type_part, default_part) = match rest.find(" = ") {
        Some(idx) => (rest[..idx].trim_end(), Some(rest[idx + 3..].trim())),
        None => (rest.trim_end(), None),
    };

    let mut type_words = type_part.splitn(2, char::is_whitespace);
    let type_word = type_words.next().unwrap_or("");
    let type_rest = type_words.next().unwrap_or("").trim();
    let type_col = rest_col;

    let ty = match type_word {
        "text" => {
            if !type_rest.is_empty() {
                return Err(err(
                    type_col,
                    type_part.len(),
                    format!("unexpected text after type `text`: `{type_rest}`"),
                ));
            }
            ArgType::Text
        }
        "int" => {
            if !type_rest.is_empty() {
                return Err(err(
                    type_col,
                    type_part.len(),
                    format!("unexpected text after type `int`: `{type_rest}`"),
                ));
            }
            ArgType::Int
        }
        "bool" => {
            if !type_rest.is_empty() {
                return Err(err(
                    type_col,
                    type_part.len(),
                    format!("unexpected text after type `bool`: `{type_rest}`"),
                ));
            }
            ArgType::Bool
        }
        "select" => {
            if type_rest.is_empty() {
                return Err(err(
                    type_col,
                    type_word.len(),
                    "select without options".to_string(),
                )
                .with_fix("list at least one option, e.g. `select a|b|c`".to_string()));
            }
            let options: Vec<String> = type_rest.split('|').map(|s| s.trim().to_string()).collect();
            for opt in &options {
                if opt.is_empty()
                    || !opt
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_.:/-".contains(c))
                {
                    return Err(err(
                        type_col,
                        type_rest.len(),
                        format!("invalid select option `{opt}`"),
                    )
                    .with_fix("options match ^[A-Za-z0-9_.:/-]+$".to_string()));
                }
            }
            ArgType::Select { options }
        }
        other => {
            return Err(err(
                type_col,
                type_word.len().max(1),
                format!("unknown arg type `{other}`"),
            )
            .with_fix("use `text`".to_string()));
        }
    };

    let default = match default_part {
        None => None,
        Some(raw) => Some(coerce_default(&ty, raw, |msg| {
            err(type_col, raw.len().max(1), msg)
        })?),
    };

    Ok(Arg {
        name: name.to_string(),
        ty,
        default,
        help,
    })
}

fn coerce_default(
    ty: &ArgType,
    raw: &str,
    err: impl Fn(String) -> Diagnostic,
) -> Result<ArgDefault, Diagnostic> {
    match ty {
        ArgType::Text => {
            if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
                Ok(ArgDefault::Text(raw[1..raw.len() - 1].to_string()))
            } else if raw.chars().any(char::is_whitespace) {
                Err(err(format!(
                    "quote a default containing spaces: `\"{raw}\"`"
                )))
            } else {
                Ok(ArgDefault::Text(raw.to_string()))
            }
        }
        ArgType::Int => raw
            .parse::<i64>()
            .map(ArgDefault::Int)
            .map_err(|_| err(format!("invalid int default `{raw}`"))),
        ArgType::Bool => match raw {
            "true" => Ok(ArgDefault::Bool(true)),
            "false" => Ok(ArgDefault::Bool(false)),
            _ => Err(err(format!(
                "invalid bool default `{raw}`, use true or false"
            ))),
        },
        ArgType::Select { options } => {
            if options.iter().any(|o| o == raw) {
                Ok(ArgDefault::Select(raw.to_string()))
            } else {
                Err(err(format!(
                    "default `{raw}` is not one of the declared options"
                )))
            }
        }
    }
}

/// The first `#`-comment paragraph after the closing `# ---`, per §4.3.
fn extract_notes(lines: &[&str], close_idx: usize) -> Option<String> {
    let mut collected = Vec::new();
    for line in lines.iter().skip(close_idx + 1) {
        if let Some(rest) = line.strip_prefix('#') {
            let text = rest.strip_prefix(' ').unwrap_or(rest);
            if text.is_empty() && !collected.is_empty() {
                break;
            }
            if !text.is_empty() {
                collected.push(text.to_string());
            }
        } else {
            break;
        }
    }
    if collected.is_empty() {
        None
    } else {
        Some(collected.join("\n"))
    }
}

/// Renders a header's fields back into `# ---`-fenced comment text, followed by `body`
/// (the script content after the header). Used by `kadou import` (§4.6) to write converted
/// kata; the same text round-trips through [`parse_header`].
#[allow(clippy::too_many_arguments)]
pub fn render_header(
    shebang: Option<&str>,
    about: &str,
    risk: RiskLevel,
    needs: &[Need],
    args: &[Arg],
    alias: &[String],
    timeout: Option<Duration>,
    body: &str,
) -> String {
    let mut out = String::new();
    if let Some(shebang) = shebang {
        out.push_str(shebang);
        out.push('\n');
    }
    out.push_str("# ---\n");
    out.push_str(&format!("# about: {about}\n"));
    out.push_str(&format!("# risk:  {risk}\n"));

    if !needs.is_empty() {
        let line = needs
            .iter()
            .map(|n| match &n.default {
                Some(d) => format!("{}={}", n.name, d),
                None => n.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("# needs: {line}\n"));
    }

    if !args.is_empty() {
        out.push_str("# args:\n");
        for arg in args {
            out.push_str(&format!("#   {}\n", render_arg_line(arg)));
        }
    }

    if !alias.is_empty() {
        out.push_str(&format!("# alias: {}\n", alias.join(" ")));
    }

    if let Some(timeout) = timeout {
        out.push_str(&format!(
            "# timeout: {}\n",
            humantime::format_duration(timeout)
        ));
    }

    out.push_str("# ---\n");
    out.push_str(body);
    out
}

fn render_arg_line(arg: &Arg) -> String {
    let type_str = match &arg.ty {
        ArgType::Text => "text".to_string(),
        ArgType::Int => "int".to_string(),
        ArgType::Bool => "bool".to_string(),
        ArgType::Select { options } => format!("select {}", options.join("|")),
    };

    let default_str = match &arg.default {
        None => String::new(),
        Some(ArgDefault::Text(s)) => format!(" = {}", quote_default(s)),
        Some(ArgDefault::Int(i)) => format!(" = {i}"),
        Some(ArgDefault::Bool(b)) => format!(" = {b}"),
        Some(ArgDefault::Select(s)) => format!(" = {s}"),
    };

    let help_str = match &arg.help {
        Some(h) if !h.is_empty() => format!("  # {h}"),
        _ => String::new(),
    };

    format!("{}: {}{}{}", arg.name, type_str, default_str, help_str)
}

fn quote_default(s: &str) -> String {
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> ParsedHeader {
        let (header, diags) = parse_header(source);
        assert!(
            !diags.iter().any(Diagnostic::is_error),
            "expected no errors, got {diags:?}"
        );
        header.expect("header should parse")
    }

    fn parse_err(source: &str) -> Vec<Diagnostic> {
        let (header, diags) = parse_header(source);
        assert!(header.is_none(), "expected parse failure");
        assert!(diags.iter().any(Diagnostic::is_error));
        diags
    }

    const MINIMAL: &str = "#!/bin/sh\n# ---\n# about: Say hello\n# risk:  low\n# ---\necho hi\n";

    /// A header block of exactly `total` lines (the opener and closer both count, §4.3),
    /// padded with distinct `args:` entries so the fill lines are valid header content.
    fn header_with_total_lines(total: usize) -> String {
        assert!(
            total >= 5,
            "need room for about, risk, args:, opener, closer"
        );
        let mut s = String::from("#!/bin/sh\n# ---\n# about: Test\n# risk:  low\n# args:\n");
        for n in 0..total - 5 {
            s.push_str(&format!("#   a{n}: text = v\n"));
        }
        s.push_str("# ---\necho hi\n");
        s
    }

    #[test]
    fn a_header_of_exactly_64_lines_is_the_maximum_allowed() {
        // §4.3: "Max 64 header lines" -- the opener line itself counts toward the budget.
        parse_ok(&header_with_total_lines(64));
    }

    #[test]
    fn a_header_of_65_lines_exceeds_the_maximum() {
        let diags = parse_err(&header_with_total_lines(65));
        assert!(diags.iter().any(|d| d.message.contains("exceeds 64 lines")));
    }

    #[test]
    fn minimal_header_parses() {
        let header = parse_ok(MINIMAL);
        assert_eq!(header.about, "Say hello");
        assert_eq!(header.risk, RiskLevel::Low);
        assert!(header.needs.is_empty());
        assert!(header.args.is_empty());
        assert_eq!(header.shebang.as_deref(), Some("#!/bin/sh"));
    }

    #[test]
    fn no_shebang_is_fine() {
        let source = "# ---\n# about: Say hello\n# risk:  low\n# ---\necho hi\n";
        let header = parse_ok(source);
        assert!(header.shebang.is_none());
    }

    #[test]
    fn unknown_key_fails() {
        let source =
            "#!/bin/sh\n# ---\n# about: Say hello\n# risk:  low\n# scope: global\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(diags.iter().any(|d| d.message.contains("unknown key")));
    }

    #[test]
    fn missing_about_fails() {
        let source = "#!/bin/sh\n# ---\n# risk:  low\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("about is required"))
        );
    }

    #[test]
    fn missing_risk_fails() {
        let source = "#!/bin/sh\n# ---\n# about: Say hello\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(diags.iter().any(|d| d.message.contains("risk is required")));
    }

    #[test]
    fn select_without_options_fails() {
        let source = "#!/bin/sh\n# ---\n# about: Pick one\n# risk:  low\n# args:\n#   mode: select\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("select without options"))
        );
    }

    #[test]
    fn reserved_arg_name_fails() {
        let source = "#!/bin/sh\n# ---\n# about: Broken\n# risk:  low\n# args:\n#   path: text = .\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(diags.iter().any(|d| d.message.contains("reserved")));
    }

    #[test]
    fn reserved_kadou_prefixed_arg_name_fails() {
        let source = "#!/bin/sh\n# ---\n# about: Broken\n# risk:  low\n# args:\n#   kadou_id: text = x\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(diags.iter().any(|d| d.message.contains("reserved")));
    }

    #[test]
    fn reserved_need_name_fails() {
        let source =
            "#!/bin/sh\n# ---\n# about: Broken\n# risk:  low\n# needs: shell\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("invalid need name"))
        );
    }

    #[test]
    fn unclosed_header_fails() {
        let source = "#!/bin/sh\n# ---\n# about: Broken\n# risk:  low\necho hi\n";
        let diags = parse_err(source);
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("header not closed"))
        );
    }

    #[test]
    fn tabs_fail() {
        let source = "#!/bin/sh\n# ---\n# about: Broken\n# risk:\tlow\n# ---\necho hi\n";
        let diags = parse_err(source);
        assert!(diags.iter().any(|d| d.message.contains("tabs")));
    }

    #[test]
    fn crlf_fails() {
        let source =
            "#!/bin/sh\r\n# ---\r\n# about: Broken\r\n# risk:  low\r\n# ---\r\necho hi\r\n";
        let diags = parse_err(source);
        assert!(diags.iter().any(|d| d.message.contains("CRLF")));
    }

    #[test]
    fn full_header_round_trips_through_render() {
        let header = parse_ok(
            "#!/bin/sh\n# ---\n# about: Trigger a branch pipeline\n# risk:  medium\n\
             # needs: jenkins_url=https://ci.example.com jenkins_user jenkins_token\n\
             # args:\n\
             #   branch: text = dev  # Branch, tag, or PR to trigger\n\
             #   version: text = \"\"  # Falls back to branch\n\
             #   send_email: bool = true\n\
             #   k8s_context: select dev|uat|prod = dev  # Target env\n\
             # alias: deploy\n\
             # timeout: 10m\n\
             # ---\nset -eu\necho hi\n",
        );
        assert_eq!(header.needs.len(), 3);
        assert_eq!(header.args.len(), 4);
        assert_eq!(header.alias, vec!["deploy".to_string()]);
        assert_eq!(header.timeout, Some(Duration::from_secs(600)));

        let rendered = render_header(
            header.shebang.as_deref(),
            &header.about,
            header.risk,
            &header.needs,
            &header.args,
            &header.alias,
            header.timeout,
            "set -eu\necho hi\n",
        );
        let (reparsed, diags) = parse_header(&rendered);
        assert!(!diags.iter().any(Diagnostic::is_error), "{diags:?}");
        assert_eq!(reparsed.unwrap(), header);
    }

    #[test]
    fn empty_default_text_arg_is_optional_and_empty() {
        let header = parse_ok(
            "#!/bin/sh\n# ---\n# about: X\n# risk:  low\n# args:\n#   version: text = \"\"\n# ---\necho hi\n",
        );
        let arg = &header.args[0];
        assert!(!arg.is_required());
        assert_eq!(arg.default, Some(ArgDefault::Text(String::new())));
    }

    #[test]
    fn no_default_arg_is_required() {
        let header = parse_ok(
            "#!/bin/sh\n# ---\n# about: X\n# risk:  low\n# args:\n#   app_name: text\n# ---\necho hi\n",
        );
        assert!(header.args[0].is_required());
    }

    #[test]
    fn looks_like_kata_candidate_requires_marker_in_first_three_lines() {
        assert!(looks_like_kata_candidate(MINIMAL));
        assert!(!looks_like_kata_candidate(
            "#!/usr/bin/env bash\n# a helper script\n#\necho hi\n"
        ));
    }

    #[test]
    fn about_over_120_chars_fails() {
        let long = "x".repeat(121);
        let source = format!("#!/bin/sh\n# ---\n# about: {long}\n# risk:  low\n# ---\necho hi\n");
        let diags = parse_err(&source);
        assert!(diags.iter().any(|d| d.message.contains("120-character")));
    }

    #[test]
    fn notes_is_first_comment_paragraph_after_close() {
        let source = "#!/bin/sh\n# ---\n# about: X\n# risk:  low\n# ---\n# Usage notes here.\n# Second line.\n\n# Not included.\necho hi\n";
        let header = parse_ok(source);
        assert_eq!(
            header.notes.as_deref(),
            Some("Usage notes here.\nSecond line.")
        );
    }
}
