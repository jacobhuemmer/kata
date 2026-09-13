//! Per-agent native-transcript parsers (`docs/design/06-session-mining.md` §1.5, §2.2). Each
//! parser binds to the key paths sketched there. Claude extracts all three `06` §2.3 kinds:
//! `Bash` tool calls (`Kind::Shell`), a `Write` whose path ends `.sh`/`.bash` (`Kind::
//! ScriptFile`), and a fenced `sh`/`bash`/`shell` code block in an assistant `text` block
//! (`Kind::ScriptFence`) -- the kind that decides whether a Claude session yields anything at
//! all once shell-invocation retention is as thin as `06` §4.7 measures it. Codex/Grok/Cursor
//! still extract only the shell-invocation shape: their §1.5 schema sketches show no `text`/
//! file-write shape to bind a fence or script-file extractor to, and Codex alone already
//! retains ~99% of its shell invocations (`06` §1.4), so this is a documented scope choice for
//! this slice, not an oversight -- see the handoff.
//!
//! Every parser drops (never persists) `stdout`/`stderr`/`toolUseResult` -- it never even reads
//! those keys -- matching `06` §2.3 "Drop: ... tool stdout/stderr".

use serde_json::Value;

use crate::model::{Agent, EventKind, RawShellEvent};

#[cfg(test)]
mod tests;

fn line_when(value: &Value) -> String {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `true` when `path` names a shell script by extension (`06` §2.3 "generated script file ...
/// ending in .sh / .bash").
fn is_shell_script_path(path: &str) -> bool {
    path.ends_with(".sh") || path.ends_with(".bash")
}

/// Finds every fenced code block in `text` tagged `sh`/`bash`/`shell` and returns each one's
/// raw body, one entry per fence (`06` §2.3 "fenced script: assistant markdown fences ...
/// tagged sh/bash/shell"). The `>=3` command-line rule is `extract`'s job, not this parser's --
/// this only recovers candidate fences for extraction to filter.
fn fenced_scripts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(lang) = line.trim_start().strip_prefix("```") else {
            continue;
        };
        if !matches!(
            lang.trim().to_ascii_lowercase().as_str(),
            "sh" | "bash" | "shell"
        ) {
            continue;
        }
        let mut body = String::new();
        for inner in lines.by_ref() {
            if inner.trim_start().starts_with("```") {
                break;
            }
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(inner);
        }
        out.push(body);
    }
    out
}

fn claude_line(value: &Value, when: &str, out: &mut Vec<RawShellEvent>) {
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for item in content {
        let item_type = item.get("type").and_then(Value::as_str);
        let name = item.get("name").and_then(Value::as_str);

        if item_type == Some("tool_use")
            && name == Some("Bash")
            && let Some(command) = item.pointer("/input/command").and_then(Value::as_str)
        {
            out.push(RawShellEvent {
                command: command.to_string(),
                when: when.to_string(),
                kind: EventKind::Shell,
            });
        }

        if item_type == Some("tool_use") && name == Some("Write") {
            let file_path = item
                .pointer("/input/file_path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if is_shell_script_path(file_path)
                && let Some(body) = item.pointer("/input/content").and_then(Value::as_str)
            {
                out.push(RawShellEvent {
                    command: body.to_string(),
                    when: when.to_string(),
                    kind: EventKind::ScriptFile,
                });
            }
        }

        if item_type == Some("text")
            && let Some(text) = item.get("text").and_then(Value::as_str)
        {
            for body in fenced_scripts(text) {
                out.push(RawShellEvent {
                    command: body,
                    when: when.to_string(),
                    kind: EventKind::ScriptFence,
                });
            }
        }
    }
}

fn codex_line(value: &Value, when: &str, out: &mut Vec<RawShellEvent>) {
    let Some(item) = value.pointer("/payload/item") else {
        return;
    };
    if item.get("type").and_then(Value::as_str) == Some("CommandExecution")
        && let Some(command) = item.get("command").and_then(Value::as_str)
    {
        out.push(RawShellEvent {
            command: command.to_string(),
            when: when.to_string(),
            kind: EventKind::Shell,
        });
    }
}

fn grok_line(value: &Value, when: &str, out: &mut Vec<RawShellEvent>) {
    let Some(calls) = value.get("tool_calls").and_then(Value::as_array) else {
        return;
    };
    for call in calls {
        if call.get("name").and_then(Value::as_str) == Some("run_terminal_command")
            && let Some(command) = call.pointer("/arguments/command").and_then(Value::as_str)
        {
            out.push(RawShellEvent {
                command: command.to_string(),
                when: when.to_string(),
                kind: EventKind::Shell,
            });
        }
    }
}

fn cursor_line(value: &Value, when: &str, out: &mut Vec<RawShellEvent>) {
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for item in content {
        let name = item.get("name").and_then(Value::as_str);
        if matches!(name, Some("Shell") | Some("execute"))
            && let Some(command) = item.pointer("/input/command").and_then(Value::as_str)
        {
            out.push(RawShellEvent {
                command: command.to_string(),
                when: when.to_string(),
                kind: EventKind::Shell,
            });
        }
    }
}

/// Parses one transcript line, dispatching on `agent` (`06` §1.5), and appends any shell
/// invocation it carries to `out`. A line that fails to parse as JSON, or doesn't carry the
/// shape this agent's shell-call pointer expects, contributes nothing -- never an error, since
/// a transcript legitimately mixes tool calls, text, and other envelope types line by line.
/// The one-line-at-a-time shape lets a caller stream a transcript (`orchestrate::ingest_row`)
/// rather than parse a whole slurped string at once.
pub fn parse_line(agent: Agent, line: &str, out: &mut Vec<RawShellEvent>) {
    let per_line: fn(&Value, &str, &mut Vec<RawShellEvent>) = match agent {
        Agent::Claude => claude_line,
        Agent::Codex => codex_line,
        Agent::Grok => grok_line,
        Agent::Cursor => cursor_line,
    };

    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let when = line_when(&value);
    per_line(&value, &when, out);
}

/// Parses one native transcript's shell invocations, dispatching on `agent` (`06` §1.5). A
/// thin wrapper over [`parse_line`] for callers that already hold the whole transcript in
/// memory (fixtures, tests); `orchestrate::ingest_row` calls `parse_line` directly, per line,
/// off a `BufReader` instead.
pub fn parse_transcript(agent: Agent, content: &str) -> Vec<RawShellEvent> {
    let mut out = Vec::new();
    for line in content.lines() {
        parse_line(agent, line, &mut out);
    }
    out
}
