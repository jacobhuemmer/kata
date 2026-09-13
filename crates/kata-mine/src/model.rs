//! Shared shapes between ingest, the per-agent parsers, and extraction
//! (`docs/design/06-session-mining.md` §2.2-2.3).

/// The three extraction kinds `06` §2.2's event stream `kind` field, and §2.3's "Extract" table,
/// name: a single shell invocation, a generated script file (a `Write`/`FileChange` whose path
/// ends `.sh`/`.bash`), or a fenced script in assistant markdown. `Shell`'s `command` is one
/// command line; `ScriptFile`/`ScriptFence`'s `command` is the whole (possibly multi-line)
/// script body -- `extract::extract_sequences` is what splits that body into steps, not the
/// parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Shell,
    ScriptFile,
    ScriptFence,
}

/// One shell invocation or script body as a parser found it in a native transcript, before
/// extraction groups consecutive `Shell` events into sequences (or turns a `ScriptFile`/
/// `ScriptFence` event into its own sequence). `when` is best-effort: a per-line timestamp when
/// the transcript carries one, otherwise the session-level `when` from the event markdown
/// pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawShellEvent {
    pub command: String,
    pub when: String,
    pub kind: EventKind,
}

/// The four transcript shapes this crate parses (`06` §1.5). Not `Cursor` yet in practice --
/// no archive rows exist for it in the real corpus -- but the parser exists so ingest treats
/// every agent the same way once a Cursor pointer does appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
    Grok,
    Cursor,
}

impl Agent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Grok => "grok",
            Self::Cursor => "cursor",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "grok" => Some(Self::Grok),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }
}
