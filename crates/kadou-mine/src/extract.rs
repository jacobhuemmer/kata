//! Sequence grouping and noise/secret filtering (`docs/design/06-session-mining.md` §2.3).
//! Operates on one session's raw shell events; the caller (ingest/orchestration) calls this
//! once per session.

use crate::model::RawShellEvent;

#[cfg(test)]
mod tests;

/// One extracted sequence: still raw commands (extraction happens before normalization and
/// redaction in the pipeline, `06` §2) -- the caller normalizes each step immediately and
/// never persists this struct as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSequence {
    pub steps: Vec<String>,
    pub when: String,
}

/// Groups one session's raw shell events into sequences, drops noise-only single-step
/// sequences, and drops any sequence containing an apparent secret in any step (`06` §2.3).
pub fn extract_sequences(events: &[RawShellEvent]) -> Vec<RawSequence> {
    todo!("{events:?}")
}
