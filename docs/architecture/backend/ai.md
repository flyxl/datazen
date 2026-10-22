# AI 模块

> [返回架构总览](../README.md)

### 1.1 概述

AI 模块采用与数据库驱动相同的 **Provider 抽象 + Registry** 模式：`packages/ai-api` 定义统一契约与 active → default → first 的共享 Profile 选择规则，`packages/ai-providers` 提供各厂商实现、共享 HTTP 协议和现有 Provider 的默认 endpoint/protocol 元数据。`packages/feature-core/src/ai/` 持有与运行形态无关的工具循环、安全门、Prompt 解析、Schema 上下文、NL2SQL 请求组装、单轮诊断与分析用例、智能过滤、Schema 文档生成、预算、取消和 editor grant 所有权策略。AI 需要的数据库元数据（表 / 列 / 类型 / schema）同样在共享层：`packages/feature-core/src/services/schema_context/` 里的 `RuntimeSchemaContextSource` 实现 `LogicalSchemaContextSource` 端口，用 `RuntimeConnectionUseCases` 解析会话归属、用注入的 `ResourceExecutionHost` 取 profile / 驱动并经共享执行网关读元数据；物理句柄契约 `PhysicalSchemaContextSource` 只对真正持有句柄的形态开放，共享层用 `PhysicalSchemaContextAdapter` 把两者接起来并持有缓存；桌面只在 `src-tauri/src/services/schema_context_source.rs` 注入这两个跨形态件，不再有自己的 AI 适配层。调用点直接引用 `datazen_ai_api` / `datazen_ai_providers` / `datazen_feature_core::ai`，宿主不再有 AI 门面；Tauri 命令负责 IPC、宿主存储与连接查询、上下文文件读取、事件流和取消接线。

### 1.2 架构分层

```
packages/ai-api/                    # 公共 AI Provider API crate
├── src/
│   ├── lib.rs                      # AI_PROTOCOL_VERSION（当前 1，最低兼容 1）+ re-exports
│   ├── traits.rs                   # AiProvider trait (async_trait, Send+Sync)
│   ├── types.rs                    # AiProviderConfig, ChatMessage, StreamChunk, AiError,
│   │                               # ToolDefinition, ToolCall, ToolResult, ModelInfo,
│   │                               # AiSettingsConfig::selected_profile shared fallback
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
│   ├── metadata.rs                 # 现有 Provider 的默认 endpoint/protocol 元数据
│   └── protocol/                   # 共享 HTTP 协议实现
│       ├── mod.rs                  # ProtocolConfig, timeouts, URL normalization, 端点日志脱敏
│       ├── openai_chat.rs          # OpenAI Chat Completions API (streaming + non-streaming)
│       ├── openai_responses.rs     # OpenAI Responses API (DeepSeek/custom Responses 模式)
│       ├── anthropic.rs            # Anthropic Messages API
│       └── test_support.rs         # 协议层单测共享的 wiremock 夹具

packages/feature-core/src/ai/       # 共享 AI 应用逻辑，不依赖 Tauri
├── context.rs                      # SchemaContextBuilder (DDL 上下文) + context-file prompt block 格式
├── prompt_resolver.rs              # PromptResolver (覆盖、模板与方言提示)
├── safety.rs / tool_loop.rs        # 出境分级、工具调用校验与流式循环
├── schema_pipeline.rs / ctx_yaml.rs # Schema 上下文预算与 .ctx.yaml 解析
├── chat_request.rs / sql_generation.rs # Chat/NL2SQL 请求组装与通用工具定义
├── single_turn.rs                   # SQL 诊断、EXPLAIN、连接摘要格式/诊断与历史分析
├── filter.rs / schema_doc.rs       # 智能过滤与 Schema 文档生成用例
├── budget.rs / cancel.rs           # token 预算与按 owner 隔离的取消
└── ownership.rs                    # AiGrantRegistry / AiGrantService、editor 归属、DB 目标绑定与结果 attribution

packages/feature-core/src/services/schema_context/  # 共享元数据源（表/列/类型/schema）
├── source.rs                       # RuntimeSchemaContextSource：会话→连接映射 + 独立 Runtime 目标上的元数据读取
└── tests.rs                        # 端口契约测试（假 ResourceExecutionHost + 假 runtime）

src-tauri/src/services/             # 桌面适配
└── schema_context_source.rs        # 向共享实现注入 ResourceExecutionHost 与 Runtime 门面

src-tauri/src/commands/ai/           # Tauri IPC 与平台接线
├── generate.rs                     # 配置/会话/历史读取、脱敏、流式交付与共享用例调用
├── chat.rs / grant.rs              # Tauri 事件流、MCP 与 Runtime 工具宿主、editor grant IPC
└── config.rs / prompts.rs          # Provider/Profile 持久化与 Prompt 管理 IPC
```

