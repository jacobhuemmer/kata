//! Sequence grouping and noise/secret filtering (`docs/design/06-session-mining.md` §2.3).
//! Operates on one session's raw shell events; the caller (ingest/orchestration) calls this
//! once per session.

use crate::model::RawShellEvent;
use crate::redact::RuleId;

#[cfg(test)]
mod tests;

/// Consecutive shell invocations in the same session with a gap under this become one
/// candidate with `steps[]` -- "that is the interesting automation, not the one-liner" (`06`
/// §2.3).
const SEQUENCE_GAP_SECONDS: u64 = 120;

/// Dropped as noise unless they appear inside a longer sequence (`06` §2.3).
const NOISE_COMMANDS: [&str; 4] = ["git status", "ls", "pwd", "whoami"];

/// One extracted sequence: still raw commands (extraction happens before normalization and
/// redaction in the pipeline, `06` §2) -- the caller normalizes each step immediately and
/// never persists this struct as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSequence {
    pub steps: Vec<String>,
    pub when: String,
}

fn seconds_between(earlier: &str, later: &str) -> Option<u64> {
    let earlier = humantime::parse_rfc3339_weak(earlier).ok()?;
    let later = humantime::parse_rfc3339_weak(later).ok()?;
    later.duration_since(earlier).ok().map(|d| d.as_secs())
}

/// `true` when `command` (trimmed) is exactly one of the noise commands (`06` §2.3) -- an
/// exact match only, so `git status --short` (a real, more specific invocation) is untouched.
fn is_noise_command(command: &str) -> bool {
    NOISE_COMMANDS.contains(&command.trim())
}

/// `true` when `command` matches a strong secret shape before any redaction has run -- fail
/// closed at extraction time, in addition to (not instead of) the redact stage before propose
/// (`06` §2.3 "Anything matching a secret pattern before normalization: fail closed").
fn looks_like_a_secret(command: &str) -> bool {
    let redacted = crate::redact::redact_all(command, &[]);
    redacted.rules_hit.iter().any(|rule| {
        matches!(
            rule,
            RuleId::PemPrivateKey | RuleId::KnownTokens | RuleId::Bearer | RuleId::Connection
        )
    })
}

/// A sequence still being assembled: `steps`/`first_when` become the final [`RawSequence`];
/// `last_when` (the most recently added step's own timestamp, not the sequence's start) is
/// what the next event's gap is measured against.
struct Building {
    steps: Vec<String>,
    first_when: String,
    last_when: String,
}

/// Groups one session's raw shell events into sequences, drops noise-only single-step
/// sequences, and drops any sequence containing an apparent secret in any step (`06` §2.3).
pub fn extract_sequences(events: &[RawShellEvent]) -> Vec<RawSequence> {
    let mut building: Vec<Building> = Vec::new();

    for event in events {
        let starts_new = match building.last() {
            Some(last) => seconds_between(&last.last_when, &event.when)
                .is_none_or(|gap| gap >= SEQUENCE_GAP_SECONDS),
            None => true,
        };

        if starts_new {
            building.push(Building {
                steps: vec![event.command.clone()],
                first_when: event.when.clone(),
                last_when: event.when.clone(),
            });
        } else if let Some(last) = building.last_mut() {
            last.steps.push(event.command.clone());
            last.last_when.clone_from(&event.when);
        }
    }

    building
        .into_iter()
        .map(|b| RawSequence {
            steps: b.steps,
            when: b.first_when,
        })
        .filter(|seq| !(seq.steps.len() == 1 && is_noise_command(&seq.steps[0])))
        .filter(|seq| !seq.steps.iter().any(|step| looks_like_a_secret(step)))
        .collect()
}
