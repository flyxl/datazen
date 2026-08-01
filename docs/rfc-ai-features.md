# RFC: DataZen AI 功能设计

> **状态**: Draft  
> **日期**: 2026-08-01  
> **作者**: DataZen Team

## 摘要

本文档描述 DataZen 的 AI 功能规划与完整实现方案。AI 模块采用与数据库驱动相同的**编译时插件架构**（`inventory` + 工厂注册），通过 `packages/ai-api` 公共 crate 定义 Provider trait，支持 OpenAI、Anthropic、Ollama 等多种 LLM 供应商。功能涵盖自然语言转 SQL、错误诊断、EXPLAIN 解读、智能筛选、AI Chat 助手等。

## 动机

DataZen 作为数据库管理工具，用户的核心工作流集中在**表数据浏览/编辑（~40%）**和 **SQL 查询编写/执行（~35%）**。当前存在以下痛点：

1. **SQL 编写门槛** — 不熟悉 SQL 的用户（数据分析师、产品经理）难以独立完成查询
2. **错误排查低效** — 执行失败时只显示原始错误字符串，缺乏可操作的修复指引
3. **查询优化困难** — 后端已实现 `get_explain`，但前端无 UI 接入，且 EXPLAIN 输出对非 DBA 难以理解
4. **筛选功能缺失** — 后端支持筛选但前端无创建筛选条件的 UI，用户只能通过手写 SQL 筛选
5. **Schema 理解成本** — 面对陌生数据库时，需逐表查看结构，缺乏全局理解辅助

## 设计目标

1. **架构一致性** — 遵循现有的驱动插件模式（`driver-api` → `ai-api`），使用相同的 `inventory` + Factory 注册机制
2. **多 Provider 支持** — 抽象层支持 OpenAI、Anthropic (Claude)、Ollama、任意 OpenAI 兼容 API
3. **隐私优先** — 支持 Ollama 等本地模型，默认仅发送 Schema 元数据
4. **非侵入** — AI 功能为可选增强，所有现有功能不依赖 AI 即可正常使用
5. **流式体验** — LLM 输出通过 Tauri Events 流式推送到前端

---

## 第一部分：后端架构设计

### 1.1 Workspace 扩展

在 Cargo workspace 中新增 `packages/ai-api` crate，与 `packages/driver-api` 并列：

```toml
# Cargo.toml (根)
[workspace]
resolver = "2"
members = [
    "src-tauri",
    "packages/driver-api",
    "packages/ai-api",         # 新增
]
```

### 1.2 `packages/ai-api` — 公共 AI Provider API

对标 `packages/driver-api` 的结构，为 AI Provider 提供统一接口：

```
packages/ai-api/
├── Cargo.toml
└── src/
    ├── lib.rs          # 版本常量 + re-export
    ├── traits.rs       # AiProvider trait
    ├── types.rs        # 共享类型 + AiError
    └── factory.rs      # AiProviderFactory + inventory + register_ai_provider!
```

#### `Cargo.toml`

```toml
[package]
name = "datazen-ai-api"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
inventory = "0.3"
tokio = { version = "1", features = ["sync"] }
```

#### `lib.rs`

```rust
pub const AI_PROTOCOL_VERSION: u32 = 1;

mod traits;
mod types;
mod factory;

pub use traits::*;
pub use types::*;
pub use factory::*;

// Re-export for plugin authors
pub use async_trait::async_trait;
pub use inventory;
```

#### `traits.rs` — AiProvider Trait

对标 `DatabaseDriver` trait 的设计模式：`Send + Sync`、`async_trait`、有默认实现的可选方法。

```rust
use crate::types::*;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// AI Provider 核心 trait
///
/// 对标 DatabaseDriver trait，每个 LLM 供应商实现一次。
/// Provider 是单例，通过 AiProviderRegistry 管理；
/// 内部可持有 HTTP client pool（类似 DatabaseDriver 持有 DB connection pool）。
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Provider 类型标识
    fn provider_type(&self) -> AiProviderType;

    /// 人类可读的 Provider 名称
    fn display_name(&self) -> &str;

    /// 该 Provider 支持的模型列表
    fn available_models(&self) -> Vec<ModelInfo>;

    /// 默认模型
    fn default_model(&self) -> &str;

    /// 是否支持流式输出
    fn supports_streaming(&self) -> bool { true }

    /// 是否支持 Function Calling / Tool Use
    fn supports_tools(&self) -> bool { false }

    /// 验证配置（API Key、Endpoint 等）
    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError>;

    /// 初始化 Provider（传入配置，创建 HTTP client 等）
    /// 类似 DatabaseDriver::connect，但 Provider 级别只需一次
    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError>;

    /// 非流式补全请求
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError>;

    /// 流式补全请求
    /// 通过 mpsc::Sender 推送 chunk，完成后 sender 自动 drop
    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        // 默认实现：非流式调用后一次性发送
        let response = self.complete(request).await?;
        let _ = sender.send(Ok(StreamChunk {
            content: response.content.clone(),
            done: true,
            usage: Some(response.usage.clone()),
        })).await;
        Ok(())
    }

    /// 取消正在进行的请求
    async fn cancel(&self, request_id: &str) -> Result<(), AiError> {
        Err(AiError::NotSupported("cancel".into()))
    }
}
```

#### `types.rs` — 共享类型

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Provider 标识 ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderType {
    OpenAi,
    Anthropic,
    Ollama,
    Custom,
}

// ─── Provider 配置 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfig {
    pub provider_type: AiProviderType,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,      // 自定义端点 (Ollama: http://localhost:11434)
    pub model: String,                 // 选用的模型 ID
    pub extra: serde_json::Value,      // Provider 特有配置（透传）
}

// ─── 模型信息 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,                    // e.g. "gpt-4o", "claude-sonnet-4-20250514"
    pub display_name: String,          // e.g. "GPT-4o", "Claude Sonnet 4"
    pub context_window: u32,           // Token 上限
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

// ─── 消息 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

// ─── 请求 & 响应 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRequest {
    pub request_id: String,            // 用于取消和关联流式事件
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stop: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResponse {
    pub request_id: String,
    pub content: String,
    pub usage: TokenUsage,
    pub model: String,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunk {
    pub content: String,
    pub done: bool,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ─── 错误类型 ───
// 对标 DriverError，使用 thiserror

#[derive(Debug, Error)]
pub enum AiError {
    #[error("Provider not configured: {0}")]
    NotConfigured(String),

    #[error("Invalid API key")]
    InvalidApiKey,

    #[error("API request failed: {0}")]
    RequestFailed(String),

    #[error("Rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Context length exceeded: {0} tokens (max {1})")]
    ContextLengthExceeded(u32, u32),

    #[error("Provider not available: {0}")]
    ProviderNotAvailable(String),

    #[error("Feature not supported: {0}")]
    NotSupported(String),

    #[error("Request cancelled")]
    Cancelled,

    #[error("Timeout after {0}s")]
    Timeout(u64),

    #[error("Internal error: {0}")]
    Internal(String),
}

// ─── AI 功能专用类型 ───

/// NL2SQL 请求上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlGenerationContext {
    pub database_type: String,         // "PostgreSQL", "MySQL" 等
    pub database_version: Option<String>,
    pub schema_ddl: String,            // 精简的 DDL 摘要
    pub current_table: Option<String>, // 当前选中的表
    pub recent_queries: Vec<String>,   // 最近执行的 SQL（最多 3 条）
}

/// 错误诊断结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisResult {
    pub explanation: String,           // 错误原因解释
    pub suggested_sql: Option<String>, // 修正后的 SQL
    pub changes: Vec<String>,          // 修改说明
}

/// EXPLAIN 分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainAnalysis {
    pub summary: String,               // 总结
    pub bottlenecks: Vec<Bottleneck>,  // 性能瓶颈
    pub suggestions: Vec<Suggestion>,  // 优化建议
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bottleneck {
    pub node: String,
    pub description: String,
    pub severity: String,              // "high" | "medium" | "low"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub description: String,
    pub sql: Option<String>,           // 建议的 SQL（如 CREATE INDEX）
    pub impact: String,                // 预期效果描述
}
```

#### `factory.rs` — Provider 工厂 + 注册宏

完全对标 `packages/driver-api/src/factory.rs`：

```rust
use crate::{AiProvider, AI_PROTOCOL_VERSION};
use std::sync::Arc;

/// AI Provider 工厂 trait
/// 对标 DatabaseDriverFactory
pub trait AiProviderFactory: Send + Sync + 'static {
    fn create(&self) -> Arc<dyn AiProvider>;
    fn provider_id(&self) -> &'static str;

    fn protocol_version(&self) -> u32 {
        AI_PROTOCOL_VERSION
    }
}

inventory::collect!(&'static dyn AiProviderFactory);

/// 注册宏，对标 register_driver!
#[macro_export]
macro_rules! register_ai_provider {
    ($factory:expr) => {
        $crate::inventory::submit!(
            $factory as &'static dyn $crate::AiProviderFactory
        );
    };
}

/// 遍历所有已注册的 AI Provider 工厂
pub fn iter_ai_provider_factories() -> inventory::iter<&'static dyn AiProviderFactory> {
    inventory::iter::<&'static dyn AiProviderFactory>
}
```

### 1.3 内置 Provider 实现 (`src-tauri/src/ai/`)

对标 `src-tauri/src/db/` 中的内置驱动（postgres.rs, mysql.rs 等），在 host crate 中实现内置 Provider：

```
src-tauri/src/ai/
├── mod.rs                   # re-export ai-api + 内置 Provider 声明
├── registry.rs              # AiProviderRegistry + init_ai_providers()
├── context.rs               # Schema 上下文构建器
├── prompt.rs                # Prompt 模板管理
├── openai.rs                # OpenAI Provider (兼容 API)
├── anthropic.rs             # Anthropic (Claude) Provider
└── ollama.rs                # Ollama Provider
```

#### `mod.rs`

```rust
// re-export ai-api，类似 db/mod.rs re-export driver-api
pub use datazen_ai_api::*;

pub mod registry;
pub mod context;
pub mod prompt;

mod openai;
mod anthropic;
mod ollama;

pub use openai::OpenAiProvider;
pub use anthropic::AnthropicProvider;
pub use ollama::OllamaProvider;
```

#### `registry.rs` — AiProviderRegistry

对标 `db/registry.rs` 中的 `DriverRegistry`：

```rust
use crate::ai::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

pub struct AiProviderRegistry {
    providers: Arc<RwLock<HashMap<AiProviderType, Arc<dyn AiProvider>>>>,
}

impl AiProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, provider: Arc<dyn AiProvider>) {
        let provider_type = provider.provider_type();
        info!(
            "Registering AI provider: {:?} ({})",
            provider_type,
            provider.display_name()
        );
        self.providers.write().await.insert(provider_type, provider);
    }

    pub async fn get(&self, provider_type: &AiProviderType) -> Option<Arc<dyn AiProvider>> {
        self.providers.read().await.get(provider_type).cloned()
    }

    pub async fn list_providers(&self) -> Vec<(AiProviderType, String)> {
        self.providers
            .read()
            .await
            .iter()
            .map(|(t, p)| (*t, p.display_name().to_string()))
            .collect()
    }
}

/// 初始化所有 AI Provider，对标 init_drivers()
pub async fn init_ai_providers() -> AiProviderRegistry {
    let registry = AiProviderRegistry::new();

    // 内置 Provider：显式注册
    registry.register(Arc::new(OpenAiProvider::new())).await;
    registry.register(Arc::new(AnthropicProvider::new())).await;
    registry.register(Arc::new(OllamaProvider::new())).await;

    // 插件 Provider：通过 inventory 自动发现
    for factory in iter_ai_provider_factories() {
        if factory.protocol_version() != AI_PROTOCOL_VERSION {
            warn!(
                "Skipping AI provider '{}': protocol version {} != {}",
                factory.provider_id(),
                factory.protocol_version(),
                AI_PROTOCOL_VERSION
            );
            continue;
        }
        info!("Discovered AI provider plugin: {}", factory.provider_id());
        registry.register(factory.create()).await;
    }

    registry
}
```

#### `openai.rs` — OpenAI Provider 实现

对标 `db/postgres.rs` 的结构模式（单例 + 内部 client pool）：

```rust
use crate::ai::*;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

pub struct OpenAiProvider {
    client: RwLock<Option<Client>>,
    config: RwLock<Option<AiProviderConfig>>,
}

impl OpenAiProvider {
    pub fn new() -> Self {
        Self {
            client: RwLock::new(None),
            config: RwLock::new(None),
        }
    }

    fn base_url(config: &AiProviderConfig) -> String {
        config
            .endpoint
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::OpenAi
    }

    fn display_name(&self) -> &str {
        "OpenAI"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: "gpt-4o-mini".into(),
                display_name: "GPT-4o Mini".into(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            },
            // ... 更多模型
        ]
    }

    fn default_model(&self) -> &str {
        "gpt-4o"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        if config.api_key.is_none() || config.api_key.as_ref().unwrap().is_empty() {
            return Err(AiError::InvalidApiKey);
        }
        // 发送一个简单请求验证 Key 有效性
        let client = Client::new();
        let base_url = Self::base_url(config);
        let resp = client
            .get(format!("{}/models", base_url))
            .bearer_auth(config.api_key.as_ref().unwrap())
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        if resp.status() == 401 {
            return Err(AiError::InvalidApiKey);
        }
        if !resp.status().is_success() {
            return Err(AiError::RequestFailed(format!("HTTP {}", resp.status())));
        }
        Ok(())
    }

    async fn initialize(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AiError::Internal(e.to_string()))?;
        *self.client.write().await = Some(client);
        *self.config.write().await = Some(config.clone());
        Ok(())
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, AiError> {
        let client = self.client.read().await;
        let client = client.as_ref().ok_or(AiError::NotConfigured("OpenAI".into()))?;
        let config = self.config.read().await;
        let config = config.as_ref().ok_or(AiError::NotConfigured("OpenAI".into()))?;
        let base_url = Self::base_url(config);

        let body = OpenAiRequest {
            model: request.model.clone(),
            messages: request.messages.iter().map(|m| OpenAiMessage {
                role: match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                }.to_string(),
                content: m.content.clone(),
            }).collect(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
        };

        let resp = client
            .post(format!("{}/chat/completions", base_url))
            .bearer_auth(config.api_key.as_ref().unwrap())
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        if resp.status() == 429 {
            return Err(AiError::RateLimited { retry_after_secs: 60 });
        }
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AiError::RequestFailed(text));
        }

        let api_resp: OpenAiResponse = resp
            .json()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        let choice = api_resp.choices.first()
            .ok_or(AiError::RequestFailed("No choices returned".into()))?;

        Ok(CompletionResponse {
            request_id: request.request_id.clone(),
            content: choice.message.content.clone(),
            model: api_resp.model,
            finish_reason: choice.finish_reason.clone(),
            usage: TokenUsage {
                prompt_tokens: api_resp.usage.prompt_tokens,
                completion_tokens: api_resp.usage.completion_tokens,
                total_tokens: api_resp.usage.total_tokens,
            },
        })
    }

    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        sender: mpsc::Sender<Result<StreamChunk, AiError>>,
    ) -> Result<(), AiError> {
        let client = self.client.read().await;
        let client = client.as_ref().ok_or(AiError::NotConfigured("OpenAI".into()))?;
        let config = self.config.read().await;
        let config = config.as_ref().ok_or(AiError::NotConfigured("OpenAI".into()))?;
        let base_url = Self::base_url(config);

        let body = OpenAiRequest {
            model: request.model.clone(),
            messages: request.messages.iter().map(|m| OpenAiMessage {
                role: match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                }.to_string(),
                content: m.content.clone(),
            }).collect(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: true,
        };

        let mut resp = client
            .post(format!("{}/chat/completions", base_url))
            .bearer_auth(config.api_key.as_ref().unwrap())
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?;

        // 解析 SSE 流
        while let Some(chunk) = resp.chunk().await
            .map_err(|e| AiError::RequestFailed(e.to_string()))?
        {
            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        let _ = sender.send(Ok(StreamChunk {
                            content: String::new(),
                            done: true,
                            usage: None,
                        })).await;
                        return Ok(());
                    }
                    if let Ok(chunk_resp) = serde_json::from_str::<OpenAiStreamChunk>(data) {
                        if let Some(delta) = chunk_resp.choices.first().and_then(|c| c.delta.content.as_ref()) {
                            let _ = sender.send(Ok(StreamChunk {
                                content: delta.clone(),
                                done: false,
                                usage: None,
                            })).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// ─── OpenAI API 内部类型 ───

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiChoiceMessage {
    content: String,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiDelta,
}

#[derive(Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
}
```

#### `anthropic.rs` — Claude Provider

与 OpenAI Provider 结构相同，适配 Anthropic Messages API（`/v1/messages`）：

```rust
#[async_trait]
impl AiProvider for AnthropicProvider {
    fn provider_type(&self) -> AiProviderType { AiProviderType::Anthropic }
    fn display_name(&self) -> &str { "Anthropic (Claude)" }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-sonnet-4-20250514".into(),
                display_name: "Claude Sonnet 4".into(),
                context_window: 200_000,
                supports_streaming: true,
                supports_tools: true,
            },
            // ... claude-haiku, opus 等
        ]
    }

    // ... 实现 complete / stream_complete
    // Anthropic API 差异：
    //   - system message 单独传（非 messages 数组）
    //   - 使用 x-api-key header（非 Bearer token）
    //   - 流式使用 SSE 格式但事件结构不同
}
```

#### `ollama.rs` — Ollama 本地模型 Provider

```rust
#[async_trait]
impl AiProvider for OllamaProvider {
    fn provider_type(&self) -> AiProviderType { AiProviderType::Ollama }
    fn display_name(&self) -> &str { "Ollama (本地)" }

