//! `kata mcp` (§5.1, §7.1, §9 slice 5).

use std::path::PathBuf;
use std::process::ExitCode;

use kata_core::RiskLevel;

use super::{materialize_starter, resolve_paths};

/// `kata mcp serve [--transport stdio|http] [--bind ...] [--max-risk LEVEL]` (§5.1, §7.1, §9
/// slice 5). HTTP stays behind a cargo feature this slice does not enable (§5.1 "HTTP stays
/// behind a build-time cargo feature"), so any transport other than `stdio` is a clean
/// refusal rather than a silent fallback.
pub fn run_mcp_serve(
    transport: String,
    _bind: Option<String>,
    max_risk: Option<String>,
) -> ExitCode {
    if transport != "stdio" {
        eprintln!("error: --transport {transport} is not available in this build");
        eprintln!(
            "  = HTTP transport stays behind a cargo feature not yet shipped (docs/design/05-prd.md §5.1); use --transport stdio"
        );
        return ExitCode::from(2);
    }

    // Structured logs to stderr, never stdout (the stdio transport reserves stdout for the
    // MCP protocol itself, §3.1) — the only channel a swallowed config/vault load failure
    // (A6) has to reach a human.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let max_risk_flag = match max_risk.as_deref() {
        None => None,
        Some(s) => match s.parse::<RiskLevel>() {
            Ok(r) => Some(r),
            Err(_) => {
                eprintln!("error: unknown risk level `{s}`");
                eprintln!("  = risk is one of low, medium, high, critical");
                return ExitCode::from(2);
            }
        },
    };

    let paths = resolve_paths();
    materialize_starter(&paths);

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // Default concurrency limit of 2 (§6.1 "A per-server concurrency limit (default 2)").
    let state = kata_mcp::ServerState::new(paths, max_risk_flag, 2, &cwd);
    let server = kata_mcp::KadouMcpServer::new(state);

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("error: failed to start the async runtime: {err}");
            return ExitCode::FAILURE;
        }
    };

    match rt.block_on(server.serve_stdio()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: MCP server failed: {err}");
            ExitCode::FAILURE
        }
    }
}

/// `kata mcp schema [--bytes]` (§7.1): prints the served `tools/list` JSON, and — with
/// `--bytes` — its byte size, the same number the CI byte gate checks (§1.3).
pub fn run_mcp_schema(bytes: bool) -> ExitCode {
    let json_bytes = match kata_mcp::schema::tools_list_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", String::from_utf8_lossy(&json_bytes));
    if bytes {
        println!("bytes: {}", json_bytes.len());
    }
    ExitCode::SUCCESS
}
