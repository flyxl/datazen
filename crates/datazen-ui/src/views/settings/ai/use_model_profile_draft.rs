//! Port of `packages/frontend/src/windows/settings/ai/useModelProfileDraft.ts`.
//!
//! The draft/validate/save state machine behind the settings'
//! `ModelProfileDialog` and the first-run journey's AI step: the tab, the
//! profile draft (provider, key, endpoint, model, custom protocol, local CLI
//! paths, safety gate, default flag), the validate badge, the high-risk
//! egress confirmation and the save payload. One implementation, so the
//! journey can never drift from Settings → AI.
//!
//! ## What maps where
//!
//! - The hook's `useAiStore()` half is a host concern, not this struct's:
//!   `providers` / `remoteModels` / `fetchingRemoteModels` / `validating` /
//!   `configError` live in the caller's state (the `ai_store.rs` slice) and
//!   are passed into the methods that read them; `fetchRemoteModels` /
//!   `validateConfig` / `onSave` / `onSaved` become the host's effect
//!   handling of the payloads this machine hands out. No IPC happens here.
//! - `useState` fields become [`ModelProfileDraft`]: `tab`, `draft`,
//!   `customProtocol`, `manualModelInput`, `validateOk` (a
//!   `validate_ok_until` deadline), `showHighRiskModal`, `savingState`.
//! - The `useEffect` on `[profile, open, clearError]` is [`ModelProfileDraft::reseed`]:
//!   the host calls it when the dialog (re)opens or the target profile
//!   changes. React resets `draft` (per branch), `tab` and `validateOk`;
//!   `clearError()` is the caller's store clear. The effect does NOT reset
//!   `customProtocol` (except when the profile is custom with an `extra`
//!   protocol), `manualModelInput`, `showHighRiskModal` or `savingState` —
//!   neither does the reseed; the stale-`customProtocol` quirk is ported
//!   verbatim.
//! - The provider-metadata effect ("fill the default endpoint once it
//!   arrives") is [`ModelProfileDraft::fill_provider_defaults`]: the host
//!   calls it whenever the provider list changes; it only fills blank
//!   fields, so extra calls are harmless (React's effect is equally
//!   idempotent).
//! - `crypto.randomUUID()` is host-side (the `ai_context_settings.rs`
//!   ruling): [`ModelProfileDraft::reseed`] takes the new profile's id from
//!   the caller.
//! - React's `setTimeout(() => setValidateOk(false), 3000)` is the
//!   `validate_ok_until` deadline: [`ModelProfileDraft::note_validate`]
//!   records `now + 3.0` and the host calls `tx.wake_after(3s)`; the view
//!   reads [`ModelProfileDraft::validate_ok`] with a caller-computed `now`.
//! - React's controlled inputs keep the draft fields as plain strings. The
//!   port keeps `TextEdit` buffers (`name`, `api_key`, `endpoint`, `model`,
//!   `max_tokens`, `local_cli_path`, `local_cli_workspace`) because Rune
//!   inputs edit `TextEdit`s; `updateDraft(patch)` was a pure spread with no
//!   side effects, so a direct buffer write is its exact counterpart.
//!   `handleLocalCliPathChange`'s `setValidateOk(false)` half is
//!   [`ModelProfileDraft::note_cli_path_edit`], which the CLI fields' edit
//!   accessors call (the text half is the buffer write itself; the
//!   `extra`-merge half is [`ModelProfileDraft::effective_extra`] below).
//! - `extra` handling: React keeps `draft.extra` as a JSON object and
//!   merges CLI-path keystrokes into it (`{...d.extra, [key]: value}`),
//!   replaces it wholesale on protocol change and on provider change. The
//!   port keeps `extra` as the passthrough blob and derives the effective
//!   value at read time ([`effective_extra`]): custom ⇒ always exactly
//!   `{protocol}` (React's save/protocol-change both write that shape),
//!   local CLI ⇒ the stored blob with the two path keys overlaid from the
//!   buffers (so unknown keys survive exactly as React's spread keeps
//!   them), anything else ⇒ the stored blob unchanged.
//! - `providerDefaults = useMemo(normalizeAiProviders(providers))` becomes
//!   [`normalize_ai_providers`], rebuilt per call (React memoizes because
//!   React; 18 entries are cheap). The needed slices of
//!   `lib/aiProviders.ts` (PROVIDER_PRESETS, LOCAL_CLI_PROVIDER_TYPES,
//!   PROVIDER_ORDER, `isLocalCliProvider`, `mergeProvider`,
//!   `normalizeAiProviders`) and of `lib/aiSafetyPresets.ts`
//!   (AI_SAFETY_PRESETS, `getDefaultSafetyGate`, `detectSafetyPreset`,
//!   `isLocalEndpoint`) are carried here because no earlier port had them;
//!   `ModelProfileListItem` (T143) reuses the provider-preset and
//!   preset-detection halves.
//! - `performSave`'s await is host-side: [`ModelProfileDraft::request_save`]
//!   returns [`SaveRequest`] — either the payload for the host's save
//!   callback or the high-risk confirmation (React's `'confirming'`
//!   outcome); `'saved'` vs `'failed'` is the host's save result, which
//!   decides whether the `onSaved` side effect (dialog close / journey
//!   advance) runs. `savingState` stays a field the host flips around its
//!   save ([`begin_save`] / [`end_save`]).
//! - `maxTokens`' `parseInt(e.target.value, 10) || 200000` is
//!   [`js_parse_int`] + the 200000 fallback ([`max_tokens_value`]).
//!
//! ## Known deltas (documented, not silent)
//!
//! - React's optional string fields distinguish absent from empty
//!   (`apiKey: undefined` vs `apiKey: ''`). The port's [`save_payload`]
//!   writes `None` for empty `apiKey`/`endpoint` where React would keep a
//!   present-empty string, and always materializes `maxTokens` /
//!   `isDefault` (React leaves them absent when editing a profile that had
//!   none and the fields stay untouched). Every consumer treats absent and
//!   empty alike; only the wire's key presence differs.
//! - `validateConfig`'s payload keeps React's raw (untrimmed) field values;
//!   the api-crate [`datazen_ui_api::AiProviderConfig`] is the typed mirror
//!   of the object React passes, so the host converts at its seam.
//!
//! Keys: this is a logic port; the view modules (`model_profile_form.rs`,
//! `model_profile_dialog.rs`, `model_profile_list_item.rs`) own the element
//! keys. The React `data-testid`s they must keep are listed there.