    fn available_models(&self) -> Vec<ModelInfo> {
        // 动态获取：GET /api/tags → 返回本地已拉取的模型列表
        // 初始化时缓存，提供 refresh 能力
        vec![]  // 动态填充
    }

    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError> {
        // 检查 Ollama 服务是否运行
        let endpoint = config.endpoint.clone()
            .unwrap_or_else(|| "http://localhost:11434".into());
        let client = Client::new();
        client.get(format!("{}/api/tags", endpoint))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|_| AiError::ProviderNotAvailable(
                "Ollama 服务未运行，请启动 Ollama".into()
            ))?;
        Ok(())
    }

    // Ollama API: POST /api/chat
    // 兼容 OpenAI 格式但端点不同
    // 流式使用 NDJSON（每行一个 JSON 对象）
}
```

### 1.4 Schema 上下文构建器 (`context.rs`)

复用现有的 `SchemaCache` + `DriverRegistry` 数据，构建发送给 LLM 的精简 DDL 摘要：

```rust
use crate::db::{DriverRegistry, SchemaCache};
use crate::ai::SqlGenerationContext;
use datazen_driver_api::{ConnectionHandle, DatabaseType};
use std::sync::Arc;

pub struct SchemaContextBuilder {
    schema_cache: Arc<SchemaCache>,
    driver_registry: Arc<DriverRegistry>,
}

impl SchemaContextBuilder {
    pub fn new(
        schema_cache: Arc<SchemaCache>,
        driver_registry: Arc<DriverRegistry>,
    ) -> Self {
        Self { schema_cache, driver_registry }
    }

    /// 构建 SQL 生成上下文
    /// max_tokens_budget: 为 Schema 分配的最大 token 预算
    pub async fn build_sql_context(
        &self,
        db_type: &DatabaseType,
        handle: &ConnectionHandle,
        current_table: Option<&str>,
        recent_queries: &[String],
        max_tokens_budget: usize,
    ) -> Result<SqlGenerationContext, String> {
        let driver = self.driver_registry.get(db_type).await
            .ok_or("Driver not found")?;

        // 从 SchemaCache 获取已缓存的表/列信息
        let tables = self.schema_cache
            .get_tables(db_type, handle, None)
            .await
            .map_err(|e| e.to_string())?;

        let mut ddl_parts = Vec::new();
        let mut token_estimate = 0;

        // 优先包含当前选中的表
        let prioritized_tables = self.prioritize_tables(&tables, current_table);

        for table in prioritized_tables {
            let schema = self.schema_cache
                .get_table_schema(db_type, handle, &table.name, None)
                .await;

            if let Ok(schema) = schema {
                let ddl_line = self.format_compact_ddl(&table.name, &schema);
                let line_tokens = ddl_line.len() / 4; // 粗略估计
                if token_estimate + line_tokens > max_tokens_budget {
                    break;
                }
                token_estimate += line_tokens;
                ddl_parts.push(ddl_line);
            }
        }

        Ok(SqlGenerationContext {
            database_type: format!("{:?}", db_type),
            database_version: None,
            schema_ddl: ddl_parts.join("\n"),
            current_table: current_table.map(String::from),
            recent_queries: recent_queries.to_vec(),
        })
    }

    /// 格式化紧凑 DDL（节省 Token）
    fn format_compact_ddl(&self, table_name: &str, schema: &TableSchema) -> String {
        let columns: Vec<String> = schema.columns.iter().map(|col| {
            let mut parts = vec![col.name.clone(), col.data_type.clone()];
            if col.is_primary_key { parts.push("PK".into()); }
            if col.is_nullable == Some(false) { parts.push("NOT NULL".into()); }
            if let Some(ref fk) = col.foreign_key {
                parts.push(format!("FK->{}.{}", fk.referenced_table, fk.referenced_column));
            }
            if col.is_unique == Some(true) { parts.push("UNIQUE".into()); }
            parts.join(" ")
        }).collect();

        format!("  {} ({})", table_name, columns.join(", "))
    }

    /// 将当前选中的表排到最前
    fn prioritize_tables(
        &self,
        tables: &[TableInfo],
        current_table: Option<&str>,
    ) -> Vec<TableInfo> {
        let mut result = tables.to_vec();
        if let Some(current) = current_table {
            result.sort_by_key(|t| if t.name == current { 0 } else { 1 });
        }
        result
    }
}
```

### 1.5 Prompt 模板管理 (`prompt.rs`)

```rust
use crate::ai::{ChatMessage, MessageRole, SqlGenerationContext, DiagnosisResult};

pub struct PromptBuilder;

impl PromptBuilder {
    /// NL2SQL 的 System Prompt
    pub fn nl2sql_system(context: &SqlGenerationContext) -> ChatMessage {
        ChatMessage {
            role: MessageRole::System,
            content: format!(
r#"You are a SQL expert. Generate executable SQL based on the user's natural language description and the database schema below.

Database: {db_type}{version}
Schema:
{schema}

Rules:
- Return ONLY executable SQL, no explanations
- Use the correct dialect for {db_type}
- Use table aliases for readability
- If the description is ambiguous, use the most common reasonable interpretation
- Reference only tables and columns that exist in the schema"#,
                db_type = context.database_type,
                version = context.database_version.as_deref().map(|v| format!(" {}", v)).unwrap_or_default(),
                schema = context.schema_ddl,
            ),
        }
    }

    /// 错误诊断的 System Prompt
    pub fn diagnose_system(
        db_type: &str,
        schema_ddl: &str,
    ) -> ChatMessage {
        ChatMessage {
            role: MessageRole::System,
            content: format!(
r#"You are a database error diagnostician. Analyze SQL errors and provide fixes.

Database: {db_type}
Schema:
{schema_ddl}

Respond in this exact JSON format:
{{
  "explanation": "Clear explanation of why the error occurred",
  "suggestedSql": "Corrected SQL query (or null if unfixable)",
  "changes": ["Description of each change made"]
}}"#,
            ),
        }
    }

    /// EXPLAIN 分析的 System Prompt
    pub fn explain_analysis_system(db_type: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::System,
            content: format!(
r#"You are a database performance expert. Analyze the EXPLAIN output and identify bottlenecks.

Database: {db_type}

Respond in this exact JSON format:
{{
  "summary": "One-line performance summary",
  "bottlenecks": [
    {{"node": "Node name", "description": "Why it's slow", "severity": "high|medium|low"}}
  ],
  "suggestions": [
    {{"description": "What to do", "sql": "CREATE INDEX ... (or null)", "impact": "Expected improvement"}}
  ]
}}"#,
            ),
        }
    }
}
```

### 1.6 AppState 扩展

在现有 `AppState` 中添加 AI 相关字段：

```rust
// src-tauri/src/commands/mod.rs
pub struct AppState {
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub store: Arc<Store>,
    pub schema_cache: Arc<SchemaCache>,
    pub sync_adapters: Arc<SyncAdapterRegistry>,
    // ── 新增 ──
    pub ai_registry: Arc<AiProviderRegistry>,
    pub schema_context_builder: Arc<SchemaContextBuilder>,
}
```

在 `lib.rs` 的初始化中：

```rust
// src-tauri/src/lib.rs setup 函数中
let ai_registry = Arc::new(init_ai_providers().await);

// 从 Store 加载已保存的 AI 配置，初始化对应 Provider
if let Ok(ai_config) = store.get_ai_config().await {
    if let Some(provider) = ai_registry.get(&ai_config.provider_type).await {
        let _ = provider.initialize(&ai_config).await;
    }
}

let schema_context_builder = Arc::new(SchemaContextBuilder::new(
    schema_cache.clone(),
    registry.clone(),
));

let app_state = AppState {
    driver_registry: registry,
    connection_manager,
    store,
    schema_cache,
    sync_adapters,
    ai_registry,
    schema_context_builder,
};
```

### 1.7 Tauri IPC 命令 (`src-tauri/src/commands/ai.rs`)

遵循现有命令的模式：`State<'_, AppState>` + `Result<T, String>` + `log_err`：

```rust
use crate::commands::{AppState, log_err};
use crate::ai::*;
use tauri::{AppHandle, State};
use tokio::sync::mpsc;
use uuid::Uuid;

// ─── Provider 管理 ───

#[tauri::command]
pub async fn ai_get_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderListItem>, String> {
    let providers = state.ai_registry.list_providers().await;
    let result = providers.into_iter().map(|(pt, name)| {
        let provider = state.ai_registry.get(&pt);
        // 需要 block_on 或提前收集
        ProviderListItem {
            provider_type: pt,
            display_name: name,
            // models 等通过单独命令获取
        }
    }).collect();
    Ok(result)
}

#[tauri::command]
pub async fn ai_get_models(
    state: State<'_, AppState>,
    provider_type: AiProviderType,
) -> Result<Vec<ModelInfo>, String> {
    let provider = state.ai_registry.get(&provider_type).await
        .ok_or_else(|| format!("Provider {:?} not found", provider_type))?;
    Ok(provider.available_models())
}

#[tauri::command]
pub async fn ai_validate_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), String> {
    let provider = state.ai_registry.get(&config.provider_type).await
        .ok_or_else(|| format!("Provider {:?} not found", config.provider_type))?;
    provider.validate_config(&config).await
        .map_err(|e| log_err("ai_validate_config", &e))
}

#[tauri::command]
pub async fn ai_save_config(
    state: State<'_, AppState>,
    config: AiProviderConfig,
) -> Result<(), String> {
    // 初始化 Provider
    let provider = state.ai_registry.get(&config.provider_type).await
        .ok_or_else(|| format!("Provider {:?} not found", config.provider_type))?;
    provider.initialize(&config).await
        .map_err(|e| log_err("ai_save_config", &e))?;

    // 持久化到 Store（API Key 加密存储）
    state.store.save_ai_config(&config).await
        .map_err(|e| log_err("ai_save_config", &e))
}

#[tauri::command]
pub async fn ai_get_config(
    state: State<'_, AppState>,
) -> Result<Option<AiProviderConfig>, String> {
    state.store.get_ai_config().await
        .map_err(|e| log_err("ai_get_config", &e))
}

// ─── NL2SQL ───

#[tauri::command]
pub async fn ai_generate_sql(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    connection_id: String,
    natural_language: String,
    current_table: Option<String>,
    recent_queries: Vec<String>,
) -> Result<String, String> {
    let ai_config = state.store.get_ai_config().await
        .map_err(|e| log_err("ai_generate_sql", &e))?
        .ok_or("AI 未配置")?;

    let provider = state.ai_registry.get(&ai_config.provider_type).await
        .ok_or("Provider not available")?;

    // 获取连接信息以构建 Schema 上下文
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| log_err("ai_generate_sql", &e))?;

    let db_type = driver.driver_type();
    let context = state.schema_context_builder
        .build_sql_context(
            &db_type,
            &handle,
            current_table.as_deref(),
            &recent_queries,
            4000, // schema token budget
        ).await
        .map_err(|e| log_err("ai_generate_sql", &e))?;

    let request_id = Uuid::new_v4().to_string();
    let system_msg = PromptBuilder::nl2sql_system(&context);
    let user_msg = ChatMessage {
        role: MessageRole::User,
        content: natural_language,
    };

    // 流式输出
    let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);
    let request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: vec![system_msg, user_msg],
        temperature: Some(0.0), // SQL 生成使用低温度
        max_tokens: Some(2000),
        stop: None,
    };

    let handle_clone = app_handle.clone();
    let req_id_clone = request_id.clone();

    // 后台任务处理流式响应
    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    let _ = handle_clone.emit("ai:stream-chunk", serde_json::json!({
                        "requestId": req_id_clone,
                        "content": chunk.content,
                        "done": chunk.done,
                        "usage": chunk.usage,
                    }));
                }
                Err(e) => {
                    let _ = handle_clone.emit("ai:stream-error", serde_json::json!({
                        "requestId": req_id_clone,
                        "error": e.to_string(),
                    }));
                }
            }
        }
    });

    provider.stream_complete(&request, tx).await
        .map_err(|e| log_err("ai_generate_sql", &e))?;

    Ok(request_id)
}

// ─── 错误诊断 ───

#[tauri::command]
pub async fn ai_diagnose_error(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
    error_message: String,
) -> Result<DiagnosisResult, String> {
    let ai_config = state.store.get_ai_config().await
        .map_err(|e| log_err("ai_diagnose_error", &e))?
        .ok_or("AI 未配置")?;

    let provider = state.ai_registry.get(&ai_config.provider_type).await
        .ok_or("Provider not available")?;

    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| log_err("ai_diagnose_error", &e))?;

    let db_type = driver.driver_type();
    let context = state.schema_context_builder
        .build_sql_context(&db_type, &handle, None, &[], 3000)
        .await
        .map_err(|e| log_err("ai_diagnose_error", &e))?;

    let request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::diagnose_system(&context.database_type, &context.schema_ddl),
            ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "SQL:\n```\n{}\n```\n\nError:\n{}",
                    sql, error_message
                ),
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(1500),
        stop: None,
    };

    let response = provider.complete(&request).await
        .map_err(|e| log_err("ai_diagnose_error", &e))?;

    // 解析 JSON 响应
    serde_json::from_str::<DiagnosisResult>(&response.content)
        .map_err(|e| log_err("ai_diagnose_error", &e))
}

// ─── EXPLAIN 分析 ───

#[tauri::command]
pub async fn ai_analyze_explain(
    state: State<'_, AppState>,
    connection_id: String,
    explain_output: String,
    original_sql: String,
) -> Result<ExplainAnalysis, String> {
    let ai_config = state.store.get_ai_config().await
        .map_err(|e| log_err("ai_analyze_explain", &e))?
        .ok_or("AI 未配置")?;

    let provider = state.ai_registry.get(&ai_config.provider_type).await
        .ok_or("Provider not available")?;

    let (driver, _) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| log_err("ai_analyze_explain", &e))?;

    let db_type = format!("{:?}", driver.driver_type());

    let request = CompletionRequest {
        request_id: Uuid::new_v4().to_string(),
        model: ai_config.model.clone(),
        messages: vec![
            PromptBuilder::explain_analysis_system(&db_type),
            ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "SQL:\n```\n{}\n```\n\nEXPLAIN output:\n```\n{}\n```",
                    original_sql, explain_output
                ),
            },
        ],
        temperature: Some(0.0),
        max_tokens: Some(2000),
        stop: None,
    };

    let response = provider.complete(&request).await
        .map_err(|e| log_err("ai_analyze_explain", &e))?;

    serde_json::from_str::<ExplainAnalysis>(&response.content)
        .map_err(|e| log_err("ai_analyze_explain", &e))
}

// ─── AI Chat（通用对话） ───

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    connection_id: Option<String>,
    messages: Vec<ChatMessage>,
    include_schema: bool,
) -> Result<String, String> {
    let ai_config = state.store.get_ai_config().await
        .map_err(|e| log_err("ai_chat", &e))?
        .ok_or("AI 未配置")?;

    let provider = state.ai_registry.get(&ai_config.provider_type).await
        .ok_or("Provider not available")?;

    let mut full_messages = Vec::new();

    // 构建 system prompt，包含可选的 schema 上下文
    if include_schema {
        if let Some(ref conn_id) = connection_id {
            let (driver, handle) = state.connection_manager
                .get_connection(conn_id).await
                .map_err(|e| log_err("ai_chat", &e))?;

            let context = state.schema_context_builder
                .build_sql_context(
                    &driver.driver_type(),
                    &handle,
                    None,
                    &[],
                    4000,
                ).await
                .unwrap_or_default();

            full_messages.push(ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "You are a database assistant. The user is connected to a {} database.\n\nSchema:\n{}",
                    context.database_type, context.schema_ddl
                ),
            });
        }
    }

    full_messages.extend(messages);

    let request_id = Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::channel::<Result<StreamChunk, AiError>>(32);

    let request = CompletionRequest {
        request_id: request_id.clone(),
        model: ai_config.model.clone(),
        messages: full_messages,
        temperature: Some(0.7),
        max_tokens: Some(4000),
        stop: None,
    };

    let handle_clone = app_handle.clone();
    let req_id_clone = request_id.clone();

    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    let _ = handle_clone.emit("ai:stream-chunk", serde_json::json!({
                        "requestId": req_id_clone,
                        "content": chunk.content,
                        "done": chunk.done,
                        "usage": chunk.usage,
                    }));
                }
                Err(e) => {
                    let _ = handle_clone.emit("ai:stream-error", serde_json::json!({
                        "requestId": req_id_clone,
                        "error": e.to_string(),
                    }));
                }
            }
        }
    });

    provider.stream_complete(&request, tx).await
        .map_err(|e| log_err("ai_chat", &e))?;

    Ok(request_id)
}
```

