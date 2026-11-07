//! Chat / NL2SQL 会话编排用例（B3 自桌面 `commands/ai` 下沉）。
//!
//! 宿主绑定只做 IPC 形参提取与端口取值（provider/model/gate/lang、连接
//! 上下文、MCP 工具、上下文文件条目、live 驱动借用），形态中立的裁决全部
//! 在这里：schema 管线降级（resolve 失败关 DB 工具、会话未知静默跳过）、
//! pinned 表合成顺序（ctx.yaml → 请求附带 → 当前表置首）、上下文块装配、
//! 取消登记的对称注册/注销、`attach_db_tools` 分流（工具循环 vs 纯流式）。
//! 把 Tauri 宿主换成 axum，本文件原样可用——这正是测试分层判据
//! （commands-layer-review §1）的落点。
//!
//! IPC 契约零变化：命令名、参数、错误文案、事件与日志语义逐字保留
//! （桌面侧行为钉子 `ai_chat_*` / `ai_generate_sql_*` 继续钉住入口）。

use super::cancel::CancellationRegistry;
use super::chat_injection::ChatInjection;
use super::chat_request::{build_chat_request, ChatRequestInput, ChatScenario, ChatSchemaPrompt};
use super::context::format_context_block;
use super::error::AiUseCaseError;
use super::prompt_resolver::PromptResolver;
use super::schema_pipeline::{compose_schema_system_suffix, PromptSeed, SchemaContextPipeline};
use super::sql_generation::build_sql_generation_request;
use super::tool_loop::{run_streaming_tool_loop, AiToolContext, AiToolLoopHost, StreamCallback};
use super::tools::{db_tool_definitions, ToolLoopLimits};
use crate::ai::ctx_yaml::extract_ctx_yaml_tables;
use crate::ai::domain::budget;
use crate::ai::domain::safety::redact_for_gate;
use crate::ai::domain::traits::AiProvider;
use crate::ai::domain::types::{
    AiAgentMode, AiSafetyGateConfig, ChatMessage, StreamChunk, ToolDefinition,
};
use crate::ai::memory::{MemoryInjection, MemoryReference};
use datazen_driver_api::DatabaseDriver;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

/// 宿主在进入用例前解析好的环境：值 + 端口。
///
/// 每个字段都是「取值」而非「裁决」——裁决（降级、合成、分流、登记）在
/// 用例函数里，两种宿主共享同一份答案。
pub struct ChatSessionEnv {
    pub provider: Arc<dyn AiProvider>,
    /// 选中档案的模型名（`resolve_active_provider` 返回的 config）。
    pub model: String,
    pub gate: AiSafetyGateConfig,
    pub lang: String,
    /// workflow 场景的连接清单块（非 workflow 场景传空串）。
    pub connections_context: String,
    /// Resolved global instructions / template / connection notes for this
    /// turn. Host resolves storage and scope; insertion stays in this layer.
    pub injection: ChatInjection,
    /// Host-resolved, already budgeted memory block for this turn (`Auto`).
    pub memory_injection: Option<MemoryInjection>,
    /// Memories carried into the first stream chunk for citation chips.
    pub memory_used: Vec<MemoryReference>,
    /// MCP 工具面（宿主按 `provider.supports_tools()` 决定是否取空）。
    pub mcp_tools: Vec<ToolDefinition>,
    /// Skill tools, only when the host enabled skills and the provider accepts
    /// tools. Empty keeps CLI/local-agent providers and Ask sessions unchanged.
    pub skill_tools: Vec<ToolDefinition>,
    /// Opt-in plugin/workflow tools resolved by the host for this turn. Empty
    /// when no plugin/workflow is enabled or the provider cannot use tools.
    pub bridge_tools: Vec<ToolDefinition>,
    /// 上下文文件条目（宿主 FS IO 读出，未脱敏；空 = 无上下文）。
    pub context_entries: Vec<(String, String)>,
    /// chat：schema 管线用的 live 驱动。`None` = 会话未知/未请求 schema
    /// —— pinned 契约：静默跳过 schema 注入（工具仍挂载，流照常）。
    pub schema_driver: Option<Arc<dyn DatabaseDriver>>,
    pub prompt_resolver: Arc<PromptResolver>,
    pub schema_pipeline: SchemaContextPipeline,
    pub cancel_registry: CancellationRegistry,
    /// 取消登记的属主指纹（宿主身份端口产出）。
    pub cancel_owner: String,
}

