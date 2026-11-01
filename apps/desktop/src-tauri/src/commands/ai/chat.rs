//! AI chat IPC thin binding.
//!
//! WR-A2：`ai_chat` 命令处理函数只做 IPC 形参提取与端口取值；B3 后编排
//! 本体（schema 管线降级、上下文块装配、取消登记对称性、工具循环）下沉
//! modules 用例 `datazen::ai::application::chat_session`。命令对
//! `R: Runtime` 泛型，测试经 `tauri::test::mock_app` 直调。
//!
//! 本文件同时是宿主端口的装配点：`AppState` 的 [`AiToolLoopHost`] 实现
//! 与请求级 [`AiRequestHost`]（claim 归因，P6 §14.4）。

use super::util::{
    borrow_runtime_session, build_connections_context, read_context_entries, resolve_ai,
    resolve_language, resolve_safety_gate, window_stream_callback,
};
use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen::ai::application::chat_session::{
    run_chat_session, ChatSessionEnv, ChatSessionInput,
};
use datazen::ai::application::schema_pipeline::SchemaContextPipeline;
use datazen::ai::ownership::AiTargetClaim;
use datazen::ai::tool_loop::AiToolLoopHost;
use datazen::ai::*;
use tauri::{State, WebviewWindow};

// ─── Database Tool Definitions & Execution ───

/// Convert connected MCP client tools into AI `ToolDefinition`s (Phase 3 wires into chat).
///
/// M3: the form-neutral assembly lives in the shared layer
/// (`datazen::ai::mcp_tools`); this is the IPC-side thin shell over the
/// `McpToolBroker` implementation on `AppState`.
pub(crate) async fn mcp_tool_definitions(state: &AppState) -> Vec<ToolDefinition> {
    datazen::ai::mcp_tools::mcp_tool_definitions(state).await
}

pub(crate) async fn execute_mcp_tool(
    state: &AppState,
    server_id: &str,
    tool_name: &str,
    arguments: &str,
) -> String {
    datazen::ai::mcp_tools::execute_mcp_tool(state, server_id, tool_name, arguments).await
}

pub(crate) async fn execute_db_tool(state: &AppState, tool_call: &ToolCall) -> String {
    let Ok(scope) = state.connection_scope() else {
        return "Error: connection scope unavailable".into();
    };
    let catalog = crate::platform::connections::DesktopConnectionStore::new(state.store.clone());
    datazen::ai::tools::execute_db_tool(state, &catalog, &scope, tool_call).await
}

#[async_trait::async_trait]
impl AiToolLoopHost for AppState {
    async fn egress_summary(&self) -> String {
        let Ok(scope) = self.connection_scope() else {
            return "No database connections configured.".into();
        };
        let catalog = crate::platform::connections::DesktopConnectionStore::new(self.store.clone());
        datazen::ai::tools::build_egress_summary(&catalog, &scope).await
    }
    async fn execute_db_tool(&self, call: &ToolCall) -> String {
        execute_db_tool(self, call).await
    }
    async fn execute_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> String {
        execute_mcp_tool(self, server, tool, arguments).await
    }
}

// ─── AI Chat ───

#[tauri::command]
pub async fn ai_chat<R: tauri::Runtime>(
    state: State<'_, AppState>,
    window: WebviewWindow<R>,
    db_session_id: Option<String>,
    database: Option<String>,
    messages: Vec<ChatMessage>,
    request_id: String,
    include_schema: bool,
    scenario: Option<String>,
    context_files: Option<Vec<String>>,
    context_tables: Option<Vec<String>>,
    target_claim: Option<AiTargetClaim>,
) -> Result<String, CommandError> {
    let on_chunk = window_stream_callback(&window);
    let is_workflow = scenario.as_deref() == Some("workflow_generate");

    // 端口取值（thin binding）。
    let (provider, ai_config) = resolve_ai(&state).await?;
    let gate = resolve_safety_gate(&state).await;
    let lang = resolve_language(&state).await;
    let connections_context = if is_workflow {
        build_connections_context(&state, &lang).await
    } else {
        String::new()
    };
    // 宿主运行时端口：借用 live 驱动供 schema 管线使用。会话未知 → None
    // （pinned 契约：静默跳过 schema 注入，`ai_chat_schema_resolve_failure_still_streams`）；
    // 借用被撤销 → 硬错误，文案钉死。
    let schema_driver = if include_schema {
        match db_session_id.as_deref() {
            Some(conn_id) => match borrow_runtime_session(&state, conn_id, "ai_chat").await {
                Ok((_session, lease)) => Some(
                    lease
                        .connection()
                        .map_err(|_| {
                            CommandError::Internal(
                                "ai_chat: runtime connection borrow revoked".into(),
                            )
                        })?
                        .driver()
                        .clone(),
                ),
                Err(_) => None,
            },
            None => None,
        }
    } else {
        None
    };
    let context_entries = read_context_entries(&state, context_files.as_deref()).await?;
    let mcp_tools = if provider.supports_tools() {
        mcp_tool_definitions(&state).await
    } else {
        Vec::new()
    };

    // 编排本体在 modules（B3 下沉）；claim 归因由请求级宿主提供。
    let host = super::AiRequestHost {
        state: &state,
        claim: target_claim.unwrap_or(AiTargetClaim::Independent),
    };
    let env = ChatSessionEnv {
        provider,
        model: ai_config.model,
        gate,
        lang,
        connections_context,
        mcp_tools,
        context_entries,
        schema_driver,
        prompt_resolver: state.prompt_resolver.clone(),
        schema_pipeline: SchemaContextPipeline::new(state.schema_context_builder.clone()),
        cancel_registry: state.cancel_registry.clone(),
        cancel_owner: super::ai_owner_fingerprint(&state),
    };
    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id,
            db_session_id,
            database,
            messages,
            include_schema,
            scenario: if is_workflow {
                datazen::ai::chat_request::ChatScenario::WorkflowGenerate
            } else {
                datazen::ai::chat_request::ChatScenario::Chat
            },
            context_tables,
        },
    )
    .await
    .map_err(Into::into)
}