命令注册（在 `lib.rs` 的 `invoke_handler` 中追加）：

```rust
commands::ai_get_providers,
commands::ai_get_models,
commands::ai_validate_config,
commands::ai_save_config,
commands::ai_get_config,
commands::ai_generate_sql,
commands::ai_diagnose_error,
commands::ai_analyze_explain,
commands::ai_chat,
```

### 1.8 Store 扩展 — AI 配置持久化

在现有 `Store`（AES 加密本地存储）中增加 AI 配置的读写：

```rust
// src-tauri/src/services/store.rs 中新增方法

impl Store {
    pub async fn save_ai_config(&self, config: &AiProviderConfig) -> Result<(), StoreError> {
        let mut cache = self.cache.write().await;
        cache.ai_config = Some(config.clone());
        self.persist(&cache).await
    }

    pub async fn get_ai_config(&self) -> Result<Option<AiProviderConfig>, StoreError> {
        let cache = self.cache.read().await;
        Ok(cache.ai_config.clone())
    }
}

// StoreCache 中新增字段
struct StoreCache {
    connections: Vec<ConnectionConfig>,
    groups: Vec<ConnectionGroup>,
    settings: AppSettings,
    query_history: Vec<QueryHistoryEntry>,
    query_favorites: Vec<QueryFavorite>,
    sync_tasks: Vec<SyncTask>,
    ai_config: Option<AiProviderConfig>,  // 新增
}
```

API Key 安全策略：
- `AiProviderConfig` 中的 `api_key` 字段随整个 Store 一起经过 AES-256-GCM 加密存储
- 与现有的连接密码使用相同的加密机制

### 1.9 `src-tauri/Cargo.toml` 依赖变更

```toml
[dependencies]
# 新增
datazen-ai-api = { path = "../packages/ai-api" }
# reqwest 已存在，确认 features 包含 stream
# tokio 已存在，确认 features 包含 sync
# serde_json 已存在
# uuid 已存在
```

无需引入新的外部依赖，全部复用现有的 `reqwest`、`tokio`、`serde` 等。

---

## 第二部分：前端架构设计

### 2.1 类型定义 (`src/types/index.ts` 扩展)

遵循现有惯例，在 `src/types/index.ts` 中追加 AI 相关类型：

```typescript
// ─── AI Types ───

export type AiProviderType = 'open_ai' | 'anthropic' | 'ollama' | 'custom';

export interface AiProviderConfig {
  providerType: AiProviderType;
  apiKey?: string;
  endpoint?: string;
  model: string;
  extra?: Record<string, unknown>;
}

export interface ModelInfo {
  id: string;
  displayName: string;
  contextWindow: number;
  supportsStreaming: boolean;
  supportsTools: boolean;
}

export interface ProviderListItem {
  providerType: AiProviderType;
  displayName: string;
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface StreamChunkPayload {
  requestId: string;
  content: string;
  done: boolean;
  usage?: TokenUsage;
}

export interface StreamErrorPayload {
  requestId: string;
  error: string;
}

export interface TokenUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

export interface DiagnosisResult {
  explanation: string;
  suggestedSql: string | null;
  changes: string[];
}

export interface ExplainAnalysis {
  summary: string;
  bottlenecks: Bottleneck[];
  suggestions: Suggestion[];
}

export interface Bottleneck {
  node: string;
  description: string;
  severity: 'high' | 'medium' | 'low';
}

export interface Suggestion {
  description: string;
  sql: string | null;
  impact: string;
}
```

### 2.2 IPC 命令封装 (`src/commands/ai.ts`)

遵循现有模式（plain object + invoke<T>）：

```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type {
  AiProviderConfig,
  AiProviderType,
  ModelInfo,
  ProviderListItem,
  ChatMessage,
  DiagnosisResult,
  ExplainAnalysis,
  StreamChunkPayload,
  StreamErrorPayload,
} from '../types';

export const aiCommands = {
  // ── Provider 管理 ──

  getProviders: () =>
    invoke<ProviderListItem[]>('ai_get_providers'),

  getModels: (providerType: AiProviderType) =>
    invoke<ModelInfo[]>('ai_get_models', { providerType }),

  validateConfig: (config: AiProviderConfig) =>
    invoke<void>('ai_validate_config', { config }),

  saveConfig: (config: AiProviderConfig) =>
    invoke<void>('ai_save_config', { config }),

  getConfig: () =>
    invoke<AiProviderConfig | null>('ai_get_config'),

  // ── NL2SQL ──

  generateSql: (params: {
    connectionId: string;
    naturalLanguage: string;
    currentTable?: string;
    recentQueries?: string[];
  }) =>
    invoke<string>('ai_generate_sql', params),

  // ── 错误诊断 ──

  diagnoseError: (params: {
    connectionId: string;
    sql: string;
    errorMessage: string;
  }) =>
    invoke<DiagnosisResult>('ai_diagnose_error', params),

  // ── EXPLAIN 分析 ──

  analyzeExplain: (params: {
    connectionId: string;
    explainOutput: string;
    originalSql: string;
  }) =>
    invoke<ExplainAnalysis>('ai_analyze_explain', params),

  // ── AI Chat ──

  chat: (params: {
    connectionId?: string;
    messages: ChatMessage[];
    includeSchema?: boolean;
  }) =>
    invoke<string>('ai_chat', params),
};

// ── 流式事件监听 ──

export function onAiStreamChunk(
  callback: (payload: StreamChunkPayload) => void
): Promise<UnlistenFn> {
  return listen<StreamChunkPayload>('ai:stream-chunk', (event) => {
    callback(event.payload);
  });
}

export function onAiStreamError(
  callback: (payload: StreamErrorPayload) => void
): Promise<UnlistenFn> {
  return listen<StreamErrorPayload>('ai:stream-error', (event) => {
    callback(event.payload);
  });
}
```

### 2.3 Zustand Store (`src/stores/aiStore.ts`)

遵循现有 Store 模式（interface 包含 state + actions，create 函数，细粒度 selector）：

```typescript
import { create } from 'zustand';
import { aiCommands, onAiStreamChunk, onAiStreamError } from '../commands/ai';
import type {
  AiProviderConfig,
  AiProviderType,
  ModelInfo,
  ProviderListItem,
  ChatMessage,
  DiagnosisResult,
  ExplainAnalysis,
  StreamChunkPayload,
  TokenUsage,
} from '../types';

// ─── Chat 会话 ───

export interface AiChatSession {
  id: string;
  messages: ChatMessage[];
  isStreaming: boolean;
  streamContent: string;       // 当前流式输出的累积内容
  lastUsage?: TokenUsage;
}

// ─── NL2SQL 状态 ───

interface Nl2SqlState {
  input: string;
  generatedSql: string;
  isGenerating: boolean;
  requestId: string | null;
}

// ─── Store Interface ───

interface AiStore {
  // ── 配置状态 ──
  config: AiProviderConfig | null;
  isConfigured: boolean;
  providers: ProviderListItem[];
  models: ModelInfo[];
  configLoading: boolean;
  configError: string | null;

  // ── NL2SQL 状态 ──
  nl2sql: Nl2SqlState;

  // ── 诊断状态 ──
  diagnosis: DiagnosisResult | null;
  isDiagnosing: boolean;
  diagnosisError: string | null;

  // ── EXPLAIN 分析 ──
  explainAnalysis: ExplainAnalysis | null;
  isAnalyzingExplain: boolean;

  // ── Chat 状态 ──
  chatSession: AiChatSession | null;

  // ── 配置操作 ──
  loadConfig: () => Promise<void>;
  loadProviders: () => Promise<void>;
  loadModels: (providerType: AiProviderType) => Promise<void>;
  validateConfig: (config: AiProviderConfig) => Promise<void>;
  saveConfig: (config: AiProviderConfig) => Promise<void>;

  // ── NL2SQL 操作 ──
  setNl2SqlInput: (input: string) => void;
  generateSql: (params: {
    connectionId: string;
    currentTable?: string;
    recentQueries?: string[];
  }) => Promise<void>;
  clearNl2Sql: () => void;

  // ── 诊断操作 ──
  diagnoseError: (params: {
    connectionId: string;
    sql: string;
    errorMessage: string;
  }) => Promise<void>;
  clearDiagnosis: () => void;

  // ── EXPLAIN 操作 ──
  analyzeExplain: (params: {
    connectionId: string;
    explainOutput: string;
    originalSql: string;
  }) => Promise<void>;
  clearExplainAnalysis: () => void;

  // ── Chat 操作 ──
  initChatSession: () => void;
  sendChatMessage: (params: {
    connectionId?: string;
    content: string;
    includeSchema?: boolean;
  }) => Promise<void>;
  clearChat: () => void;

  // ── 流式事件处理（内部） ──
  handleStreamChunk: (payload: StreamChunkPayload) => void;

  // ── 生命周期 ──
  reset: () => void;
  setupEventListeners: () => Promise<() => void>;
}

// ─── 初始状态 ───

const initialNl2Sql: Nl2SqlState = {
  input: '',
  generatedSql: '',
  isGenerating: false,
  requestId: null,
};

// ─── Store 实现 ───

export const useAiStore = create<AiStore>((set, get) => ({
  config: null,
  isConfigured: false,
  providers: [],
  models: [],
  configLoading: false,
  configError: null,

  nl2sql: { ...initialNl2Sql },

  diagnosis: null,
  isDiagnosing: false,
  diagnosisError: null,

  explainAnalysis: null,
  isAnalyzingExplain: false,

  chatSession: null,

  // ── 配置 ──

  loadConfig: async () => {
    set({ configLoading: true, configError: null });
    try {
      const config = await aiCommands.getConfig();
      set({
        config,
        isConfigured: config !== null,
        configLoading: false,
      });
    } catch (e) {
      set({
        configLoading: false,
        configError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  loadProviders: async () => {
    try {
      const providers = await aiCommands.getProviders();
      set({ providers });
    } catch (e) {
      console.error('Failed to load AI providers:', e);
    }
  },

  loadModels: async (providerType) => {
    try {
      const models = await aiCommands.getModels(providerType);
      set({ models });
    } catch (e) {
      console.error('Failed to load models:', e);
    }
  },

  validateConfig: async (config) => {
    set({ configLoading: true, configError: null });
    try {
      await aiCommands.validateConfig(config);
      set({ configLoading: false });
    } catch (e) {
      set({
        configLoading: false,
        configError: e instanceof Error ? e.message : String(e),
      });
      throw e;
    }
  },

  saveConfig: async (config) => {
    set({ configLoading: true, configError: null });
    try {
      await aiCommands.saveConfig(config);
      set({
        config,
        isConfigured: true,
        configLoading: false,
      });
    } catch (e) {
      set({
        configLoading: false,
        configError: e instanceof Error ? e.message : String(e),
      });
      throw e;
    }
  },

  // ── NL2SQL ──

  setNl2SqlInput: (input) => {
    set({ nl2sql: { ...get().nl2sql, input } });
  },

  generateSql: async ({ connectionId, currentTable, recentQueries }) => {
    const { nl2sql } = get();
    if (!nl2sql.input.trim()) return;

    set({
      nl2sql: {
        ...nl2sql,
        isGenerating: true,
        generatedSql: '',
        requestId: null,
      },
    });

    try {
      const requestId = await aiCommands.generateSql({
        connectionId,
        naturalLanguage: nl2sql.input,
        currentTable,
        recentQueries,
      });
      set({
        nl2sql: { ...get().nl2sql, requestId },
      });
    } catch (e) {
      set({
        nl2sql: { ...get().nl2sql, isGenerating: false },
      });
    }
  },

  clearNl2Sql: () => {
    set({ nl2sql: { ...initialNl2Sql } });
  },

  // ── 诊断 ──

  diagnoseError: async ({ connectionId, sql, errorMessage }) => {
    set({ isDiagnosing: true, diagnosis: null, diagnosisError: null });
    try {
      const result = await aiCommands.diagnoseError({
        connectionId,
        sql,
        errorMessage,
      });
      set({ diagnosis: result, isDiagnosing: false });
    } catch (e) {
      set({
        isDiagnosing: false,
        diagnosisError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearDiagnosis: () => {
    set({ diagnosis: null, diagnosisError: null });
  },

  // ── EXPLAIN ──

  analyzeExplain: async ({ connectionId, explainOutput, originalSql }) => {
    set({ isAnalyzingExplain: true, explainAnalysis: null });
    try {
      const result = await aiCommands.analyzeExplain({
        connectionId,
        explainOutput,
        originalSql,
      });
      set({ explainAnalysis: result, isAnalyzingExplain: false });
    } catch (e) {
      set({ isAnalyzingExplain: false });
    }
  },

  clearExplainAnalysis: () => {
    set({ explainAnalysis: null });
  },

  // ── Chat ──

  initChatSession: () => {
    set({
      chatSession: {
        id: crypto.randomUUID(),
        messages: [],
        isStreaming: false,
        streamContent: '',
      },
    });
  },

  sendChatMessage: async ({ connectionId, content, includeSchema }) => {
    const { chatSession } = get();
    if (!chatSession) return;

    const userMessage: ChatMessage = { role: 'user', content };
    const updatedMessages = [...chatSession.messages, userMessage];

    set({
      chatSession: {
        ...chatSession,
        messages: updatedMessages,
        isStreaming: true,
        streamContent: '',
      },
    });

    try {
      const requestId = await aiCommands.chat({
        connectionId,
        messages: updatedMessages,
        includeSchema: includeSchema ?? true,
      });

      set({
        chatSession: {
          ...get().chatSession!,
          requestId,
        } as AiChatSession,
      });
    } catch (e) {
      set({
        chatSession: {
          ...get().chatSession!,
          isStreaming: false,
        } as AiChatSession,
      });
    }
  },

  clearChat: () => {
    set({ chatSession: null });
  },

  // ── 流式事件 ──

  handleStreamChunk: (payload) => {
    const { nl2sql, chatSession } = get();

    // NL2SQL 流
    if (nl2sql.requestId === payload.requestId) {
      const newSql = nl2sql.generatedSql + payload.content;
      set({
        nl2sql: {
          ...nl2sql,
          generatedSql: newSql,
          isGenerating: !payload.done,
        },
      });
      return;
    }

    // Chat 流
    if (chatSession && (chatSession as any).requestId === payload.requestId) {
      const newContent = chatSession.streamContent + payload.content;
      if (payload.done) {
        const assistantMessage: ChatMessage = {
          role: 'assistant',
          content: newContent,
        };
        set({
          chatSession: {
            ...chatSession,
            messages: [...chatSession.messages, assistantMessage],
            isStreaming: false,
            streamContent: '',
            lastUsage: payload.usage,
          },
        });
      } else {
        set({
          chatSession: {
            ...chatSession,
            streamContent: newContent,
          },
        });
      }
    }
  },

  // ── 事件监听器（在 ConnectionWindow 挂载时调用） ──

  setupEventListeners: async () => {
    const unlistenChunk = await onAiStreamChunk((payload) => {
      get().handleStreamChunk(payload);
    });
    const unlistenError = await onAiStreamError((payload) => {
      console.error('AI stream error:', payload.error);
      const { nl2sql, chatSession } = get();
      if (nl2sql.requestId === payload.requestId) {
        set({ nl2sql: { ...nl2sql, isGenerating: false } });
      }
      if (chatSession) {
        set({
          chatSession: { ...chatSession, isStreaming: false },
        });
      }
    });

    return () => {
      unlistenChunk();
      unlistenError();
    };
  },

  reset: () => {
    set({
      nl2sql: { ...initialNl2Sql },
      diagnosis: null,
      isDiagnosing: false,
      diagnosisError: null,
      explainAnalysis: null,
      isAnalyzingExplain: false,
      chatSession: null,
    });
  },
}));
```