// The settings AI section (T172) and the first-run journey's AI step are the
// production consumers; until they land the form/dialog/list-item ports and
// this crate's tests are the only callers.
#![allow(dead_code)]

use std::time::Duration;

use rune::TextEdit;
use serde_json::{json, Value};

use datazen_ui_api::{
    AiDataEgressLevel, AiModelProfile, AiProviderConfig, AiProviderType, AiSafetyGateConfig,
    AiToolPermissionPolicy, ProviderListItem,
};

/// React's validate-badge timeout (`setTimeout(..., 3000)`), for the host's
/// `tx.wake_after` and the `validate_ok_until` deadline.
pub(crate) const VALIDATE_OK_FEEDBACK: Duration = Duration::from_millis(3000);
/// React `createEmptyProfile`'s `maxTokens` (and the number input's parse
/// fallback): 200000.
pub(crate) const DEFAULT_MAX_TOKENS: u32 = 200_000;

// ---- lib/aiSafetyPresets.ts slice ------------------------------------------

/// React `AiSafetyPresetType` (`lib/aiSafetyPresets.ts`): the three named
/// presets. `'custom'` is not a key — it is only ever a detection result
/// ([`crate::components::ai::AiSafetyPreset`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SafetyPresetKey {
    CloudStrict,
    EnterpriseBalanced,
    LocalTrust,
}

impl SafetyPresetKey {
    /// The React literal (`'cloud_strict'` …).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SafetyPresetKey::CloudStrict => "cloud_strict",
            SafetyPresetKey::EnterpriseBalanced => "enterprise_balanced",
            SafetyPresetKey::LocalTrust => "local_trust",
        }
    }
}

/// React `AI_SAFETY_PRESETS[key]` / `getDefaultSafetyGate(key)`: the full
/// twelve-field gate of a named preset (a fresh clone every call, like the
/// spread).
pub(crate) fn default_safety_gate(kind: SafetyPresetKey) -> AiSafetyGateConfig {
    match kind {
        SafetyPresetKey::CloudStrict => AiSafetyGateConfig {
            redact_credentials: true,
            data_egress_level: AiDataEgressLevel::Strict,
            max_sample_rows: 0,
            db_tool_policy: AiToolPermissionPolicy::ReadOnly,
            mcp_tool_policy: AiToolPermissionPolicy::Disabled,
            require_sql_confirm: true,
            max_context_bytes: 32000,
            max_tool_rounds: 30,
            max_tool_tokens: 48000,
            max_tool_result_bytes: 8192,
            max_tool_duration_secs: 300,
            api_retries: 2,
        },
        SafetyPresetKey::EnterpriseBalanced => AiSafetyGateConfig {
            redact_credentials: true,
            data_egress_level: AiDataEgressLevel::SampleMasked,
            max_sample_rows: 3,
            db_tool_policy: AiToolPermissionPolicy::RequireConfirm,
            mcp_tool_policy: AiToolPermissionPolicy::RequireConfirm,
            require_sql_confirm: true,
            max_context_bytes: 32000,
            max_tool_rounds: 30,
            max_tool_tokens: 48000,
            max_tool_result_bytes: 8192,
            max_tool_duration_secs: 300,
            api_retries: 2,
        },
        SafetyPresetKey::LocalTrust => AiSafetyGateConfig {
            redact_credentials: true,
            data_egress_level: AiDataEgressLevel::Unrestricted,
            max_sample_rows: 10,
            db_tool_policy: AiToolPermissionPolicy::Unrestricted,
            mcp_tool_policy: AiToolPermissionPolicy::Unrestricted,
            require_sql_confirm: false,
            max_context_bytes: 64000,
            max_tool_rounds: 30,
            max_tool_tokens: 48000,
            max_tool_result_bytes: 8192,
            max_tool_duration_secs: 300,
            api_retries: 2,
        },
    }
}

