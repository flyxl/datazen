//! Chat / NL2SQL 会话编排用例的行为钉子（B3 归位）。
//!
//! 被测裁决：schema 管线降级（resolve 失败关 DB 工具）、会话未知静默
//! 跳过、上下文块脱敏、pinned 表合成顺序（ctx.yaml → 请求附带 → 当前表
//! 置首）、`attach_db_tools` 分流（工具循环 vs 纯流式）与取消登记的对称
//! 注销。全部经脚本化 fake provider / 端口 fake 驱动，无 HTTP。

use super::*;
use crate::ai::application::cancel::CancelReceipt;
use crate::schema::application::LogicalSchemaContextSource;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{TableInfo, TableType};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// ── fakes ──

type StreamChunkT = crate::ai::domain::types::StreamChunk;
type CompletionRequestT = crate::ai::domain::types::CompletionRequest;
type AiErrorT = crate::ai::domain::types::AiError;
type MessageRoleT = crate::ai::domain::types::MessageRole;

/// 脚本化 provider：记录收到的请求并吐出预置流式块。
#[derive(Default)]
struct ScriptedProvider {
    with_tools: bool,
    streams: Mutex<Vec<Vec<StreamChunkT>>>,
    requests: Mutex<Vec<CompletionRequestT>>,
    stream_calls: AtomicUsize,
}

impl ScriptedProvider {
    fn with_tools(mut self) -> Self {
        self.with_tools = true;
        self
    }

    fn push_stream(&self, chunks: Vec<StreamChunkT>) {
        self.streams.lock().unwrap().push(chunks);
    }

    fn last_request(&self) -> Option<CompletionRequestT> {
        self.requests.lock().unwrap().last().cloned()
    }
}

#[async_trait::async_trait]
impl AiProvider for ScriptedProvider {
    fn provider_type(&self) -> crate::ai::domain::types::AiProviderType {
        crate::ai::domain::types::AiProviderType::Custom
    }
    fn display_name(&self) -> &str {
        "scripted"
    }
    fn supports_tools(&self) -> bool {
        self.with_tools
    }
    async fn validate_config(
        &self,
        _config: &crate::ai::domain::types::AiProviderConfig,
    ) -> Result<(), AiErrorT> {
        Ok(())
    }
    async fn initialize(
        &self,
        _config: &crate::ai::domain::types::AiProviderConfig,
    ) -> Result<(), AiErrorT> {
        Ok(())
    }
    async fn complete(
        &self,
        _request: &CompletionRequestT,
    ) -> Result<crate::ai::domain::types::CompletionResponse, AiErrorT> {
        Err(AiErrorT::NotConfigured(
            "complete is not used by scripted tests".into(),
        ))
    }
    async fn stream_complete(
        &self,
        request: &CompletionRequestT,
        sender: tokio::sync::mpsc::Sender<Result<StreamChunkT, AiErrorT>>,
    ) -> Result<(), AiErrorT> {
        self.stream_calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request.clone());
        let chunks = self
            .streams
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| vec![stream_chunk("ok", true)]);
        for chunk in chunks {
            let _ = sender.send(Ok(chunk)).await;
        }
        Ok(())
    }
}

/// 工具循环宿主 fake：只数调用。
#[derive(Default)]
struct RecordingLoopHost {
    db_calls: AtomicUsize,
}

#[async_trait::async_trait]
impl AiToolLoopHost for RecordingLoopHost {
    async fn egress_summary(&self) -> String {
        String::new()
    }
    async fn execute_db_tool(
        &self,
        _ctx: &AiToolContext,
        _call: &crate::ai::domain::types::ToolCall,
    ) -> String {
        self.db_calls.fetch_add(1, Ordering::SeqCst);
        "{}".into()
    }
    async fn execute_mcp_tool(&self, _server: &str, _tool: &str, _args: &str) -> String {
        "{}".into()
    }
}

/// 可编程的 schema 上下文源：逐表记录 table_schema 调用顺序（即管线收
/// 到的 pinned 顺序——端口本身不接收整表列表）。
struct RecordingSource {
    driver: Arc<dyn DatabaseDriver>,
    tables: Vec<TableInfo>,
    tables_error: Mutex<Option<String>>,
    schema_calls: Mutex<Vec<String>>,
}

impl RecordingSource {
    async fn schema_calls(&self) -> Vec<String> {
        self.schema_calls.lock().unwrap().clone()
    }