/// `ai_chat` 的形态中立输入（IPC 形参已由宿主提取）。
pub struct ChatSessionInput {
    pub request_id: String,
    pub db_session_id: Option<String>,
    /// Persisted connection id: the stable identity for the chat session.
    /// `db_session_id` remains the runtime handle for schema borrowing only.
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub include_schema: bool,
    pub scenario: ChatScenario,
    pub context_tables: Option<Vec<String>>,
    pub agent_mode: AiAgentMode,
    /// Session-level skill ids whose bodies were already loaded. Hosts seed the
    /// catalog with them so the two-body budget survives across turns.
    pub loaded_skill_ids: Option<Vec<String>>,
    /// Auto-context snapshot frozen by the frontend on send.
    pub editor_sql: Option<String>,
    pub selection: Option<String>,
    pub last_error: Option<String>,
    pub result_preview: Option<ChatResultPreview>,
}

/// Structured query-result excerpt attached to a chat turn automatically.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResultPreview {
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub rows: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    pub row_count: Option<u64>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
}

/// `ai_chat` 编排本体：schema 管线裁决 + 请求组装 + 取消登记 + 工具循环。
pub async fn run_chat_session(
    env: ChatSessionEnv,
    tool_loop_host: &dyn AiToolLoopHost,
    on_chunk: StreamCallback,
    input: ChatSessionInput,
) -> Result<String, AiUseCaseError> {
    tracing::info!(
        request_id = %input.request_id,
        db_session_id = ?input.db_session_id,
        connection_id = ?input.connection_id,
        database = ?input.database,
        messages_count = input.messages.len(),
        include_schema = input.include_schema,
        scenario = ?input.scenario,
        agent_mode = ?input.agent_mode,
        context_tables_count = input.context_tables.as_ref().map(|t| t.len()).unwrap_or(0),
        last_user_msg_len = input.messages.last().map(|m| m.content.len()).unwrap_or(0),
        "ai_chat: start"
    );

    let mut schema_prompt: Option<(Arc<dyn DatabaseDriver>, PromptSeed)> = None;
    let mut attach_db_tools = true;

    if input.include_schema {
        if let Some(ref conn_id) = input.db_session_id {
            // 会话未知（宿主借不到驱动）→ 静默跳过 schema 注入；这是钉死
            // 的契约：`ai_chat_schema_resolve_failure_still_streams`。
            if let Some(driver) = &env.schema_driver {
                let db = input.database.as_deref().unwrap_or("");
                let pinned = input.context_tables.clone().unwrap_or_default();
                let supports_tools = env.provider.supports_tools();
                match env
                    .schema_pipeline
                    .resolve(
                        conn_id,
                        db,
                        &pinned,
                        supports_tools,
                        budget::PINNED_DDL,
                        budget::FALLBACK_DDL,
                    )
                    .await
                {
                    Ok(seed) => {
                        attach_db_tools = seed.attach_db_tools;
                        schema_prompt = Some((driver.clone(), seed));
                    }
                    Err(e) => {
                        tracing::warn!(
                            request_id = %input.request_id,
                            db_session_id = %conn_id,
                            database = %db,
                            error_len = e.to_string().len(),
                            "ai_chat: schema context pipeline resolve failed; disabling DB tools"
                        );
                        attach_db_tools = false;
                    }
                }
            }
        }
    }

    // 上下文块：自动快照在前，手选 @ 文件在后；逐段脱敏、整体按 gate 截断。
    let auto_context = build_auto_context_block(&input, &env.gate, &env.context_entries);
    let picked_context = chat_context_block(&env.context_entries, &env.gate);
    let context_block = merge_context_blocks(auto_context, picked_context, &env.gate);

    // Ask never sees DB or MCP tools. A schema seed that says
    // `attach_db_tools = false` still wins for Agent, and a failed schema
    // resolve still stays tool-less under the existing contract.
    let agent_tools_allowed = input.agent_mode == AiAgentMode::Agent;
    attach_db_tools = agent_tools_allowed && attach_db_tools;

    let has_bound_connection = input
        .connection_id
        .as_deref()
        .is_some_and(|id| !id.trim().is_empty());
    let mut tool_context = AiToolContext {
        allowed_connection_id: input.connection_id.clone(),
        max_sample_rows: env.gate.max_sample_rows,
        cancel_token: None,
    };

    let schema = schema_prompt
        .as_ref()
        .map(|(driver, seed)| ChatSchemaPrompt {
            driver: driver.as_ref(),
            seed,
        });
    let mut request = build_chat_request(
        env.prompt_resolver.as_ref(),
        ChatRequestInput {
            request_id: &input.request_id,
            model: &env.model,
            lang: &env.lang,
            scenario: input.scenario,
            schema,
            connections_context: &env.connections_context,
            messages: input.messages,
            context_block: context_block.as_deref(),
            injection: env.injection,
            memory: env.memory_injection,
            safety_gate: &env.gate,
            attach_db_tools,
            attach_agent_db_tools: agent_tools_allowed,
            attach_agent_query_tools: agent_tools_allowed && has_bound_connection,
            mcp_tools: if agent_tools_allowed {
                env.mcp_tools
            } else {
                Vec::new()
            },
        },
    )
    .await;

    if agent_tools_allowed && !env.skill_tools.is_empty() {
        let tools = request.tools.get_or_insert_with(Vec::new);
        for tool in env.skill_tools {
            if !tools.iter().any(|existing| existing.name == tool.name) {
                tools.push(tool);
            }
        }
    }
    if agent_tools_allowed && !env.bridge_tools.is_empty() {
        let tools = request.tools.get_or_insert_with(Vec::new);
        for tool in env.bridge_tools {
            if !tools.iter().any(|existing| existing.name == tool.name) {
                tools.push(tool);
            }
        }
    }

    // Register cancel token so `ai_cancel` IPC can interrupt this stream.
    let cancel_token = env
        .cancel_registry
        .register(&input.request_id, &env.cancel_owner)
        .await;
    request.cancel_token = Some(cancel_token.clone());
    tool_context.cancel_token = Some(cancel_token);

    emit_memory_used(&on_chunk, &input.request_id, &env.memory_used);
    let result = run_streaming_tool_loop(
        env.provider.clone(),
        tool_loop_host,
        &tool_context,
        on_chunk,
        &input.request_id,
        request,
        ToolLoopLimits::from_gate(&env.gate),
        "ai_chat",
        &env.gate,
    )
    .await;

    // Always unregister — normal completion or cancellation.
    env.cancel_registry
        .unregister(&input.request_id, &env.cancel_owner)
        .await;
    result
}

