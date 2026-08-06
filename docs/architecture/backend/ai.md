# AI 模块

> [返回架构总览](../README.md)

### 1.1 概述

AI 模块采用与数据库驱动相同的 **Provider 抽象 + Registry** 模式，通过 `packages/ai-api` 公共 crate 定义统一接口，支持多种 LLM Provider。

### 1.2 架构分层

```
packages/ai-api/                    # 公共 AI Provider API crate
├── src/
│   ├── lib.rs                      # AI_PROTOCOL_VERSION + re-exports
│   ├── traits.rs                   # AiProvider trait (async_trait, Send+Sync)
│   ├── types.rs                    # AiProviderConfig, ChatMessage, StreamChunk, AiError,
│   │                               # ToolDefinition, ToolCall, ToolResult, ModelInfo
│   └── factory.rs                  # AiProviderFactory + inventory + register_ai_provider!

src-tauri/src/ai/                   # 内置 AI Provider 实现
├── mod.rs                          # 模块组织 + re-exports
├── openai.rs                       # OpenAI Provider (Chat Completions API)
├── anthropic.rs                    # Anthropic Provider (Messages API)
├── deepseek.rs                     # DeepSeek Provider (Responses API, reasoning 支持)
├── custom.rs                       # 自定义 Provider (三种协议: Chat/Responses/Anthropic 兼容,
│                                   #   远程模型列表获取)
├── registry.rs                     # AiProviderRegistry (动态注册/获取, inventory 插件发现)
├── context.rs                      # SchemaContextBuilder (DDL 上下文, token 预算控制)
├── prompt_resolver.rs              # PromptResolver (资源文件加载 + 用户/驱动覆盖 + 多语言 fallback)
└── protocol/                       # 共享 HTTP 协议实现
    ├── mod.rs                      # ProtocolConfig, timeouts, URL normalization
    ├── openai_chat.rs              # OpenAI Chat Completions API (streaming + non-streaming)
    ├── openai_responses.rs         # OpenAI Responses API (DeepSeek/custom Responses 模式)
    └── anthropic.rs                # Anthropic Messages API
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

多个 Provider 共享底层 HTTP 协议实现，避免重复代码：

| Provider | 协议 | 模块 |
|----------|------|------|
| OpenAI | Chat Completions | `protocol/openai_chat.rs` |
| DeepSeek | Responses API（含 reasoning） | `protocol/openai_responses.rs` |
| Anthropic | Messages API | `protocol/anthropic.rs` |
| Custom | 三选一：Chat / Responses / Anthropic | 对应 protocol 模块 |

### 1.5 AI IPC 命令

| 命令 | 功能 | 流式 |
|------|------|------|
| `ai_generate_sql` | NL2SQL | Tauri Events |
| `ai_diagnose_error` | SQL 错误诊断 | - |
| `ai_analyze_explain` | EXPLAIN 计划 AI 分析 | - |
| `ai_chat` | AI 对话（支持 MCP 工具调用） | Tauri Events |
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

AI Prompt 模板管理（替代原 `PromptBuilder`），支持多层覆盖和多语言：

**解析优先级**：
1. 用户覆盖 — `prompt_overrides.json`（通过设置界面修改，按场景存储）
2. 驱动覆盖 — `DatabaseDriver::prompt_overrides()`（运行时按连接的驱动类型动态应用）
3. 资源文件模板 — `resources/prompts/{lang}/*.txt`（运行时按语言加载）
4. 编译时嵌入 — `embedded_default()`（仅 fallback 到英文模板）

**语言 Fallback 链**：请求语言 → `zh-CN` → `en` → 编译时英文嵌入

**Prompt 场景**（`PromptScenario`枚举）：

| 场景 | 模板文件 | 用途 |
|------|---------|------|
| `Nl2Sql` | `nl2sql.txt` | 自然语言转 SQL |
| `Diagnose` | `diagnose.txt` | SQL 错误诊断 |
| `NlFilter` | `nl_filter.txt` | 自然语言表筛选 |
| `SchemaDocSelectTables` | `schema_doc_select_tables.txt` | 选择 Schema 文档表 |
| `SchemaDoc` | `schema_doc.txt` | 生成 Schema 文档 |
| `ConnectionDiagnose` | `connection_diagnose.txt` | 连接故障排查 |
| `QuerySummary` | `query_summary.txt` | 查询历史分析 |
| `ExplainAnalysis` | `explain_analysis.txt` | EXPLAIN 计划分析 |
| `Chat` | `chat.txt` | AI 侧边栏对话 |
| `WorkflowGenerate` | `workflow_generate.txt` | AI 辅助 Workflow 生成 |

**模板存放位置**：`src-tauri/resources/prompts/{en,zh-CN}/`

**运行时加载**：启动时异步加载（`tokio::spawn`），切换语言时重新加载。

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