    fn with_error(self: Arc<Self>, error: &str) -> Arc<Self> {
        *self.tables_error.lock().unwrap() = Some(error.to_string());
        self
    }
}

#[async_trait::async_trait]
impl LogicalSchemaContextSource for RecordingSource {
    async fn driver_for_session(
        &self,
        _session_id: &str,
    ) -> Result<Arc<dyn DatabaseDriver>, String> {
        Ok(self.driver.clone())
    }
    async fn session_schema(&self, _session_id: &str) -> Option<String> {
        None
    }
    async fn tables(
        &self,
        _session_id: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String> {
        let error = self.tables_error.lock().unwrap().clone();
        match error {
            Some(e) => Err(e),
            None => Ok(self.tables.clone()),
        }
    }
    async fn table_schema(
        &self,
        _session_id: &str,
        _database: &str,
        _schema: Option<&str>,
        table: &str,
    ) -> Result<datazen_driver_api::TableSchema, String> {
        self.schema_calls.lock().unwrap().push(table.to_string());
        Ok(datazen_driver_api::TableSchema {
            table_name: table.to_string(),
            columns: vec![datazen_driver_api::ColumnSchema {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                is_primary_key: true,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        })
    }
}

fn user_message(content: &str) -> ChatMessage {
    ChatMessage {
        role: MessageRoleT::User,
        content: content.into(),
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
    }
}

fn recording_source() -> Arc<RecordingSource> {
    Arc::new(RecordingSource {
        driver: MockDriver::new("postgres", MockDriverOptions::default()),
        tables: vec![
            TableInfo {
                name: "users".into(),
                schema: None,
                table_type: TableType::Table,
                row_count: Some(0),
            },
            TableInfo {
                name: "orders".into(),
                schema: None,
                table_type: TableType::Table,
                row_count: Some(0),
            },
        ],
        tables_error: Mutex::new(None),
        schema_calls: Mutex::new(Vec::new()),
    })
}

/// 宿主在 IPC 入口处解析好的环境（fake 对应物）。
fn test_env(provider: Arc<dyn AiProvider>, src: Arc<RecordingSource>) -> ChatSessionEnv {
    let overrides = std::env::temp_dir().join(format!(
        "datazen-chat-session-tests-{}",
        uuid::Uuid::new_v4()
    ));
    ChatSessionEnv {
        provider,
        model: "test-model".into(),
        gate: AiSafetyGateConfig::default(),
        injection: crate::ai::chat_injection::ChatInjection::default(),
        memory_injection: None,
        memory_used: Vec::new(),
        lang: "en".into(),
        connections_context: String::new(),
        mcp_tools: Vec::new(),
        skill_tools: Vec::new(),
        context_entries: Vec::new(),
        schema_driver: None,
        prompt_resolver: Arc::new(PromptResolver::new(&overrides, None)),
        schema_pipeline: SchemaContextPipeline::new(Arc::new(
            crate::ai::application::context::SchemaContextBuilder::new(src),
        )),
        cancel_registry: CancellationRegistry::default(),
        cancel_owner: "test-owner".into(),
    }
}

fn mcp_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "mcp/demo/ping".into(),
        description: "Ping the demo MCP server".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    }
}

fn stream_chunk(content: &str, done: bool) -> StreamChunkT {
    StreamChunkT {
        content: content.into(),
        reasoning: None,
        done,
        cancelled: false,
        usage: None,
        tool_calls: None,
        response_id: None,
        egress_summary: None,
        memory_used: None,
    }
}

fn sink() -> (StreamCallback, Arc<Mutex<Vec<StreamChunkT>>>) {
    let seen: Arc<Mutex<Vec<StreamChunkT>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let on_chunk: StreamCallback = Arc::new(move |_id, result| {
        if let Ok(chunk) = result {
            sink.lock().unwrap().push(chunk);
        }
    });
    (on_chunk, seen)
}

// ── log capture ──

/// Process-global capture installed once. A thread-local `with_default` is
/// not enough: tracing caches callsite interest globally, and a parallel test
/// that emits this callsite first without a subscriber can cache it as
/// `never`. A global subscriber keeps the callsite enabled regardless of test
/// scheduling, so assertions do not depend on which test runs first.
mod log_capture {
    use std::fmt::Write as _;
    use std::sync::{Mutex, Once};
    use tracing::{Event, Subscriber};
    use tracing_subscriber::layer::{Context, Layer};
    use tracing_subscriber::prelude::*;

