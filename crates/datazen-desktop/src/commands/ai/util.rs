//! Shared helpers for AI command handlers.
//!
//! B3 后这里只剩宿主绑定：tauri 事件投递、配置端口取值、运行时会话借用
//! 与上下文文件的宿主 FS IO（路径政策 + 读取）。被测逻辑已全部归位
//! modules（响应解析钉子在 `ai/application/response_tests.rs`）。

use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen::ai::AiProviderConfig;
use datazen::ai::*;
use datazen::execution::application::{LiveConnectionLease, LivePurpose};
use datazen::execution::SessionView;
use std::sync::Arc;
use tauri::{Emitter, WebviewWindow};

pub(crate) use datazen::ai::response::language_hint;

/// Delivers streaming chunks to the UI (or test collector).
pub(crate) type StreamCallback = Arc<dyn Fn(&str, Result<StreamChunk, AiError>) + Send + Sync>;

pub(crate) fn window_stream_callback<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
) -> StreamCallback {
    let window = window.clone();
    Arc::new(move |request_id, result| {
        emit_stream_chunk_or_error(&window, request_id, result);
    })
}

/// Resolve the active model profile's safety gate configuration.
///
/// Falls back to a relaxed gate (credentials only) when no profile is found,
/// matching the previous `ai_strict_egress = false` default behavior.
///
/// 编排体（设置档案 + 执行设置合并）在 `datazen::ai::api::effective_safety_gate`；
/// 这里只递配置端口。
pub(crate) async fn resolve_safety_gate(state: &AppState) -> AiSafetyGateConfig {
    datazen::ai::api::effective_safety_gate(state.store.as_ref()).await
}

pub(crate) async fn resolve_ai(
    state: &AppState,
) -> Result<(Arc<dyn AiProvider>, AiProviderConfig), CommandError> {
    state.ensure_ai_ready().await;
    // 未配置 / 供应商不可用的码文本是前端契约，由用例层定稿并钉测试。
    datazen::ai::api::resolve_active_provider(&state.ai_registry, state.store.as_ref())
        .await
        .map_err(Into::into)
}

/// 界面语言经配置端口读取（host-boundary §3：AI 命令不直读宿主 store）。
///
/// chat / generate 族的提示词解析与响应处理都以语言为请求输入；这里统一
/// 从 `datazen::ai::api` 取，替代旧的 `state.store.get_settings().language`。
pub(crate) async fn resolve_language(state: &AppState) -> String {
    datazen::ai::api::settings_language(state.store.as_ref()).await
}

pub(crate) async fn build_connections_context(state: &AppState, lang: &str) -> String {
    let Ok(scope) = state.connection_scope() else {
        return String::new();
    };
    let catalog = crate::platform::connections::DesktopConnectionStore::new(state.store.clone());
    datazen::ai::response::build_connections_context(&catalog, &scope, lang).await
}

/// Load one connection's user-authored AI note for prompt injection. Scope is
/// organization + current desktop principal, exactly like the notes IPC.
pub(crate) fn load_connection_note(state: &AppState, connection_id: &str) -> Option<String> {
    let organization_id = state
        .organization_scope()
        .ok()?
        .organization_id()
        .as_str()
        .to_string();
    let principal_id = crate::platform::local_principal()
        .map(|principal| principal.as_str().to_string())
        .unwrap_or_else(|| "datazen-local-user".to_string());
    match state.store.app_db().get_ai_connection_note(
        &organization_id,
        &principal_id,
        connection_id,
    ) {
        Ok(note) => note.map(|note| note.content),
        Err(error) => {
            tracing::warn!(connection_id, error = %error, "failed to load AI connection note");
            None
        }
    }
}

/// Resolve an AI command's runtime session and borrow its already-open driver
/// connection. The IPC contract remains a scalar `dbSessionId`; the Runtime
/// enforces ownership, epoch validation, and physical-resource lifetime.
pub(crate) async fn borrow_runtime_session(
    state: &AppState,
    db_session_id: &str,
    command: &str,
) -> Result<(SessionView, LiveConnectionLease), CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?
        .borrow_live_connection_by_id(call.context(), db_session_id, LivePurpose::Inspection)
        .await
        .map_err(crate::platform::into_command_error)
        .map_err(|error| match error {
            CommandError::NotFound(message) => {
                CommandError::NotFound(format!("{command}: {message}"))
            }
            other => other,
        })
}

/// Resolve only the runtime session view when an AI operation needs owner or
/// namespace metadata but does not execute a database command.
pub(crate) async fn runtime_session(
    state: &AppState,
    db_session_id: &str,
    command: &str,
) -> Result<SessionView, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?
        .get_session_by_id(call.context(), db_session_id)
        .await
        .map_err(crate::platform::into_command_error)
        .map_err(|error| match error {
            CommandError::NotFound(message) => {
                CommandError::NotFound(format!("{command}: {message}"))
            }
            other => other,
        })
}

/// Host-side context-file IO for chat / NL2SQL: `None`/empty yields no
/// entries; otherwise resolve the context dir (host settings) and read the
/// requested paths (path policy lives in `commands::context`). Contents stay
/// unredacted — the modules use cases own the redaction policy per entry
/// point (chat: per-file; NL2SQL: whole assembled NL).
pub(crate) async fn read_context_entries(
    state: &AppState,
    files: Option<&[String]>,
) -> Result<Vec<(String, String)>, CommandError> {
    let Some(files) = files else {
        return Ok(Vec::new());
    };
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let ctx_dir = crate::commands::context::resolve_context_dir_from_state(state).await?;
    crate::commands::context::read_context_paths(&ctx_dir, files).await
}

pub(crate) fn emit_stream_chunk_or_error<R: tauri::Runtime>(
    emitter: &impl Emitter<R>,
    request_id: &str,
    result: Result<StreamChunk, AiError>,
) {
    match result {
        Ok(chunk) => {
            let mut payload = serde_json::json!({
                "requestId": request_id,
                "content": chunk.content,
                "done": chunk.done,
                "usage": chunk.usage,
            });
            if let Some(reasoning) = &chunk.reasoning {
                payload["reasoning"] = serde_json::Value::String(reasoning.clone());
            }
            if let Some(tool_calls) = &chunk.tool_calls {
                payload["toolCalls"] = serde_json::to_value(tool_calls).unwrap_or_default();
            }
            let _ = emitter.emit("ai:stream-chunk", payload);
        }
        Err(e) => {
            let _ = emitter.emit(
                "ai:stream-error",
                serde_json::json!({
                    "requestId": request_id,
                    "error": e.to_string(),
                }),
            );
        }
    }
}