fn emit_memory_used(on_chunk: &StreamCallback, request_id: &str, memory_used: &[MemoryReference]) {
    if memory_used.is_empty() {
        return;
    }
    on_chunk(
        request_id,
        Ok(StreamChunk {
            content: String::new(),
            reasoning: None,
            done: false,
            cancelled: false,
            usage: None,
            tool_calls: None,
            response_id: None,
            egress_summary: None,
            memory_used: Some(memory_used.to_vec()),
        }),
    );
}

const AUTO_PREVIEW_MAX_ROWS: usize = 5;
const AUTO_PREVIEW_MAX_COLUMNS: usize = 20;
const AUTO_PREVIEW_MAX_CELL_CHARS: usize = 200;

/// Build the automatic context block from the frozen editor/error/result
/// snapshot. Empty fields contribute no heading or blank block. The hand-picked
/// `@` entries and the last user message are used to avoid repeating the same
/// SQL/selection/error in two places.
fn build_auto_context_block(
    input: &ChatSessionInput,
    gate: &AiSafetyGateConfig,
    picked_entries: &[(String, String)],
) -> Option<String> {
    let last_user = input
        .messages
        .iter()
        .rev()
        .find(|message| message.role == crate::ai::domain::types::MessageRole::User)
        .map(|message| message.content.as_str())
        .unwrap_or("");
    let already_present = |text: &str| {
        !text.is_empty()
            && (last_user.contains(text)
                || picked_entries
                    .iter()
                    .any(|(_, content)| content.contains(text)))
    };

    let mut sections = Vec::new();
    if let Some(sql) = input.editor_sql.as_deref().map(str::trim) {
        if !sql.is_empty() && !already_present(sql) {
            sections.push(format!("[Current SQL]\n{sql}"));
        }
    }
    if let Some(selection) = input.selection.as_deref().map(str::trim) {
        if !selection.is_empty()
            && input.editor_sql.as_deref().map(str::trim) != Some(selection)
            && !already_present(selection)
        {
            sections.push(format!("[Editor selection]\n{selection}"));
        }
    }
    if let Some(error) = input.last_error.as_deref().map(str::trim) {
        if !error.is_empty() && !already_present(error) {
            sections.push(format!("[Recent query error]\n{error}"));
        }
    }
    if let Some(preview) = input.result_preview.as_ref() {
        if let Some(rendered) = render_result_preview(preview) {
            if !already_present(&rendered) {
                sections.push(rendered);
            }
        }
    }

    if sections.is_empty() {
        return None;
    }
    let block = redact_for_gate(&sections.join("\n\n"), gate);
    let block = truncate_bytes(&block, gate.max_context_bytes.max(1));
    if block.trim().is_empty() {
        None
    } else {
        Some(block)
    }
}