### 2.4 自定义 Hook (`src/hooks/useAiStream.ts`)

遵循现有 hook 模式（async IIFE + cleanup ref）：

```typescript
import { useEffect, useRef } from 'react';
import { useAiStore } from '../stores/aiStore';

/**
 * 在 ConnectionWindow 挂载时初始化 AI 流式事件监听。
 * 遵循 useThemeListener / useTauriEvent 的 cleanup 模式。
 */
export function useAiEventListeners() {
  const setupEventListeners = useAiStore((s) => s.setupEventListeners);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const init = async () => {
      cleanupRef.current = await setupEventListeners();
    };
    void init();

    return () => {
      cleanupRef.current?.();
    };
  }, [setupEventListeners]);
}
```

### 2.5 前端组件设计

#### 组件目录结构

```
src/components/ai/
├── NL2SqlInput.tsx            # 自然语言转 SQL 输入框
├── AiErrorDiagnosis.tsx       # SQL 错误 AI 诊断面板
├── AiChatPanel.tsx            # 侧边栏 AI 对话助手
├── AiChatMessage.tsx          # 单条聊天消息
├── ExplainVisualizer.tsx      # EXPLAIN 可视化 + AI 解读
├── NlFilterInput.tsx          # 智能筛选输入框
├── AiConfigSection.tsx        # 设置页面中的 AI 配置板块
└── AiStatusBadge.tsx          # AI 状态指示器（已配置/未配置）
```

#### 2.5.1 `NL2SqlInput` — 自然语言转 SQL

嵌入位置：`QueryPanel` 中 `SqlEditor` 上方

```tsx
// src/components/ai/NL2SqlInput.tsx

interface NL2SqlInputProps {
  connectionId: string;
  currentTable?: string;
  recentQueries: string[];
  onSqlGenerated: (sql: string) => void;  // 将 SQL 插入 SqlEditor
}

export function NL2SqlInput({
  connectionId,
  currentTable,
  recentQueries,
  onSqlGenerated,
}: NL2SqlInputProps) {
  const { t } = useI18n();
  const nl2sql = useAiStore((s) => s.nl2sql);
  const isConfigured = useAiStore((s) => s.isConfigured);
  const setInput = useAiStore((s) => s.setNl2SqlInput);
  const generateSql = useAiStore((s) => s.generateSql);

  // 当生成完成时，传递给编辑器
  useEffect(() => {
    if (!nl2sql.isGenerating && nl2sql.generatedSql) {
      onSqlGenerated(nl2sql.generatedSql);
    }
  }, [nl2sql.isGenerating, nl2sql.generatedSql]);

  if (!isConfigured) return null;  // AI 未配置时不显示

  const handleSubmit = () => {
    generateSql({ connectionId, currentTable, recentQueries });
  };

  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-muted/30">
      <SparklesIcon className="w-4 h-4 text-muted-foreground shrink-0" />
      <input
        className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        placeholder={t('ai.nl2sql.placeholder')}
        value={nl2sql.input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            handleSubmit();
          }
        }}
        disabled={nl2sql.isGenerating}
      />
      {nl2sql.isGenerating && (
        <LoadingSpinner className="w-4 h-4" />
      )}
    </div>
  );
}
```

#### 2.5.2 `AiErrorDiagnosis` — 错误诊断面板

嵌入位置：`QueryPanel` 的错误显示区域内

```tsx
// src/components/ai/AiErrorDiagnosis.tsx

interface AiErrorDiagnosisProps {
  connectionId: string;
  sql: string;
  errorMessage: string;
  onApplyFix: (sql: string) => void;  // 将修复后的 SQL 替换到编辑器
}

export function AiErrorDiagnosis({
  connectionId,
  sql,
  errorMessage,
  onApplyFix,
}: AiErrorDiagnosisProps) {
  const { t } = useI18n();
  const diagnosis = useAiStore((s) => s.diagnosis);
  const isDiagnosing = useAiStore((s) => s.isDiagnosing);
  const diagnoseError = useAiStore((s) => s.diagnoseError);
  const clearDiagnosis = useAiStore((s) => s.clearDiagnosis);
  const isConfigured = useAiStore((s) => s.isConfigured);

  if (!isConfigured) return null;

  // 未开始诊断时，只显示按钮
  if (!diagnosis && !isDiagnosing) {
    return (
      <button
        className="flex items-center gap-1 text-xs text-blue-500 hover:text-blue-600 mt-2"
        onClick={() => diagnoseError({ connectionId, sql, errorMessage })}
      >
        <SparklesIcon className="w-3 h-3" />
        {t('ai.diagnose.button')}
      </button>
    );
  }

  if (isDiagnosing) {
    return (
      <div className="flex items-center gap-2 mt-2 text-xs text-muted-foreground">
        <LoadingSpinner className="w-3 h-3" />
        {t('ai.diagnose.analyzing')}
      </div>
    );
  }

  if (!diagnosis) return null;

  return (
    <div className="mt-3 p-3 rounded-md bg-blue-500/10 border border-blue-500/20 text-sm">
      <div className="flex items-center gap-1 font-medium text-blue-500 mb-2">
        <SparklesIcon className="w-4 h-4" />
        {t('ai.diagnose.title')}
      </div>

      <p className="text-foreground/80 mb-2">{diagnosis.explanation}</p>

      {diagnosis.changes.length > 0 && (
        <ul className="list-disc list-inside text-xs text-muted-foreground mb-2">
          {diagnosis.changes.map((change, i) => (
            <li key={i}>{change}</li>
          ))}
        </ul>
      )}

      {diagnosis.suggestedSql && (
        <div className="mt-2">
          <SqlCodeBlock sql={diagnosis.suggestedSql} />
          <div className="flex gap-2 mt-2">
            <Button size="sm" onClick={() => onApplyFix(diagnosis.suggestedSql!)}>
              {t('ai.diagnose.applyFix')}
            </Button>
            <Button size="sm" variant="ghost" onClick={clearDiagnosis}>
              {t('ai.diagnose.dismiss')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
```

#### 2.5.3 `AiChatPanel` — 侧边栏对话助手

嵌入位置：`SqlConnectionView` 右侧可收起面板（类似 `DetailPanel` 的模式）

```tsx
// src/components/ai/AiChatPanel.tsx

interface AiChatPanelProps {
  connectionId: string;
  databaseType: string;
  isOpen: boolean;
  onClose: () => void;
  onInsertSql?: (sql: string) => void;
}

export function AiChatPanel({
  connectionId,
  databaseType,
  isOpen,
  onClose,
  onInsertSql,
}: AiChatPanelProps) {
  const { t } = useI18n();
  const chatSession = useAiStore((s) => s.chatSession);
  const initChat = useAiStore((s) => s.initChatSession);
  const sendMessage = useAiStore((s) => s.sendChatMessage);
  const clearChat = useAiStore((s) => s.clearChat);
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isOpen && !chatSession) {
      initChat();
    }
  }, [isOpen]);

  // 自动滚动到底部
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [chatSession?.messages.length, chatSession?.streamContent]);

  if (!isOpen) return null;

  const handleSend = () => {
    if (!input.trim() || chatSession?.isStreaming) return;
    sendMessage({
      connectionId,
      content: input.trim(),
      includeSchema: true,
    });
    setInput('');
  };

  return (
    <div className="flex flex-col h-full border-l border-border bg-background w-[360px]">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <div className="flex items-center gap-2">
          <SparklesIcon className="w-4 h-4 text-blue-500" />
          <span className="text-sm font-medium">{t('ai.chat.title')}</span>
        </div>
        <div className="flex items-center gap-1">
          <button onClick={clearChat} title={t('ai.chat.newChat')}>
            <PlusIcon className="w-4 h-4" />
          </button>
          <button onClick={onClose}>
            <XIcon className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Context Badge */}
      <div className="px-3 py-1.5 text-xs text-muted-foreground bg-muted/30 border-b border-border">
        📎 {databaseType} · {connectionId.slice(0, 8)}
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-3 py-2 space-y-3">
        {chatSession?.messages.map((msg, i) => (
          <AiChatMessage
            key={i}
            message={msg}
            onInsertSql={onInsertSql}
          />
        ))}

        {/* 流式输出中的内容 */}
        {chatSession?.isStreaming && chatSession.streamContent && (
          <AiChatMessage
            message={{ role: 'assistant', content: chatSession.streamContent }}
            isStreaming
          />
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div className="p-3 border-t border-border">
        <div className="flex items-end gap-2">
          <textarea
            className="flex-1 resize-none bg-muted/30 rounded-md px-3 py-2 text-sm outline-none border border-border focus:border-blue-500"
            rows={2}
            placeholder={t('ai.chat.inputPlaceholder')}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                handleSend();
              }
            }}
            disabled={chatSession?.isStreaming}
          />
          <Button
            size="sm"
            onClick={handleSend}
            disabled={!input.trim() || chatSession?.isStreaming}
          >
            {chatSession?.isStreaming ? <LoadingSpinner /> : <SendIcon />}
          </Button>
        </div>
      </div>
    </div>
  );
}
```

#### 2.5.4 `ExplainVisualizer` — EXPLAIN 可视化

嵌入位置：`QueryPanel` 结果区新增 Tab

```tsx
// src/components/ai/ExplainVisualizer.tsx

interface ExplainVisualizerProps {
  connectionId: string;
  sql: string;
  explainOutput: string;
}

export function ExplainVisualizer({
  connectionId,
  sql,
  explainOutput,
}: ExplainVisualizerProps) {
  const { t } = useI18n();
  const analysis = useAiStore((s) => s.explainAnalysis);
  const isAnalyzing = useAiStore((s) => s.isAnalyzingExplain);
  const analyzeExplain = useAiStore((s) => s.analyzeExplain);
  const isConfigured = useAiStore((s) => s.isConfigured);

  return (
    <div className="p-4 space-y-4">
      {/* 原始 EXPLAIN 输出 */}
      <div>
        <h3 className="text-sm font-medium mb-2">{t('ai.explain.rawOutput')}</h3>
        <pre className="text-xs bg-muted/30 rounded p-3 overflow-auto max-h-[300px]">
          {explainOutput}
        </pre>
      </div>

      {/* AI 分析 */}
      {isConfigured && !analysis && (
        <Button
          size="sm"
          variant="outline"
          onClick={() => analyzeExplain({ connectionId, explainOutput, originalSql: sql })}
          disabled={isAnalyzing}
        >
          <SparklesIcon className="w-3.5 h-3.5 mr-1" />
          {isAnalyzing ? t('ai.explain.analyzing') : t('ai.explain.analyzeButton')}
        </Button>
      )}

      {analysis && (
        <div className="space-y-3">
          <div className="p-3 rounded-md bg-blue-500/10 border border-blue-500/20">
            <p className="text-sm font-medium">{analysis.summary}</p>
          </div>

          {/* 瓶颈列表 */}
          {analysis.bottlenecks.length > 0 && (
            <div>
              <h4 className="text-sm font-medium mb-1">{t('ai.explain.bottlenecks')}</h4>
              <div className="space-y-1">
                {analysis.bottlenecks.map((b, i) => (
                  <div key={i} className="flex items-start gap-2 text-xs p-2 rounded bg-muted/30">
                    <SeverityBadge severity={b.severity} />
                    <div>
                      <span className="font-mono">{b.node}</span>
                      <p className="text-muted-foreground">{b.description}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* 优化建议 */}
          {analysis.suggestions.length > 0 && (
            <div>
              <h4 className="text-sm font-medium mb-1">{t('ai.explain.suggestions')}</h4>
              {analysis.suggestions.map((s, i) => (
                <div key={i} className="p-2 rounded bg-green-500/10 border border-green-500/20 mb-1">
                  <p className="text-xs">{s.description}</p>
                  {s.sql && (
                    <SqlCodeBlock sql={s.sql} className="mt-1" />
                  )}
                  <p className="text-xs text-muted-foreground mt-1">
                    {t('ai.explain.expectedImpact')}: {s.impact}
                  </p>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
```

#### 2.5.5 `AiConfigSection` — 设置页面 AI 配置

