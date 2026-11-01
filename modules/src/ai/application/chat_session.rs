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
use super::chat_request::{build_chat_request, ChatRequestInput, ChatScenario, ChatSchemaPrompt};
use super::context::format_context_block;
use super::error::AiUseCaseError;
use super::prompt_resolver::PromptResolver;
use super::schema_pipeline::{compose_schema_system_suffix, PromptSeed, SchemaContextPipeline};
use super::sql_generation::build_sql_generation_request;
use super::tool_loop::{run_streaming_tool_loop, AiToolLoopHost, StreamCallback};
use super::tools::db_tool_definitions;
use crate::ai::ctx_yaml::extract_ctx_yaml_tables;
use crate::ai::domain::budget;
use crate::ai::domain::safety::redact_for_gate;
use crate::ai::domain::traits::AiProvider;
use crate::ai::domain::types::{AiSafetyGateConfig, ChatMessage, ToolDefinition};
use datazen_driver_api::DatabaseDriver;
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
    /// MCP 工具面（宿主按 `provider.supports_tools()` 决定是否取空）。
    pub mcp_tools: Vec<ToolDefinition>,
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
    pub database: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub include_schema: bool,
    pub scenario: ChatScenario,
    pub context_tables: Option<Vec<String>>,
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
        database = ?input.database,
        messages_count = input.messages.len(),
        include_schema = input.include_schema,
        scenario = ?input.scenario,
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

    // 上下文块（chat 政策：逐文件脱敏后拼装）。
    let context_block = chat_context_block(&env.context_entries, &env.gate);

    let schema = schema_prompt.as_ref().map(
        |(driver, seed)| ChatSchemaPrompt {
            driver: driver.as_ref(),
            seed,
        },
    );
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
            safety_gate: &env.gate,
            attach_db_tools,
            mcp_tools: env.mcp_tools,
        },
    )
    .await;

    // Register cancel token so `ai_cancel` IPC can interrupt this stream.
    let cancel_token = env
        .cancel_registry
        .register(&input.request_id, &env.cancel_owner)
        .await;
    request.cancel_token = Some(cancel_token);

    let result = run_streaming_tool_loop(
        env.provider.clone(),
        tool_loop_host,
        on_chunk,
        &input.request_id,
        request,
        10,
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
        &env.gate,
    )
    .await;

    // Register cancel token so `ai_cancel` IPC can interrupt this stream.
    let cancel_token = env
        .cancel_registry
        .register(&input.request_id, &env.cancel_owner)
        .await;
    request.cancel_token = Some(cancel_token);

    tracing::debug!(
        request_id = %input.request_id,
        model = %request.model,
        messages_count = request.messages.len(),
        system_prompt_len = request.messages.first().map(|m| m.content.len()).unwrap_or(0),
        attach_db_tools = seed.attach_db_tools,
        "ai_generate_sql: sending to provider (stream)"
    );

    let result = if seed.attach_db_tools {
        request.tools = Some(db_tool_definitions());
        run_streaming_tool_loop(
            env.provider.clone(),
            tool_loop_host,
            on_chunk,
            &input.request_id,
            request,
            10,
            "ai_generate_sql",
            &env.gate,
        )
        .await
    } else {
        let (tx, mut rx) = mpsc::channel::<Result<crate::ai::domain::types::StreamChunk, crate::ai::domain::types::AiError>>(32);
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
