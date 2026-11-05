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
    borrow_runtime_session, build_connections_context, load_connection_note, read_context_entries,
    resolve_ai, resolve_language, resolve_memory_settings, resolve_safety_gate,
    window_stream_callback,
};
use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen::ai::application::chat_injection::resolve_chat_injection;
use datazen::ai::application::chat_session::{run_chat_session, ChatSessionEnv, ChatSessionInput};
use datazen::ai::application::schema_pipeline::SchemaContextPipeline;
use datazen::ai::memory::{MemoryMode, MemoryScopeKind};
use datazen::ai::ownership::AiTargetClaim;
use datazen::ai::tool_loop::{AiToolContext, AiToolLoopHost};
use datazen::ai::*;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tauri::{Emitter, Runtime, State, WebviewWindow};

/// Hard upper bound for one native tool-approval dialog.
pub(crate) const TOOL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) type ConfirmToolFuture =
    Pin<Box<dyn Future<Output = Result<bool, CommandError>> + Send>>;

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

/// Build the exact prompt shown by the host confirmation dialog. Arguments
/// are credential-redacted and byte-bounded before they can reach a window.
pub(crate) fn tool_confirmation_prompt(tool: &str, arguments: &str) -> (String, String) {
    let gate = AiSafetyGateConfig {
        max_context_bytes: 600,
        ..Default::default()
    };
    let safe_args = datazen::ai::safety::redact_for_gate(arguments, &gate);
    let safe_args = if safe_args.trim().is_empty() {
        "(none)".to_string()
    } else {
        safe_args
    };
    let safe_tool = truncate_chars(&tool.replace(['\n', '\r'], " "), 120);
    let title = format!("Allow AI tool '{safe_tool}'?");
    let message = format!(
        "The AI assistant wants to run this tool once.\n\nTool: {safe_tool}\nArguments:\n{safe_args}\n\nClick OK to allow this call, or Cancel to deny it."
    );
    (title, message)
}

/// Confirm one tool call through an injectable dialog port. The production
/// callers pass `dialog::confirm_message`; tests inject accept/reject/pending
/// answers without opening native UI. Missing handle, dialog error and timeout
/// all fail closed.
pub(crate) async fn confirm_tool_call_with<R: Runtime, F>(
    app: Option<tauri::AppHandle<R>>,
    tool: &str,
    arguments: &str,
    timeout: Duration,
    confirm: F,
) -> bool
where
    F: FnOnce(tauri::AppHandle<R>, String, String) -> ConfirmToolFuture,
{
    let Some(app) = app else {
        tracing::warn!(tool, "AI tool confirmation requested without an app handle");
        return false;
    };
    let (title, message) = tool_confirmation_prompt(tool, arguments);
    match tokio::time::timeout(timeout, async move { confirm(app, title, message).await }).await {
        Ok(Ok(accepted)) => accepted,
        Ok(Err(error)) => {
            tracing::warn!(tool, error = %error, "AI tool confirmation dialog failed");
            false
        }
        Err(_) => {
            tracing::warn!(tool, ?timeout, "AI tool confirmation timed out");
            false
        }
    }
}

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

pub(crate) async fn execute_db_tool(
    state: &AppState,
    ctx: &AiToolContext,
    tool_call: &ToolCall,
) -> String {
    let Ok(scope) = state.connection_scope() else {
        return "Error: connection scope unavailable".into();
    };
    let catalog = crate::platform::connections::DesktopConnectionStore::new(state.store.clone());
    datazen::ai::tools::execute_db_tool(state, ctx, &catalog, &scope, tool_call).await
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
    async fn execute_db_tool(&self, ctx: &AiToolContext, call: &ToolCall) -> String {
        execute_db_tool(self, ctx, call).await
    }
    async fn execute_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> String {
        execute_mcp_tool(self, server, tool, arguments).await
    }
    async fn confirm_tool_call(&self, tool: &str, arguments: &str) -> bool {
        confirm_tool_call_with(
            self.app_handle.get().cloned(),
            tool,
            arguments,
            TOOL_CONFIRM_TIMEOUT,
            |app, title, message| {
                Box::pin(async move {
                    crate::commands::dialog::confirm_message(&app, &title, &message).await
                })
            },
        )
        .await
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
    connection_id: Option<String>,
    agent_mode: Option<AiAgentMode>,
    editor_sql: Option<String>,
    selection: Option<String>,
    last_error: Option<String>,
    result_preview: Option<datazen::ai::chat_session::ChatResultPreview>,
    template_id: Option<String>,
    database_type: Option<String>,
    memory_mode: Option<MemoryMode>,
    memory_scope: Option<MemoryScopeKind>,
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

    // Resolve user-authored prompt material on the host (storage + scope),
    // then hand the immutable decision to the shared use case. The UI receives
    // only identity/existence, never the note or template body.
    let memory_settings = resolve_memory_settings(&state).await;
    let memory_mode = if memory_settings.enabled {
        memory_mode.unwrap_or_default()
    } else {
        MemoryMode::Off
    };
    let last_user_text = messages
        .iter()
        .rev()
        .find(|message| message.role == datazen::ai::domain::types::MessageRole::User)
        .map(|message| message.content.clone())
        .unwrap_or_default();
    let memory = match super::resolve_chat_memory(
        &state,
        memory_mode,
        memory_scope,
        connection_id.as_deref(),
        database.as_deref(),
        &last_user_text,
        super::provider_is_cloud(provider.as_ref(), &ai_config),
        gate.data_egress_level,
        memory_settings.cloud_policy,
    )
    .await
    {
        Ok(memory) => memory,
        Err(error) => {
            tracing::warn!(error = %error, "ai_chat: memory retrieval failed; continuing without memory");
            super::ChatMemory::default()
        }
    };
    if memory_mode == MemoryMode::Ask && !memory.candidates.is_empty() {
        super::emit_memory_candidates(&window, &request_id, &memory.candidates).await;
    }

    let settings = datazen::ai::api::get_settings_config(state.store.as_ref()).await;
    let connection_notes = connection_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .and_then(|id| load_connection_note(&state, id));
    let injection = resolve_chat_injection(
        &settings,
        template_id.as_deref(),
        database_type.as_deref(),
        connection_notes,
    );
    let snapshot = injection.snapshot();
    let _ = window.emit(
        "ai:chat-injection",
        serde_json::json!({ "requestId": request_id, "snapshot": snapshot }),
    );

    // 编排本体在 modules（B3 下沉）；claim 归因由请求级宿主提供。
    let host = super::AiRequestHost {
        state: &state,
        claim: target_claim.unwrap_or(AiTargetClaim::Independent),
    };
    let env = ChatSessionEnv {
        provider,
        model: ai_config.model,
        gate,
        injection,
        memory_injection: memory.injection,
        memory_used: memory.used,
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
            connection_id,
            database,
            messages,
            include_schema,
            scenario: if is_workflow {
                datazen::ai::chat_request::ChatScenario::WorkflowGenerate
            } else {
                datazen::ai::chat_request::ChatScenario::Chat
            },
            context_tables,
            // Workflow generation keeps its historical tool access unless the
            // caller explicitly picks a mode; plain chat defaults to Ask.
            agent_mode: agent_mode.unwrap_or(if is_workflow {
                AiAgentMode::Agent
            } else {
                AiAgentMode::Ask
            }),
            editor_sql,
            selection,
            last_error,
            result_preview,
        },
    )
    .await
    .map_err(Into::into)
}