    static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static INSTALL: Once = Once::new();

    struct Capture;

    struct Visitor(String);

    impl tracing::field::Visit for Visitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            let _ = write!(self.0, "{}={value:?};", field.name());
        }
    }

    impl<S> Layer<S> for Capture
    where
        S: Subscriber,
    {
        fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = Visitor(String::new());
            event.record(&mut visitor);
            EVENTS.lock().unwrap().push(visitor.0);
        }
    }

    pub(super) fn install() {
        INSTALL.call_once(|| {
            let subscriber = tracing_subscriber::registry()
                .with(tracing_subscriber::filter::LevelFilter::INFO)
                .with(Capture);
            tracing::subscriber::set_global_default(subscriber)
                .expect("global capture subscriber installs once");
            tracing::callsite::rebuild_interest_cache();
        });
        EVENTS.lock().unwrap().clear();
    }

    pub(super) fn joined() -> String {
        EVENTS.lock().unwrap().join("\n")
    }
}

// ── chat 编排 ──

#[tokio::test]
async fn chat_with_driver_resolves_schema_and_keeps_db_tools() {
    let provider = Arc::new(ScriptedProvider::default().with_tools());
    provider.push_stream(vec![stream_chunk("hi", true)]);
    let src = recording_source();
    let mut env = test_env(provider.clone(), src.clone());
    // 宿主借到 live 驱动后才进管线（会话未知 = None = 静默跳过）。
    env.schema_driver = Some(MockDriver::new("postgres", MockDriverOptions::default()));
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    let result = run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-chat-schema".into(),
            db_session_id: Some("sess-1".into()),
            connection_id: Some("conn-1".into()),
            database: Some("app".into()),
            messages: vec![user_message("Hi")],
            include_schema: true,
            scenario: ChatScenario::Chat,
            context_tables: Some(vec!["users".into()]),
            agent_mode: AiAgentMode::Agent,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("chat session completes");

    assert_eq!(result, "req-chat-schema");
    // pinned 表进 selective context（逐表 schema 调用顺序即裁决顺序）。
    assert_eq!(src.schema_calls().await, vec!["users".to_string()]);
    // 工具循环分支：请求带 DB 工具，系统提示带 schema 后缀。
    let request = provider.last_request().expect("loop request");
    let tools = request.tools.expect("tools attached");
    assert!(tools.iter().any(|t| t.name == "list_tables"));
    let system = &request.messages[0];
    assert_eq!(system.role, MessageRoleT::System);
    assert!(
        system.content.contains("Database type:"),
        "schema suffix expected"
    );
    assert!(system.content.contains("users"));
}

#[tokio::test]
async fn chat_ask_mode_strips_db_and_mcp_tools() {
    let provider = Arc::new(ScriptedProvider::default().with_tools());
    provider.push_stream(vec![stream_chunk("answer", true)]);
    let mut env = test_env(provider.clone(), recording_source());
    env.schema_driver = Some(MockDriver::new("postgres", MockDriverOptions::default()));
    env.mcp_tools = vec![mcp_tool_definition()];
    env.skill_tools = crate::ai::skills::tools::skill_tool_definitions();
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-ask-mode".into(),
            db_session_id: Some("sess-1".into()),
            connection_id: Some("conn-1".into()),
            database: Some("app".into()),
            messages: vec![user_message("What is a join?")],
            include_schema: true,
            scenario: ChatScenario::Chat,
            context_tables: None,
            agent_mode: AiAgentMode::Ask,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("ask chat completes");

    let request = provider.last_request().expect("request");
    let tools = request.tools.expect("tools attached");
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ask_questions"],
        "Ask must expose only ask_questions"
    );
}

#[tokio::test]
async fn chat_agent_mode_exposes_skill_tools_but_ask_does_not() {
    let provider = Arc::new(ScriptedProvider::default().with_tools());
    provider.push_stream(vec![stream_chunk("answer", true)]);
    let mut env = test_env(provider.clone(), recording_source());
    env.skill_tools = crate::ai::skills::tools::skill_tool_definitions();
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-skill-agent".into(),
            db_session_id: None,
            connection_id: None,
            database: None,
            messages: vec![user_message("Use a skill")],
            include_schema: false,
            scenario: ChatScenario::Chat,
            context_tables: None,
            agent_mode: AiAgentMode::Agent,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("agent chat completes");
    let request = provider.last_request().expect("request");
    let names: Vec<_> = request
        .tools
        .expect("tools")
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert!(names.contains(&"list_skills".to_string()));
    assert!(names.contains(&"use_skill".to_string()));
    assert!(names.contains(&"read_skill_file".to_string()));
}

