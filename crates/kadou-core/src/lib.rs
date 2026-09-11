//! Domain types, TOML config, and XDG paths shared by every kadou interface (CLI and MCP).
//!
//! This crate owns the I/O for `Config` and `KadouPaths` (file reads/writes, XDG
//! resolution); it never runs a script and never speaks MCP — `kadou-exec` and `kadou-mcp`
//! own those (`docs/design/05-prd.md` §3).

mod config;
mod header;
mod kata;
mod paths;
mod risk;

pub use config::{
    AgentConfig, Config, ConfigError, EMPTY_TEMPLATE, ExecConfig, FolderConfig, McpConfig,
    NotifyConfig, TrustConfig, VaultConfig,
};
pub use header::{
    Diagnostic, ParsedHeader, Severity, looks_like_kata_candidate, parse_header, render_header,
};
pub use kata::{Arg, ArgDefault, ArgType, Kata, Need};
pub use paths::{KadouPaths, PathsError, Resolved, discover, resolve};
pub use risk::RiskLevel;
