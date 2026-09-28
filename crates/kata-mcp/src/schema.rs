//! The four hand-authored tool schemas, byte-identical to `docs/design/tools-list.json`
//! (`docs/design/05-prd.md` §5.4). This module is the single source of truth: the checked
//! JSON byte payload (`tools_list_bytes`, used by `kata mcp schema --bytes` and the
//! insta/byte-equality tests) and the real [`rmcp::model::Tool`] values the server actually
//! serves (`build_tools`) are both derived from the same [`tools_list_value`], so the two can
//! never drift apart.
//!
//! Field order inside every `inputSchema` object matters for the byte-identity test: this
//! crate turns on serde_json's `preserve_order` feature (workspace-wide, via feature
//! unification) precisely so the `serde_json::json!` literals below serialize in the order
//! they're written, not alphabetically.

use std::sync::Arc;

use rmcp::model::{CacheScope, ListToolsResult, Tool, ToolAnnotations};
use serde_json::{Value, json};

/// `propose_kata`'s `id` shape (§4.2 segment rule, applied to every `/`-separated segment,
/// at least two of them). Shared with `drafts::propose`'s own check (R3) so the wire schema
/// and the enforced rule cannot drift apart — `rmcp` does not validate `inputSchema` itself
/// (I-13), so the schema string alone is advisory without a matching runtime check.
pub const PROPOSE_ID_PATTERN: &str = "^[a-z0-9][a-z0-9-]*(/[a-z0-9][a-z0-9-]*)+$";
pub const PROPOSE_ID_MAX_LEN: usize = 128;
pub const PROPOSE_SOURCE_MAX_BYTES: usize = 65536;

/// The exact `tools/list` response body, matching `docs/design/tools-list.json` byte for
/// byte once serialized compactly.
///
/// Deliberately one `json!` literal, not four smaller functions: this *is* the wire contract
/// (CLAUDE.md invariant 1, "`crates/kata-mcp/src/schema.rs` is the only place this JSON is
/// built") and reads as the single source of truth a byte-identity test can check against
/// `docs/design/tools-list.json` — splitting it by tool would only move lines around a value
/// that must stay reviewable as one block.
#[allow(clippy::too_many_lines)]
pub fn tools_list_value() -> Value {
    json!({
        // SEP-2549 cache hints, required by spec 2026-07-28 (Claude Code rejects a result
        // without them). 0 ms and private: clients re-fetch as before, nothing is shared.
        // Serialized before "tools" because that is rmcp's ListToolsResult field order.
        "ttlMs": 0,
        "cacheScope": "private",
        "tools": [
            {
                "name": "list_kata",
                "description": "Kata is a DevOps script library of reviewed scripts. Search here before kubectl, helm, git, or unittest one-offs. Returns id, about, risk. No schemas.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": { "type": "string", "maxLength": 200, "description": "Substring of id, alias, or about." },
                        "prefix": { "type": "string", "description": "Id prefix (starter, sesami, agent)." },
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
                        "id": { "type": "string", "description": "prefix/name, ./name, or alias." },
                        "include_source": { "type": "boolean", "default": true }
                    }
                },
                "annotations": { "readOnlyHint": true }
            },
            {
                "name": "run_kata",
                "description": "Run a kata. Prefer this over a one-off shell when list_kata found a match. Secrets are needs from the vault, never args. Above your grant it returns pending_grant.",
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
                        "id": { "type": "string", "pattern": PROPOSE_ID_PATTERN, "maxLength": PROPOSE_ID_MAX_LEN },
                        "source": { "type": "string", "maxLength": PROPOSE_SOURCE_MAX_BYTES }
                    }
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": false }
            }
        ]
    })
}

/// [`tools_list_value`] is a `json!` literal in this file, not user or file input — every
/// `SchemaError` here names a shape defect that could only come from editing that literal
/// incorrectly, and both [`tools_list_bytes_matches_the_checked_in_fixture_byte_for_byte`] and
/// the wire-level byte gate in `tests/mcp_server.rs` would fail immediately if it ever did
/// (R6: a typed `Result` instead of `expect`, so a mistake here is still a clean error, not a
/// panic, on whichever path notices first).
#[derive(Debug, thiserror::Error)]
#[error("malformed tools_list_value: {0}")]
pub struct SchemaError(String);

/// Compact (non-pretty) UTF-8 bytes of [`tools_list_value`] — what `kata mcp schema --bytes`
/// prints and what the byte-identity tests compare against `docs/design/tools-list.json`.
pub fn tools_list_bytes() -> Result<Vec<u8>, SchemaError> {
    serde_json::to_vec(&tools_list_value()).map_err(|e| SchemaError(e.to_string()))
}

/// The four tools as real [`Tool`] values, built from [`tools_list_value`] so the served
/// `tools/list` response can never drift from the checked-in schema (§5.1: "exactly these
/// four tools, in this order").
pub fn build_tools() -> Result<Vec<Tool>, SchemaError> {
    let value = tools_list_value();
    value["tools"]
        .as_array()
        .ok_or_else(|| SchemaError("no top-level \"tools\" array".to_string()))?
        .iter()
        .map(tool_from_value)
        .collect()
}

/// The whole `tools/list` result: the four tools plus the cache hints, all read from
/// [`tools_list_value`] so the served result matches `docs/design/tools-list.json`.
pub fn build_list_tools_result() -> Result<ListToolsResult, SchemaError> {
    let value = tools_list_value();
    let ttl_ms = value["ttlMs"]
        .as_u64()
        .ok_or_else(|| SchemaError("no \"ttlMs\" number".to_string()))?;
    let cache_scope: CacheScope = serde_json::from_value(value["cacheScope"].clone())
        .map_err(|e| SchemaError(format!("bad \"cacheScope\": {e}")))?;
    Ok(ListToolsResult::with_all_items(build_tools()?)
        .with_ttl_ms(ttl_ms)
        .with_cache_scope(cache_scope))
}

fn tool_from_value(entry: &Value) -> Result<Tool, SchemaError> {
    let name = entry["name"]
        .as_str()
        .ok_or_else(|| SchemaError("a tool entry has no \"name\" string".to_string()))?
        .to_string();
    let description = entry["description"]
        .as_str()
        .ok_or_else(|| SchemaError(format!("tool `{name}` has no \"description\" string")))?
        .to_string();
    let input_schema = entry["inputSchema"]
        .as_object()
        .ok_or_else(|| SchemaError(format!("tool `{name}` has no \"inputSchema\" object")))?
        .clone();

    let mut tool = Tool::new(name, description, Arc::new(input_schema));
    if let Some(annotations) = entry.get("annotations") {
        tool = tool.with_annotations(annotations_from_value(annotations));
    }
    Ok(tool)
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
            String::from_utf8(tools_list_bytes().unwrap()).unwrap(),
            String::from_utf8(expected).unwrap()
        );
    }

    #[test]
    fn tools_list_is_at_most_2800_bytes() {
        assert!(
            tools_list_bytes().unwrap().len() <= 2800,
            "tools/list grew past the 2800-byte CI gate (§1.3, §9 slice 5): {} bytes",
            tools_list_bytes().unwrap().len()
        );
    }

    #[test]
    fn build_tools_has_the_four_tools_in_order() {
        let tools = build_tools().unwrap();
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
