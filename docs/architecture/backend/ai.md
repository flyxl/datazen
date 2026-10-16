# AI 模块

> [返回架构总览](../README.md)

### 1.1 概述

AI 模块采用与数据库驱动相同的 **Provider 抽象 + Registry** 模式，分两个 Rust crate：`packages/ai-api` 定义统一接口（契约层），`packages/ai-providers` 提供各厂商的 Provider 实现与共享 HTTP 协议（传输层）。宿主 `src-tauri/src/ai/` 只做门面 re-export，并保留确实需要宿主服务的那部分（安全门、Prompt 解析、Schema 上下文、token 预算、取消）。

### 1.2 架构分层

```
packages/ai-api/                    # 公共 AI Provider API crate
├── src/
│   ├── lib.rs                      # AI_PROTOCOL_VERSION（当前 1，最低兼容 1）+ re-exports
│   ├── traits.rs                   # AiProvider trait (async_trait, Send+Sync)
│   ├── types.rs                    # AiProviderConfig, ChatMessage, StreamChunk, AiError,
│   │                               # ToolDefinition, ToolCall, ToolResult, ModelInfo
│   └── factory.rs                  # AiProviderFactory + inventory + register_ai_provider!

packages/ai-providers/              # Provider 实现 + 共享 HTTP 协议（传输层，不依赖宿主）
│   src/
│   ├── lib.rs                      # 模块组织 + re-exports
│   ├── openai.rs                   # OpenAI Provider (Chat Completions API)
│   ├── anthropic.rs                # Anthropic Provider (Messages API)
│   ├── deepseek.rs                 # DeepSeek Provider (Responses API, reasoning 支持)
│   ├── ollama.rs                   # Ollama Provider (本地端点)
│   ├── custom.rs                   # 自定义 Provider (三种协议: Chat/Responses/Anthropic 兼容,
│   │                               #   远程模型列表获取)
│   ├── registry.rs                 # AiProviderRegistry (动态注册/获取, inventory 发现)
│   └── protocol/                   # 共享 HTTP 协议实现
│       ├── mod.rs                  # ProtocolConfig, timeouts, URL normalization, 端点日志脱敏
│       ├── openai_chat.rs          # OpenAI Chat Completions API (streaming + non-streaming)
│       ├── openai_responses.rs     # OpenAI Responses API (DeepSeek/custom Responses 模式)
│       ├── anthropic.rs            # Anthropic Messages API
│       └── test_support.rs         # 协议层单测共享的 wiremock 夹具

src-tauri/src/ai/                   # 宿主门面：re-export + 真正需要宿主服务的部分
├── mod.rs                          # re-export datazen_ai_api / datazen_ai_providers + 本地模块
├── context.rs                      # SchemaContextBuilder (DDL 上下文, token 预算控制)
├── prompt_resolver.rs              # PromptResolver (资源文件加载 + 用户/驱动覆盖 + 多语言 fallback)
├── safety.rs                       # AiSafetyGate (调用方文本出境分级与工具调用校验)
├── schema_pipeline.rs              # SchemaContextPipeline (PromptSeed → 系统后缀)
├── ctx_yaml.rs                     # .ctx.yaml 表分组上下文解析
├── budget.rs                       # token 预算常量
└── cancel.rs                       # CancellationRegistry (request_id → CancellationToken)
```