/// React `detectSafetyPreset(gate)` on the typed gate: each preset is an
/// exact conjunction of the four policy fields, anything else is `custom`.
/// The comparison itself reuses `components/ai/ai_chat_header.rs`'s
/// [`crate::components::ai::detect_safety_preset`] (the T009-S3 port of the
/// same function on its four-field gate view).
pub(crate) fn detect_safety_preset(gate: &AiSafetyGateConfig) -> crate::components::ai::AiSafetyPreset {
    crate::components::ai::detect_safety_preset(&crate::components::ai::AiSafetyGate::new(
        gate.data_egress_level.as_str(),
        gate.db_tool_policy.as_str(),
        gate.mcp_tool_policy.as_str(),
        gate.require_sql_confirm,
    ))
}

/// React `isLocalEndpoint(endpoint)` (`lib/aiSafetyPresets.ts`): any of the
/// local host markers, case-insensitive. An empty endpoint is NOT local
/// (React's `if (!endpoint) return false`), so an unrestricted gate with a
/// blank endpoint still triggers the high-risk confirmation.
pub(crate) fn is_local_endpoint(endpoint: &str) -> bool {
    if endpoint.is_empty() {
        return false;
    }
    let lower = endpoint.to_lowercase();
    lower.contains("localhost")
        || lower.contains("127.0.0.1")
        || lower.contains("0.0.0.0")
        || lower.contains("192.168.")
        || lower.contains("10.")
        || lower.contains(":11434")
}

// ---- lib/aiProviders.ts slice ----------------------------------------------

/// React `LOCAL_CLI_PROVIDER_TYPES`: the eight local CLI agents, advertised
/// after the ten API presets.
pub(crate) const LOCAL_CLI_PROVIDER_TYPES: [AiProviderType; 8] = [
    AiProviderType::ClaudeCode,
    AiProviderType::Codex,
    AiProviderType::Cursor,
    AiProviderType::OpenCode,
    AiProviderType::Grok,
    AiProviderType::Qoder,
    AiProviderType::CodeBuddy,
    AiProviderType::Pi,
];

/// React `PROVIDER_ORDER`: the ten API presets in PRD order, then the local
/// CLI agents.
pub(crate) const PROVIDER_ORDER: [AiProviderType; 18] = [
    AiProviderType::Anthropic,
    AiProviderType::OpenAi,
    AiProviderType::Gemini,
    AiProviderType::DeepSeek,
    AiProviderType::Qwen,
    AiProviderType::MiniMax,
    AiProviderType::Ollama,
    AiProviderType::OpenAiCompatible,
    AiProviderType::AnthropicCompatible,
    AiProviderType::Custom,
    AiProviderType::ClaudeCode,
    AiProviderType::Codex,
    AiProviderType::Cursor,
    AiProviderType::OpenCode,
    AiProviderType::Grok,
    AiProviderType::Qoder,
    AiProviderType::CodeBuddy,
    AiProviderType::Pi,
];

/// React `isLocalCliProvider(value)`.
pub(crate) fn is_local_cli_provider(value: AiProviderType) -> bool {
    LOCAL_CLI_PROVIDER_TYPES.contains(&value)
}

/// The wire literal → typed provider, the counterpart of React's
/// `val as AiProviderType` cast at the select's edge (an unknown literal
/// answers `None`, where React would carry the bogus string).
pub(crate) fn provider_type_from_str(value: &str) -> Option<AiProviderType> {
    PROVIDER_ORDER.into_iter().find(|t| t.as_str() == value)
}

/// React `PROVIDER_PRESETS[type]`: canonical metadata for one provider.
/// The ten API presets first (PRD order), then the local CLI agents.
pub(crate) fn provider_preset(provider_type: AiProviderType) -> ProviderListItem {
    let both = |display: &str, endpoint: &str, protocol: &str, model: &str| ProviderListItem {
        provider_type,
        display_name: display.to_owned(),
        supports_streaming: true,
        supports_tools: true,
        default_endpoint: endpoint.to_owned(),
        default_protocol: protocol.to_owned(),
        default_model: (!model.is_empty()).then(|| model.to_owned()),
    };
    let cli = |display: &str| ProviderListItem {
        provider_type,
        display_name: display.to_owned(),
        supports_streaming: true,
        supports_tools: false,
        default_endpoint: String::new(),
        default_protocol: "local_cli".to_owned(),
        default_model: None,
    };
    match provider_type {
        AiProviderType::Anthropic => both(
            "Anthropic (Claude)",
            "https://api.anthropic.com",
            "anthropic_compatible",
            "claude-sonnet-4-20250514",
        ),
        AiProviderType::OpenAi => both(
            "OpenAI",
            "https://api.openai.com/v1",
            "open_ai_compatible",
            "gpt-4o",
        ),
        AiProviderType::Gemini => both(
            "Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai/",
            "open_ai_compatible",
            "gemini-2.5-pro",
        ),
        AiProviderType::DeepSeek => both(
            "DeepSeek",
            "https://api.deepseek.com",
            "open_ai_responses",
            "deepseek-chat",
        ),
        AiProviderType::Qwen => both(
            "Qwen",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "open_ai_compatible",
            "qwen-plus",
        ),
        AiProviderType::MiniMax => both(
            "MiniMax",
            "https://api.minimax.io/v1",
            "open_ai_compatible",
            "MiniMax-M3.1-Flash",
        ),
        AiProviderType::Ollama => both(
            "Ollama",
            "http://127.0.0.1:11434/v1",
            "open_ai_compatible",
            "llama3.2",
        ),
        AiProviderType::OpenAiCompatible => {
            both("OpenAI Compatible", "", "open_ai_compatible", "")
        }
        AiProviderType::AnthropicCompatible => {
            both("Anthropic Compatible", "", "anthropic_compatible", "")
        }
        AiProviderType::Custom => both("Custom", "", "open_ai_compatible", ""),
        AiProviderType::ClaudeCode => cli("Claude Code"),
        AiProviderType::Codex => cli("Codex"),
        AiProviderType::Cursor => cli("Cursor"),
        AiProviderType::OpenCode => cli("OpenCode"),
        AiProviderType::Grok => cli("Grok"),
        AiProviderType::Qoder => cli("Qoder"),
        AiProviderType::CodeBuddy => cli("CodeBuddy"),
        AiProviderType::Pi => cli("Pi"),
    }
}