#[tokio::test]
async fn chat_agent_mode_exposes_db_and_mcp_tools() {
    let provider = Arc::new(ScriptedProvider::default().with_tools());
    provider.push_stream(vec![stream_chunk("answer", true)]);
    let mut env = test_env(provider.clone(), recording_source());
    env.schema_driver = Some(MockDriver::new("postgres", MockDriverOptions::default()));
    env.mcp_tools = vec![mcp_tool_definition()];
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-agent-mode".into(),
            db_session_id: Some("sess-1".into()),
            connection_id: Some("conn-1".into()),
            database: Some("app".into()),
            messages: vec![user_message("List the tables")],
            include_schema: true,
            scenario: ChatScenario::Chat,
            context_tables: None,
            agent_mode: AiAgentMode::Agent,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("agent chat completes");

    let request = provider.last_request().expect("request");
    let tools = request.tools.expect("tools attached");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert!(names.contains(&"ask_questions"));
    assert!(names.contains(&"list_tables"), "Agent keeps DB tools");
    assert!(
        names.contains(&"query_readonly"),
        "Agent exposes read-only query"
    );
    assert!(names.contains(&"sample_rows"), "Agent exposes sampling");
    assert!(names.contains(&"mcp/demo/ping"), "Agent keeps MCP tools");
}

#[test]
fn chat_logs_persisted_connection_id() {
    log_capture::install();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    runtime.block_on(async {
        let provider = Arc::new(ScriptedProvider::default());
        provider.push_stream(vec![stream_chunk("ok", true)]);
        let env = test_env(provider, recording_source());
        let host = RecordingLoopHost::default();
        let (on_chunk, _seen) = sink();

        run_chat_session(
            env,
            &host,
            on_chunk,
            ChatSessionInput {
                request_id: "req-log-conn".into(),
                db_session_id: Some("runtime-session-1".into()),
                connection_id: Some("persisted-conn-1".into()),
                database: Some("app".into()),
                messages: vec![user_message("Hi")],
                include_schema: false,
                scenario: ChatScenario::Chat,
                context_tables: None,
                agent_mode: AiAgentMode::Ask,
                loaded_skill_ids: None,
                editor_sql: None,
                selection: None,
                last_error: None,
                result_preview: None,
            },
        )
        .await
        .expect("chat completes");
    });

    let logged = log_capture::joined();
    assert!(
        logged.contains("connection_id=Some(\"persisted-conn-1\")"),
        "ai_chat log must carry the persisted connection id: {logged}"
    );
    assert!(logged.contains("ai_chat: start"));
}

#[tokio::test]
async fn chat_unknown_session_skips_schema_silently_and_keeps_tools() {
    let provider = Arc::new(ScriptedProvider::default().with_tools());
    provider.push_stream(vec![stream_chunk("fallback", true)]);
    let src = recording_source();
    let env = test_env(provider.clone(), src);
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-unknown".into(),
            db_session_id: Some("not-connected".into()),
            connection_id: None,
            database: Some("app".into()),
            messages: vec![user_message("Hi")],
            include_schema: true,
            scenario: ChatScenario::Chat,
            context_tables: None,
            agent_mode: AiAgentMode::Agent,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("unknown session must still stream");

    // 静默跳过：宿主未借到驱动（None），管线从未被调。
    let request = provider.last_request().expect("request");
    let tools = request.tools.expect("tools attached");
    assert!(tools.iter().any(|t| t.name == "list_tables"));
    let system = &request.messages[0];
    assert!(
        !system.content.contains("Database type:"),
        "no schema suffix expected"
    );
}

