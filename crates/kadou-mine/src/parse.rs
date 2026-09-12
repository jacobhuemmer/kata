//! Per-agent native-transcript parsers (`docs/design/06-session-mining.md` §1.5, §2.2). Each
//! parser binds to the key paths sketched there and extracts only the shell-invocation shape
//! (`Kind::Shell`); `Write`/`FileChange` script-file and fenced-script extraction (§2.3's
//! other two kinds) are out of scope for this slice -- shell invocations are the dominant,
//! highest-value source per the `06` §1 survey, and the fixture/pipeline tests below only
//! need this kind to reproduce the §6.2 worked example.
//!
//! Every parser drops (never persists) `stdout`/`stderr`/`toolUseResult` -- it never even reads
//! those keys -- matching `06` §2.3 "Drop: ... tool stdout/stderr".

use serde_json::Value;

use crate::model::{Agent, RawShellEvent};

#[cfg(test)]
mod tests;

fn line_when(value: &Value) -> String {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn claude_line(value: &Value, when: &str, out: &mut Vec<RawShellEvent>) {
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for item in content {
        if item.get("type").and_then(Value::as_str) == Some("tool_use")
            && item.get("name").and_then(Value::as_str) == Some("Bash")
            && let Some(command) = item.pointer("/input/command").and_then(Value::as_str)
        {
            out.push(RawShellEvent {
                command: command.to_string(),
                when: when.to_string(),
            });
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
