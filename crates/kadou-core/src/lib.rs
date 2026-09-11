//! Domain types, TOML config, and XDG paths shared by every kadou interface (CLI and MCP).
//!
//! This crate owns the I/O for `Config` and `KadouPaths` (file reads/writes, XDG
//! resolution); it never runs a script and never speaks MCP — `kadou-exec` and `kadou-mcp`
//! own those (`docs/design/05-prd.md` §3).

mod check;
mod config;
mod digest;
mod header;
mod import;
mod kata;
mod lookup;
mod paths;
mod resolve;
mod risk;
mod scan;

pub use check::{CheckReport, FolderReport, check_all, check_folder, check_path, render_report};
pub use config::{
    AgentConfig, Config, ConfigError, EMPTY_TEMPLATE, ExecConfig, FolderConfig, McpConfig,
    NotifyConfig, TrustConfig, VaultConfig,
};
pub use digest::file_sha256;
pub use header::{
    Diagnostic, ParsedHeader, Severity, looks_like_kata_candidate, parse_header, render_header,
};
pub use import::{ImportError, ImportSummary, import_catalog};
pub use kata::{Arg, ArgDefault, ArgType, Kata, Need};
pub use lookup::{LookupResult, find_kata, kata_from_scanned};
pub use paths::{KadouPaths, PathsError, Resolved, discover, resolve};
pub use resolve::{ResolveError, ResolvedNeed, ResolvedVar, resolve_args, resolve_needs};
pub use risk::RiskLevel;
pub use scan::{ScanError, ScannedFile, scan_folder, scan_kata_dir};
