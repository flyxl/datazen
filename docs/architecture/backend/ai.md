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
│   ├── types.rs                    # AiProviderConfig, AiMessage, AiError, ModelInfo
│   └── factory.rs                  # AiProviderFactory + inventory + register_ai_provider!

src-tauri/src/ai/                   # 内置 AI Provider 实现
├── mod.rs                          # 模块组织 + init_ai_providers()
├── openai.rs                       # OpenAI Provider (Chat Completions + Responses API)
├── anthropic.rs                    # Anthropic Provider (Messages API)
├── custom.rs                       # 自定义 OpenAI 兼容 Provider (含远程模型列表获取)
├── registry.rs                     # AiProviderRegistry (动态注册/获取)
├── context.rs                      # SchemaContextBuilder (DDL 上下文, token 预算控制)
└── prompt.rs                       # PromptBuilder (多语言 prompt 模板, 随 i18n 切换)
```

### 1.3 AiProvider Trait

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn provider_type(&self) -> &str;
    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError>;
    async fn complete(&self, messages: &[AiMessage]) -> Result<String, AiError>;
    async fn stream_complete(
        &self,
        messages: &[AiMessage],
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<(), AiError>;
    async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError>;
}
```

### 1.4 AI IPC 命令

| 命令 | 功能 | 流式 |
|------|------|------|
| `ai_generate_sql` | NL2SQL | ✅ Tauri Events |
| `ai_diagnose_error` | SQL 错误诊断 | ❌ |
| `ai_analyze_explain` | EXPLAIN 计划 AI 分析 | ❌ |
| `ai_chat` | AI 对话 | ✅ Tauri Events |
| `ai_parse_filter` | 自然语言筛选解析 | ❌ |
| `ai_generate_schema_doc` | Schema 文档生成 | ❌ |
| `ai_diagnose_connection` | 连接故障排查 | ❌ |
| `ai_analyze_queries` | 查询历史分析 | ❌ |
| `ai_list_skills` | 列出所有 Skills | ❌ |
| `ai_execute_skill` | 执行 Skill | ❌ |
| `ai_save_skill` / `ai_delete_skill` | Skill CRUD | ❌ |

### 1.5 SchemaContextBuilder

构建紧凑 DDL 作为 LLM 上下文：
- 首次仅发送表名列表（减少 token 消耗）
- LLM 需要时再补充详细列/约束信息
- 支持 token 预算控制

### 1.6 PromptBuilder

多语言 prompt 模板管理：
- 模板跟随应用 i18n 设置自动切换语言
- 涵盖 NL2SQL、错误诊断、EXPLAIN 分析、Schema 文档、连接故障排查等场景