嵌入位置：`SettingsWindow` 中新增一个 Section

```tsx
// src/components/ai/AiConfigSection.tsx

interface AiConfigSectionProps {
  draft: AppSettings;
  onDraftChange: (settings: AppSettings) => void;
}

export function AiConfigSection() {
  const { t } = useI18n();
  const config = useAiStore((s) => s.config);
  const providers = useAiStore((s) => s.providers);
  const models = useAiStore((s) => s.models);
  const loadProviders = useAiStore((s) => s.loadProviders);
  const loadModels = useAiStore((s) => s.loadModels);
  const validateConfig = useAiStore((s) => s.validateConfig);
  const saveConfig = useAiStore((s) => s.saveConfig);
  const configLoading = useAiStore((s) => s.configLoading);
  const configError = useAiStore((s) => s.configError);

  const [draft, setDraft] = useState<AiProviderConfig>(
    config ?? {
      providerType: 'open_ai',
      apiKey: '',
      endpoint: '',
      model: 'gpt-4o',
    }
  );

  useEffect(() => { loadProviders(); }, []);
  useEffect(() => { loadModels(draft.providerType); }, [draft.providerType]);

  const handleTest = async () => {
    try {
      await validateConfig(draft);
      // 显示成功提示
    } catch {
      // configError 已在 store 中设置
    }
  };

  const handleSave = () => saveConfig(draft);

  return (
    <div>
      <SectionTitle>{t('settings.ai.title')}</SectionTitle>

      <SettingRow label={t('settings.ai.provider')}>
        <Select
          value={draft.providerType}
          onChange={(v) => setDraft({ ...draft, providerType: v as AiProviderType })}
          options={providers.map((p) => ({
            value: p.providerType,
            label: p.displayName,
          }))}
        />
      </SettingRow>

      {draft.providerType !== 'ollama' && (
        <SettingRow label="API Key">
          <Input
            type="password"
            value={draft.apiKey ?? ''}
            onChange={(e) => setDraft({ ...draft, apiKey: e.target.value })}
            placeholder="sk-..."
          />
        </SettingRow>
      )}

      {(draft.providerType === 'ollama' || draft.providerType === 'custom') && (
        <SettingRow label={t('settings.ai.endpoint')}>
          <Input
            value={draft.endpoint ?? ''}
            onChange={(e) => setDraft({ ...draft, endpoint: e.target.value })}
            placeholder="http://localhost:11434"
          />
        </SettingRow>
      )}

      <SettingRow label={t('settings.ai.model')}>
        <Select
          value={draft.model}
          onChange={(v) => setDraft({ ...draft, model: v })}
          options={models.map((m) => ({
            value: m.id,
            label: `${m.displayName} (${Math.round(m.contextWindow / 1000)}K)`,
          }))}
        />
      </SettingRow>

      {configError && (
        <div className="text-xs text-red-500 px-4 py-1">{configError}</div>
      )}

      <div className="flex gap-2 px-4 py-2">
        <Button size="sm" variant="outline" onClick={handleTest} disabled={configLoading}>
          {t('settings.ai.test')}
        </Button>
        <Button size="sm" onClick={handleSave} disabled={configLoading}>
          {t('settings.ai.save')}
        </Button>
      </div>

      <Divider />
    </div>
  );
}
```

### 2.6 现有组件集成点

#### `QueryPanel.tsx` 修改

```tsx
// 在 SqlEditor 上方添加 NL2SqlInput
// 在错误显示区域添加 AiErrorDiagnosis
// 在结果 tab 中添加 Explain tab

// 伪代码展示关键集成点：

function QueryPanel({ queryTabId, connectionId }: QueryPanelProps) {
  const isAiConfigured = useAiStore((s) => s.isConfigured);

  return (
    <div className="flex flex-col h-full">
      {/* NL2SQL 输入框（AI 已配置时显示） */}
      {isAiConfigured && (
        <NL2SqlInput
          connectionId={connectionId}
          currentTable={currentTable}
          recentQueries={recentQueries}
          onSqlGenerated={(sql) => {
            editorRef.current?.setValue(sql);
          }}
        />
      )}

      {/* 现有 SqlEditor */}
      <SqlEditor ref={editorRef} ... />

      {/* 结果区域 */}
      <div>
        {/* 现有结果 tabs */}
        {results.map((result) => ...)}

        {/* 错误区域增加 AI 诊断 */}
        {error && (
          <>
            <div className="text-red-500">{error}</div>
            <AiErrorDiagnosis
              connectionId={connectionId}
              sql={currentSql}
              errorMessage={error}
              onApplyFix={(sql) => editorRef.current?.setValue(sql)}
            />
          </>
        )}
      </div>
    </div>
  );
}
```

#### `SqlConnectionView.tsx` 修改

```tsx
// 在现有布局中添加 AiChatPanel（右侧侧边栏）

function SqlConnectionView(props: ConnectionViewProps) {
  const [isAiChatOpen, setAiChatOpen] = useState(false);
  const isAiConfigured = useAiStore((s) => s.isConfigured);

  // 初始化 AI 事件监听
  useAiEventListeners();

  return (
    <div className="flex h-full">
      {/* 现有 SchemaTree sidebar */}
      <aside>...</aside>

      {/* 主内容区 */}
      <main className="flex-1">
        {/* 工具栏增加 AI Chat 按钮 */}
        <Toolbar>
          ...existing buttons...
          {isAiConfigured && (
            <button onClick={() => setAiChatOpen(!isAiChatOpen)}>
              <SparklesIcon />
            </button>
          )}
        </Toolbar>

        {/* 现有面板内容 */}
        ...
      </main>

      {/* AI Chat 侧边栏 */}
      {isAiConfigured && (
        <AiChatPanel
          connectionId={props.connectionId}
          databaseType={props.databaseType}
          isOpen={isAiChatOpen}
          onClose={() => setAiChatOpen(false)}
          onInsertSql={(sql) => {
            // 将 SQL 插入当前活跃的 QueryPanel
          }}
        />
      )}
    </div>
  );
}
```

#### `SettingsWindow.tsx` 修改

```tsx
// 在现有设置项之后添加 AI 配置 Section

function SettingsWindow() {
  return (
    <div>
      ...existing settings sections...

      {/* AI 配置（新增） */}
      <AiConfigSection />

      ...footer...
    </div>
  );
}
```

### 2.7 i18n 扩展

在 `src/locales/zh.ts` 和 `src/locales/en.ts` 中追加 AI 相关翻译：

```typescript
// zh.ts
ai: {
  nl2sql: {
    placeholder: '用自然语言描述你的查询...',
  },
  diagnose: {
    button: 'AI 诊断',
    analyzing: '正在分析...',
    title: 'AI 诊断',
    applyFix: '应用修复',
    dismiss: '关闭',
  },
  explain: {
    rawOutput: '执行计划',
    analyzeButton: 'AI 分析',
    analyzing: '正在分析...',
    bottlenecks: '性能瓶颈',
    suggestions: '优化建议',
    expectedImpact: '预期效果',
  },
  chat: {
    title: 'AI 助手',
    newChat: '新对话',
    inputPlaceholder: '输入消息...',
  },
},
settings: {
  ai: {
    title: 'AI 设置',
    provider: 'AI 供应商',
    endpoint: '服务地址',
    model: '模型',
    test: '测试连接',
    save: '保存',
  },
}
```

---

## 第三部分：数据流全景

### NL2SQL 完整数据流

```
用户输入自然语言
  → NL2SqlInput.handleSubmit()
    → aiStore.generateSql()
      → aiCommands.generateSql()
        → invoke('ai_generate_sql', ...)
          → [Rust] ai_generate_sql()
            → SchemaContextBuilder.build_sql_context()
              → SchemaCache 获取表/列信息
            → PromptBuilder.nl2sql_system() 构建 prompt
            → provider.stream_complete(request, tx)
              → [HTTP] POST /v1/chat/completions (stream=true)
              → SSE 事件流 → mpsc::channel
            → tokio::spawn → rx.recv()
              → app_handle.emit("ai:stream-chunk", payload)
        ← 返回 request_id
      ← store 记录 requestId

Tauri Event 流:
  ai:stream-chunk → onAiStreamChunk callback
    → aiStore.handleStreamChunk()
      → 匹配 requestId → 累积 nl2sql.generatedSql
    → NL2SqlInput useEffect 检测完成
      → onSqlGenerated(sql) → editorRef.setValue(sql)
```

### AI Chat 完整数据流

```
用户在 AiChatPanel 输入消息
  → sendChatMessage({ connectionId, content, includeSchema })
    → store: 追加 user message, 设置 isStreaming=true
    → aiCommands.chat(...)
      → invoke('ai_chat', ...)
        → [Rust] ai_chat()
          → 可选: SchemaContextBuilder 构建 schema 上下文
          → 构建 system + 历史 messages
          → provider.stream_complete()
            → SSE 流 → mpsc → emit("ai:stream-chunk")

Tauri Event 流:
  ai:stream-chunk → handleStreamChunk()
    → 匹配 chat requestId
    → 累积 streamContent（实时渲染）
    → done=true 时：
      → 完整内容作为 assistant message 追加到 chatSession.messages
      → 清空 streamContent, 设置 isStreaming=false
```

---

## 第四部分：实施路线

### Phase 0 — AI 基础设施（1 周）

| 任务 | 位置 | 说明 |
|------|------|------|
| 创建 `packages/ai-api` crate | 后端 | lib.rs + traits.rs + types.rs + factory.rs |
| 实现 OpenAI Provider | 后端 `src-tauri/src/ai/openai.rs` | 含流式支持 |
| 实现 Anthropic Provider | 后端 `src-tauri/src/ai/anthropic.rs` | Claude Messages API |
| 实现 Ollama Provider | 后端 `src-tauri/src/ai/ollama.rs` | 本地模型 |
| Provider Registry + init | 后端 `src-tauri/src/ai/registry.rs` | 对标 init_drivers() |
| Schema Context Builder | 后端 `src-tauri/src/ai/context.rs` | DDL 摘要生成 |
| Prompt Builder | 后端 `src-tauri/src/ai/prompt.rs` | 模板管理 |
| AppState 扩展 | 后端 `src-tauri/src/commands/mod.rs` | 新增 ai_registry 字段 |
| AI 配置持久化 | 后端 Store | save/get_ai_config |
| AI Commands | 后端 `src-tauri/src/commands/ai.rs` | IPC 命令 |
| 前端类型定义 | `src/types/index.ts` | AI 相关类型 |
| 前端命令封装 | `src/commands/ai.ts` | invoke 封装 |
| aiStore 基础 | `src/stores/aiStore.ts` | 配置管理部分 |
| AI 设置页面 | `src/components/ai/AiConfigSection.tsx` | Provider/Model/Key 配置 |

### Phase 1 — NL2SQL + 错误诊断（1-2 周）

| 任务 | 位置 | 说明 |
|------|------|------|
| aiStore NL2SQL 逻辑 | `src/stores/aiStore.ts` | 生成 + 流式处理 |
| NL2SqlInput 组件 | `src/components/ai/NL2SqlInput.tsx` | UI |
| QueryPanel 集成 NL2SQL | `src/windows/connection/QueryPanel.tsx` | 嵌入编辑器上方 |
| aiStore 诊断逻辑 | `src/stores/aiStore.ts` | diagnose 部分 |
| AiErrorDiagnosis 组件 | `src/components/ai/AiErrorDiagnosis.tsx` | UI |
| QueryPanel 集成诊断 | `src/windows/connection/QueryPanel.tsx` | 嵌入错误区域 |
| useAiEventListeners hook | `src/hooks/useAiStream.ts` | 事件监听 |
| i18n 翻译 | `src/locales/zh.ts` + `en.ts` | AI 文案 |

### Phase 2 — EXPLAIN 可视化（1 周）

| 任务 | 位置 | 说明 |
|------|------|------|
| 前端接入 get_explain | 命令层 | 后端已实现，前端调用 |
| ExplainVisualizer 组件 | `src/components/ai/ExplainVisualizer.tsx` | 树形展示 + AI 分析 |
| QueryPanel 新增 Explain Tab | `src/windows/connection/QueryPanel.tsx` | 结果 Tab 扩展 |

### Phase 3 — AI Chat 侧边栏（2 周）

| 任务 | 位置 | 说明 |
|------|------|------|
| aiStore Chat 逻辑 | `src/stores/aiStore.ts` | 对话管理 + 流式 |
| AiChatPanel 组件 | `src/components/ai/AiChatPanel.tsx` | 主面板 |
| AiChatMessage 组件 | `src/components/ai/AiChatMessage.tsx` | 消息渲染（含 Markdown + 代码块） |
| SqlConnectionView 集成 | `src/windows/connection/` | 侧边栏 + 工具栏按钮 |
| Chat 快捷命令解析 | `src/components/ai/AiChatPanel.tsx` | /explain, /optimize 等 |

### Phase 4 — 智能筛选（1 周）

| 任务 | 位置 | 说明 |
|------|------|------|
| NlFilterInput 组件 | `src/components/ai/NlFilterInput.tsx` | 自然语言筛选输入 |
| TableView 工具栏集成 | `src/windows/connection/` | 嵌入筛选区域 |
| AI 筛选条件解析后端 | `src-tauri/src/commands/ai.rs` | 新增 ai_parse_filter 命令 |

### Phase 5 — 增值功能（2-3 周）

- Schema 文档自动生成
- 数据导入智能映射
- 连接故障排查
- 查询历史智能管理

---

## 安全与隐私

| 关注点 | 策略 |
|--------|------|
| API Key 存储 | 随 Store 整体 AES-256-GCM 加密，复用现有加密机制 |
| 数据外发 | 默认仅发送 Schema 元数据（表名、列名、类型、约束），不发送数据行 |
| 本地模型 | Ollama 支持确保数据不离开本机 |
| 传输安全 | 所有 API 请求通过 HTTPS（Ollama 除外，本地通信） |
| 日志脱敏 | tracing 日志中不记录 API Key、不记录 SQL 中的具体数据值 |

## 竞品对比

| 功能 | DataGrip | DBeaver | TablePlus | **DataZen (规划)** |
|------|----------|---------|-----------|-------------------|
| NL2SQL | ✅ | ✅ (插件) | ❌ | ✅ |
| 错误诊断 | ✅ | ❌ | ❌ | ✅ |
| EXPLAIN 可视化 | ✅ | ✅ | ❌ | ✅ + AI 解读 |
| AI Chat | ✅ | ❌ | ❌ | ✅ |
| 多 Provider | ❌ (仅 JetBrains AI) | ❌ | ❌ | ✅ (OpenAI/Claude/Ollama/Custom) |
| 本地模型 | ❌ | ❌ | ❌ | ✅ Ollama (差异化) |
| 智能筛选 | ❌ | ❌ | ❌ | ✅ (差异化) |
| 插件化 Provider | ❌ | ❌ | ❌ | ✅ register_ai_provider! |

## 第五部分：MCP Server — 将 DataZen 作为 AI 工具服务

### 5.1 概述

DataZen 作为 MCP Server 运行，通过标准 MCP 协议暴露数据库操作能力，使外部 LLM 应用（Claude Desktop、Cursor、自定义 Agent 等）可以直接调用 DataZen 管理的数据库连接来执行查询、浏览 Schema、分析数据等。

同时，DataZen 自身的 AI 模块也通过 MCP 协议与 LLM 交互，使内置 AI 功能和外部 MCP 工具共享同一套能力层。

