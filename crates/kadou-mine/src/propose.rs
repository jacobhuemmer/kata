//! Draft writer: turns a ranked cluster into a single-file kata draft plus its `meta.json`
//! (`docs/design/05-prd.md` §6.8 "[shape] The miner writes a single-file draft with a header,
//! not `format_version: 2` YAML"; `docs/design/06-session-mining.md` §2.8).
//!
//! Every string that reaches [`Proposal`] has passed through [`crate::redact::redact_all`]
//! first; if the fail-closed high-entropy check still trips afterward, the whole candidate is
//! dropped (`06` §2.7, §4.1) rather than written anywhere.

use std::collections::BTreeMap;

use kadou_core::{Arg, ArgDefault, ArgType, RiskLevel};
use serde::Serialize;

use crate::cluster::Cluster;
use crate::redact;

#[cfg(test)]
mod tests;

/// One normalized step plus the concrete values its placeholders stood for, as recorded when
/// the orchestrator first normalized this exact template (`06` §2.4/§2.8).
#[derive(Debug, Clone)]
pub struct StepWithParams {
    pub template: String,
    pub params: BTreeMap<String, String>,
}

/// Placeholder names that are shape-generic, not a meaningful named parameter -- their
/// captured value is exactly the kind of thing `06` §4.1's never-written list forbids
/// (`EMAIL`/`HOST`/`IP`), or otherwise uninteresting to expose as a header default (`ID`,
/// `TS`, `VAL`). These never become a header `args:` line; their template token is left
/// as-is in the script body for a human reviewer to fill in.
const GENERIC_PARAM_NAMES: [&str; 6] = ["ID", "TS", "EMAIL", "HOST", "IP", "VAL"];