fn render_result_preview(preview: &ChatResultPreview) -> Option<String> {
    let columns: Vec<&str> = preview
        .columns
        .iter()
        .take(AUTO_PREVIEW_MAX_COLUMNS)
        .map(String::as_str)
        .collect();
    let rows: Vec<&Vec<serde_json::Value>> =
        preview.rows.iter().take(AUTO_PREVIEW_MAX_ROWS).collect();
    if columns.is_empty() && rows.is_empty() {
        return None;
    }

    let total = preview.row_count.unwrap_or(preview.rows.len() as u64);
    let elapsed = preview
        .elapsed_ms
        .map(|ms| format!(", {ms}ms"))
        .unwrap_or_default();
    let mut lines = vec![format!(
        "[Recent result preview: showing {} of {} rows{}]",
        rows.len(),
        total,
        elapsed
    )];
    if !columns.is_empty() {
        lines.push(columns.join(" | "));
    }
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .take(AUTO_PREVIEW_MAX_COLUMNS)
            .map(format_preview_cell)
            .collect();
        lines.push(cells.join(" | "));
    }
    if preview.rows.len() > AUTO_PREVIEW_MAX_ROWS {
        lines.push(format!(
            "… {} more preview rows omitted",
            preview.rows.len() - AUTO_PREVIEW_MAX_ROWS
        ));
    }
    Some(lines.join("\n"))
}

fn format_preview_cell(value: &serde_json::Value) -> String {
    let raw = match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let single_line = raw.replace(['\n', '\r'], " ").replace('|', "\\|");
    truncate_chars(&single_line, AUTO_PREVIEW_MAX_CELL_CHARS)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

fn truncate_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn merge_context_blocks(
    auto: Option<String>,
    picked: Option<String>,
    gate: &AiSafetyGateConfig,
) -> Option<String> {
    let combined = match (auto, picked) {
        (Some(auto), Some(picked)) => format!("{auto}\n\n{picked}"),
        (Some(auto), None) => auto,
        (None, Some(picked)) => picked,
        (None, None) => return None,
    };
    let truncated = truncate_bytes(&combined, gate.max_context_bytes.max(1));
    if truncated.trim().is_empty() {
        None
    } else {
        Some(truncated)
    }
}

/// chat 的上下文块政策：逐文件内容脱敏，再走共享拼装格式。
fn chat_context_block(entries: &[(String, String)], gate: &AiSafetyGateConfig) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    let sanitized: Vec<(String, String)> = entries
        .iter()
        .map(|(path, content)| (path.clone(), redact_for_gate(content, gate)))
        .collect();
    Some(format_context_block(&sanitized))
}

/// `ai_generate_sql` 的形态中立输入。
///
/// `context_entries` 由宿主读出（路径政策与 FS IO 留宿主），条目内容未
/// 脱敏——yaml 表提取在前、整段 NL 脱敏在后，顺序即行为。
pub struct SqlGenerationInput {
    pub request_id: String,
    pub db_session_id: String,
    pub database: String,
    pub natural_language: String,
    pub current_table: Option<String>,
    pub recent_queries: Vec<String>,
    pub context_tables: Vec<String>,
}