```
┌─────────────────────────────────────────────────────────────────┐
│                    外部 MCP Clients                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐              │
│  │ Claude   │  │ Cursor   │  │ Custom Agent     │              │
│  │ Desktop  │  │ IDE      │  │ (Python/TS/etc.) │              │
│  └────┬─────┘  └────┬─────┘  └────────┬─────────┘              │
│       │              │                 │                        │
│       └──────────────┼─────────────────┘                        │
│                      │ MCP Protocol                             │
│               (Streamable HTTP / stdio)                         │
│                      │                                          │
├──────────────────────▼──────────────────────────────────────────┤
│                DataZen MCP Server                                │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │  Tools                                                     │ │
│  │  ├── query          (执行 SQL 查询)                        │ │
│  │  ├── get_schema     (获取表结构)                           │ │
│  │  ├── list_tables    (列出表名)                             │ │
│  │  ├── list_databases (列出数据库)                           │ │
│  │  ├── explain_query  (获取执行计划)                         │ │
│  │  ├── describe_table (表的详细信息)                         │ │
│  │  └── run_skill      (执行用户自定义 Skill)                │ │
│  ├────────────────────────────────────────────────────────────┤ │
│  │  Resources                                                 │ │
│  │  ├── datazen://connections          (连接列表)             │ │
│  │  ├── datazen://schema/{conn}/{db}   (数据库 Schema)       │ │
│  │  ├── datazen://query-history/{conn} (查询历史)            │ │
│  │  └── datazen://skills               (可用 Skills)         │ │
│  ├────────────────────────────────────────────────────────────┤ │
│  │  Prompts                                                   │ │
│  │  ├── nl2sql          (自然语言→SQL 模板)                  │ │
│  │  ├── diagnose-error  (错误诊断模板)                       │ │
│  │  └── explain-plan    (执行计划分析模板)                   │ │
│  └────────────────────────────────────────────────────────────┘ │
│                      │                                          │
│                 共享 AppState                                    │
│                      │                                          │
│  ┌───────────────────▼────────────────────────────────────────┐ │
│  │  DriverRegistry · ConnectionManager · SchemaCache · Store  │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 依赖

```toml
# src-tauri/Cargo.toml 新增依赖
[dependencies]
rmcp = { version = "3", features = ["server", "transport-io", "transport-streamable-http-server", "macros"] }
schemars = "0.8"
```

### 5.3 MCP Server 实现 (`src-tauri/src/mcp/`)

```
src-tauri/src/mcp/
├── mod.rs               # 模块入口 + MCP Server 启动逻辑
├── server.rs            # DataZenMcpServer struct + ServerHandler
├── tools.rs             # #[tool] 定义：query, get_schema, list_tables 等
├── resources.rs         # Resource 定义：connections, schema, history
├── prompts.rs           # Prompt 模板定义
└── transport.rs         # 传输层管理（stdio + Streamable HTTP）
```

#### `server.rs` — MCP Server Handler

```rust
use crate::commands::AppState;
use crate::mcp::tools::*;
use rmcp::handler::server::wrapper::Json;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// DataZen MCP Server
///
/// 持有 AppState 的引用，通过 MCP 协议暴露数据库操作能力。
/// 遵循与内置驱动相同的模式：单例 + 内部共享状态。
#[derive(Clone)]
pub struct DataZenMcpServer {
    app_state: Arc<AppState>,
    skill_registry: Arc<SkillRegistry>,
}

impl DataZenMcpServer {
    pub fn new(app_state: Arc<AppState>, skill_registry: Arc<SkillRegistry>) -> Self {
        Self { app_state, skill_registry }
    }
}

impl ServerHandler for DataZenMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new("datazen", "0.4.0")
            .with_capabilities(
                ServerCapabilities::builder()
                    .enable_tools()
                    .enable_resources()
                    .enable_prompts()
                    .build(),
            )
    }
}

