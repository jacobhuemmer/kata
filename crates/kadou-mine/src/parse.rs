//! Per-agent native-transcript parsers (`docs/design/06-session-mining.md` §1.5, §2.2). Each
//! parser binds to the key paths sketched there and extracts only the shell-invocation shape
//! (`Kind::Shell`); `Write`/`FileChange` script-file and fenced-script extraction (§2.3's
//! other two kinds) are out of scope for this slice -- shell invocations are the dominant,
//! highest-value source per the `06` §1 survey, and the fixture/pipeline tests below only
//! need this kind to reproduce the §6.2 worked example.
//!
//! Every parser drops (never persists) `stdout`/`stderr`/`toolUseResult` -- it never even reads
//! those keys -- matching `06` §2.3 "Drop: ... tool stdout/stderr".

use crate::model::{Agent, RawShellEvent};

#[cfg(test)]
mod tests;

/// Parses one native transcript's shell invocations, dispatching on `agent` (`06` §1.5). A
/// line that fails to parse as JSON, or doesn't carry the shape this agent's shell-call
/// pointer expects, contributes nothing -- never an error, since a transcript legitimately
/// mixes tool calls, text, and other envelope types line by line.
pub fn parse_transcript(agent: Agent, content: &str) -> Vec<RawShellEvent> {
    todo!("{agent:?} {content}")
}