/// React `mergeProvider`: the preset overlaid with the backend's entry —
/// the backend's streaming/tool bits win outright (the `...provider`
/// spread), while the identity strings fall back to the preset when the
/// backend's are empty (JS `||`).
fn merge_provider(provider: &ProviderListItem, preset: &ProviderListItem) -> ProviderListItem {
    ProviderListItem {
        provider_type: preset.provider_type,
        display_name: (!provider.display_name.is_empty())
            .then(|| provider.display_name.clone())
            .unwrap_or_else(|| preset.display_name.clone()),
        supports_streaming: provider.supports_streaming,
        supports_tools: provider.supports_tools,
        default_endpoint: (!provider.default_endpoint.is_empty())
            .then(|| provider.default_endpoint.clone())
            .unwrap_or_else(|| preset.default_endpoint.clone()),
        default_protocol: (!provider.default_protocol.is_empty())
            .then(|| provider.default_protocol.clone())
            .unwrap_or_else(|| preset.default_protocol.clone()),
        default_model: match provider.default_model.as_deref() {
            Some(model) if !model.is_empty() => Some(model.to_owned()),
            _ => preset.default_model.clone(),
        },
    }
}

/// React `normalizeAiProviders`: backend provider metadata normalized into
/// the eighteen canonical presets. Unknown types are skipped; DeepSeek and
/// Ollama always take the canonical preset (their backends historically
/// shipped stale defaults); missing entries are filled from
/// [`provider_preset`]; the result follows [`PROVIDER_ORDER`].
pub(crate) fn normalize_ai_providers(providers: &[ProviderListItem]) -> Vec<ProviderListItem> {
    let mut by_type: Vec<(AiProviderType, ProviderListItem)> = Vec::new();
    for provider in providers {
        // React's `isKnownProviderType` guard is vacuous on the typed mirror:
        // the host's serde parse already rejected unknown wire literals.
        let provider_type = provider.provider_type;
        let preset = provider_preset(provider_type);
        let merged = if provider_type == AiProviderType::DeepSeek || provider_type == AiProviderType::Ollama {
            preset
        } else {
            merge_provider(provider, &preset)
        };
        match by_type.iter_mut().find(|(t, _)| *t == provider_type) {
            Some(slot) => slot.1 = merged,
            None => by_type.push((provider_type, merged)),
        }
    }
    PROVIDER_ORDER
        .into_iter()
        .map(|t| {
            by_type
                .iter()
                .find(|(known, _)| *known == t)
                .map(|(_, item)| item.clone())
                .unwrap_or_else(|| provider_preset(t))
        })
        .collect()
}

// ---- pure helpers -----------------------------------------------------------

/// React `normalizeCustomProtocol(value: unknown)`: accept the pre-T16
/// `'anthropic'` spelling while saving canonical values; anything unknown
/// falls back to `'open_ai_compatible'`.
pub(crate) fn normalize_custom_protocol(value: &str) -> &'static str {
    match value {
        "anthropic" => "anthropic_compatible",
        "open_ai_compatible" => "open_ai_compatible",
        "open_ai_responses" => "open_ai_responses",
        "anthropic_compatible" => "anthropic_compatible",
        _ => "open_ai_compatible",
    }
}

/// JS `parseInt(value, 10)` for the practical inputs: optional leading
/// whitespace and sign, then leading digits; anything else is `NaN` (`None`).
fn js_parse_int(value: &str) -> Option<i64> {
    let trimmed = value.trim_start();
    let mut chars = trimmed.char_indices();
    let mut digits = String::new();
    if let Some((_, first)) = chars.next() {
        if first == '+' || first == '-' {
            digits.push(first);
        } else if first.is_ascii_digit() {
            digits.push(first);
        } else {
            return None;
        }
    } else {
        return None;
    }
    for (_, ch) in chars {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            break;
        }
    }
    digits.parse::<i64>().ok()
}