// ─── Tool 输入类型 ───

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryInput {
    /// The connection ID to use (from list_connections)
    pub connection_id: String,
    /// SQL query to execute
    pub sql: String,
    /// Maximum number of rows to return (default: 100)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTablesInput {
    /// The connection ID to use
    pub connection_id: String,
    /// Optional database name (for multi-database systems)
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSchemaInput {
    /// The connection ID to use
    pub connection_id: String,
    /// Table name to get schema for
    pub table: String,
    /// Optional database name
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainQueryInput {
    /// The connection ID to use
    pub connection_id: String,
    /// SQL query to analyze
    pub sql: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DescribeTableInput {
    /// The connection ID to use
    pub connection_id: String,
    /// Table name
    pub table: String,
    /// Optional database name
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunSkillInput {
    /// Skill ID to execute
    pub skill_id: String,
    /// Input variables for the skill (JSON object)
    pub variables: serde_json::Value,
    /// Optional connection ID (some skills require a database connection)
    pub connection_id: Option<String>,
}

// ─── Tool 实现 ───

#[tool(tool_box)]
impl DataZenMcpServer {
    /// List all configured database connections with their status
    #[tool(description = "List all configured database connections. Returns connection IDs, names, types, and connection status.")]
    pub async fn list_connections(&self) -> Result<String, anyhow::Error> {
        let connections = self.app_state.store.get_connections().await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let result: Vec<serde_json::Value> = connections.iter().map(|c| {
            serde_json::json!({
                "id": c.id,
                "name": c.name,
                "databaseType": format!("{:?}", c.database_type),
                "host": c.host,
                "database": c.database,
            })
        }).collect();

        serde_json::to_string_pretty(&result)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Execute a SQL query on a connected database
    #[tool(description = "Execute a SQL query on a connected database. Returns query results as JSON. Use list_connections first to get valid connection IDs.")]
    pub async fn query(
        &self,
        #[tool(aggr)] input: Json<QueryInput>,
    ) -> Result<String, anyhow::Error> {
        let limit = input.limit.unwrap_or(100);
        let (driver, handle) = self.app_state.connection_manager
            .get_connection(&input.connection_id).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let result = driver.query_multi(&handle, &input.sql, Some(limit as usize)).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        serde_json::to_string_pretty(&result)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// List all tables in a database
    #[tool(description = "List all tables in a database. Returns table names with row counts.")]
    pub async fn list_tables(
        &self,
        #[tool(aggr)] input: Json<ListTablesInput>,
    ) -> Result<String, anyhow::Error> {
        let (driver, handle) = self.app_state.connection_manager
            .get_connection(&input.connection_id).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let tables = driver.get_tables(&handle, input.database.as_deref()).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        serde_json::to_string_pretty(&tables)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Get the schema (columns, types, constraints) of a table
    #[tool(description = "Get detailed schema of a table including columns, types, primary keys, foreign keys, and indexes.")]
    pub async fn get_schema(
        &self,
        #[tool(aggr)] input: Json<GetSchemaInput>,
    ) -> Result<String, anyhow::Error> {
        let db_type = {
            let (driver, _) = self.app_state.connection_manager
                .get_connection(&input.connection_id).await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            driver.driver_type()
        };

        let schema = self.app_state.schema_cache
            .get_table_schema(&db_type, &handle, &input.table, input.database.as_deref())
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        serde_json::to_string_pretty(&schema)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Get the query execution plan (EXPLAIN)
    #[tool(description = "Get the execution plan for a SQL query. Useful for performance analysis and optimization.")]
    pub async fn explain_query(
        &self,
        #[tool(aggr)] input: Json<ExplainQueryInput>,
    ) -> Result<String, anyhow::Error> {
        let (driver, handle) = self.app_state.connection_manager
            .get_connection(&input.connection_id).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let result = driver.explain(&handle, &input.sql).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        serde_json::to_string_pretty(&result)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Get a human-readable description of a table
    #[tool(description = "Get a human-readable description of a table including column details, relationships, and sample DDL.")]
    pub async fn describe_table(
        &self,
        #[tool(aggr)] input: Json<DescribeTableInput>,
    ) -> Result<String, anyhow::Error> {
        // 组合 schema + DDL 信息
        let (driver, handle) = self.app_state.connection_manager
            .get_connection(&input.connection_id).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let schema = driver.get_table_schema(&handle, &input.table, input.database.as_deref()).await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let mut desc = format!("Table: {}\n\nColumns:\n", input.table);
        for col in &schema.columns {
            desc.push_str(&format!(
                "  - {} {} {}{}{}\n",
                col.name,
                col.data_type,
                if col.is_primary_key { "PK " } else { "" },
                if col.is_nullable == Some(false) { "NOT NULL " } else { "" },
                col.foreign_key.as_ref().map(|fk|
                    format!("FK→{}.{}", fk.referenced_table, fk.referenced_column)
                ).unwrap_or_default(),
            ));
        }

        if !schema.indexes.is_empty() {
            desc.push_str("\nIndexes:\n");
            for idx in &schema.indexes {
                desc.push_str(&format!("  - {} ({})\n", idx.name, idx.columns.join(", ")));
            }
        }

        Ok(desc)
    }

    /// List all available database types
    #[tool(description = "List all database types supported by DataZen (e.g., PostgreSQL, MySQL, SQLite, Redis).")]
    pub async fn list_database_types(&self) -> Result<String, anyhow::Error> {
        let types = self.app_state.driver_registry.list_types().await;
        serde_json::to_string_pretty(&types)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Execute a user-defined skill
    #[tool(description = "Execute a user-defined skill by ID. Skills are reusable workflows combining prompts and database operations. Use list_skills resource to see available skills.")]
    pub async fn run_skill(
        &self,
        #[tool(aggr)] input: Json<RunSkillInput>,
    ) -> Result<String, anyhow::Error> {
        let skill = self.skill_registry.get(&input.skill_id)
            .ok_or_else(|| anyhow::anyhow!("Skill '{}' not found", input.skill_id))?;

        skill.execute(
            &self.app_state,
            input.connection_id.as_deref(),
            &input.variables,
        ).await
    }
}
```

#### `resources.rs` — MCP Resources

```rust
use rmcp::model::{Resource, ResourceTemplate, ResourceContents};
use crate::commands::AppState;
use std::sync::Arc;

impl DataZenMcpServer {
    /// 实现 ServerHandler 的 list_resources
    pub async fn list_resources(&self) -> Vec<Resource> {
        vec![
            Resource {
                uri: "datazen://connections".into(),
                name: "Database Connections".into(),
                description: Some("All configured database connections".into()),
                mime_type: Some("application/json".into()),
            },
            Resource {
                uri: "datazen://skills".into(),
                name: "Available Skills".into(),
                description: Some("User-defined skills that can be executed".into()),
                mime_type: Some("application/json".into()),
            },
        ]
    }

    /// Resource 模板（动态 URI）
    pub async fn list_resource_templates(&self) -> Vec<ResourceTemplate> {
        vec![
            ResourceTemplate {
                uri_template: "datazen://schema/{connectionId}/{database}".into(),
                name: "Database Schema".into(),
                description: Some("Full schema for a specific database connection".into()),
                mime_type: Some("application/json".into()),
            },
            ResourceTemplate {
                uri_template: "datazen://query-history/{connectionId}".into(),
                name: "Query History".into(),
                description: Some("Recent query history for a connection".into()),
                mime_type: Some("application/json".into()),
            },
        ]
    }

    /// 读取资源
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContents, String> {
        match uri {
            "datazen://connections" => {
                let connections = self.app_state.store.get_connections().await
                    .map_err(|e| e.to_string())?;
                let json = serde_json::to_string_pretty(&connections)
                    .map_err(|e| e.to_string())?;
                Ok(ResourceContents::text(json, uri))
            }
            "datazen://skills" => {
                let skills = self.skill_registry.list();
                let json = serde_json::to_string_pretty(&skills)
                    .map_err(|e| e.to_string())?;
                Ok(ResourceContents::text(json, uri))
            }
            uri if uri.starts_with("datazen://schema/") => {
                // 解析 connectionId 和 database
                let parts: Vec<&str> = uri.strip_prefix("datazen://schema/")
                    .unwrap().splitn(2, '/').collect();
                let conn_id = parts.first().ok_or("Missing connection ID")?;
                let db = parts.get(1).map(|s| *s);
                self.read_schema_resource(conn_id, db).await
            }
            _ => Err(format!("Unknown resource: {}", uri)),
        }
    }
}
```

#### `prompts.rs` — MCP Prompts

```rust
use rmcp::model::{Prompt, PromptArgument, PromptMessage};

impl DataZenMcpServer {
    pub async fn list_prompts(&self) -> Vec<Prompt> {
        vec![
            Prompt {
                name: "nl2sql".into(),
                description: Some("Convert natural language to SQL based on database schema".into()),
                arguments: Some(vec![
                    PromptArgument {
                        name: "connection_id".into(),
                        description: Some("Database connection to use for schema context".into()),
                        required: Some(true),
                    },
                    PromptArgument {
                        name: "question".into(),
                        description: Some("Natural language description of the query".into()),
                        required: Some(true),
                    },
                ]),
            },
            Prompt {
                name: "diagnose-error".into(),
                description: Some("Diagnose a SQL error and suggest fixes".into()),
                arguments: Some(vec![
                    PromptArgument {
                        name: "connection_id".into(),
                        description: Some("Database connection".into()),
                        required: Some(true),
                    },
                    PromptArgument {
                        name: "sql".into(),
                        description: Some("The SQL that caused the error".into()),
                        required: Some(true),
                    },
                    PromptArgument {
                        name: "error".into(),
                        description: Some("The error message".into()),
                        required: Some(true),
                    },
                ]),
            },
            Prompt {
                name: "explain-plan".into(),
                description: Some("Analyze a query execution plan and suggest optimizations".into()),
                arguments: Some(vec![
                    PromptArgument {
                        name: "connection_id".into(),
                        description: Some("Database connection".into()),
                        required: Some(true),
                    },
                    PromptArgument {
                        name: "sql".into(),
                        description: Some("The SQL query".into()),
                        required: Some(true),
                    },
                ]),
            },
        ]
    }

    /// 获取 prompt，填充 Schema 上下文后返回
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: &HashMap<String, String>,
    ) -> Result<Vec<PromptMessage>, String> {
        match name {
            "nl2sql" => {
                let conn_id = arguments.get("connection_id")
                    .ok_or("Missing connection_id")?;
                let question = arguments.get("question")
                    .ok_or("Missing question")?;

                let context = self.build_schema_context(conn_id).await?;

                Ok(vec![
                    PromptMessage::system(format!(
                        "You are a SQL expert. Generate executable SQL.\n\nDatabase: {}\nSchema:\n{}",
                        context.database_type, context.schema_ddl
                    )),
                    PromptMessage::user(question.clone()),
                ])
            }
            // ... 其他 prompt 类似
            _ => Err(format!("Unknown prompt: {}", name)),
        }
    }
}
```

#### `transport.rs` — 传输层管理

DataZen 支持两种 MCP 传输方式：

```rust
use crate::commands::AppState;
use crate::mcp::server::DataZenMcpServer;
use rmcp::ServiceExt;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, error};

/// 启动 Streamable HTTP 传输的 MCP Server
/// 外部客户端通过 HTTP 连接
pub async fn start_http_mcp_server(
    app_state: Arc<AppState>,
    skill_registry: Arc<SkillRegistry>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = DataZenMcpServer::new(app_state, skill_registry);
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    info!("MCP Server listening on http://127.0.0.1:{}", port);

    // rmcp 的 Streamable HTTP server
    rmcp::transport::streamable_http_server::StreamableHttpService::new(
        move || server.clone(),
    )
    .serve(listener)
    .await?;

    Ok(())
}

/// 启动 stdio 传输的 MCP Server
/// 通过命令行参数 `--mcp-stdio` 激活
pub async fn start_stdio_mcp_server(
    app_state: Arc<AppState>,
    skill_registry: Arc<SkillRegistry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = DataZenMcpServer::new(app_state, skill_registry);
    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
```

#### `mod.rs` — MCP 模块入口

```rust
pub mod server;
mod tools;      // #[tool] 定义在 server.rs 中，也可拆到此文件
mod resources;
mod prompts;
pub mod transport;
pub mod skills;  // Skill 系统

pub use server::DataZenMcpServer;
pub use skills::{Skill, SkillRegistry};
```

### 5.4 MCP Server 启动集成

在 `lib.rs` 中，MCP Server 作为后台服务启动：

```rust
// src-tauri/src/lib.rs

// 在 setup 回调中，AppState 创建之后：
let mcp_port = app_state.store.get_settings().await
    .map(|s| s.mcp_server_port)
    .unwrap_or(Some(9515));

if let Some(port) = mcp_port {
    let state_clone = Arc::new(app_state.clone());
    let skills_clone = skill_registry.clone();
    tokio::spawn(async move {
        if let Err(e) = mcp::transport::start_http_mcp_server(
            state_clone, skills_clone, port
        ).await {
            tracing::error!("MCP Server failed: {}", e);
        }
    });
}
```

### 5.5 前端 MCP 配置

Settings 页面新增 MCP Server 配置板块：

```typescript
// SettingsWindow 中新增 MCP Section

<SectionTitle>{t('settings.mcp.title')}</SectionTitle>

<ToggleRow
  label={t('settings.mcp.enable')}
  description={t('settings.mcp.enableDesc')}
  checked={draft.mcpServerEnabled}
  onChange={(v) => setDraft({ ...draft, mcpServerEnabled: v })}
/>

{draft.mcpServerEnabled && (
  <SettingRow label={t('settings.mcp.port')}>
    <Input
      type="number"
      value={draft.mcpServerPort ?? 9515}
      onChange={(e) => setDraft({ ...draft, mcpServerPort: Number(e.target.value) })}
    />
  </SettingRow>
)}
```

### 5.6 MCP Server 客户端配置示例

用户在外部 LLM 应用中配置 DataZen MCP Server：

**Claude Desktop (`claude_desktop_config.json`)**:
```json
{
  "mcpServers": {
    "datazen": {
      "url": "http://localhost:9515/mcp"
    }
  }
}
```

**Cursor (`.cursor/mcp.json`)**:
```json
{
  "mcpServers": {
    "datazen": {
      "url": "http://localhost:9515/mcp"
    }
  }
}
```

---

## 第六部分：Skills 系统 — 用户自定义 AI 工作流

### 6.1 概述

Skills 是用户自定义的可复用 AI 工作流，将 Prompt 模板、数据库操作、变量替换组合成一个可调用的单元。Skills 通过 MCP 的 `run_skill` tool 和 DataZen 内置 AI 功能两个入口被调用。

```
┌─────────────────────────────────────────────────────┐
│                 Skill 调用入口                       │
│                                                      │
│  ┌─────────────────┐    ┌─────────────────────────┐ │
│  │ MCP run_skill   │    │ DataZen AI Chat         │ │
│  │ (外部 LLM 调用) │    │ (内置 /skill 命令)     │ │
│  └────────┬────────┘    └────────────┬────────────┘ │
│           └──────────────┬───────────┘              │
│                          ▼                           │
│              ┌───────────────────────┐               │
│              │   Skill Registry     │               │
│              │   (运行时加载管理)    │               │
│              └───────────┬───────────┘               │
│                          ▼                           │
│              ┌───────────────────────┐               │
│              │   Skill Executor     │               │
│              │   1. 变量替换        │               │
│              │   2. SQL 执行 (可选) │               │
│              │   3. LLM 调用 (可选) │               │
│              │   4. 结果格式化      │               │
│              └───────────────────────┘               │
└─────────────────────────────────────────────────────┘
```

### 6.2 Skill 定义格式

Skills 以 YAML 文件定义，存储在用户数据目录下的 `skills/` 文件夹中：

```
~/.datazen/skills/
├── monthly-report.yaml
├── find-slow-queries.yaml
├── table-health-check.yaml
└── data-quality-audit.yaml
```

#### Skill YAML Schema

```yaml
# ~/.datazen/skills/monthly-report.yaml

# ─── 元数据 ───
id: monthly-report
name: 月度数据报告
description: 生成指定表的月度数据统计报告
version: "1.0"
author: user

# ─── 输入变量 ───
variables:
  - name: table_name
    type: string
    description: 要分析的表名
    required: true

  - name: date_column
    type: string
    description: 日期列名
    default: created_at

  - name: month
    type: string
    description: 目标月份 (YYYY-MM)
    default: "{{current_month}}"

# ─── 执行步骤 ───
steps:
  # Step 1: 查询数据量
  - id: row_count
    type: query
    sql: |
      SELECT COUNT(*) as total_rows
      FROM {{table_name}}
      WHERE {{date_column}} >= '{{month}}-01'
        AND {{date_column}} < '{{month}}-01'::date + INTERVAL '1 month'

  # Step 2: 查询每日分布
  - id: daily_distribution
    type: query
    sql: |
      SELECT DATE({{date_column}}) as day, COUNT(*) as count
      FROM {{table_name}}
      WHERE {{date_column}} >= '{{month}}-01'
        AND {{date_column}} < '{{month}}-01'::date + INTERVAL '1 month'
      GROUP BY day
      ORDER BY day

  # Step 3: AI 分析
  - id: analysis
    type: ai
    prompt: |
      基于以下数据，生成一份简洁的月度报告：

      表名: {{table_name}}
      月份: {{month}}
      总行数: {{steps.row_count.result}}
      每日分布:
      {{steps.daily_distribution.result}}

      请包含：
      1. 数据量趋势分析
      2. 异常日期标注
      3. 环比对比（如数据可推断）

# ─── 输出格式 ───
output:
  format: markdown
  template: |
    # {{table_name}} — {{month}} 月度报告

    ## 数据概览
    {{steps.row_count.result}}

    ## AI 分析
    {{steps.analysis.result}}
```

#### 更多 Skill 示例

```yaml
# ~/.datazen/skills/find-slow-queries.yaml
id: find-slow-queries
name: 慢查询发现
description: 分析 pg_stat_statements 找出慢查询并给出优化建议
variables:
  - name: min_duration_ms
    type: number
    description: 最小执行时间阈值（毫秒）
    default: 1000

steps:
  - id: slow_queries
    type: query
    sql: |
      SELECT query, calls, mean_exec_time, total_exec_time
      FROM pg_stat_statements
      WHERE mean_exec_time > {{min_duration_ms}}
      ORDER BY mean_exec_time DESC
      LIMIT 10

  - id: optimization
    type: ai
    prompt: |
      分析以下慢查询并给出优化建议（包括索引建议）：
      {{steps.slow_queries.result}}

output:
  format: markdown
```

```yaml
# ~/.datazen/skills/table-health-check.yaml
id: table-health-check
name: 表健康检查
description: 检查表的数据质量和健康状况
variables:
  - name: table_name
    type: string
    required: true

steps:
  - id: null_check
    type: query
    sql: |
      SELECT column_name,
             COUNT(*) - COUNT({{column_name}}) as null_count,
             ROUND(100.0 * (COUNT(*) - COUNT({{column_name}})) / COUNT(*), 2) as null_pct
      FROM information_schema.columns c
      CROSS JOIN LATERAL (
        SELECT * FROM {{table_name}} LIMIT 10000
      ) t
      WHERE c.table_name = '{{table_name}}'
      GROUP BY column_name
      HAVING COUNT(*) - COUNT({{column_name}}) > 0

  - id: duplicates
    type: query
    sql: |
      SELECT COUNT(*) - COUNT(DISTINCT *) as duplicate_rows
      FROM {{table_name}}

  - id: report
    type: ai
    prompt: |
      生成表 {{table_name}} 的健康报告:
      空值分析: {{steps.null_check.result}}
      重复行: {{steps.duplicates.result}}

output:
  format: markdown
```

### 6.3 Skill 后端实现

#### `src-tauri/src/mcp/skills.rs`

```rust
use crate::ai::{AiProviderConfig, ChatMessage, CompletionRequest, MessageRole};
use crate::commands::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ─── Skill 定义类型 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub variables: Vec<SkillVariable>,
    pub steps: Vec<SkillStep>,
    pub output: Option<SkillOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,       // "string" | "number" | "boolean"
    pub description: String,
    pub required: Option<bool>,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SkillStep {
    #[serde(rename = "query")]
    Query {
        id: String,
        sql: String,
    },
    #[serde(rename = "ai")]
    Ai {
        id: String,
        prompt: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    pub format: String,        // "markdown" | "json" | "text"
    pub template: Option<String>,
}

// ─── Skill Registry ───

pub struct SkillRegistry {
    skills: RwLock<HashMap<String, SkillDefinition>>,
    skills_dir: PathBuf,
}

impl SkillRegistry {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills: RwLock::new(HashMap::new()),
            skills_dir,
        }
    }

    /// 从文件系统加载所有 skill 定义
    pub async fn load_all(&self) -> Result<(), String> {
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        let mut skills = self.skills.write().await;
        skills.clear();

        let entries = std::fs::read_dir(&self.skills_dir)
            .map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml") {
                match self.load_skill_file(&path) {
                    Ok(skill) => {
                        info!("Loaded skill: {} ({})", skill.name, skill.id);
                        skills.insert(skill.id.clone(), skill);
                    }
                    Err(e) => {
                        warn!("Failed to load skill {:?}: {}", path, e);
                    }
                }
            }
        }

        info!("Loaded {} skills", skills.len());
        Ok(())
    }

    fn load_skill_file(&self, path: &PathBuf) -> Result<SkillDefinition, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
        serde_yaml::from_str::<SkillDefinition>(&content)
            .map_err(|e| format!("Failed to parse {:?}: {}", path, e))
    }

    pub async fn get(&self, id: &str) -> Option<SkillDefinition> {
        self.skills.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<SkillListItem> {
        self.skills.read().await.values().map(|s| SkillListItem {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            variables: s.variables.clone(),
        }).collect()
    }

    /// 保存用户创建的 skill
    pub async fn save_skill(&self, skill: &SkillDefinition) -> Result<(), String> {
        let yaml = serde_yaml::to_string(skill)
            .map_err(|e| e.to_string())?;
        let path = self.skills_dir.join(format!("{}.yaml", skill.id));
        std::fs::write(&path, yaml)
            .map_err(|e| e.to_string())?;
        self.skills.write().await.insert(skill.id.clone(), skill.clone());
        Ok(())
    }

    /// 删除 skill
    pub async fn delete_skill(&self, id: &str) -> Result<(), String> {
        let path = self.skills_dir.join(format!("{}.yaml", id));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        self.skills.write().await.remove(id);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variables: Vec<SkillVariable>,
}

// ─── Skill 执行引擎 ───

pub struct SkillExecutor;

impl SkillExecutor {
    /// 执行一个 Skill
    pub async fn execute(
        skill: &SkillDefinition,
        app_state: &AppState,
        connection_id: Option<&str>,
        variables: &serde_json::Value,
    ) -> Result<String, anyhow::Error> {
        let mut context = SkillContext::new(variables);

        // 填充内置变量
        context.set_builtin_variables();

        // 按顺序执行步骤
        for step in &skill.steps {
            match step {
                SkillStep::Query { id, sql } => {
                    let conn_id = connection_id
                        .ok_or_else(|| anyhow::anyhow!("Skill requires a database connection"))?;

                    let resolved_sql = context.resolve_template(sql)?;
                    let (driver, handle) = app_state.connection_manager
                        .get_connection(conn_id).await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                    let result = driver.query(&handle, &resolved_sql, Some(1000)).await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                    let result_str = serde_json::to_string_pretty(&result.rows)
                        .unwrap_or_default();
                    context.set_step_result(id, &result_str);
                }

                SkillStep::Ai { id, prompt } => {
                    let resolved_prompt = context.resolve_template(prompt)?;

                    // 使用 AI Provider 执行
                    let ai_config = app_state.store.get_ai_config().await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?
                        .ok_or_else(|| anyhow::anyhow!("AI not configured"))?;

                    let provider = app_state.ai_registry
                        .get(&ai_config.provider_type).await
                        .ok_or_else(|| anyhow::anyhow!("AI provider not available"))?;

                    let request = CompletionRequest {
                        request_id: uuid::Uuid::new_v4().to_string(),
                        model: ai_config.model.clone(),
                        messages: vec![ChatMessage {
                            role: MessageRole::User,
                            content: resolved_prompt,
                        }],
                        temperature: Some(0.3),
                        max_tokens: Some(4000),
                        stop: None,
                    };

                    let response = provider.complete(&request).await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                    context.set_step_result(id, &response.content);
                }
            }
        }

        // 格式化输出
        if let Some(ref output) = skill.output {
            if let Some(ref template) = output.template {
                context.resolve_template(template)
                    .map_err(|e| anyhow::anyhow!(e))
            } else {
                Ok(context.get_last_result().unwrap_or_default())
            }
        } else {
            Ok(context.get_last_result().unwrap_or_default())
        }
    }
}

/// Skill 执行上下文：变量替换 + 步骤结果
struct SkillContext {
    variables: HashMap<String, String>,
    step_results: HashMap<String, String>,
}

impl SkillContext {
    fn new(input: &serde_json::Value) -> Self {
        let mut variables = HashMap::new();
        if let Some(obj) = input.as_object() {
            for (k, v) in obj {
                variables.insert(k.clone(), match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                });
            }
        }
        Self {
            variables,
            step_results: HashMap::new(),
        }
    }

    fn set_builtin_variables(&mut self) {
        let now = chrono::Local::now();
        self.variables.insert(
            "current_month".into(),
            now.format("%Y-%m").to_string(),
        );
        self.variables.insert(
            "current_date".into(),
            now.format("%Y-%m-%d").to_string(),
        );
        self.variables.insert(
            "current_year".into(),
            now.format("%Y").to_string(),
        );
    }

    fn set_step_result(&mut self, step_id: &str, result: &str) {
        self.step_results.insert(step_id.into(), result.into());
    }

    fn get_last_result(&self) -> Option<String> {
        self.step_results.values().last().cloned()
    }

    /// 模板变量替换：{{var}} 和 {{steps.id.result}}
    fn resolve_template(&self, template: &str) -> Result<String, String> {
        let mut result = template.to_string();

        // 替换步骤结果引用 {{steps.xxx.result}}
        let step_re = regex::Regex::new(r"\{\{steps\.(\w+)\.result\}\}")
            .map_err(|e| e.to_string())?;
        result = step_re.replace_all(&result, |caps: &regex::Captures| {
            let step_id = &caps[1];
            self.step_results.get(step_id).cloned().unwrap_or_default()
        }).to_string();

        // 替换普通变量 {{var}}
        let var_re = regex::Regex::new(r"\{\{(\w+)\}\}")
            .map_err(|e| e.to_string())?;
        result = var_re.replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            self.variables.get(var_name).cloned().unwrap_or_default()
        }).to_string();

        Ok(result)
    }
}
```

### 6.4 Skill IPC 命令

```rust
// src-tauri/src/commands/ai.rs 中新增

#[tauri::command]
pub async fn skill_list(
    state: State<'_, AppState>,
) -> Result<Vec<SkillListItem>, String> {
    Ok(state.skill_registry.list().await)
}

#[tauri::command]
pub async fn skill_execute(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    skill_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<String, String> {
    let skill = state.skill_registry.get(&skill_id).await
        .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;

    SkillExecutor::execute(
        &skill,
        &state,
        connection_id.as_deref(),
        &variables,
    ).await
    .map_err(|e| log_err("skill_execute", &e))
}

#[tauri::command]
pub async fn skill_save(
    state: State<'_, AppState>,
    skill: SkillDefinition,
) -> Result<(), String> {
    state.skill_registry.save_skill(&skill).await
}

#[tauri::command]
pub async fn skill_delete(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<(), String> {
    state.skill_registry.delete_skill(&skill_id).await
}

#[tauri::command]
pub async fn skill_reload(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.skill_registry.load_all().await
}
```

### 6.5 前端 Skills 管理 UI

#### 命令封装 (`src/commands/ai.ts` 扩展)

```typescript
// 追加到 aiCommands 对象
skillList: () =>
  invoke<SkillListItem[]>('skill_list'),

skillExecute: (params: {
  skillId: string;
  variables: Record<string, unknown>;
  connectionId?: string;
}) =>
  invoke<string>('skill_execute', params),

skillSave: (skill: SkillDefinition) =>
  invoke<void>('skill_save', { skill }),

skillDelete: (skillId: string) =>
  invoke<void>('skill_delete', { skillId }),

skillReload: () =>
  invoke<void>('skill_reload'),
```

#### Skills Store 扩展 (`src/stores/aiStore.ts`)

```typescript
// aiStore 中新增 Skill 相关状态
interface AiStore {
  // ... 现有字段 ...

  // Skills
  skills: SkillListItem[];
  skillsLoading: boolean;
  skillExecutionResult: string | null;
  isExecutingSkill: boolean;

  loadSkills: () => Promise<void>;
  executeSkill: (params: {
    skillId: string;
    variables: Record<string, unknown>;
    connectionId?: string;
  }) => Promise<void>;
  clearSkillResult: () => void;
}
```

#### Skills Panel 组件

在 AI Chat 侧边栏中集成 Skill 快速执行面板：

```tsx
// src/components/ai/SkillsPanel.tsx

export function SkillsPanel({ connectionId }: { connectionId: string }) {
  const { t } = useI18n();
  const skills = useAiStore((s) => s.skills);
  const loadSkills = useAiStore((s) => s.loadSkills);
  const executeSkill = useAiStore((s) => s.executeSkill);
  const result = useAiStore((s) => s.skillExecutionResult);
  const isExecuting = useAiStore((s) => s.isExecutingSkill);
  const [selectedSkill, setSelectedSkill] = useState<SkillListItem | null>(null);
  const [variables, setVariables] = useState<Record<string, string>>({});

  useEffect(() => { loadSkills(); }, []);

  return (
    <div className="space-y-3">
      <h3 className="text-sm font-medium flex items-center gap-2">
        <WandIcon className="w-4 h-4" />
        {t('ai.skills.title')}
      </h3>

      {/* Skill 列表 */}
      <div className="space-y-1">
        {skills.map((skill) => (
          <button
            key={skill.id}
            className="w-full text-left px-3 py-2 rounded-md hover:bg-muted/50 text-sm"
            onClick={() => {
              setSelectedSkill(skill);
              // 初始化默认值
              const defaults: Record<string, string> = {};
              skill.variables.forEach((v) => {
                if (v.default) defaults[v.name] = String(v.default);
              });
              setVariables(defaults);
            }}
          >
            <div className="font-medium">{skill.name}</div>
            <div className="text-xs text-muted-foreground">{skill.description}</div>
          </button>
        ))}
      </div>

      {/* 选中 Skill 的变量输入 */}
      {selectedSkill && (
        <div className="p-3 rounded-md border border-border space-y-2">
          <h4 className="text-sm font-medium">{selectedSkill.name}</h4>

          {selectedSkill.variables.map((v) => (
            <div key={v.name}>
              <label className="text-xs text-muted-foreground">{v.description}</label>
              <Input
                value={variables[v.name] ?? ''}
                onChange={(e) => setVariables({ ...variables, [v.name]: e.target.value })}
                placeholder={v.default ? String(v.default) : v.name}
              />
            </div>
          ))}

          <Button
            size="sm"
            onClick={() => executeSkill({
              skillId: selectedSkill.id,
              variables,
              connectionId,
            })}
            disabled={isExecuting}
          >
            {isExecuting ? <LoadingSpinner /> : t('ai.skills.run')}
          </Button>
        </div>
      )}

      {/* 执行结果 */}
      {result && (
        <div className="p-3 rounded-md bg-muted/30 text-sm whitespace-pre-wrap">
          {result}
        </div>
      )}
    </div>
  );
}
```

#### AI Chat 中的 `/skill` 命令

在 `AiChatPanel` 中支持 `/skill` 快捷命令：

```typescript
// AiChatPanel.tsx 中的命令解析

const parseSlashCommand = (input: string): SlashCommand | null => {
  const match = input.match(/^\/(\w+)\s*(.*)/s);
  if (!match) return null;

  switch (match[1]) {
    case 'skill':
      return { type: 'skill', args: match[2] };
    case 'explain':
      return { type: 'explain', args: match[2] };
    case 'optimize':
      return { type: 'optimize', args: match[2] };
    default:
      return null;
  }
};

// 处理 /skill 命令
// 用法: /skill monthly-report table_name=users month=2026-07
const handleSkillCommand = async (args: string) => {
  const parts = args.split(/\s+/);
  const skillId = parts[0];
  const variables: Record<string, string> = {};
  parts.slice(1).forEach((part) => {
    const [key, value] = part.split('=');
    if (key && value) variables[key] = value;
  });

  await executeSkill({ skillId, variables, connectionId });
};
```

### 6.6 Skill 与 MCP Prompt 的关系

Skills 和 MCP Prompts 互相补充：

| 特性 | MCP Prompts | DataZen Skills |
|------|------------|----------------|
| 定义方式 | 代码中硬编码 | 用户自定义 YAML |
| 多步骤 | 单个 prompt | 多步骤工作流 |
| SQL 执行 | 通过 tool 调用 | 内置 query step |
| AI 调用 | 客户端控制 | 自动调用配置的 LLM |
| 变量 | 简单参数 | 类型化变量 + 默认值 + 步骤结果引用 |
| 调用方式 | MCP `get_prompt` | MCP `run_skill` tool / UI / Chat 命令 |

### 6.7 内置 Skills

DataZen 预装一组开箱即用的 Skills：

```
src-tauri/resources/builtin-skills/
├── table-summary.yaml          # 表数据概览
├── find-duplicates.yaml        # 查找重复数据
├── column-statistics.yaml      # 列统计分析
├── foreign-key-report.yaml     # 外键关系报告
├── index-recommendations.yaml  # 索引推荐
└── data-dictionary.yaml        # 数据字典生成
```

内置 Skills 在首次启动时复制到用户的 `~/.datazen/skills/` 目录，用户可自由修改。

---

## 第七部分：DataZen 作为 MCP Client

### 7.1 概述

除了作为 MCP Server 暴露能力，DataZen 还可以作为 MCP Client 连接外部 MCP Server，扩展 AI 助手的能力范围。

```
┌─────────────────────────────────────────────────┐
│                DataZen                            │
│                                                   │
│  ┌─────────────────────────────────────────────┐ │
│  │  AI Chat 助手                                │ │
│  │  ┌─────────┐  ┌──────────┐  ┌────────────┐ │ │
│  │  │ 内置    │  │ MCP      │  │ 外部 MCP   │ │ │
│  │  │ DB 工具 │  │ Server   │  │ Client     │ │ │
│  │  │ (直接)  │  │ (本地)   │  │ (连接外部) │ │ │
│  │  └─────────┘  └──────────┘  └──────┬─────┘ │ │
│  └─────────────────────────────────────┼───────┘ │
│                                        │         │
└────────────────────────────────────────┼─────────┘
                                         │
                    ┌────────────────────┼────────────────────┐
                    │                    │                     │
            ┌───────▼──────┐   ┌────────▼─────┐   ┌─────────▼────┐
            │ GitHub MCP   │   │ Slack MCP    │   │ Custom MCP   │
            │ Server       │   │ Server       │   │ Server       │
            └──────────────┘   └──────────────┘   └──────────────┘
```

### 7.2 外部 MCP Server 配置

在 Settings 中配置外部 MCP Servers：

```typescript
// AppSettings 扩展
interface McpServerConfig {
  id: string;
  name: string;
  transport: 'stdio' | 'http';
  // stdio 模式
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  // http 模式
  url?: string;
  headers?: Record<string, string>;
  enabled: boolean;
}

interface AppSettings {
  // ... 现有字段 ...
  mcpServers: McpServerConfig[];
}
```

### 7.3 MCP Client 实现

```rust
// src-tauri/src/mcp/client.rs

use rmcp::{ServiceExt, transport::{TokioChildProcess, ConfigureCommandExt}};
use rmcp::model::Tool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct McpClientManager {
    clients: RwLock<HashMap<String, McpClientInfo>>,
}

struct McpClientInfo {
    name: String,
    tools: Vec<Tool>,
    // client handle for calling tools
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// 连接一个外部 MCP Server
    pub async fn connect(&self, config: &McpServerConfig) -> Result<(), String> {
        match config.transport.as_str() {
            "stdio" => {
                let cmd = config.command.as_ref().ok_or("Missing command")?;
                let mut command = tokio::process::Command::new(cmd);
                if let Some(args) = &config.args {
                    command.args(args);
                }
                if let Some(env) = &config.env {
                    for (k, v) in env {
                        command.env(k, v);
                    }
                }
                command.configure_mcp();
                let transport = TokioChildProcess::new(&mut command)
                    .map_err(|e| e.to_string())?;

                // 启动 MCP client
                let client = ().serve(transport).await
                    .map_err(|e| e.to_string())?;

                // 获取可用工具
                let tools = client.list_tools(Default::default()).await
                    .map_err(|e| e.to_string())?;

                self.clients.write().await.insert(
                    config.id.clone(),
                    McpClientInfo {
                        name: config.name.clone(),
                        tools: tools.tools,
                    },
                );
                Ok(())
            }
            "http" => {
                // Streamable HTTP 客户端
                let url = config.url.as_ref().ok_or("Missing URL")?;
                // ... 使用 rmcp 的 StreamableHttpClientTransport
                Ok(())
            }
            _ => Err(format!("Unknown transport: {}", config.transport)),
        }
    }

    /// 获取所有已连接 MCP Server 的工具
    pub async fn all_tools(&self) -> Vec<(String, Tool)> {
        let clients = self.clients.read().await;
        clients.iter().flat_map(|(server_id, info)| {
            info.tools.iter().map(move |tool| (server_id.clone(), tool.clone()))
        }).collect()
    }

    /// 调用外部 MCP Server 的工具
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        // ... 通过对应 client 调用 tool
        todo!()
    }
}
```

### 7.4 AI Chat 中集成外部 MCP 工具

当 AI Chat 需要调用外部工具时，通过 Function Calling / Tool Use 能力实现：

```
用户提问 → AI 分析 → 选择调用:
  ├── 内置工具 (直接调用 DriverRegistry)
  ├── MCP Server 工具 (本地 MCP Tools)
  └── 外部 MCP 工具 (通过 McpClientManager)
```

---

## 更新后的实施路线

| 阶段 | 功能模块 | 预估周期 | 前置依赖 |
|------|---------|---------|---------|
| **Phase 0** | AI 设置页面 + Provider 抽象层 + API Key 存储 | 1 周 | 无 |
| **Phase 1** | NL2SQL + SQL 错误诊断 | 1-2 周 | Phase 0 |
| **Phase 2** | EXPLAIN 可视化 + AI 解读 | 1 周 | Phase 0 |
| **Phase 3** | AI Chat 侧边栏助手 | 2 周 | Phase 0 |
| **Phase 4** | MCP Server 基础（tools + resources） | 1-2 周 | Phase 0 |
| **Phase 5** | Skills 系统（定义 + 执行 + UI） | 2 周 | Phase 4 |
| **Phase 6** | MCP Prompts + MCP Client（连接外部 Server） | 1-2 周 | Phase 4 |
| **Phase 7** | 智能筛选 + SQL 补全增强 | 1 周 | Phase 0 |
| **Phase 8** | Schema 文档 + 数据导入 + 其他增值功能 | 2-3 周 | Phase 0-1 |

---

## 安全与隐私

| 关注点 | 策略 |
|--------|------|
| API Key 存储 | 随 Store 整体 AES-256-GCM 加密，复用现有加密机制 |
| 数据外发 | 默认仅发送 Schema 元数据（表名、列名、类型、约束），不发送数据行 |
| 本地模型 | Ollama 支持确保数据不离开本机 |
| 传输安全 | 所有 API 请求通过 HTTPS（Ollama 除外，本地通信） |
| 日志脱敏 | tracing 日志中不记录 API Key、不记录 SQL 中的具体数据值 |
| MCP Server 访问控制 | 默认仅监听 127.0.0.1，可配置白名单 |
| Skill SQL 注入 | 模板变量替换不直接拼接用户输入到 SQL，使用参数化查询 |
| 外部 MCP Server | 用户显式配置，显示工具列表供确认 |

## 竞品对比

| 功能 | DataGrip | DBeaver | TablePlus | **DataZen (规划)** |
|------|----------|---------|-----------|-------------------|
| NL2SQL | ✅ | ✅ (插件) | ❌ | ✅ |
| 错误诊断 | ✅ | ❌ | ❌ | ✅ |
| EXPLAIN 可视化 | ✅ | ✅ | ❌ | ✅ + AI 解读 |
| AI Chat | ✅ | ❌ | ❌ | ✅ |
| 多 Provider | ❌ (仅 JetBrains AI) | ❌ | ❌ | ✅ (OpenAI/Claude/Ollama/Custom) |
| 本地模型 | ❌ | ❌ | ❌ | ✅ Ollama (差异化) |
| 智能筛选 | ❌ | ❌ | ❌ | ✅ (差异化) |
| 插件化 Provider | ❌ | ❌ | ❌ | ✅ register_ai_provider! |
| **MCP Server** | ❌ | ❌ | ❌ | **✅ (独有)** |
| **MCP Client** | ❌ | ❌ | ❌ | **✅ (独有)** |
| **Skills 系统** | ❌ | ❌ | ❌ | **✅ (独有)** |

## 开放问题

1. **流式中断** — 用户取消生成时，需要中断 HTTP 请求。方案：`AbortHandle` + `cancel_token` 透传
2. **Token 用量展示** — 是否在 UI 中展示每次请求消耗？初期可在开发模式下显示
3. **多语言 Prompt** — 初期 Prompt 统一用英文（LLM 对英文理解更好），用户输入可为中/英文
4. **大 Schema 截断** — 数百张表的数据库，通过 `max_tokens_budget` 参数控制，优先包含当前表及其关联表
5. **Ollama 模型列表动态获取** — 需在 validate_config 阶段调用 `/api/tags` 获取并缓存
6. **错误诊断 JSON 解析失败** — LLM 不保证返回合法 JSON，需要 fallback 策略（正则提取或重试）
7. **MCP Server 认证** — 是否需要为 MCP Server 添加 Token 认证？初期仅本地访问可不加
8. **Skill 安全沙箱** — 用户自定义 Skill 的 SQL 是否需要权限控制（如禁止 DROP/DELETE）？
9. **MCP 2026-07-28 无状态** — 新版 MCP 无 session，每个请求自包含，需确认 rmcp 3.x 的正确用法
10. **Skill 编辑器** — 是否在 UI 中提供可视化 Skill 编辑器，还是仅支持 YAML 文件编辑？