### 1.3 AiProvider Trait

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn provider_type(&self) -> &str;
    fn display_name(&self) -> &str;
    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError>;
    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError>;
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError>;
    fn supports_streaming(&self) -> bool { true }
    fn supports_tools(&self) -> bool { false }
    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<(), AiError>;
    async fn cancel(&self) -> Result<(), AiError> { Ok(()) }
    async fn reset(&self) -> Result<(), AiError> { Ok(()) }
}
```

### 1.4 Provider 协议层

多个 Provider 共享底层 HTTP 协议实现，避免重复代码。协议层位于 `packages/ai-providers/src/protocol/`，
宿主通过 `crate::ai::*` 门面引用（`src-tauri/src/ai/mod.rs` 同时 re-export `datazen_ai_api` 与
`datazen_ai_providers`），调用方无须改写 import 路径：

| Provider | 协议 | 模块 |
|----------|------|------|
| OpenAI | Chat Completions | `packages/ai-providers/src/protocol/openai_chat.rs` |
| DeepSeek | Responses API（含 reasoning） | `packages/ai-providers/src/protocol/openai_responses.rs` |
| Anthropic | Messages API | `packages/ai-providers/src/protocol/anthropic.rs` |
| Custom | 三选一：Chat / Responses / Anthropic | 对应 protocol 模块 |

`packages/ai-providers` 只依赖 `datazen-ai-api` 与 HTTP 客户端，不依赖 Tauri 与宿主存储：
Provider 配置的持久化留在宿主。启动时的两步拆成两个方法——
`AiProviderRegistry::ensure_registered()` 负责内置 Provider 的注册与去重，
并在**首次**调用时返回 `true`；宿主据此只在该次调用后读取已解密配置并交给
`AiProviderRegistry::restore_config()`，保证配置恢复与 Provider 注册同样只发生一次：

```rust
pub async fn ensure_ai_ready(&self) {
    if self.ai_registry.ensure_registered().await {
        if let Some(ai_config) = self.store.get_ai_config().await {
            self.ai_registry.restore_config(&ai_config).await;
        }
    }
    let lang = self.store.get_settings().await.language;
    self.prompt_resolver.ensure_ready(&lang).await;
}
```

`restore_config()` 内部只调用 `AiProvider::initialize()`（纯内存状态，无网络请求），
失败时记录 warn 并保持 Provider 未初始化，因此调用方无需处理错误分支。

### AI HTTP 出站边界

内置 Provider 与三种共享协议的模型列表、连接测试、普通与流式完成请求统一经
`datazen_ai_providers::http::HttpClient` 发送。该传输边界与数据库 `NetworkProvider`
的路由/隧道职责分离；不把 HTTP 请求伪装为数据库路由。

`HttpClient::default()` 使用 `PublicInternet` 策略。每次请求解析全部 A/AAAA 地址，任一地址
命中私网、环回、链路本地、共享地址或特殊用途范围就拒绝；随后将已批准地址固定到该次
连接，禁用环境代理和自动重定向，避免代理解析、DNS rebinding 与跨跳绕过。
地址拒绝与传输错误使用脱敏文案，不回显端点或凭据。每次实际请求新建固定地址的客户端，
目前以放弃跨请求连接池复用换取 DNS 校验和连接目标严格一致。

Provider/Registry 的 `new()` 及无客户端参数的模型获取均采用受限默认策略。
桌面宿主通过 `with_http_client` 和 `fetch_remote_models_with_client` 显式采用 `HttpClient::desktop()`，保留用户配置的本地 Ollama、
私网网关及企业环境代理。桌面同样不自动跟随重定向，并保持 HTTP(S)/userinfo URL 校验；宽松桌面策略由 reqwest/可信环境代理执行目标 DNS，支持仅在代理侧可解析的域名。服务端适配尚未落地；未来适配必须使用
`AiProviderRegistry::with_http_client(HttpClient::default())`，或通过公开的 `EgressPolicy`
trait 注入管理员出站策略，并将相同客户端传给 `fetch_remote_models_with_client`。
模型列表保留 10 秒连接超时与覆盖解析、请求及读体的 30 秒总超时。
各内置 Provider 也提供 `with_http_client`。策略由部署方构造，不能从用户 AI 配置开启私网。
外部 inventory Provider 属于可信原生扩展，其自有网络实现需由部署方另行审查。

### 1.5 AI IPC 命令

| 命令 | 功能 | 流式 |
|------|------|------|
| `ai_generate_sql` | NL2SQL | Tauri Events |
| `ai_diagnose_error` | SQL 错误诊断 | - |
| `ai_analyze_explain` | EXPLAIN 计划 AI 分析 | - |
| `ai_chat` | AI 对话（DB tools + 已连接 MCP Client tools） | Tauri Events |
| `ai_parse_filter` | 自然语言筛选解析 | - |
| `ai_generate_schema_doc` | Schema 文档生成 | - |
| `ai_diagnose_connection` | 连接故障排查 | - |
| `ai_analyze_queries` | 查询历史分析 | - |
| `workflow_list` | 列出所有 Workflows | - |
| `workflow_execute` | 执行 Workflow | - |
| `workflow_save` / `workflow_delete` | Workflow CRUD | - |
| `workflow_reload` | 重新加载 Workflows | - |
| `workflow_get_history` / `workflow_delete_history` | 执行历史管理 | - |
| `prompt_list` / `prompt_get` / `prompt_set` / `prompt_reset` | Prompt 覆盖管理 | - |

### 1.6 SchemaContextBuilder

构建紧凑 DDL 作为 LLM 上下文：
- 首次仅发送表名列表（减少 token 消耗）
- LLM 需要时再补充详细列/约束信息
- 支持 token 预算控制

### 1.7 PromptResolver

AI Prompt 模板管理（替代原 `PromptBuilder`），统一使用英文系统 Prompt，并通过注入强约束语言指令输出当前应用设置的语言：

**解析优先级**：
1. 用户覆盖 — `prompt_overrides.json`（通过设置界面修改，按场景存储）
2. 驱动覆盖 — `DatabaseDriver::prompt_overrides()`（运行时按连接的驱动类型动态应用）
3. 资源文件模板 — `resources/prompts/*.md`（运行时加载）
4. 编译时嵌入 — `embedded_default()`（编译期二进制兜底）

**语言控制机制**：Prompt 模板统一保持英文以最大化模型指令遵循与工具调用稳定性；自然语言输出通过 `inject_language_hint` 动态注入系统指令，强制模型输出当前应用配置语言（如简体中文、英文等）。

**Prompt 场景**（`PromptScenario`枚举）：

| 场景 | 模板文件 | 用途 |
|------|---------|------|
| `Nl2Sql` | `nl2sql.md` | 自然语言转 SQL |
| `Diagnose` | `diagnose.md` | SQL 错误诊断 |
| `NlFilter` | `nl_filter.md` | 自然语言表筛选 |
| `SchemaDocSelectTables` | `schema_doc_select_tables.md` | 选择 Schema 文档表 |
| `SchemaDoc` | `schema_doc.md` | 生成 Schema 文档 |
| `ConnectionDiagnose` | `connection_diagnose.md` | 连接故障排查 |
| `QuerySummary` | `query_summary.md` | 查询历史分析 |
| `ExplainAnalysis` | `explain_analysis.md` | EXPLAIN 计划分析 |
| `Chat` | `chat.md` | AI 侧边栏对话 |
| `WorkflowGenerate` | `workflow_generate.md` | AI 辅助 Workflow 生成 |

**模板存放位置**：`src-tauri/resources/prompts/*.md`

**运行时加载**：首次使用 AI 时加载至内存缓存。

### 1.8 AI 流式响应

`StreamChunk` 包含 `content` 和 `reasoning` 两个独立字段：
- `content` — 正式回答内容
- `reasoning` — 模型思考/推理过程（`reasoning_content` from OpenAI/DeepSeek）
- 前端 `aiStore` 分别累积两个字段，完成后合并到 `AiChatMessage`
- 聊天界面将推理过程渲染为可折叠区域（默认折叠）
- NL2SQL 在流完成时通过 `extractSqlFromResponse()` 过滤非 SQL 内容

### 1.9 AI 上下文引用（@ Mentions）

所有 AI 输入区域支持 `@` 引用本地文件作为上下文：

**后端**（`commands/context.rs`）：
- `context_get_dir` — 获取上下文目录路径
- `context_list_files` — 列出目录中的文本文件（递归扫描，扩展名白名单）
- `context_read_files` — 读取选中文件的内容（512KB 大小限制，路径遍历防护）

**支持的文件类型**：`.txt`, `.md`, `.sql`, `.json`, `.yaml`, `.yml`, `.csv`, `.toml`, `.xml`, `.html`, `.css`, `.js`, `.ts`, `.py`, `.sh`, `.log`, `.conf`, `.ini`, `.cfg`, `.env`, `.properties`

### 1.10 NL2SQL 大量表优化

针对包含大量表（数千个）的数据库，NL2SQL 模块提供以下优化：

#### search_tables 工具

`search_tables` 是 AI DB tools 中新增的关键字搜索工具（大小写不敏感的子串匹配），支持 `limit` 参数。当数据库表数量超过 500 个时，`SchemaContextPipeline` 的系统提示会自动引导 LLM 优先使用 `search_tables` 按关键字查找相关表，而不是使用 `list_tables` 列出全部表名。

#### .ctx.yaml 表组上下文文件

用户可创建 `.ctx.yaml`（或 `.ctx.yml`）文件来定义命名的表组：

```yaml
groups:
  - name: "User Management"
    tables:
      - users
      - roles
      - permissions
  - name: "Orders"
    tables:
      - orders
      - order_items
```

当用户在 NL2SQL 输入中 `@` 引用 `.ctx.yaml` 文件时，后端会：
1. 解析 YAML 提取所有表名（跨组去重）
2. 将表名合并到 pinned tables 列表
3. 由 `SchemaContextPipeline` 实时获取这些表的 DDL
4. 注入到系统 prompt 中

普通上下文文件（非 `.ctx.yaml`）仍按原有方式作为文本附加到用户消息。

**解析模块**：`src-tauri/src/ai/ctx_yaml.rs`

**前端识别**：`ContextPicker` 中 `.ctx.yaml` 文件使用 `Layers` 图标区分于普通文件。

### 1.11 AI Chat MCP 工具

`ai_chat` 在流式 tool loop 中合并两类工具定义：

1. **DB tools** — `list_tables`、`search_tables`、`query_db` 等，经 `ConnectionManager` / Driver Command API 执行。
2. **MCP Client tools** — 来自 Settings 中已连接的外部 MCP Server，qualified name 形如 `mcp/{serverId}/{toolName}`。

实现要点（`commands/ai/mod.rs`）：

- `collect_mcp_tool_definitions()` 从 `McpClientManager` 拉取已连接 server 的 tool schema，转换为 Provider 的 `ToolDefinition`。
- `run_streaming_tool_loop` 识别 `mcp/` 前缀并调用 `mcp_client_call_tool`；DB tool 与 MCP tool 可在同一轮对话中交替执行。
- 未连接 MCP Server 时行为与原先一致，仅暴露 DB tools。

配置与连接管理见 [`mcp.md` — MCP Client AI 集成](./mcp.md#mcp-client-ai-集成)。

### 1.12 会话、目标与取消

AI 侧与数据库的接触分两类，边界互不重叠：

**一、会话语义（诊断 / NL2SQL / EXPLAIN 分析 / Schema 文档）——只消费已存在的会话，不创建。**

- 这些 IPC 的形参是 `db_session_id`（运行时会话 ID），不是 `connectionId`；命令体内只用 `get_session` / `get_session_config` 查找。这条边界由契约测试直接对源码断言：会话语义命令的形参必须含 `db_session_id` 且**不得**含 `connection_id`，且命令体内不得出现「把名为 `connection_id` 的变量喂给 `get_session`」的写法（`commands/ai/ipc_contract_guards.rs`）。
- **`ai_chat` 的 schema 上下文是尽力而为，但降级的方向和直觉相反**。`commands/ai/chat.rs:810-813` 用 `if let Ok((driver, _handle)) = state.connection_manager.get_session(conn_id)` 包住整段 schema 上下文构建：会话取不到时这一整块被**静默跳过**——不报错、不记日志、不追加系统消息，而 `attach_db_tools` 保持初值 `true`（`chat.rs:808`），因此 **db tools 照常注入模型**（`chat.rs:979-981`）。也就是说会话失效时的实际后果是「模型拿不到 DDL 上下文」，而不是「失去模型自己查表的能力」。
  真正会 `warn!` 并把 `attach_db_tools` 置为 `false` 的是 `chat.rs:866-875` 的管线 `Err` 分支。但由于 `SchemaContextPipeline::resolve`（`ai/schema_pipeline.rs:74-128`）在 `:83-87` 吞掉了 `get_table_names` 的错误、在 `:98-107` 吞掉了 `build_selective_context` 的错误，唯一的 `?`（`:111-119`）只存在于 `supports_tools == false` 的分支——而那种情况下 `attach_db_tools` 本来就等于 `supports_tools`，即 `false`。**对支持工具的 provider，这条「告警 + 禁用 db tools」的分支实际不可达。**
  与之对照，配置语义的诊断命令（`ai_diagnose_connection`）不碰会话，连接配置缺失时直接返回 `CommandError::NotFound`（`commands/ai/generate.rs:813-817`）。
- **目标不切库**。AI 与其他消费方一样把目标 database / schema 随命令传递；宿主不实现生产环境的切库路径。

**二、配置语义（`ai_diagnose_connection`）——收 `connectionId`，读的是落盘配置**，用于「连不上」的故障排查，与已建立会话无关。契约测试同样钉住这一侧。

**三、唯一的反向映射：`ai_analyze_queries`。** 它收运行时会话 ID，但**不碰会话**——把该 ID 反查成所属的 `connectionId`，再按连接过滤查询历史。这样「AI 分析这个面板的历史」不会把同名的其它连接的历史混进来。反查失败时退化为不按连接过滤，而不是报错。

**四、AI 的工具调用与 MCP 共用一条取会话路径。** AI DB tools 走 `services/db_tools.rs`，与 MCP 完全相同：拿持久化 `connectionId`、先过连接白名单、再转成运行时会话，且**不释放引用**。因此被 AI 工具访问过的会话同样不会被空闲回收扫掉。

**五、取消是 AI 自己的注册表。** AI 侧用一张按调用 ID 登记的取消令牌表（`ai/cancel.rs`），在开始时登记、结束时注销；取消一个已结束或从未开始的调用不报错，也不产生任何效果。它与长任务机制互不相通，key 也不同（AI 是调用 ID，Job 是 job ID）。AI 调用与仍走兼容旧路径的任务取消状态只在内存；P5 迁移三件套 Job API 使用 AppDb `DesktopJobHost` 的持久 cancel intent，进程重启时保留任务事实但不恢复旧 handler/session。
