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

/// Explicit chat mode: Ask answers from supplied context only; Agent may use
/// database and MCP tools to investigate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiAgentMode {
    Ask,
    Agent,
}

impl Default for AiAgentMode {
    fn default() -> Self {
        Self::Ask
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
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: usize,
    #[serde(default = "default_max_tool_tokens")]
    pub max_tool_tokens: usize,
    #[serde(default = "default_max_tool_result_bytes")]
    pub max_tool_result_bytes: usize,
    #[serde(default = "default_max_tool_duration_secs")]
    pub max_tool_duration_secs: u64,
    #[serde(default = "default_api_retries")]
    pub api_retries: u32,
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

fn default_max_tool_rounds() -> usize {
    30
}

fn default_max_tool_tokens() -> usize {
    48_000
}

fn default_max_tool_result_bytes() -> usize {
    8_192
}

fn default_max_tool_duration_secs() -> u64 {
    300
}

fn default_api_retries() -> u32 {
    2
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
            max_tool_rounds: 30,
            max_tool_tokens: 48_000,
            max_tool_result_bytes: 8_192,
            max_tool_duration_secs: 300,
            api_retries: 2,
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

/// Upper bounds shared by the settings store, IPC validation and the prompt
/// injector. They are UTF-8 byte limits, not character counts.
pub const MAX_GLOBAL_INSTRUCTIONS_BYTES: usize = 4 * 1024;
pub const MAX_PROMPT_TEMPLATE_BYTES: usize = 8 * 1024;
pub const MAX_PROMPT_TEMPLATES_TOTAL_BYTES: usize = 16 * 1024;
pub const MAX_CONNECTION_NOTES_BYTES: usize = 4 * 1024;

/// A user-authored system prompt template.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub content: String,
    /// Empty means the template is eligible for every database type.
    #[serde(default)]
    pub database_types: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsConfig {
    #[serde(default)]
    pub active_profile_id: String,
    #[serde(default)]
    pub profiles: Vec<AiModelProfile>,
    #[serde(default)]
    pub global_instructions: String,
    #[serde(default)]
    pub prompt_templates: Vec<PromptTemplate>,
}

impl AiSettingsConfig {
    /// Validate user-authored prompt limits. Reject rather than truncate: a
    /// silently shortened instruction or template is worse than a visible
    /// validation error.
    pub fn validate_prompt_limits(&self) -> Result<(), String> {
        if self.global_instructions.len() > MAX_GLOBAL_INSTRUCTIONS_BYTES {
            return Err(format!(
                "global instructions exceed {} bytes",
                MAX_GLOBAL_INSTRUCTIONS_BYTES
            ));
        }
        let mut total = 0usize;
        for template in &self.prompt_templates {
            if template.content.len() > MAX_PROMPT_TEMPLATE_BYTES {
                return Err(format!(
                    "prompt template '{}' exceeds {} bytes",
                    template.name, MAX_PROMPT_TEMPLATE_BYTES
                ));
            }
            total = total.saturating_add(template.content.len());
        }
        if total > MAX_PROMPT_TEMPLATES_TOTAL_BYTES {
            return Err(format!(
                "prompt templates exceed {} bytes in total",
                MAX_PROMPT_TEMPLATES_TOTAL_BYTES
            ));
        }
        Ok(())
    }

    /// Explicit template id first; otherwise the first default whose
    /// `databaseTypes` matches. A generic default (empty `databaseTypes`)
    /// covers an unmatched/absent database type. When neither matches, return
    /// `None`: never inject a template that targets another database.
    pub fn select_prompt_template(
        &self,
        template_id: Option<&str>,
        database_type: Option<&str>,
    ) -> Option<&PromptTemplate> {
        if let Some(id) = template_id.map(str::trim).filter(|id| !id.is_empty()) {
            if let Some(template) = self.prompt_templates.iter().find(|t| t.id == id) {
                return Some(template);
            }
        }
        let database_type = database_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase);
        let defaults: Vec<&PromptTemplate> = self
            .prompt_templates
            .iter()
            .filter(|template| template.is_default)
            .collect();
        if let Some(database_type) = database_type.as_deref() {
            if let Some(template) = defaults.iter().copied().find(|template| {
                template
                    .database_types
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(database_type))
            }) {
                return Some(template);
            }
        }
        defaults
            .iter()
            .copied()
            .find(|template| template.database_types.is_empty())
    }

    pub fn selected_prompt_template(
        &self,
        template_id: Option<&str>,
        database_type: Option<&str>,
    ) -> Option<&PromptTemplate> {
        self.select_prompt_template(template_id, database_type)
    }

    /// Resolve the selected model profile using the shared fallback order:
    /// active id, explicitly default profile, then the first configured one.
    pub fn selected_profile(&self) -> Option<&AiModelProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.active_profile_id)
            .or_else(|| self.profiles.iter().find(|profile| profile.is_default))
            .or_else(|| self.profiles.first())
    }
}

#[cfg(test)]
mod ai_settings_config_tests {
    use super::*;

    fn profile(id: &str, is_default: bool) -> AiModelProfile {
        AiModelProfile {
            id: id.into(),
            name: id.into(),
            provider_type: AiProviderType::OpenAi,
            api_key: None,
            endpoint: None,
            model: "model".into(),
            max_tokens: 1_000,
            extra: serde_json::Value::Null,
            safety_gate: AiSafetyGateConfig::default(),
            is_default,
        }
    }

    #[test]
    fn agent_mode_defaults_to_ask_and_uses_snake_case() {
        assert_eq!(AiAgentMode::default(), AiAgentMode::Ask);
        assert_eq!(serde_json::to_string(&AiAgentMode::Ask).unwrap(), "\"ask\"");
        assert_eq!(
            serde_json::to_string(&AiAgentMode::Agent).unwrap(),
            "\"agent\""
        );
        assert_eq!(
            serde_json::from_str::<AiAgentMode>("\"agent\"").unwrap(),
            AiAgentMode::Agent
        );
    }

