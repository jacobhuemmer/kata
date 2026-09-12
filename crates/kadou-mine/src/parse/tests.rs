//! Per-agent parser tests, one synthetic transcript line per shape sketched in
//! `docs/design/06-session-mining.md` §1.5.

use super::*;

#[test]
fn claude_extracts_bash_tool_use_commands() {
    let line = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-01T00:00:00Z","cwd":"/repo","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"kubectl get pods","description":"list pods"}}]},"toolUseResult":{"stdout":"should never be read","stderr":""}}"#;
    let events = parse_transcript(Agent::Claude, line);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "kubectl get pods");
    assert_eq!(events[0].when, "2026-09-01T00:00:00Z");
}

#[test]
fn claude_ignores_non_bash_tool_use_and_text_content() {
    let line = r#"{"type":"assistant","timestamp":"2026-09-01T00:00:00Z","message":{"content":[{"type":"text","text":"hello"},{"type":"tool_use","name":"Read","input":{"file_path":"/a"}}]}}"#;
    assert!(parse_transcript(Agent::Claude, line).is_empty());
}

#[test]
fn codex_extracts_command_execution_items() {
    let line = r#"{"timestamp":"2026-09-04T00:00:00Z","ordinal":1,"type":"response_item","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":"kubectl get pods","parsed_cmd":[{"cmd":"kubectl"}],"stdout":"should never be read"}}}"#;
    let events = parse_transcript(Agent::Codex, line);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "kubectl get pods");
    assert_eq!(events[0].when, "2026-09-04T00:00:00Z");
}

#[test]
fn codex_ignores_non_command_execution_items() {
    let line = r#"{"timestamp":"2026-09-04T00:00:00Z","payload":{"type":"function_call","item":{"type":"FileChange","path":"/a"}}}"#;
    assert!(parse_transcript(Agent::Codex, line).is_empty());
}

#[test]
fn grok_extracts_run_terminal_command_tool_calls() {
    let line = r#"{"type":"assistant","timestamp":"2026-09-01T00:00:00Z","tool_calls":[{"id":"1","name":"run_terminal_command","arguments":{"command":"kubectl get pods","description":"list"}}]}"#;
    let events = parse_transcript(Agent::Grok, line);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "kubectl get pods");
}

#[test]
fn grok_ignores_other_tool_calls_and_tool_results() {
    let line = r#"{"type":"tool_result","tool_call_id":"1","content":"should never be read"}"#;
    assert!(parse_transcript(Agent::Grok, line).is_empty());
}

#[test]
fn cursor_extracts_shell_and_execute_tool_input() {
    let line = r#"{"role":"assistant","timestamp":"2026-09-01T00:00:00Z","message":{"content":[{"type":"tool_use","name":"Shell","input":{"command":"kubectl get pods","working_directory":"/repo"}}]}}"#;
    let events = parse_transcript(Agent::Cursor, line);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "kubectl get pods");
}

#[test]
fn multi_line_transcript_preserves_order() {
    let content = [
        r#"{"timestamp":"2026-09-04T00:00:00Z","payload":{"item":{"type":"CommandExecution","command":"kubectl get pods"}}}"#,
        r#"{"timestamp":"2026-09-04T00:01:00Z","payload":{"item":{"type":"CommandExecution","command":"kubectl logs deploy/api"}}}"#,
    ]
    .join("\n");
    let events = parse_transcript(Agent::Codex, &content);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].command, "kubectl get pods");
    assert_eq!(events[1].command, "kubectl logs deploy/api");
}

#[test]
fn a_malformed_json_line_is_skipped_not_an_error() {
    let content = "not json at all\n{\"timestamp\":\"2026-09-04T00:00:00Z\",\"payload\":{\"item\":{\"type\":\"CommandExecution\",\"command\":\"kubectl get pods\"}}}";
    let events = parse_transcript(Agent::Codex, content);
    assert_eq!(events.len(), 1);
}

#[test]
fn agent_parse_round_trips_every_spelling() {
    assert_eq!(Agent::parse("claude"), Some(Agent::Claude));
    assert_eq!(Agent::parse("codex"), Some(Agent::Codex));
    assert_eq!(Agent::parse("grok"), Some(Agent::Grok));
    assert_eq!(Agent::parse("cursor"), Some(Agent::Cursor));
    assert_eq!(Agent::parse("unknown"), None);
    for agent in [Agent::Claude, Agent::Codex, Agent::Grok, Agent::Cursor] {
        assert_eq!(Agent::parse(agent.as_str()), Some(agent));
    }
}
