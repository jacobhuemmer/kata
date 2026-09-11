//! The stdio (default) and opt-in loopback-HTTP MCP server: the four meta-tools
//! `list_kata`, `describe_kata`, `run_kata`, `propose_kata`, with hand-authored
//! `inputSchema`s and no resources/prompts on the default list (`docs/design/05-prd.md` §5).

mod concurrency;
mod drafts;
mod env;
mod history;
mod redact;
pub mod schema;
mod visibility;