Chat 和单轮用例接收宿主已选择的 Provider、语言、driver、schema seed 与安全处理后的输入；共享代码组装 prompt、请求、通用工具声明和结构化响应。共享层也格式化宿主已读取的 context 文件内容与连接诊断摘要；文件/连接配置读取及 egress 脱敏仍由 host 负责。连接 Profile、SchemaCache、查询历史、Provider 配置/凭据、MCP 工具发现、Tauri 事件、Runtime 工具调用与请求取消仍由 host 提供；共享层不读取本地存储或选择数据库会话。流式生命周期和工具宿主继续由平台适配器接线。

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
宿主直接引用 `datazen_ai_api` 与 `datazen_ai_providers`，不再经由门面中转：

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
3. 资源文件模板 — `packages/feature-core/resources/prompts/*.md`（编译期嵌入 + 运行时覆盖）
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

**模板存放位置**：`packages/feature-core/resources/prompts/*.md`

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

**解析模块**：`packages/feature-core/src/ai/ctx_yaml.rs`

**前端识别**：`ContextPicker` 中 `.ctx.yaml` 文件使用 `Layers` 图标区分于普通文件。

### 1.11 AI Chat MCP 工具

`ai_chat` 在流式 tool loop 中合并两类工具定义：

1. **DB tools** — `list_tables`、`search_tables`、`query_db` 等，经共享 Runtime resource execution / Driver Command API 执行。
2. **MCP Client tools** — 来自 Settings 中已连接的外部 MCP Server，qualified name 形如 `mcp/{serverId}/{toolName}`。

实现要点（Tauri 适配在 `src-tauri/src/commands/ai/chat.rs`，共享工具定义与循环在 `packages/feature-core/src/ai/`）：

- `mcp_tool_definitions()` 从 `McpClientManager` 拉取已连接 server 的 tool schema，转换为 Provider 的 `ToolDefinition`；MCP 连接发现和调用留在 Tauri adapter。
- `ai/chat_request.rs` 组装通用 Chat 请求与工具定义；`ai/tool_loop.rs` 识别 `mcp/` 前缀并经 `AiToolLoopHost` 调用 MCP，DB tool 与 MCP tool 可在同一轮对话中交替执行。
- 未连接 MCP Server 时行为与原先一致，仅暴露 DB tools。

