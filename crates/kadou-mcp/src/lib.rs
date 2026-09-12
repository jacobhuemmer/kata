//! The stdio (default) and opt-in loopback-HTTP MCP server: the four meta-tools
//! `list_kata`, `describe_kata`, `run_kata`, `propose_kata`, with hand-authored
//! `inputSchema`s and no resources/prompts on the default list (`docs/design/05-prd.md` §5).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod concurrency;
mod drafts;
mod env;
mod notify;
pub mod pending;
pub mod schema;
mod server;
pub mod state;
mod tools;
mod visibility;

pub use drafts::{AcceptError, AcceptPreparation, apply_accept, prepare_accept};
pub use server::KadouMcpServer;
pub use state::ServerState;

/// Re-exported from `kadou-core` for one release (R2, `docs/design/05-prd.md` §3 assigns
/// history/redaction to `kadou-core`); callers should move to `kadou_core::history` and
/// `kadou_core::redact` directly.
pub use kadou_core::history;
pub use kadou_core::redact;
