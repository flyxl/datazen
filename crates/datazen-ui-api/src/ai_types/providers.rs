//! Provider identity, the safety gate, model profiles and the persisted
//! settings config — React `types/ai.ts` lines 5–165.
//!
//! The provider type list is the one place a string union here spans 18
//! literals, several of them multi-word (`open_ai_compatible`,
//! `anthropic_compatible`); serde's `snake_case` rename covers all of them
//! except the deliberately-unhyphenated local-CLI spellings
//! `opencode`/`codebuddy`, which get explicit renames. The kernel twin
//! (`datazen::ai::domain::types::AiProviderType`) additionally accepts
//! legacy wire aliases (`minimax`, `open-code`, `code-buddy`, …) for old
//! encrypted configs; this UI mirror takes none of them — see the module
//! docs for the cfg-gated-dependency trade.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::memory::AiMemorySettings;
use super::skills::AiSkillsSettings;

/// React `AiProviderType` — the 18 provider spellings the UI and the
/// persisted config use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderType {
    OpenAi,
    Anthropic,
    DeepSeek,
    Ollama,
    Custom,
    Gemini,
    Qwen,
    MiniMax,
    OpenAiCompatible,
    AnthropicCompatible,
    ClaudeCode,
    Codex,
    Cursor,
    /// Local CLI agent; the wire spelling is the unhyphenated `opencode`
    /// (serde's snake_case would say `open_code`, so it is renamed here).
    #[serde(rename = "opencode")]
    OpenCode,
    Grok,
    Qoder,
    /// Local CLI agent; the wire spelling is the unhyphenated `codebuddy`.
    #[serde(rename = "codebuddy")]
    CodeBuddy,
    Pi,
}

impl AiProviderType {
    /// The wire spelling React (and its tests) knows this provider by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "open_ai",
            Self::Anthropic => "anthropic",
            Self::DeepSeek => "deep_seek",
            Self::Ollama => "ollama",
            Self::Custom => "custom",
            Self::Gemini => "gemini",
            Self::Qwen => "qwen",
            Self::MiniMax => "mini_max",
            Self::OpenAiCompatible => "open_ai_compatible",
            Self::AnthropicCompatible => "anthropic_compatible",
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::OpenCode => "opencode",
            Self::Grok => "grok",
            Self::Qoder => "qoder",
            Self::CodeBuddy => "codebuddy",
            Self::Pi => "pi",
        }
    }
}

/// React `AiAgentMode = 'ask' | 'agent'` — Ask answers from supplied
/// context only; Agent may use database and MCP tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiAgentMode {
    Ask,
    Agent,
}

impl AiAgentMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Agent => "agent",
        }
    }
}

/// React `AiDataEgressLevel` — how much row data may leave the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiDataEgressLevel {
    Strict,
    SampleMasked,
    Unrestricted,
}

impl AiDataEgressLevel {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::SampleMasked => "sample_masked",
            Self::Unrestricted => "unrestricted",
        }
    }
}

/// React `AiToolPermissionPolicy` — what the model may do with one tool
/// class (DB tools, MCP tools).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiToolPermissionPolicy {
    Disabled,
    ReadOnly,
    RequireConfirm,
    Unrestricted,
}

impl AiToolPermissionPolicy {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::ReadOnly => "read_only",
            Self::RequireConfirm => "require_confirm",
            Self::Unrestricted => "unrestricted",
        }
    }
}

/// React `AiSafetyGateConfig` — the per-profile safety gate. All twelve
/// fields are required on the React interface (the settings dialog always
/// materializes a full gate), so nothing here is `Option`.
///
/// Integer widths follow the kernel twin (`max_tokens: u32`, byte/round
/// caps `usize`, duration `u64`, retries `u32`) to keep a future adapter
/// conversion lossless; the wire carries them as plain JSON numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSafetyGateConfig {
    pub redact_credentials: bool,
    pub data_egress_level: AiDataEgressLevel,
    pub max_sample_rows: u32,
    pub db_tool_policy: AiToolPermissionPolicy,
    pub mcp_tool_policy: AiToolPermissionPolicy,
    pub require_sql_confirm: bool,
    pub max_context_bytes: usize,
    pub max_tool_rounds: usize,
    pub max_tool_tokens: usize,
    pub max_tool_result_bytes: usize,
    pub max_tool_duration_secs: u64,
    pub api_retries: u32,
}

/// React `AiModelProfile` — one persisted provider profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelProfile {
    pub id: String,
    pub name: String,
    pub provider_type: AiProviderType,
    /// Optional secret; the persisted store keeps it out of summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Custom endpoint URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Provider-specific extra config (pass-through).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
    pub safety_gate: AiSafetyGateConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