/// The draft's tab (`useState<'basic' | 'safety'>`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DraftTab {
    #[default]
    Basic,
    Safety,
}

/// Which local-CLI path a keystroke edits (`handleLocalCliPathChange`'s key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalCliPathField {
    ExecutablePath,
    WorkspaceDir,
}

/// The fetch-models request `fetchRemoteModels(protocol, endpoint, apiKey)`
/// receives; `None` is React's early return (no endpoint, or no key on a
/// non-Ollama provider).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FetchModelsRequest {
    pub protocol: String,
    pub endpoint: String,
    pub api_key: String,
}

/// What `requestSave` / `confirmHighRisk` ask the host to do. React's
/// `'confirming'` outcome is the [`SaveRequest::ConfirmHighRisk`] arm; the
/// `'saved'` / `'failed'` split is the host's save result.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SaveRequest {
    /// Show the high-risk egress confirmation; nothing is saved yet.
    ConfirmHighRisk,
    /// Hand this payload to the save callback (`performSave`'s
    /// `profileToSave`).
    Save(AiModelProfile),
}

/// Every `useState` of the React hook, plus the draft profile's editable
/// fields as `TextEdit` buffers (React's controlled inputs).
#[derive(Clone, Debug)]
pub(crate) struct ModelProfileDraft {
    pub tab: DraftTab,
    /// `draft.id` (the profile being edited, or the host's fresh uuid).
    pub id: String,
    pub name: TextEdit,
    pub provider_type: AiProviderType,
    pub api_key: TextEdit,
    pub endpoint: TextEdit,
    pub model: TextEdit,
    /// The number input's buffer; [`Self::max_tokens_value`] applies the
    /// `parseInt || 200000` rule.
    pub max_tokens: TextEdit,
    /// `draft.extra` passthrough (see [`Self::effective_extra`]).
    pub extra: Option<Value>,
    /// Buffers for `extra.executablePath` / `extra.workspaceDir`.
    pub local_cli_path: TextEdit,
    pub local_cli_workspace: TextEdit,
    pub safety_gate: AiSafetyGateConfig,
    pub is_default: bool,
    pub custom_protocol: String,
    pub manual_model_input: bool,
    /// When the validate badge expires (`tx.clock() + 3.0`); read via
    /// [`Self::validate_ok`].
    pub validate_ok_until: Option<f64>,
    pub show_high_risk_modal: bool,
    pub saving: bool,
}

impl Default for ModelProfileDraft {
    fn default() -> Self {
        Self {
            tab: DraftTab::Basic,
            id: String::new(),
            name: TextEdit::new(String::new()),
            provider_type: AiProviderType::OpenAi,
            api_key: TextEdit::new(String::new()),
            endpoint: TextEdit::new(String::new()),
            model: TextEdit::new(String::new()),
            max_tokens: TextEdit::new(DEFAULT_MAX_TOKENS.to_string()),
            extra: None,
            local_cli_path: TextEdit::new(String::new()),
            local_cli_workspace: TextEdit::new(String::new()),
            safety_gate: default_safety_gate(SafetyPresetKey::CloudStrict),
            is_default: false,
            custom_protocol: "open_ai_compatible".to_owned(),
            manual_model_input: false,
            validate_ok_until: None,
            show_high_risk_modal: false,
            saving: false,
        }
    }
}

/// React `createEmptyProfile(providers)`: a brand-new profile, seeded from
/// the provider metadata when available. The id comes from the caller
/// (`crypto.randomUUID()` is a host concern).
pub(crate) fn create_empty_profile(
    id: &str,
    providers: &[ProviderListItem],
) -> AiModelProfile {
    let default_provider = providers
        .iter()
        .find(|p| p.provider_type == AiProviderType::OpenAi);
    AiModelProfile {
        id: id.to_owned(),
        name: String::new(),
        provider_type: AiProviderType::OpenAi,
        api_key: None,
        endpoint: default_provider.map(|p| p.default_endpoint.clone()),
        model: default_provider
            .and_then(|p| p.default_model.clone())
            .unwrap_or_default(),
        max_tokens: Some(DEFAULT_MAX_TOKENS),
        extra: None,
        safety_gate: default_safety_gate(SafetyPresetKey::CloudStrict),
        is_default: Some(false),
    }
}

