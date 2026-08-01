//! Shared types for AI providers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderType {
    OpenAi,
    Anthropic,
    Ollama,
    Custom,
}

impl std::fmt::Display for AiProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenAi => write!(f, "OpenAI"),
            Self::Anthropic => write!(f, "Anthropic"),
            Self::Ollama => write!(f, "Ollama"),
            Self::Custom => write!(f, "Custom"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfig {
    pub provider_type: AiProviderType,
    pub api_key: Option<String>,
    /// Custom endpoint URL (e.g. Ollama: http://localhost:11434)
    pub endpoint: Option<String>,
    pub model: String,
    /// Provider-specific extra config (pass-through)
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub context_window: u32,
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRequest {
    pub request_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

// ─── Error type (mirrors DriverError pattern) ───

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

    #[error("Context length exceeded: {used} tokens (max {limit})")]
    ContextLengthExceeded { used: u32, limit: u32 },

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

// ─── Feature-specific types ───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlGenerationContext {
    pub database_type: String,
    pub database_version: Option<String>,
    pub schema_ddl: String,
    pub current_table: Option<String>,
    pub recent_queries: Vec<String>,
}

impl Default for SqlGenerationContext {
    fn default() -> Self {
        Self {
            database_type: String::new(),
            database_version: None,
            schema_ddl: String::new(),
            current_table: None,
            recent_queries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisResult {
    pub explanation: String,
    pub suggested_sql: Option<String>,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainAnalysis {
    pub summary: String,
    pub bottlenecks: Vec<Bottleneck>,
    pub suggestions: Vec<Suggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bottleneck {
    pub node: String,
    pub description: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub description: String,
    pub sql: Option<String>,
    pub impact: String,
}