/// React `PromptTemplate` — a reusable prompt skeleton, scoped to the
/// database types it applies to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub content: String,
    /// Empty list = applies to every database type.
    pub database_types: Vec<String>,
    pub is_default: bool,
}

/// React `AiSettingsConfig` — the persisted `ai_config` root. `skills` is
/// optional because pre-T17 hosts returned the config without it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsConfig {
    pub active_profile_id: String,
    pub profiles: Vec<AiModelProfile>,
    pub global_instructions: String,
    pub prompt_templates: Vec<PromptTemplate>,
    pub memory: AiMemorySettings,
    /// Absent on settings returned by pre-T17 hosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<AiSkillsSettings>,
}

/// React `AiProviderConfig` — the connection half of a profile (what a
/// chat request actually needs: no ids, no safety gate).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfig {
    pub provider_type: AiProviderType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

/// React `ModelInfo` — one entry of a provider's model list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub context_window: u64,
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

/// React `ProviderListItem` — one entry of the provider preset list the
/// connection dialog shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListItem {
    pub provider_type: AiProviderType,
    pub display_name: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub default_endpoint: String,
    pub default_protocol: String,
    /// Model preselected by the preset; empty for compatible/custom
    /// endpoints (absent in React rather than empty — `None` here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test decode helper: fails the test with the serde error instead of
    /// silently defaulting (the wire round-trips must be exact).
    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    /// Every provider literal, one row per variant — the wire table is
    /// the contract; a dropped or misspelled variant fails here.
    #[test]
    fn provider_types_keep_the_react_literals() {
        let table = [
            (AiProviderType::OpenAi, "open_ai"),
            (AiProviderType::Anthropic, "anthropic"),
            (AiProviderType::DeepSeek, "deep_seek"),
            (AiProviderType::Ollama, "ollama"),
            (AiProviderType::Custom, "custom"),
            (AiProviderType::Gemini, "gemini"),
            (AiProviderType::Qwen, "qwen"),
            (AiProviderType::MiniMax, "mini_max"),
            (AiProviderType::OpenAiCompatible, "open_ai_compatible"),
            (AiProviderType::AnthropicCompatible, "anthropic_compatible"),
            (AiProviderType::ClaudeCode, "claude_code"),
            (AiProviderType::Codex, "codex"),
            (AiProviderType::Cursor, "cursor"),
            (AiProviderType::OpenCode, "opencode"),
            (AiProviderType::Grok, "grok"),
            (AiProviderType::Qoder, "qoder"),
            (AiProviderType::CodeBuddy, "codebuddy"),
            (AiProviderType::Pi, "pi"),
        ];
        assert_eq!(table.len(), 18, "the React union has exactly 18 members");
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            // The serde wire spelling is the same literal.
            assert_eq!(serde_json::to_value(value).ok(), Some(serde_json::json!(literal)));
            let parsed: AiProviderType = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
    }

    #[test]
    fn the_small_enums_keep_the_react_literals() {
        assert_eq!(AiAgentMode::Ask.as_str(), "ask");
        assert_eq!(AiAgentMode::Agent.as_str(), "agent");
        assert_eq!(AiDataEgressLevel::Strict.as_str(), "strict");
        assert_eq!(AiDataEgressLevel::SampleMasked.as_str(), "sample_masked");
        assert_eq!(AiDataEgressLevel::Unrestricted.as_str(), "unrestricted");
        assert_eq!(AiToolPermissionPolicy::Disabled.as_str(), "disabled");
        assert_eq!(AiToolPermissionPolicy::ReadOnly.as_str(), "read_only");
        assert_eq!(AiToolPermissionPolicy::RequireConfirm.as_str(), "require_confirm");
        assert_eq!(AiToolPermissionPolicy::Unrestricted.as_str(), "unrestricted");
        // serde round-trips the same spellings, both directions.
        for (literal, value) in [
            ("ask", AiAgentMode::Ask),
            ("agent", AiAgentMode::Agent),
        ] {
            let parsed: AiAgentMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
        for (literal, value) in [
            ("strict", AiDataEgressLevel::Strict),
            ("sample_masked", AiDataEgressLevel::SampleMasked),
            ("unrestricted", AiDataEgressLevel::Unrestricted),
        ] {
            let parsed: AiDataEgressLevel = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
        for (literal, value) in [
            ("disabled", AiToolPermissionPolicy::Disabled),
            ("read_only", AiToolPermissionPolicy::ReadOnly),
            ("require_confirm", AiToolPermissionPolicy::RequireConfirm),
            ("unrestricted", AiToolPermissionPolicy::Unrestricted),
        ] {
            let parsed: AiToolPermissionPolicy = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    /// The kernel's legacy wire aliases are intentionally *not* accepted
    /// here — the UI contract is exactly types/ai.ts.
    #[test]
    fn legacy_wire_aliases_are_rejected_like_the_react_union() {
        for alias in ["minimax", "openai_compatible", "open_code", "open-code", "code-buddy"] {
            assert!(
                serde_json::from_value::<AiProviderType>(serde_json::json!(alias)).is_err(),
                "alias {alias} is not a types/ai.ts spelling"
            );
        }
    }

    /// A full settings config round-trips through the wire unchanged,
    /// including the optional-key omission rules.
    #[test]
    fn a_settings_config_round_trips_the_wire() {
        let wire = serde_json::json!({
            "activeProfileId": "p1",
            "profiles": [{
                "id": "p1",
                "name": "GPT",
                "providerType": "open_ai",
                "apiKey": "secret",
                "endpoint": "https://api.openai.com/v1",
                "model": "gpt-4o",
                "maxTokens": 8192,
                "extra": { "temperature": 0.2 },
                "safetyGate": {
                    "redactCredentials": true,
                    "dataEgressLevel": "sample_masked",
                    "maxSampleRows": 3,
                    "dbToolPolicy": "read_only",
                    "mcpToolPolicy": "require_confirm",
                    "requireSqlConfirm": true,
                    "maxContextBytes": 32000,
                    "maxToolRounds": 30,
                    "maxToolTokens": 48000,
                    "maxToolResultBytes": 8192,
                    "maxToolDurationSecs": 300,
                    "apiRetries": 2
                },
                "isDefault": true
            }],
            "globalInstructions": "be terse",
            "promptTemplates": [{
                "id": "t1",
                "name": "Default",
                "content": "You are a SQL expert.",
                "databaseTypes": ["mysql", "postgres"],
                "isDefault": true
            }],
            "memory": {
                "enabled": false,
                "autoLearn": false,
                "cloudPolicy": "normal_only",
                "privacyAcknowledged": false
            }
        });
        let config: AiSettingsConfig = decode(wire.clone());
        assert_eq!(config.profiles[0].provider_type, AiProviderType::OpenAi);
        assert_eq!(
            config.profiles[0].safety_gate.data_egress_level,
            AiDataEgressLevel::SampleMasked
        );
        assert_eq!(
            config.profiles[0].safety_gate.mcp_tool_policy,
            AiToolPermissionPolicy::RequireConfirm
        );
        assert_eq!(config.profiles[0].max_tokens, Some(8192));
        assert_eq!(config.memory.cloud_policy.as_str(), "normal_only");
        // Pre-T17 host: the skills key is absent and stays absent.
        assert_eq!(config.skills, None);
        let back = serde_json::to_value(&config).ok();
        assert_eq!(back, Some(wire));
    }

    /// Option semantics: absent optional keys deserialize to `None` and
    /// `None` serializes back to absent (never an explicit null).
    #[test]
    fn optional_provider_fields_stay_absent_rather_than_null() {
        let wire = serde_json::json!({
            "providerType": "ollama",
            "model": "llama3"
        });
        let config: AiProviderConfig = decode(wire.clone());
        assert_eq!(config.api_key, None);
        assert_eq!(config.endpoint, None);
        assert_eq!(config.max_tokens, None);
        assert_eq!(config.extra, None);
        assert_eq!(serde_json::to_value(&config).ok(), Some(wire));

        let item: ProviderListItem = decode(serde_json::json!({
            "providerType": "custom",
            "displayName": "Custom",
            "supportsStreaming": false,
            "supportsTools": true,
            "defaultEndpoint": "",
            "defaultProtocol": "openai"
        }));
        assert_eq!(item.default_model, None);
        let item_wire = serde_json::to_value(&item).ok();
        assert_eq!(
            item_wire.and_then(|wire| wire.get("defaultModel").cloned()),
            None
        );
    }

    #[test]
    fn a_minimal_profile_needs_only_the_required_fields() {
        let wire = serde_json::json!({
            "id": "p2",
            "name": "Local",
            "providerType": "ollama",
            "model": "llama3",
            "safetyGate": {
                "redactCredentials": false,
                "dataEgressLevel": "strict",
                "maxSampleRows": 5,
                "dbToolPolicy": "disabled",
                "mcpToolPolicy": "disabled",
                "requireSqlConfirm": false,
                "maxContextBytes": 1000,
                "maxToolRounds": 1,
                "maxToolTokens": 2,
                "maxToolResultBytes": 3,
                "maxToolDurationSecs": 4,
                "apiRetries": 5
            }
        });
        let profile: AiModelProfile = decode(wire.clone());
        assert_eq!(profile.api_key, None);
        assert_eq!(profile.is_default, None);
        assert_eq!(profile.safety_gate.api_retries, 5);
        assert_eq!(serde_json::to_value(&profile).ok(), Some(wire));
    }
}