/// Per-environment identifiers a human should supply fresh, not a value worth hardcoding as a
/// default -- `06` §6.4's own worked example declares these `required: true` with no default
/// at all, unlike `app`/`tail` which do get one. A side effect: the captured example value
/// (an environment/context name) never gets written into the draft at all.
const REQUIRED_NO_DEFAULT_NAMES: [&str; 2] = ["CONTEXT", "NAMESPACE"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposeError {
    /// Redaction could not be proven safe -- fail closed, drop the candidate (`06` §4.1).
    RedactionUnproven,
}

#[derive(Debug, Serialize)]
struct SourceRef {
    agent: String,
    session_id: String,
    when: String,
}

#[derive(Debug, Serialize)]
struct Meta {
    fingerprint: String,
    slug: String,
    score: f64,
    freq: usize,
    unique_sessions: usize,
    unique_agents: usize,
    first_seen: String,
    last_seen: String,
    risk: String,
    source_refs: Vec<SourceRef>,
}

#[derive(Debug, Clone)]
pub struct Proposal {
    pub fingerprint: String,
    pub slug: String,
    pub kata_source: String,
    pub meta_json: String,
}

/// A short, id-segment-shaped slug from the first step's argv0 and its first couple of bare
/// (non-flag, non-placeholder) words -- `kubectl --context $CONTEXT get pods` becomes
/// `kubectl-get-pods`.
fn slug_from_first_step(first_step: &str) -> String {
    let words: Vec<&str> = first_step
        .split_whitespace()
        .filter(|w| !w.starts_with('-') && !w.starts_with('$'))
        .take(3)
        .collect();
    let joined = words.join("-").to_ascii_lowercase();
    let cleaned: String = joined
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "mined-automation".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Runs `redact_all` and the R14 fail-closed check together; `Err` means the caller must drop
/// the whole candidate rather than write anything (`06` §4.1).
fn redact_or_drop(text: &str, extra: &[String]) -> Result<String, ProposeError> {
    let redacted = redact::redact_all(text, extra);
    if redact::has_high_entropy_leak(&redacted.text) {
        return Err(ProposeError::RedactionUnproven);
    }
    Ok(redacted.text)
}

/// Declared (non-generic) params across every step, sorted longest-name-first so substitution
/// never matches a shorter name as a prefix of a longer one (`$N` inside `$NAMESPACE`).
fn declared_params(steps: &[StepWithParams]) -> BTreeMap<String, String> {
    let mut declared = BTreeMap::new();
    for step in steps {
        for (name, value) in &step.params {
            if !GENERIC_PARAM_NAMES.contains(&name.as_str()) {
                declared
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    declared
}

fn arg_for(name: &str, example_value: &str) -> Arg {
    if REQUIRED_NO_DEFAULT_NAMES.contains(&name) {
        return Arg {
            name: name.to_ascii_lowercase(),
            ty: ArgType::Text,
            default: None,
            help: None,
        };
    }
    let (ty, default) = match example_value.parse::<i64>() {
        Ok(n) => (ArgType::Int, ArgDefault::Int(n)),
        Err(_) => (ArgType::Text, ArgDefault::Text(example_value.to_string())),
    };
    Arg {
        name: name.to_ascii_lowercase(),
        ty,
        default: Some(default),
        help: None,
    }
}

/// Replaces `$NAME` (word-boundary) with a properly quoted `"${NAME}"` shell reference for
/// every declared param -- longest name first so `$NAMESPACE` substitutes before `$N` can
/// match its prefix.
fn quote_declared_params(step: &str, declared_names: &[String]) -> String {
    let mut ordered = declared_names.to_vec();
    ordered.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let mut out = step.to_string();
    for name in ordered {
        out = replace_placeholder(&out, &name, &format!("\"${{{name}}}\""));
    }
    out
}

/// A hand-rolled word-boundary token replace (no regex dependency needed here): finds
/// `$NAME` where the character right after `NAME` is not itself an identifier character.
fn replace_placeholder(text: &str, name: &str, replacement: &str) -> String {
    let needle = format!("${name}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(&needle) {
        let after = pos + needle.len();
        let boundary_ok = rest[after..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        out.push_str(&rest[..pos]);
        if boundary_ok {
            out.push_str(replacement);
        } else {
            out.push_str(&needle);
        }
        rest = &rest[after..];
    }
    out.push_str(rest);
    out
}

/// One declared param's env-assignment line: `${NAME:?msg}` when it has no header default
/// (`REQUIRED_NO_DEFAULT_NAMES`), `${NAME:-value}` otherwise -- shell's own required-vs-
/// defaulted syntax, matching whatever `arg_for` declared in the header.
fn env_line_for(name: &str, redacted_value: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if REQUIRED_NO_DEFAULT_NAMES.contains(&name) {
        format!("{name}=\"${{{name}:?{lower} is required}}\"\n")
    } else {
        format!("{name}=\"${{{name}:-{redacted_value}}}\"\n")
    }
}

fn build_body(steps: &[StepWithParams], declared: &BTreeMap<String, String>) -> String {
    let declared_names: Vec<String> = declared.keys().cloned().collect();
    let mut env_lines = String::new();
    for (name, value) in declared {
        env_lines.push_str(&env_line_for(name, value));
    }

    let mut main_body = String::new();
    let total = steps.len();
    for (i, step) in steps.iter().enumerate() {
        main_body.push_str(&format!("  echo \"==> Step {}/{}\"\n", i + 1, total));
        main_body.push_str("  ");
        main_body.push_str(&quote_declared_params(&step.template, &declared_names));
        main_body.push('\n');
    }

    format!("set -eu\n\n{env_lines}\nmain() {{\n{main_body}}}\n\nmain \"$@\"\n")
}

/// Builds a redacted, single-file kata draft plus its `meta.json` from a ranked cluster
/// (`06` §2.7-2.8). `steps` carries the medoid's own normalized templates and the concrete
/// values recorded when the orchestrator first normalized them.
pub fn build_proposal(
    cluster: &Cluster,
    steps: &[StepWithParams],
    score: f64,
    risk: RiskLevel,
    redact_extra: &[String],
) -> Result<Proposal, ProposeError> {
    let declared = declared_params(steps);
    let mut redacted_declared = BTreeMap::new();
    for (name, value) in &declared {
        redacted_declared.insert(name.clone(), redact_or_drop(value, redact_extra)?);
    }

    let about_raw = format!(
        "Mined automation: {}",
        steps
            .first()
            .map(|s| s.template.as_str())
            .unwrap_or_default()
    );
    let about = redact_or_drop(&about_raw, redact_extra)?;
    let about = about.chars().take(120).collect::<String>();

    let args: Vec<Arg> = redacted_declared
        .iter()
        .map(|(name, value)| arg_for(name, value))
        .collect();

    let body_raw = build_body(steps, &redacted_declared);
    let body = redact_or_drop(&body_raw, redact_extra)?;

    let kata_source = kadou_core::render_header(
        Some("#!/bin/sh"),
        &about,
        risk,
        &[],
        &args,
        &[],
        None,
        &body,
    );

    let source_refs = cluster
        .members
        .iter()
        .map(|m| SourceRef {
            agent: m.agent.clone(),
            session_id: m.session_id.clone(),
            when: m.when.clone(),
        })
        .collect();

    let slug = slug_from_first_step(steps.first().map(|s| s.template.as_str()).unwrap_or(""));
    let meta = Meta {
        fingerprint: cluster.fingerprint.clone(),
        slug: slug.clone(),
        score,
        freq: cluster.freq(),
        unique_sessions: cluster.unique_sessions(),
        unique_agents: cluster.unique_agents(),
        first_seen: cluster.first_seen.clone(),
        last_seen: cluster.last_seen.clone(),
        risk: risk.as_str().to_string(),
        source_refs,
    };
    let meta_json = serde_json::to_string(&meta).unwrap_or_default();

    Ok(Proposal {
        fingerprint: cluster.fingerprint.clone(),
        slug,
        kata_source,
        meta_json,
    })
}
