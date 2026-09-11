//! The [`rmcp::ServerHandler`] implementation: wires [`crate::tools`]'s domain logic to the
//! MCP wire protocol (`docs/design/05-prd.md` §5).
//!
//! Only `list_tools`, `call_tool`, and `get_info` are overridden — every other
//! `ServerHandler` method (resources, prompts, sampling, tasks, …) keeps its default
//! implementation, which is what makes the server advertise the `tools` capability alone
//! (§5.1 "The server advertises only the `tools` capability").

use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData as McpError,
    Implementation, InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServiceExt};

use crate::state::ServerState;
use crate::{schema, tools};

pub struct KadouMcpServer {
    state: Arc<ServerState>,
}

impl KadouMcpServer {
    pub fn new(state: ServerState) -> Self {
        Self {
            state: Arc::new(state),
        }
    }

    /// Serves this handler over stdio and blocks until the peer disconnects (`kadou mcp
    /// serve`'s implementation, §5.1 "stdio default").
    pub async fn serve_stdio(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let running = self.serve(rmcp::transport::io::stdio()).await?;
        running.waiting().await?;
        Ok(())
    }
}

impl ServerHandler for KadouMcpServer {
    /// No `resources`/`prompts` capability, no `instructions` field (§5.1) — bytes spent
    /// there count against the connect budget the same as `tools/list`.
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("kadou", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(schema::build_tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let config = self.state.load_config();
        let arguments = request.arguments.unwrap_or_default();
        let mcp_client = context.client_info().map(|info| info.name);

        let (value, is_error) = match request.name.as_ref() {
            "list_kata" => {
                let args = parse_list_args(&arguments);
                (tools::list_kata(&self.state, &config, args), false)
            }
            "describe_kata" => {
                let Some(args) = parse_describe_args(&arguments) else {
                    return Err(McpError::invalid_params("`id` is required", None));
                };
                let vault = self.state.load_vault();
                tools::describe_kata(&self.state, &config, &vault, args)
            }
            "run_kata" => {
                let Some(args) = parse_run_args(&arguments) else {
                    return Err(McpError::invalid_params("`id` is required", None));
                };
                tools::run_kata(&self.state, &config, mcp_client, context.ct.clone(), args).await
            }
            "propose_kata" => {
                let Some(args) = parse_propose_args(&arguments) else {
                    return Err(McpError::invalid_params(
                        "`id` and `source` are required",
                        None,
                    ));
                };
                tools::propose_kata(&self.state, args)
            }
            _other => {
                return Err(McpError::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >());
            }
        };

        let text = serde_json::to_string(&value).expect("domain result values always serialize");
        let mut result = if is_error {
            CallToolResult::error(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::success(vec![ContentBlock::text(text)])
        };
        result.result_type = None;
        Ok(CallToolResponse::Complete(result))
    }
}

fn parse_list_args(arguments: &serde_json::Map<String, serde_json::Value>) -> tools::ListArgs {
    tools::ListArgs {
        query: arguments
            .get("query")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        folder: arguments
            .get("folder")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        risk: arguments
            .get("risk")
            .and_then(|v| v.as_str())
            .and_then(parse_risk),
        limit: arguments
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(50),
        offset: arguments
            .get("offset")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(0),
        include_drafts: arguments
            .get("include_drafts")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_describe_args(
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> Option<tools::DescribeArgs> {
    Some(tools::DescribeArgs {
        id: arguments.get("id")?.as_str()?.to_string(),
        include_source: arguments
            .get("include_source")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
    })
}

fn parse_run_args(
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> Option<tools::RunArgs> {
    Some(tools::RunArgs {
        id: arguments.get("id")?.as_str()?.to_string(),
        args: arguments
            .get("args")
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default(),
        dry_run: arguments
            .get("dry_run")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_propose_args(
    arguments: &serde_json::Map<String, serde_json::Value>,
) -> Option<tools::ProposeArgs> {
    Some(tools::ProposeArgs {
        id: arguments.get("id")?.as_str()?.to_string(),
        source: arguments.get("source")?.as_str()?.to_string(),
    })
}

fn parse_risk(s: &str) -> Option<kadou_core::RiskLevel> {
    match s {
        "low" => Some(kadou_core::RiskLevel::Low),
        "medium" => Some(kadou_core::RiskLevel::Medium),
        "high" => Some(kadou_core::RiskLevel::High),
        "critical" => Some(kadou_core::RiskLevel::Critical),
        _ => None,
    }
}