#[tokio::test]
async fn chat_schema_pipeline_failure_disables_db_tools_but_streams() {
    // 管线 resolve 的错误面只在无工具分支出现（build_sql_context 传播）。
    let provider = Arc::new(ScriptedProvider::default());
    provider.push_stream(vec![stream_chunk("no-tools", true)]);
    let src = recording_source().with_error("tables boom");
    let mut env = test_env(provider.clone(), src);
    env.schema_driver = Some(MockDriver::new("postgres", MockDriverOptions::default()));
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-pipeline-fail".into(),
            db_session_id: Some("sess-1".into()),
            connection_id: None,
            database: Some("app".into()),
            messages: vec![user_message("Hi")],
            include_schema: true,
            scenario: ChatScenario::Chat,
            context_tables: None,
            agent_mode: AiAgentMode::Agent,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("pipeline failure downgrades instead of failing the command");

    let request = provider.last_request().expect("request");
    let tools = request.tools.expect("ask_questions stays");
    assert!(
        !tools.iter().any(|t| t.name == "list_tables"),
        "DB tools must be disabled after the pipeline failure"
    );
}

#[tokio::test]
async fn chat_context_entries_are_redacted_then_formatted() {
    let provider = Arc::new(ScriptedProvider::default());
    provider.push_stream(vec![stream_chunk("ok", true)]);
    let mut env = test_env(provider.clone(), recording_source());
    env.context_entries = vec![("notes.md".into(), "api_key=super-secret-value".into())];
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-ctx".into(),
            db_session_id: None,
            connection_id: None,
            database: None,
            messages: vec![user_message("Question")],
            include_schema: false,
            scenario: ChatScenario::Chat,
            context_tables: None,
            agent_mode: AiAgentMode::Ask,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("chat completes");

    let request = provider.last_request().expect("request");
    let user = request
        .messages
        .iter()
        .find(|m| m.role == MessageRoleT::User)
        .expect("user message");
    assert!(
        user.content.starts_with("[Context: notes.md]"),
        "context block leads: {}",
        user.content
    );
    assert!(
        !user.content.contains("super-secret-value"),
        "content must be redacted"
    );
    assert!(user.content.contains("[REDACTED]"));
}

#[tokio::test]
async fn chat_workflow_scenario_injects_connections_context() {
    let provider = Arc::new(ScriptedProvider::default());
    provider.push_stream(vec![stream_chunk("yaml", true)]);
    let mut env = test_env(provider.clone(), recording_source());
    env.connections_context = "cfg-x available".into();
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-wf".into(),
            db_session_id: None,
            connection_id: None,
            database: None,
            messages: vec![user_message("Build a workflow")],
            include_schema: false,
            scenario: ChatScenario::WorkflowGenerate,
            context_tables: None,
            agent_mode: AiAgentMode::Agent,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("workflow chat completes");

    let request = provider.last_request().expect("request");
    let tools = request.tools.as_ref().expect("workflow tools");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert!(
        names.contains(&"list_tables"),
        "workflow generation keeps metadata tools under Agent mode"
    );
    assert!(
        !names.contains(&"query_readonly"),
        "unbound workflow must not expose query_readonly"
    );
    assert!(
        !names.contains(&"sample_rows"),
        "unbound workflow must not expose sample_rows"
    );
    let system = &request.messages[0];
    assert!(
        system.content.contains("cfg-x available"),
        "connections context must reach the system prompt: {}",
        system.content
    );
}

#[tokio::test]
async fn chat_unregisters_cancel_entry_after_completion() {
    let provider = Arc::new(ScriptedProvider::default());
    provider.push_stream(vec![stream_chunk("done", true)]);
    let env = test_env(provider.clone(), recording_source());
    let registry = env.cancel_registry.clone();
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();

    run_chat_session(
        env,
        &host,
        on_chunk,
        ChatSessionInput {
            request_id: "req-cancel".into(),
            db_session_id: None,
            connection_id: None,
            database: None,
            messages: vec![user_message("Hi")],
            include_schema: false,
            scenario: ChatScenario::Chat,
            context_tables: None,
            agent_mode: AiAgentMode::Ask,
            loaded_skill_ids: None,
            editor_sql: None,
            selection: None,
            last_error: None,
            result_preview: None,
        },
    )
    .await
    .expect("chat completes");

    let receipt = registry.cancel("req-cancel", "test-owner").await;
    assert_eq!(
        receipt,
        CancelReceipt::AlreadyFinished {
            request_id: "req-cancel".into()
        },
        "completion must unregister the cancel entry"
    );
}

// ── NL2SQL 编排 ──