配置与连接管理见 [`mcp.md` — MCP Client AI 集成](./mcp.md#mcp-client-ai-集成)。

### 1.12 会话、目标与取消

AI 侧与数据库的接触分两类，边界互不重叠：

**一、会话语义（诊断 / NL2SQL / EXPLAIN 分析 / Schema 文档）——只消费已存在的会话，不创建。**

- 这些 IPC 的形参是 `db_session_id`（运行时会话 ID），不是 `connectionId`；`ai_chat`、SQL 生成、诊断、过滤与分析通过 Runtime session adapter 获取 live session/driver，不把运行时 ID 当持久化连接 ID。这条边界由契约测试直接对源码断言：会话语义命令的形参必须含 `db_session_id` 且**不得**含 `connection_id`，且命令体内不得出现「把名为 `connection_id` 的变量喂给 `get_session`」的写法（`commands/ai/ipc_contract_guards.rs`）。
- **`ai_chat` 的 schema 上下文是尽力而为，但降级的方向和直觉相反**。`src-tauri/src/commands/ai/chat.rs` 借用 Runtime live session 后构建 schema 上下文：会话取不到时这一整块被静默跳过，不追加 schema system prompt，而 `attach_db_tools` 保持初值 `true`，因此 DB tools 仍会加入模型请求。会话失效时的实际后果是「模型拿不到 DDL 上下文」，不是「失去模型自己查表的能力」。
- 管线返回 `Err` 时 host 会记录 `warn!` 并把 `attach_db_tools` 置为 `false`。共享 `packages/feature-core/src/ai/schema_pipeline.rs` 会吞掉表名和 pinned schema 的读取错误；仅在 provider 不支持 tools、而 fallback schema 也失败时向 host 返回错误。**对支持 tools 的 provider，这条告警并禁用 DB tools 的分支不可达。**
- 与之对照，配置语义的诊断命令（`ai_diagnose_connection`）不碰会话，连接配置缺失时返回 `CommandError::NotFound`（`src-tauri/src/commands/ai/generate.rs`）。
- **目标不切库**。AI 与其他消费方一样把目标 database / schema 随命令传递；宿主不实现生产环境的切库路径。

**二、配置语义（`ai_diagnose_connection`）——收 `connectionId`，读的是落盘配置**，用于「连不上」的故障排查，与已建立会话无关。契约测试同样钉住这一侧。

**三、唯一的反向映射：`ai_analyze_queries`。** 它收运行时会话 ID，但**不碰会话**——把该 ID 反查成所属的 `connectionId`，再按连接过滤查询历史。这样「AI 分析这个面板的历史」不会把同名的其它连接的历史混进来。反查失败时退化为不按连接过滤，而不是报错。

**四、AI 独立工具调用与 MCP 共用 `DatabaseToolHost`，SQL 执行经共享 Runtime 执行器；显式授权时改走编辑器会话（见第六点）。** 当前桌面 adapter 按持久化 `connectionId` 解析 schema metadata 所需的 live session/driver，并把 SQL 命令交给共享 `ResourceExecutionHost`/Runtime。`editorGrant` 模式跳过独立目标执行，直接借用授权记录中的编辑器 session handle；它不会拥有或关闭该会话。

**五、取消是 AI 自己的注册表，且按 owner 隔离。** AI 侧用一张按「调用 ID + owner 指纹」登记的取消令牌表（`packages/feature-core/src/ai/cancel.rs`，宿主按需直接引用 `datazen_feature_core::ai::cancel::CancellationRegistry`）：

- `register(request_id, owner_fingerprint)` 在开始时登记、`unregister(request_id, owner_fingerprint)` 在结束时注销；owner 指纹由 `OwnerRef` 的稳定身份构成（桌面为本进程 `clientInstanceId`）。
- `cancel(request_id, owner_fingerprint) -> CancelReceipt` 返回三态：`marked`（已登记标志并触发 token）、`alreadyFinished`（未注册：已完成 / 已取消 / 从未开始）、`forbidden`（同名请求属于另一个 owner，**不动它**）。伪造的 `request_id` 不能取消别的客户端的流（CM-53）。
- 两段式取消：先登记标志、再触发 `CancellationToken`；工具循环在轮次开始、流式轮次结束、以及每次工具派发前检查取消。终止 chunk 必须是 `done: true, cancelled: true`——**不得把取消伪装成正常完成**；取消路径不写新历史，只回收本次 AI 自有资源。
- `cancel_owner(owner_fingerprint)` 用于客户端整体离开时只停自己的流（目前由前端释放链路与 grant 撤销配合使用，尚无后端独立调用点）。
- 它与长任务机制互不相通，key 也不同（AI 是调用 ID，Job 是 job ID）。AI 调用取消状态只在内存；P5 迁移三件套 Job API 使用 AppDb `JobHost` 的持久 cancel intent。

**六、显式 editor 会话授权（P6 §14.4 / CM-53）。**

AI 默认**独立操作**：每个 DB 工具调用经共享 `ResourceExecutionHost` → ExecutionGateway 借一次性资源，看不到某个编辑器会话里的临时对象。用户显式授权当前编辑器会话后，工具改在**该编辑器的固定 runtime session** 上执行（同队列、临时对象可见）；授权可撤销，撤销后新调用明确拒绝且编辑器资源仍存活。

- 请求携带 `AiTargetClaim`：`{kind: "independent"}` 或 `{kind: "editorGrant", grantId, claimedClientInstanceId, editorSessionId}`（`packages/feature-core/src/ai/ownership.rs`）。
- 共享授权策略 `packages/feature-core/src/ai/ownership.rs` 提供 `AiGrantRegistry` 与 `AiGrantService`：`issue / verify / verify_claim / touch / revoke / revoke_for_editor_session / purge_expired`，并负责 grant 的 database 目标绑定与结构化 DB tool-result attribution。记录含 `client_instance_id`、`editor_session_id`、`owner_fingerprint`、`connection_id`、`database`、`config_revision`、`deadline_ms` 与编辑器活会话句柄 `SessionHandle`。时间全部由调用方注入（`now_ms`），默认 TTL 15 分钟，活动对话经 `touch` 续期。`verify_claim` 先计算本次拒绝原因，再清理过期记录，因此发现过期 grant 的请求仍返回 `aiGrantExpired`，后续请求才会看到记录已回收。
- IPC：`ai_grant_editor_session`（经 `OwnerIntent::Editor` 归属校验，claimed client 与本机实例不符即拒绝；再核验句柄指向活会话并钉住配置版本）、`ai_revoke_editor_grant`、`ai_touch_editor_grant`。`ai_chat` / `ai_generate_sql` 增参 `targetClaim`。
- 执行分流：独立调用走一次性 Runtime 资源；`editorGrant` 调用先 `verify_claim`，失败按 `AiGrantRejection::code()` 明确报错（`aiGrantUnknown` / `aiGrantClientMismatch` / `aiGrantSessionGone` / `aiGrantExpired` / `aiGrantRevoked`），**禁止静默降级为独立操作**；成功则经 `resource_execution::execute_with_existing_session` 借用编辑器句柄执行，目标与授权记录不一致时在派发前拒绝。
- **借用而不拥有**：grant 执行走 `resource_execution::execute_with_existing_session`——只借用编辑器会话句柄，执行器**不关闭也不回滚**该会话。此前用 `RetainedSession` 包装会在 drop 时关闭编辑器会话，已废弃。
- 工具指定的 `database` 与授权记录不一致时在派发前拒绝；未指定时用授权记录补齐，目标规范化由共享执行器统一完成（因此对象寻址命令如 `get_table_schema` 走同一条 `prepare` 路径）。
- 结果归属：DB 工具结果 JSON 附 `attribution`：`{kind: "independent" | "editorGrant", grantId?, editorSessionId?}`。这是**面向模型与审计的来源标注**，不回流前端 UI；前端消息上的归属元数据由 `targetClaim` 直接构造。

- **归属校验由共享 grant 策略完成**（runtime 的 `get_session` 只按 org/principal/clientInstanceId 过滤，同进程两个 pane 在它眼里没有区别）：`verify_session_ownership` 逐项比对 `SessionView.owner` 的 clientInstanceId 与 editorSessionId、会话所在 connectionId、以及 profile 当前 configRevision，任一不符即具名拒绝；非 editor owner（Job/WorkflowBlock/ClientSession）一律拒绝。Tauri 适配器只读取活 `SessionView` 和 profile revision，再调用该纯策略。单测覆盖「同 client 的另一个 pane」「另一 client」「另一连接」「旧 revision」「非 editor owner」五种拒绝。
- **回收已接线**：`close_session` 在关闭（含关闭被拒）后撤销借用该 db session 的 grant；`platform_update_profile` / `platform_disable_profile` 撤销该连接上的全部 grant（revision 变更使会话失效）；`verify_claim` 与 `issue` 顺带 `purge_expired` 回收过期记录。

**七、验收落点。**

- H 层（已跑）：`packages/feature-core/src/ai/ownership.rs` 的 grant 生命周期与三条撤销轴（按 grant / 按连接 / 按借用的 db session）；`ai/cancel.rs` 的 owner 隔离与三态回执；`ai/tool_loop.rs` 的取消终态（`done && cancelled`，且取消后不再派发工具）；`src-tauri/src/commands/ai/grant.rs` 的六条归属拒绝。
- F 层（已编写、**尚未执行**）：`e2e/specs/ai-grant-lifecycle-journey.ts` 通过真实 IPC 驱动 `open_session → ai_grant_editor_session → touch/revoke → 伪造 client / 他 pane 拒绝 → close_session 后 grant 失效`。它不调用 Provider，因此不依赖 AI 密钥。
  **发布前必做**：`pnpm tauri:build:webdriver` 后执行该 spec（`pnpm e2e`）。本地 DMG 打包失败不算功能阻塞，以应用启动与 WDIO 结果判定；执行结果须回写本节，把「尚未执行」改成实测结论。在这条跑通之前，CM-53 的 F 层不合格，P6 不关闭。

> 当前限制（P6 未关闭）：上述 F 层旅程尚未在 packaged app 上执行，§9 那条旅程目前只有 H 层实测证据；MCP 工具仍走各自会话，`editorGrant` 只覆盖 DB 工具。仅 PG + MySQL 在本轮验证范围内。