impl ModelProfileDraft {
    /// The `useEffect` on `[profile, open, clearError]`: re-seed the draft
    /// from the target profile (or a fresh empty one under `new_id`), reset
    /// the tab and the validate badge. `providers` is the store's raw list;
    /// the empty profile seeds from its normalization. The caller clears its
    /// store error afterwards (`clearError()`). What React's effect leaves
    /// alone, this leaves alone: `customProtocol` (unless the profile is
    /// custom with an `extra.protocol`), `manualModelInput`,
    /// `showHighRiskModal`, `saving`.
    pub(crate) fn reseed(
        &mut self,
        profile: Option<&AiModelProfile>,
        new_id: &str,
        providers: &[ProviderListItem],
    ) {
        match profile {
            Some(profile) => {
                self.id = profile.id.clone();
                self.name = TextEdit::new(profile.name.clone());
                self.provider_type = profile.provider_type;
                self.api_key =
                    TextEdit::new(profile.api_key.clone().unwrap_or_default());
                self.endpoint =
                    TextEdit::new(profile.endpoint.clone().unwrap_or_default());
                self.model = TextEdit::new(profile.model.clone());
                self.max_tokens = TextEdit::new(
                    profile
                        .max_tokens
                        .unwrap_or(DEFAULT_MAX_TOKENS)
                        .to_string(),
                );
                self.extra = profile.extra.clone();
                let extra = self.extra.as_ref();
                self.local_cli_path = TextEdit::new(
                    extra
                        .and_then(|e| e.get("executablePath"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                );
                self.local_cli_workspace = TextEdit::new(
                    extra
                        .and_then(|e| e.get("workspaceDir"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                );
                self.safety_gate = profile.safety_gate.clone();
                self.is_default = profile.is_default.unwrap_or(false);
                if profile.provider_type == AiProviderType::Custom {
                    if let Some(protocol) = extra
                        .and_then(|e| e.get("protocol"))
                        .and_then(Value::as_str)
                    {
                        self.custom_protocol =
                            normalize_custom_protocol(protocol).to_owned();
                    }
                }
            }
            None => {
                let fresh = create_empty_profile(new_id, &normalize_ai_providers(providers));
                self.id = fresh.id;
                self.name = TextEdit::new(fresh.name);
                self.provider_type = fresh.provider_type;
                self.api_key = TextEdit::new(String::new());
                self.endpoint = TextEdit::new(fresh.endpoint.unwrap_or_default());
                self.model = TextEdit::new(fresh.model);
                self.max_tokens =
                    TextEdit::new(fresh.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS).to_string());
                self.extra = None;
                self.local_cli_path = TextEdit::new(String::new());
                self.local_cli_workspace = TextEdit::new(String::new());
                self.safety_gate = fresh.safety_gate;
                self.is_default = false;
            }
        }
        self.tab = DraftTab::Basic;
        self.validate_ok_until = None;
    }

    /// The provider-metadata effect: fill a blank endpoint / model from the
    /// (now arrived) provider metadata, keeping what the user already typed.
    pub(crate) fn fill_provider_defaults(&mut self, providers: &[ProviderListItem]) {
        let Some(provider) = normalize_ai_providers(providers)
            .into_iter()
            .find(|p| p.provider_type == self.provider_type)
        else {
            return;
        };
        if self.endpoint.text.trim().is_empty() {
            self.endpoint = TextEdit::new(provider.default_endpoint);
        }
        if self.model.text.trim().is_empty() {
            self.model = TextEdit::new(provider.default_model.unwrap_or_default());
        }
    }

    // ---- derived values (the hook's computed locals) ------------------------

    /// `isCustom`.
    pub(crate) fn is_custom(&self) -> bool {
        self.provider_type == AiProviderType::Custom
    }

    /// `isOllama`.
    pub(crate) fn is_ollama(&self) -> bool {
        self.provider_type == AiProviderType::Ollama
    }

    /// `isLocalCli`.
    pub(crate) fn is_local_cli(&self) -> bool {
        is_local_cli_provider(self.provider_type)
    }

    /// `localCliPath` (`draft.extra?.executablePath` when it is a string):
    /// the buffer is that input's state, kept equal by
    /// [`Self::effective_extra`].
    pub(crate) fn local_cli_path(&self) -> &str {
        &self.local_cli_path.text
    }

    /// `localCliWorkspace`.
    pub(crate) fn local_cli_workspace(&self) -> &str {
        &self.local_cli_workspace.text
    }

    /// `selectedProvider`: the draft's provider in the normalized list
    /// (`providerDefaults.find(...)`).
    pub(crate) fn selected_provider(&self, providers: &[ProviderListItem]) -> Option<ProviderListItem> {
        normalize_ai_providers(providers)
            .into_iter()
            .find(|p| p.provider_type == self.provider_type)
    }

    /// `currentPreset = detectSafetyPreset(draft.safetyGate)`.
    pub(crate) fn current_preset(&self) -> crate::components::ai::AiSafetyPreset {
        detect_safety_preset(&self.safety_gate)
    }

    /// `hasCredentials`: Ollama and local CLIs need no key.
    pub(crate) fn has_credentials(&self) -> bool {
        self.is_ollama() || self.is_local_cli() || !self.api_key.text.trim().is_empty()
    }

    /// The validate badge shows while the deadline has not passed.
    pub(crate) fn validate_ok(&self, now: f64) -> bool {
        self.validate_ok_until.is_some_and(|until| until > now)
    }

    /// `draft.maxTokens ?? 200000` under the input's
    /// `parseInt(value, 10) || 200000` rule. A negative parse clamps to 0:
    /// the api-crate mirror types the field `u32`, so React's negative
    /// literals cannot round-trip (the input's `min=1000` already steered
    /// away from them).
    pub(crate) fn max_tokens_value(&self) -> u32 {
        match js_parse_int(&self.max_tokens.text) {
            Some(value) => value.clamp(0, u32::MAX as i64) as u32,
            None => DEFAULT_MAX_TOKENS,
        }
    }

    /// The effective `extra` the port saves / validates with: see the module
    /// docs for the custom / local-CLI / passthrough derivation.
    pub(crate) fn effective_extra(&self) -> Option<Value> {
        if self.is_custom() {
            Some(json!({ "protocol": self.custom_protocol }))
        } else if self.is_local_cli() {
            let mut extra = self.extra.clone().unwrap_or_else(|| json!({}));
            if let Some(object) = extra.as_object_mut() {
                object.insert("executablePath".to_owned(), json!(self.local_cli_path.text));
                object.insert("workspaceDir".to_owned(), json!(self.local_cli_workspace.text));
            }
            Some(extra)
        } else {
            self.extra.clone()
        }
    }

    // ---- actions ------------------------------------------------------------

    /// `handleProviderChange`: re-seed the draft's provider-dependent fields
    /// from the chosen provider's metadata (falling back to the preset),
    /// including the ollama key, the CLI blank endpoint, the strict default
    /// gate and the matching `extra` shape; reset the protocol, the manual
    /// model toggle and the validate badge.
    pub(crate) fn handle_provider_change(&mut self, provider_type: AiProviderType, providers: &[ProviderListItem]) {
        let provider = normalize_ai_providers(providers)
            .into_iter()
            .find(|p| p.provider_type == provider_type)
            .unwrap_or_else(|| provider_preset(provider_type));
        let is_ollama = provider_type == AiProviderType::Ollama;
        let is_cli = is_local_cli_provider(provider_type);
        let protocol = normalize_custom_protocol(&provider.default_protocol);
        self.provider_type = provider_type;
        self.api_key = TextEdit::new(if is_ollama { "ollama".to_owned() } else { String::new() });
        self.endpoint = TextEdit::new(if is_cli {
            String::new()
        } else {
            provider.default_endpoint.clone()
        });
        self.model = TextEdit::new(provider.default_model.unwrap_or_default());
        // A local CLI is a client of a vendor model, not a trusted local
        // endpoint; keep the same strict egress default as cloud presets.
        self.safety_gate = default_safety_gate(if is_ollama {
            SafetyPresetKey::LocalTrust
        } else {
            SafetyPresetKey::CloudStrict
        });
        self.extra = if is_cli {
            // React writes `{executablePath: '', workspaceDir: ''}`; the
            // buffers are that object's state (see `effective_extra`).
            Some(json!({ "executablePath": "", "workspaceDir": "" }))
        } else if provider_type == AiProviderType::Custom {
            Some(json!({ "protocol": protocol }))
        } else {
            None
        };
        self.local_cli_path = TextEdit::new(String::new());
        self.local_cli_workspace = TextEdit::new(String::new());
        self.custom_protocol = protocol.to_owned();
        self.manual_model_input = false;
        self.validate_ok_until = None;
    }

    /// `handleProtocolChange`: the custom-protocol choice replaces `extra`
    /// with exactly `{protocol}` (the derivation enforces the same shape).
    pub(crate) fn handle_protocol_change(&mut self, value: &str) {
        self.custom_protocol = value.to_owned();
        self.extra = Some(json!({ "protocol": value }));
    }

    /// `handleLocalCliPathChange`'s `setValidateOk(false)` half; the text
    /// half is the buffer write the field's edit accessor performs, and the
    /// `extra` merge is [`Self::effective_extra`].
    pub(crate) fn note_cli_path_edit(&mut self) {
        self.validate_ok_until = None;
    }

    /// `handleFetchModels`'s guard and request: `None` is the early return.
    pub(crate) fn fetch_models_request(&self, providers: &[ProviderListItem]) -> Option<FetchModelsRequest> {
        let endpoint = self.endpoint.text.trim();
        let api_key = {
            let key = self.api_key.text.trim();
            if key.is_empty() && self.is_ollama() {
                "ollama".to_owned()
            } else {
                key.to_owned()
            }
        };
        if endpoint.is_empty() || (api_key.is_empty() && !self.is_ollama()) {
            return None;
        }
        let protocol = if self.is_custom() {
            self.custom_protocol.clone()
        } else {
            normalize_custom_protocol(
                &self
                    .selected_provider(providers)
                    .map(|p| p.default_protocol)
                    .unwrap_or_default(),
            )
            .to_owned()
        };
        Some(FetchModelsRequest {
            protocol,
            endpoint: endpoint.to_owned(),
            api_key: if api_key.is_empty() { "ollama".to_owned() } else { api_key },
        })
    }

    /// `handleValidate`'s payload: the raw field values, with the custom
    /// protocol folded into `extra` (the api-crate mirror of the object
    /// React passes to `validateConfig`).
    pub(crate) fn validate_payload(&self) -> AiProviderConfig {
        AiProviderConfig {
            provider_type: self.provider_type,
            api_key: (!self.api_key.text.is_empty()).then(|| self.api_key.text.clone()),
            endpoint: (!self.endpoint.text.is_empty()).then(|| self.endpoint.text.clone()),
            model: self.model.text.clone(),
            max_tokens: Some(self.max_tokens_value()),
            extra: self.effective_extra(),
        }
    }

    /// The validate result: `ok` arms the 3s badge deadline (the host calls
    /// `tx.wake_after(VALIDATE_OK_FEEDBACK)`); a failure clears it.
    pub(crate) fn note_validate(&mut self, ok: bool, now: f64) {
        self.validate_ok_until = ok.then_some(now + VALIDATE_OK_FEEDBACK.as_secs_f64());
    }

    /// `applyPreset`: the draft's gate becomes the named preset.
    pub(crate) fn apply_preset(&mut self, kind: SafetyPresetKey) {
        self.safety_gate = default_safety_gate(kind);
    }

    /// `updateGate(key, val)`: one gate field. The form's selects, sliders
    /// and toggles each call their field's setter (React's generic key
    /// spread decomposed into typed setters).
    pub(crate) fn set_gate_data_egress(&mut self, level: AiDataEgressLevel) {
        self.safety_gate.data_egress_level = level;
    }

    pub(crate) fn set_gate_max_sample_rows(&mut self, rows: u32) {
        self.safety_gate.max_sample_rows = rows;
    }

    pub(crate) fn set_gate_max_tool_rounds(&mut self, rounds: usize) {
        self.safety_gate.max_tool_rounds = rounds;
    }

    pub(crate) fn set_gate_api_retries(&mut self, retries: u32) {
        self.safety_gate.api_retries = retries;
    }

    pub(crate) fn set_gate_max_tool_tokens(&mut self, tokens: usize) {
        self.safety_gate.max_tool_tokens = tokens;
    }

    pub(crate) fn set_gate_max_tool_result_bytes(&mut self, bytes: usize) {
        self.safety_gate.max_tool_result_bytes = bytes;
    }

    pub(crate) fn set_gate_max_tool_duration_secs(&mut self, secs: u64) {
        self.safety_gate.max_tool_duration_secs = secs;
    }

    pub(crate) fn set_gate_db_tool_policy(&mut self, policy: AiToolPermissionPolicy) {
        self.safety_gate.db_tool_policy = policy;
    }

    pub(crate) fn set_gate_mcp_tool_policy(&mut self, policy: AiToolPermissionPolicy) {
        self.safety_gate.mcp_tool_policy = policy;
    }

    pub(crate) fn set_gate_require_sql_confirm(&mut self, confirm: bool) {
        self.safety_gate.require_sql_confirm = confirm;
    }

    /// `performSave`'s `profileToSave`: the name fallback chain
    /// (`name.trim() || model || selectedProvider.displayName || providerType`),
    /// the custom protocol folded into `extra`, and the draft's fields.
    pub(crate) fn save_payload(&self, providers: &[ProviderListItem]) -> AiModelProfile {
        let selected = self.selected_provider(providers);
        let name = {
            let trimmed = self.name.text.trim();
            if !trimmed.is_empty() {
                trimmed.to_owned()
            } else if !self.model.text.is_empty() {
                self.model.text.clone()
            } else if selected
                .as_ref()
                .is_some_and(|p| !p.display_name.is_empty())
            {
                selected.map(|p| p.display_name).unwrap_or_default()
            } else {
                self.provider_type.as_str().to_owned()
            }
        };
        AiModelProfile {
            id: self.id.clone(),
            name,
            provider_type: self.provider_type,
            api_key: (!self.api_key.text.is_empty()).then(|| self.api_key.text.clone()),
            endpoint: (!self.endpoint.text.is_empty()).then(|| self.endpoint.text.clone()),
            model: self.model.text.clone(),
            max_tokens: Some(self.max_tokens_value()),
            extra: self.effective_extra(),
            safety_gate: self.safety_gate.clone(),
            is_default: Some(self.is_default),
        }
    }

    /// `requestSave`: unrestricted egress on a remote endpoint asks for the
    /// explicit confirmation first (`ModelProfileForm` renders that dialog);
    /// otherwise the host receives the payload for its save callback.
    pub(crate) fn request_save(&mut self, providers: &[ProviderListItem]) -> SaveRequest {
        let is_egress_unrestricted =
            self.safety_gate.data_egress_level == AiDataEgressLevel::Unrestricted;
        let is_remote = !is_local_endpoint(&self.endpoint.text);
        if is_egress_unrestricted && is_remote {
            self.show_high_risk_modal = true;
            SaveRequest::ConfirmHighRisk
        } else {
            SaveRequest::Save(self.save_payload(providers))
        }
    }

    /// `confirmHighRisk`: close the confirmation and hand the payload to the
    /// host's save callback.
    pub(crate) fn confirm_high_risk(&mut self, providers: &[ProviderListItem]) -> SaveRequest {
        self.show_high_risk_modal = false;
        SaveRequest::Save(self.save_payload(providers))
    }

    /// `performSave`'s `setSavingState(true)`; the host flips it back with
    /// [`Self::end_save`] when its save resolves.
    pub(crate) fn begin_save(&mut self) {
        self.saving = true;
    }

    /// `performSave`'s trailing `setSavingState(false)`.
    pub(crate) fn end_save(&mut self) {
        self.saving = false;
    }
}