#[tokio::test]
async fn generate_pins_merge_yaml_then_request_then_current_table_first() {
    let provider = Arc::new(ScriptedProvider::default().with_tools());
    provider.push_stream(vec![stream_chunk("SELECT 1", true)]);
    let mut env = test_env(provider.clone(), recording_source());
    env.context_entries = vec![
        (
            "schema.ctx.yaml".into(),
            "groups:\n  - name: Billing\n    tables:\n      - invoices\n      - customers".into(),
        ),
        ("notes.md".into(), "hint".into()),
    ];
    let src = recording_source();
    env.schema_pipeline = SchemaContextPipeline::new(Arc::new(
        crate::ai::application::context::SchemaContextBuilder::new(src.clone()),
    ));
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();
    let driver = MockDriver::new("postgres", MockDriverOptions::default());

    run_sql_generation_session(
        env,
        driver,
        &host,
        on_chunk,
        SqlGenerationInput {
            request_id: "req-nl2sql".into(),
            db_session_id: "sess-1".into(),
            database: "app".into(),
            natural_language: "count rows".into(),
            current_table: Some("users".into()),
            recent_queries: Vec::new(),
            context_tables: vec!["orders".into()],
        },
    )
    .await
    .expect("nl2sql session completes");

    // 合成顺序：ctx.yaml 追加在请求附带之后，当前表置首
    // （selective context 的逐表 schema 调用顺序即 pinned 顺序）。
    assert_eq!(
        src.schema_calls().await,
        vec![
            "users".to_string(),
            "orders".to_string(),
            "invoices".to_string(),
            "customers".to_string()
        ]
    );
    // 工具分支：请求被替换为 DB 工具集。
    let request = provider.last_request().expect("request");
    let tools = request.tools.expect("db tools attached");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"list_tables"));
    assert!(
        !names.contains(&"query_readonly"),
        "SQL generation must not expose query_readonly"
    );
    assert!(
        !names.contains(&"sample_rows"),
        "SQL generation must not expose sample_rows"
    );
    // yaml 注释块进 NL（前缀），且 yaml 文件本身不重复出现为上下文块。
    let user = request
        .messages
        .iter()
        .find(|m| m.role == MessageRoleT::User)
        .expect("user message");
    assert!(
        user.content.contains("[Context: notes.md]"),
        "remaining entry formats into NL"
    );
    assert!(
        !user.content.contains("schema.ctx.yaml"),
        "yaml source file is consumed as pinned tables"
    );
}

#[tokio::test]
async fn generate_without_tool_support_streams_plainly() {
    let provider = Arc::new(ScriptedProvider::default());
    provider.push_stream(vec![stream_chunk("SELECT * FROM users", true)]);
    let env = test_env(provider.clone(), recording_source());
    let host = RecordingLoopHost::default();
    let (on_chunk, seen) = sink();
    let driver = MockDriver::new("postgres", MockDriverOptions::default());

    let result = run_sql_generation_session(
        env,
        driver,
        &host,
        on_chunk.clone(),
        SqlGenerationInput {
            request_id: "req-plain".into(),
            db_session_id: "sess-1".into(),
            database: "app".into(),
            natural_language: "show users".into(),
            current_table: None,
            recent_queries: Vec::new(),
            context_tables: Vec::new(),
        },
    )
    .await
    .expect("plain stream completes");

    assert_eq!(result, "req-plain");
    assert_eq!(
        provider.stream_calls.load(Ordering::SeqCst),
        1,
        "plain stream branch"
    );
    assert_eq!(host.db_calls.load(Ordering::SeqCst), 0, "no tool loop");
    // 转发任务把块送到 on_chunk（spawn 后需要让出调度）。
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let chunks = seen.lock().unwrap();
    assert!(
        chunks
            .iter()
            .any(|c| c.done && c.content == "SELECT * FROM users"),
        "stream chunks forwarded: {chunks:?}"
    );
}

#[tokio::test]
async fn generate_pipeline_failure_surfaces_the_source_error() {
    // resolve 的错误面只在无工具分支（build_sql_context 传播）。
    let provider = Arc::new(ScriptedProvider::default());
    let src = recording_source().with_error("catalog unreachable");
    let env = test_env(provider.clone(), src);
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();
    let driver = MockDriver::new("postgres", MockDriverOptions::default());

    let err = run_sql_generation_session(
        env,
        driver,
        &host,
        on_chunk,
        SqlGenerationInput {
            request_id: "req-fail".into(),
            db_session_id: "sess-1".into(),
            database: "app".into(),
            natural_language: "show users".into(),
            current_table: None,
            recent_queries: Vec::new(),
            context_tables: Vec::new(),
        },
    )
    .await
    .expect_err("pipeline failure surfaces");

    match err {
        AiUseCaseError::Internal(message) => {
            assert_eq!(
                message, "catalog unreachable",
                "source error text is the wire text"
            )
        }
        other => panic!("expected Internal, got: {other:?}"),
    }
}