    #[test]
    fn old_settings_without_prompt_fields_deserialize_with_defaults() {
        let config: AiSettingsConfig =
            serde_json::from_str(r#"{"activeProfileId":"p1","profiles":[]}"#).unwrap();
        assert_eq!(config.active_profile_id, "p1");
        assert!(config.global_instructions.is_empty());
        assert!(config.prompt_templates.is_empty());
        assert!(config.validate_prompt_limits().is_ok());
    }

    #[test]
    fn prompt_limits_are_utf8_bytes_and_reject_instead_of_truncating() {
        let mut config = AiSettingsConfig::default();
        config.global_instructions = "a".repeat(MAX_GLOBAL_INSTRUCTIONS_BYTES + 1);
        assert!(config.validate_prompt_limits().is_err());
        config.global_instructions = "中".repeat(MAX_GLOBAL_INSTRUCTIONS_BYTES / 3 + 1);
        assert!(config.validate_prompt_limits().is_err());

        let mut config = AiSettingsConfig::default();
        config.prompt_templates.push(PromptTemplate {
            id: "too-big".into(),
            name: "Too big".into(),
            content: "a".repeat(MAX_PROMPT_TEMPLATE_BYTES + 1),
            database_types: Vec::new(),
            is_default: true,
        });
        assert!(config.validate_prompt_limits().is_err());

        let mut config = AiSettingsConfig::default();
        for index in 0..3 {
            config.prompt_templates.push(PromptTemplate {
                id: format!("t{index}"),
                name: format!("Template {index}"),
                content: "a".repeat(MAX_PROMPT_TEMPLATE_BYTES),
                database_types: Vec::new(),
                is_default: index == 0,
            });
        }
        assert!(config.validate_prompt_limits().is_err());

        config.prompt_templates.truncate(2);
        assert!(config.validate_prompt_limits().is_ok());
    }

    #[test]
    fn selected_profile_prefers_active_then_default_then_first() {
        let config = AiSettingsConfig {
            active_profile_id: "active".into(),
            profiles: vec![
                profile("first", false),
                profile("default", true),
                profile("active", false),
            ],
            ..Default::default()
        };
        assert_eq!(
            config.selected_profile().map(|value| value.id.as_str()),
            Some("active")
        );

        let config = AiSettingsConfig {
            active_profile_id: "missing".into(),
            profiles: vec![profile("first", false), profile("default", true)],
            ..Default::default()
        };
        assert_eq!(
            config.selected_profile().map(|value| value.id.as_str()),
            Some("default")
        );

        let config = AiSettingsConfig {
            active_profile_id: "missing".into(),
            profiles: vec![profile("first", false), profile("second", false)],
            ..Default::default()
        };
        assert_eq!(
            config.selected_profile().map(|value| value.id.as_str()),
            Some("first")
        );
        assert!(AiSettingsConfig::default().selected_profile().is_none());
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
    /// Optional cancellation token for streaming requests. The protocol layer
    /// checks this before each SSE chunk and breaks the loop if cancelled.
    /// Not serialized over the wire — runtime only.
    #[serde(skip)]
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
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
    /// Whether this chunk was produced after the request was cancelled.
    #[serde(default)]
    pub cancelled: bool,
    pub usage: Option<TokenUsage>,
    /// Accumulated tool calls (only present in the final chunk when done == true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// OpenAI Responses API: the server-assigned response ID (only in done chunk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// Egress summary sent before the AI reply begins (FR-13).
    /// Contains a brief summary of connections/tables the AI has access to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress_summary: Option<String>,
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

    #[error("cancelled: {0}")]
    Cancelled(String),

    #[error("{0}")]
    Validation(String),

    #[error("Timeout after {0}s")]
    Timeout(u64),

    #[error("Internal error: {0}")]
    Internal(String),
}

// ─── Feature-specific types ───

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlGenerationContext {
    pub database_type: String,
    pub database_version: Option<String>,
    pub schema_ddl: String,
    pub current_table: Option<String>,
    pub recent_queries: Vec<String>,
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

#[cfg(test)]
mod safety_gate_serde_tests {
    use super::AiSafetyGateConfig;

    #[test]
    fn old_profiles_get_new_tool_loop_and_retry_defaults() {
        let gate: AiSafetyGateConfig =
            serde_json::from_str(r#"{"maxSampleRows":3}"#).expect("legacy gate");
        assert_eq!(gate.max_tool_rounds, 30);
        assert_eq!(gate.max_tool_tokens, 48_000);
        assert_eq!(gate.max_tool_result_bytes, 8_192);
        assert_eq!(gate.max_tool_duration_secs, 300);
        assert_eq!(gate.api_retries, 2);
    }

    #[test]
    fn configured_tool_loop_and_retry_values_round_trip() {
        let gate = AiSafetyGateConfig {
            max_tool_rounds: 12,
            max_tool_tokens: 9_000,
            max_tool_result_bytes: 1_024,
            max_tool_duration_secs: 45,
            api_retries: 0,
            ..AiSafetyGateConfig::default()
        };
        let encoded = serde_json::to_string(&gate).expect("serialize gate");
        let decoded: AiSafetyGateConfig = serde_json::from_str(&encoded).expect("deserialize gate");
        assert_eq!(decoded.max_tool_rounds, 12);
        assert_eq!(decoded.max_tool_tokens, 9_000);
        assert_eq!(decoded.max_tool_result_bytes, 1_024);
        assert_eq!(decoded.max_tool_duration_secs, 45);
        assert_eq!(decoded.api_retries, 0);
    }
}
