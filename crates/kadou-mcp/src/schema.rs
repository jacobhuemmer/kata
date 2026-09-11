//! The four hand-authored tool schemas, byte-identical to `docs/design/tools-list.json`
//! (`docs/design/05-prd.md` §5.4). This module is the single source of truth: the checked
//! JSON byte payload (`tools_list_bytes`, used by `kadou mcp schema --bytes` and the
//! insta/byte-equality tests) and the real [`rmcp::model::Tool`] values the server actually
//! serves (`build_tools`) are both derived from the same [`tools_list_value`], so the two can
//! never drift apart.
//!
//! Field order inside every `inputSchema` object matters for the byte-identity test: this
//! crate turns on serde_json's `preserve_order` feature (workspace-wide, via feature
//! unification) precisely so the `serde_json::json!` literals below serialize in the order
//! they're written, not alphabetically.

use std::sync::Arc;

use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{Value, json};

/// The exact `tools/list` response body, matching `docs/design/tools-list.json` byte for
/// byte once serialized compactly.
pub fn tools_list_value() -> Value {
    json!({
        "tools": [
            {
                "name": "list_kata",
                "description": "Search kata (reviewed scripts) visible to this agent. Returns id, about, risk. No schemas.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": { "type": "string", "maxLength": 200, "description": "Substring of id, alias, or about." },
                        "folder": { "type": "string" },
                        "risk": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 },
                        "offset": { "type": "integer", "minimum": 0, "default": 0 },
                        "include_drafts": { "type": "boolean", "default": false, "description": "Also list non-runnable drafts (mined, proposed)." }
                    }
                },
                "annotations": { "readOnlyHint": true }
            },
            {
                "name": "describe_kata",
                "description": "One kata: args schema, needs, risk, source. Read it before run_kata.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id"],
                    "properties": {
                        "id": { "type": "string", "description": "folder/name, ./name, or alias." },
                        "include_source": { "type": "boolean", "default": true }
                    }
                },
                "annotations": { "readOnlyHint": true }
            },
            {
                "name": "run_kata",
                "description": "Run one kata with args. Secrets come from the vault as needs; never pass them. Above your grant it returns pending_grant.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id"],
                    "properties": {
                        "id": { "type": "string" },
                        "args": { "type": "object", "default": {}, "additionalProperties": true, "description": "Parameter name to value, per describe_kata." },
                        "dry_run": { "type": "boolean", "default": false, "description": "Resolve args and env names without executing." }
                    }
                },
                "annotations": { "destructiveHint": true, "openWorldHint": true }
            },
            {
                "name": "propose_kata",
                "description": "Draft a kata (one file with a header) for human review. Never registers or runs it.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "source"],
                    "properties": {
                        "id": { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]*(/[a-z0-9][a-z0-9-]*)+$", "maxLength": 128 },
                        "source": { "type": "string", "maxLength": 65536 }
                    }
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": false }
            }
        ]
    })
}

/// Compact (non-pretty) UTF-8 bytes of [`tools_list_value`] — what `kadou mcp schema --bytes`
/// prints and what the byte-identity tests compare against `docs/design/tools-list.json`.
pub fn tools_list_bytes() -> Vec<u8> {
    serde_json::to_vec(&tools_list_value()).expect("the static tool schema always serializes")
}

/// The four tools as real [`Tool`] values, built from [`tools_list_value`] so the served
/// `tools/list` response can never drift from the checked-in schema (§5.1: "exactly these
/// four tools, in this order").
pub fn build_tools() -> Vec<Tool> {
    let value = tools_list_value();
    value["tools"]
        .as_array()
        .expect("tools_list_value always has a tools array")
        .iter()
        .map(tool_from_value)
        .collect()
}

fn tool_from_value(entry: &Value) -> Tool {
    let name = entry["name"]
        .as_str()
        .expect("every tool entry has a name")
        .to_string();
    let description = entry["description"]
        .as_str()
        .expect("every tool entry has a description")
        .to_string();
    let input_schema = entry["inputSchema"]
        .as_object()
        .expect("every tool entry has an inputSchema object")
        .clone();

    let mut tool = Tool::new(name, description, Arc::new(input_schema));
    if let Some(annotations) = entry.get("annotations") {
        tool = tool.with_annotations(annotations_from_value(annotations));
    }
    tool
}

fn annotations_from_value(value: &Value) -> ToolAnnotations {
    ToolAnnotations::from_raw(
        None,
        value.get("readOnlyHint").and_then(Value::as_bool),
        value.get("destructiveHint").and_then(Value::as_bool),
        value.get("idempotentHint").and_then(Value::as_bool),
        value.get("openWorldHint").and_then(Value::as_bool),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_bytes() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/design/tools-list.json"
        ))
        .expect("docs/design/tools-list.json must exist")
    }

    #[test]
    fn tools_list_bytes_matches_the_checked_in_fixture_byte_for_byte() {
        let mut expected = fixture_bytes();
        // The checked-in file ends with a trailing newline from being saved as a text file;
        // the served payload (and the byte-budget gate, §1.3) is the JSON content itself.
        while expected.last() == Some(&b'\n') {
            expected.pop();
        }
        assert_eq!(
            String::from_utf8(tools_list_bytes()).unwrap(),
            String::from_utf8(expected).unwrap()
        );
    }

    #[test]
    fn tools_list_is_at_most_2800_bytes() {
        assert!(
            tools_list_bytes().len() <= 2800,
            "tools/list grew past the 2800-byte CI gate (§1.3, §9 slice 5): {} bytes",
            tools_list_bytes().len()
        );
    }

    #[test]
    fn build_tools_has_the_four_tools_in_order() {
        let tools = build_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(
            names,
            vec!["list_kata", "describe_kata", "run_kata", "propose_kata"]
        );
        assert_eq!(
            tools[2].annotations.as_ref().unwrap().destructive_hint,
            Some(true)
        );
        assert_eq!(
            tools[2].annotations.as_ref().unwrap().open_world_hint,
            Some(true)
        );
    }
}