#[tokio::test]
async fn generate_unregisters_cancel_entry_after_tool_loop() {
    let provider = Arc::new(ScriptedProvider::default().with_tools());
    provider.push_stream(vec![stream_chunk("SELECT 1", true)]);
    let env = test_env(provider.clone(), recording_source());
    let registry = env.cancel_registry.clone();
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();
    let driver = MockDriver::new("postgres", MockDriverOptions::default());

    run_sql_generation_session(
        env,
        driver,
        &host,
        on_chunk,
        SqlGenerationInput {
            request_id: "req-gen-cancel".into(),
            db_session_id: "sess-1".into(),
            database: "app".into(),
            natural_language: "show users".into(),
            current_table: None,
            recent_queries: Vec::new(),
            context_tables: Vec::new(),
        },
    )
    .await
    .expect("nl2sql completes");

    let receipt = registry.cancel("req-gen-cancel", "test-owner").await;
    assert_eq!(
        receipt,
        CancelReceipt::AlreadyFinished {
            request_id: "req-gen-cancel".into()
        }
    );
}

// ── 自动上下文快照 ──

fn snapshot_input(
    editor_sql: Option<&str>,
    selection: Option<&str>,
    last_error: Option<&str>,
    result_preview: Option<ChatResultPreview>,
) -> ChatSessionInput {
    ChatSessionInput {
        request_id: "req-snapshot".into(),
        db_session_id: None,
        connection_id: None,
        database: Some("app".into()),
        messages: vec![user_message("what changed?")],
        include_schema: false,
        scenario: ChatScenario::Chat,
        context_tables: None,
        agent_mode: AiAgentMode::Ask,
        loaded_skill_ids: None,
        editor_sql: editor_sql.map(str::to_string),
        selection: selection.map(str::to_string),
        last_error: last_error.map(str::to_string),
        result_preview,
    }
}

fn preview(rows: Vec<Vec<serde_json::Value>>, row_count: u64) -> ChatResultPreview {
    ChatResultPreview {
        columns: vec!["id".into(), "name".into()],
        rows,
        row_count: Some(row_count),
        elapsed_ms: Some(12),
    }
}

fn row(id: u64, name: &str) -> Vec<serde_json::Value> {
    vec![serde_json::json!(id), serde_json::json!(name)]
}

#[test]
fn auto_context_orders_sql_selection_error_then_preview() {
    let input = snapshot_input(
        Some("SELECT * FROM users"),
        Some("SELECT * FROM users WHERE id = 1"),
        Some("syntax error near FROM"),
        Some(preview(vec![row(1, "alice")], 1)),
    );
    let block = build_auto_context_block(&input, &AiSafetyGateConfig::default(), &[])
        .expect("snapshot block");

    let sql_at = block.find("[Current SQL]").expect("SQL section");
    let selection_at = block.find("[Editor selection]").expect("selection section");
    let error_at = block.find("[Recent query error]").expect("error section");
    let preview_at = block
        .find("[Recent result preview")
        .expect("preview section");
    assert!(sql_at < selection_at);
    assert!(selection_at < error_at);
    assert!(error_at < preview_at);
    assert!(block.contains("SELECT * FROM users"));
    assert!(block.contains("syntax error near FROM"));
    assert!(block.contains("alice"));
    assert!(block.contains("showing 1 of 1 rows, 12ms"));
}

#[test]
fn auto_context_partial_and_empty_fields_produce_no_empty_headings() {
    let only_error = snapshot_input(None, None, Some("boom"), None);
    let block = build_auto_context_block(&only_error, &AiSafetyGateConfig::default(), &[])
        .expect("error block");
    assert!(block.contains("[Recent query error]\nboom"));
    assert!(!block.contains("[Current SQL]"));
    assert!(!block.contains("[Editor selection]"));
    assert!(!block.contains("[Recent result preview"));

    let empty = snapshot_input(Some("   "), Some(""), Some(" \n "), None);
    assert!(
        build_auto_context_block(&empty, &AiSafetyGateConfig::default(), &[]).is_none(),
        "empty fields must not create headings"
    );
}

