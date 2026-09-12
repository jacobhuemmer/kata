//! `kadou-mine`: session mining engine (`docs/design/05-prd.md` §6.8, §9 slice 9;
//! `docs/design/06-session-mining.md` §2-4). Turns repeated agent shell work recorded in
//! `~/Documents/Sessions/` into redacted, single-file kata drafts under
//! `~/.local/state/kadou/mine/queue/<fingerprint>/`, gated behind human review.
//!
//! This crate never runs a mined script and never writes the vault or the kata library
//! directly (`06` §4.1). It has no MCP types: `crates/kadou-mcp` already lists and describes
//! `mined/*` drafts generically through [`kadou_core::scan_folder`] once a human approves one
//! into `~/.local/state/kadou/mined/` (the same namespace `propose_kata` drafts use).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod cluster;
pub mod extract;
pub mod model;
pub mod normalize;
pub mod parse;
pub mod propose;
pub mod rank;
pub mod redact;
pub mod risk;
