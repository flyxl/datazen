//! Shared types for AI providers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderType {
    OpenAi,
    Anthropic,
    DeepSeek,
    Ollama,
    Custom,
}

impl std::fmt::Display for AiProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenAi => write!(f, "OpenAI"),
            Self::Anthropic => write!(f, "Anthropic"),
            Self::DeepSeek => write!(f, "DeepSeek"),
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
    /// Custom endpoint URL
    pub endpoint: Option<String>,
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Provider-specific extra config (pass-through)
    #[serde(default)]
    pub extra: serde_json::Value,
}

fn default_max_tokens() -> u32 {
    200_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiDataEgressLevel {
    Strict,
    SampleMasked,
    Unrestricted,
}

impl Default for AiDataEgressLevel {
    fn default() -> Self {
        Self::Strict
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiToolPermissionPolicy {
    Disabled,
    ReadOnly,
    RequireConfirm,
    Unrestricted,
}

impl Default for AiToolPermissionPolicy {
    fn default() -> Self {
        Self::ReadOnly
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSafetyGateConfig {
    #[serde(default = "default_true")]
    pub redact_credentials: bool,
    #[serde(default)]
    pub data_egress_level: AiDataEgressLevel,
    #[serde(default = "default_max_sample_rows")]
    pub max_sample_rows: u32,
    #[serde(default)]
    pub db_tool_policy: AiToolPermissionPolicy,
    #[serde(default)]
    pub mcp_tool_policy: AiToolPermissionPolicy,
    #[serde(default = "default_true")]
    pub require_sql_confirm: bool,
    #[serde(default = "default_max_context_bytes")]
    pub max_context_bytes: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_sample_rows() -> u32 {
    3
}

fn default_max_context_bytes() -> usize {
    32_000
}

impl Default for AiSafetyGateConfig {
    fn default() -> Self {
        Self {
            redact_credentials: true,
            data_egress_level: AiDataEgressLevel::Strict,
            max_sample_rows: 3,
            db_tool_policy: AiToolPermissionPolicy::ReadOnly,
            mcp_tool_policy: AiToolPermissionPolicy::ReadOnly,
            require_sql_confirm: true,
            max_context_bytes: 32_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelProfile {
    pub id: String,
    pub name: String,
    pub provider_type: AiProviderType,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub extra: serde_json::Value,
    #[serde(default)]
    pub safety_gate: AiSafetyGateConfig,
    #[serde(default)]
    pub is_default: bool,
}

impl From<&AiModelProfile> for AiProviderConfig {
    fn from(p: &AiModelProfile) -> Self {
        Self {
            provider_type: p.provider_type,
            api_key: p.api_key.clone(),
            endpoint: p.endpoint.clone(),
            model: p.model.clone(),
            max_tokens: p.max_tokens,
            extra: p.extra.clone(),
        }
    }
}

impl From<&AiProviderConfig> for AiModelProfile {
    fn from(c: &AiProviderConfig) -> Self {
        Self {
            id: "default".into(),
            name: format!("{} ({})", c.provider_type, c.model),
            provider_type: c.provider_type,
            api_key: c.api_key.clone(),
            endpoint: c.endpoint.clone(),
            model: c.model.clone(),
            max_tokens: c.max_tokens,
            extra: c.extra.clone(),
            safety_gate: AiSafetyGateConfig::default(),
            is_default: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsConfig {
    pub active_profile_id: String,
    pub profiles: Vec<AiModelProfile>,
}

impl Default for AiSettingsConfig {
    fn default() -> Self {
        Self {
            active_profile_id: String::new(),
            profiles: Vec::new(),
        }
    }
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

// ─── Tool calling types (protocol-agnostic) ───

/// JSON Schema definition for a tool parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's parameters.
    pub parameters: serde_json::Value,
}

/// A tool call emitted by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// Provider-assigned call id (used to correlate results).
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Result of a tool invocation, sent back to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
}

// ─── Messages ───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    /// Model reasoning/thinking content (for reasoning-capable models like DeepSeek thinking mode).
    /// Must be passed back in subsequent requests when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Tool calls requested by the assistant (present when role == Assistant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool result payload (present when role == Tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

// ─── Request / Response ───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionRequest {
    pub request_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Tools available for the model to call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// For OpenAI Responses API: reference a previous response to maintain
    /// server-side conversation state instead of re-sending full history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResponse {
    pub request_id: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    pub usage: TokenUsage,
    pub model: String,
    pub finish_reason: Option<String>,
    /// Tool calls requested by the model (present when finish_reason == "tool_calls").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// OpenAI Responses API: the server-assigned response ID for `previous_response_id` chaining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunk {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    pub done: bool,
    pub usage: Option<TokenUsage>,
    /// Accumulated tool calls (only present in the final chunk when done == true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// OpenAI Responses API: the server-assigned response ID (only in done chunk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
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