#[test]
fn auto_context_is_truncated_and_redacted() {
    let long_sql = format!(
        "SELECT * FROM users WHERE password=supersecret -- {}",
        "x".repeat(400)
    );
    let input = snapshot_input(Some(&long_sql), None, None, None);
    let gate = AiSafetyGateConfig {
        max_context_bytes: 120,
        ..AiSafetyGateConfig::default()
    };
    let block = build_auto_context_block(&input, &gate, &[]).expect("truncated block");
    assert!(
        block.len() <= 120,
        "block must respect gate cap: {}",
        block.len()
    );
    assert!(!block.contains("supersecret"), "{block}");
}

#[test]
fn auto_context_preview_keeps_five_rows_and_omits_the_rest() {
    let rows = (0..8).map(|id| row(id, &format!("name-{id}"))).collect();
    let input = snapshot_input(None, None, None, Some(preview(rows, 8)));
    let block = build_auto_context_block(&input, &AiSafetyGateConfig::default(), &[])
        .expect("preview block");

    assert!(block.contains("name-4"), "{block}");
    assert!(!block.contains("name-5"), "row 6 must be omitted: {block}");
    assert!(block.contains("showing 5 of 8 rows, 12ms"));
    assert!(block.contains("3 more preview rows omitted"));
}

#[test]
fn auto_context_dedupes_picked_files_and_query_error_prompt() {
    let sql = "SELECT * FROM users";
    let input = snapshot_input(Some(sql), None, Some("relation users does not exist"), None);
    let entries = vec![("query.sql".into(), sql.to_string())];
    let block = build_auto_context_block(&input, &AiSafetyGateConfig::default(), &entries)
        .expect("error only");
    assert!(!block.contains("[Current SQL]"), "{block}");
    assert!(block.contains("[Recent query error]"));

    let mut prompt_input = snapshot_input(None, None, Some("relation users does not exist"), None);
    prompt_input.messages = vec![user_message(
        "Please explain this error: relation users does not exist",
    )];
    let block = build_auto_context_block(&prompt_input, &AiSafetyGateConfig::default(), &[]);
    assert!(
        block.is_none(),
        "query-error prompt already carries the error"
    );
}

#[tokio::test]
async fn chat_attaches_auto_context_before_picked_files() {
    let provider = Arc::new(ScriptedProvider::default());
    provider.push_stream(vec![stream_chunk("ok", true)]);
    let src = recording_source();
    let mut env = test_env(provider.clone(), src);
    env.context_entries = vec![("notes.md".into(), "picked notes".into())];
    let host = RecordingLoopHost::default();
    let (on_chunk, _seen) = sink();
    let mut input = snapshot_input(
        Some("SELECT * FROM users"),
        None,
        Some("syntax error near FROM"),
        Some(preview(vec![row(1, "alice")], 1)),
    );
    input.messages = vec![user_message("why did this fail?")];

    run_chat_session(env, &host, on_chunk, input)
        .await
        .expect("chat session completes");

    let request = provider.last_request().expect("request");
    let user = request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRoleT::User)
        .expect("user request");
    let auto_at = user.content.find("[Current SQL]").expect("auto block");
    let picked_at = user
        .content
        .find("[Context: notes.md]")
        .expect("picked block");
    assert!(auto_at < picked_at, "{}", user.content);
    assert!(user.content.contains("[Recent query error]"));
    assert!(user.content.contains("alice"));
}

#[test]
fn auto_context_dedupes_a_result_preview_already_in_a_picked_file() {
    let preview = preview(vec![row(1, "alice")], 1);
    let rendered = render_result_preview(&preview).expect("rendered preview");
    let input = snapshot_input(None, None, None, Some(preview));
    let entries = vec![("rows.txt".into(), rendered)];

    assert!(
        build_auto_context_block(&input, &AiSafetyGateConfig::default(), &entries).is_none(),
        "preview already present in the picked file must not be repeated"
    );
}

#[test]
fn auto_context_preview_caps_columns() {
    let columns = (0..25).map(|i| format!("c{i}")).collect();
    let preview = ChatResultPreview {
        columns,
        rows: vec![vec![serde_json::json!(1)]],
        row_count: Some(1),
        elapsed_ms: None,
    };
    let input = snapshot_input(None, None, None, Some(preview));
    let block = build_auto_context_block(&input, &AiSafetyGateConfig::default(), &[])
        .expect("preview block");

    assert!(block.contains("c19"), "{block}");
    assert!(!block.contains("c20"), "column 21 must be omitted: {block}");
}