/// `ai_generate_sql` 编排本体。
///
/// `schema_driver` 必须在（命令入口对未知会话直接报错，宿主完成借用与
/// 错误映射后才进入用例）。
#[allow(clippy::too_many_arguments)]
pub async fn run_sql_generation_session(
    env: ChatSessionEnv,
    schema_driver: Arc<dyn DatabaseDriver>,
    tool_loop_host: &dyn AiToolLoopHost,
    on_chunk: StreamCallback,
    input: SqlGenerationInput,
) -> Result<String, AiUseCaseError> {
    tracing::info!(
        request_id = %input.request_id,
        db_session_id = %input.db_session_id,
        database = %input.database,
        input_len = input.natural_language.len(),
        current_table = ?input.current_table,
        recent_queries_count = input.recent_queries.len(),
        "ai_generate_sql: start"
    );

    // ctx.yaml 条目先拆出 pinned 表，其余内容作为前缀块拼进 NL。
    let mut natural_language = input.natural_language;
    let mut pinned = input.context_tables;
    if !env.context_entries.is_empty() {
        let (yaml_tables, remaining) = extract_ctx_yaml_tables(&env.context_entries);
        for table in yaml_tables {
            if !pinned.iter().any(|p| p == &table) {
                pinned.push(table);
            }
        }
        if !remaining.is_empty() {
            let context_block = format_context_block(&remaining);
            natural_language = format!("{context_block}\n\n{natural_language}");
        }
    }

    // Context files and natural language are caller-provided prompt content;
    // sanitize the complete assembled value at the command boundary.
    let natural_language = redact_for_gate(&natural_language, &env.gate);

    if let Some(ref t) = input.current_table {
        if !pinned.iter().any(|p| p == t) {
            pinned.insert(0, t.clone());
        }
    }

    let supports_tools = env.provider.supports_tools();
    let seed = env
        .schema_pipeline
        .resolve(
            &input.db_session_id,
            &input.database,
            &pinned,
            supports_tools,
            budget::PINNED_DDL,
            budget::FALLBACK_DDL,
        )
        .await
        .map_err(|e| {
            tracing::error!(
                cmd = "ai_generate_sql",
                error = %datazen_infra::redaction::redact_secrets_for_log(&e)
            );
            AiUseCaseError::Internal(e)
        })?;

    tracing::debug!(
        schema_suffix_len = compose_schema_system_suffix(&seed).len(),
        attach_db_tools = seed.attach_db_tools,
        pinned_count = pinned.len(),
        "ai_generate_sql: schema context built"
    );

    let mut request = build_sql_generation_request(
        &input.request_id,
        &env.model,
        env.prompt_resolver.as_ref(),
        schema_driver.as_ref(),
        &env.lang,
        &seed,
        natural_language,
        &input.recent_queries,
        env.memory_injection
            .as_ref()
            .filter(|memory| !memory.is_empty())
            .and_then(|memory| memory.text.as_deref()),
        &env.gate,
    )
    .await;

    // Register cancel token so `ai_cancel` IPC can interrupt this stream.
    let cancel_token = env
        .cancel_registry
        .register(&input.request_id, &env.cancel_owner)
        .await;
    request.cancel_token = Some(cancel_token);
    emit_memory_used(&on_chunk, &input.request_id, &env.memory_used);

    tracing::debug!(
        request_id = %input.request_id,
        model = %request.model,
        messages_count = request.messages.len(),
        system_prompt_len = request.messages.first().map(|m| m.content.len()).unwrap_or(0),
        attach_db_tools = seed.attach_db_tools,
        "ai_generate_sql: sending to provider (stream)"
    );

    let tool_context = AiToolContext::default();
    let result = if seed.attach_db_tools {
        request.tools = Some(db_tool_definitions());
        run_streaming_tool_loop(
            env.provider.clone(),
            tool_loop_host,
            &tool_context,
            on_chunk,
            &input.request_id,
            request,
            ToolLoopLimits::default(),
            "ai_generate_sql",
            &env.gate,
        )
        .await
    } else {
        let (tx, mut rx) = mpsc::channel::<
            Result<crate::ai::domain::types::StreamChunk, crate::ai::domain::types::AiError>,
        >(32);
        let req_id_clone = input.request_id.clone();
        let on_chunk_bg = on_chunk.clone();

        tokio::spawn(async move {
            while let Some(chunk_result) = rx.recv().await {
                on_chunk_bg(&req_id_clone, chunk_result);
            }
        });

        env.provider
            .stream_complete(&request, tx)
            .await
            .map_err(|e| {
                tracing::error!(
                    cmd = "ai_generate_sql",
                    error = %datazen_infra::redaction::redact_secrets_for_log(&e.to_string())
                );
                AiUseCaseError::Ai(e)
            })?;

        Ok(input.request_id.clone())
    };

    // Always unregister — normal completion or cancellation.
    env.cancel_registry
        .unregister(&input.request_id, &env.cancel_owner)
        .await;
    result
}

#[cfg(test)]
#[path = "chat_session_tests.rs"]
mod tests;
