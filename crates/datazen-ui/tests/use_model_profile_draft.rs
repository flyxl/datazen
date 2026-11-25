//! T479: `windows/settings/ai/useModelProfileDraft.ts` — the draft state
//! machine behind the model-profile form. Every assertion comes from
//! re-reading the React hook (and the `lib/aiSafetyPresets.ts` /
//! `lib/aiProviders.ts` slices it carries): the preset tables, the provider
//! normalization rules, the reseed effect's exact reset set, the provider
//! change branches, the fetch/validate payloads, the save name chain and
//! the high-risk egress gate.

use datazen_ui_api::{
    AiDataEgressLevel, AiModelProfile, AiProviderType, AiSafetyGateConfig,
    AiToolPermissionPolicy, ProviderListItem,
};
use rune::TextEdit;

use datazen_ui::components::ai::AiSafetyPreset;
use datazen_ui::views::ai::use_model_profile_draft::{
    create_empty_profile, default_safety_gate, detect_safety_preset, is_local_endpoint,
    normalize_ai_providers, normalize_custom_protocol, provider_preset, provider_type_from_str,
    DraftTab, FetchModelsRequest, ModelProfileDraft, SafetyPresetKey, SaveRequest,
    DEFAULT_MAX_TOKENS, PROVIDER_ORDER,
};

fn provider(provider_type: AiProviderType, display: &str, endpoint: &str, model: Option<&str>) -> ProviderListItem {
    ProviderListItem {
        provider_type,
        display_name: display.to_owned(),
        supports_streaming: false,
        supports_tools: false,
        default_endpoint: endpoint.to_owned(),
        default_protocol: "open_ai_compatible".to_owned(),
        default_model: model.map(str::to_owned),
    }
}

fn gate(data_egress_level: AiDataEgressLevel) -> AiSafetyGateConfig {
    AiSafetyGateConfig {
        redact_credentials: true,
        data_egress_level,
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
    }
}

fn profile(id: &str, provider_type: AiProviderType) -> AiModelProfile {
    AiModelProfile {
        id: id.to_owned(),
        name: format!("Profile {id}"),
        provider_type,
        api_key: Some("sk-1".to_owned()),
        endpoint: Some("https://api.remote.com".to_owned()),
        model: "gpt-4o".to_owned(),
        max_tokens: Some(8192),
        extra: None,
        safety_gate: default_safety_gate(SafetyPresetKey::CloudStrict),
        is_default: Some(false),
    }
}

// ---- normalizeCustomProtocol -------------------------------------------------

#[test]
fn normalize_custom_protocol_accepts_the_legacy_and_canonical_spellings() {
    assert_eq!(normalize_custom_protocol("anthropic"), "anthropic_compatible");
    assert_eq!(normalize_custom_protocol("open_ai_compatible"), "open_ai_compatible");
    assert_eq!(normalize_custom_protocol("open_ai_responses"), "open_ai_responses");
    assert_eq!(normalize_custom_protocol("anthropic_compatible"), "anthropic_compatible");
    // Unknown values (including undefined's empty stand-in) fall back.
    assert_eq!(normalize_custom_protocol(""), "open_ai_compatible");
    assert_eq!(normalize_custom_protocol("bogus"), "open_ai_compatible");
}

// ---- the AI_SAFETY_PRESETS table and detection -------------------------------

#[test]
fn the_three_preset_gates_match_the_react_table() {
    let cloud = default_safety_gate(SafetyPresetKey::CloudStrict);
    assert!(cloud.redact_credentials);
    assert_eq!(cloud.data_egress_level, AiDataEgressLevel::Strict);
    assert_eq!(cloud.max_sample_rows, 0);
    assert_eq!(cloud.db_tool_policy, AiToolPermissionPolicy::ReadOnly);
    assert_eq!(cloud.mcp_tool_policy, AiToolPermissionPolicy::Disabled);
    assert!(cloud.require_sql_confirm);
    assert_eq!(cloud.max_context_bytes, 32000);
    assert_eq!(cloud.max_tool_rounds, 30);
    assert_eq!(cloud.max_tool_tokens, 48000);
    assert_eq!(cloud.max_tool_result_bytes, 8192);
    assert_eq!(cloud.max_tool_duration_secs, 300);
    assert_eq!(cloud.api_retries, 2);

    let balanced = default_safety_gate(SafetyPresetKey::EnterpriseBalanced);
    assert_eq!(balanced.data_egress_level, AiDataEgressLevel::SampleMasked);
    assert_eq!(balanced.max_sample_rows, 3);
    assert_eq!(balanced.db_tool_policy, AiToolPermissionPolicy::RequireConfirm);
    assert_eq!(balanced.mcp_tool_policy, AiToolPermissionPolicy::RequireConfirm);
    assert!(balanced.require_sql_confirm);
    assert_eq!(balanced.max_context_bytes, 32000);

    let local = default_safety_gate(SafetyPresetKey::LocalTrust);
    assert_eq!(local.data_egress_level, AiDataEgressLevel::Unrestricted);
    assert_eq!(local.max_sample_rows, 10);
    assert_eq!(local.db_tool_policy, AiToolPermissionPolicy::Unrestricted);
    assert_eq!(local.mcp_tool_policy, AiToolPermissionPolicy::Unrestricted);
    assert!(!local.require_sql_confirm);
    assert_eq!(local.max_context_bytes, 64000);
    assert_eq!(local.api_retries, 2);

    // The three literals.
    assert_eq!(SafetyPresetKey::CloudStrict.as_str(), "cloud_strict");
    assert_eq!(SafetyPresetKey::EnterpriseBalanced.as_str(), "enterprise_balanced");
    assert_eq!(SafetyPresetKey::LocalTrust.as_str(), "local_trust");
}

#[test]
fn detect_safety_preset_recognizes_the_presets_and_calls_the_rest_custom() {
    assert_eq!(detect_safety_preset(&default_safety_gate(SafetyPresetKey::CloudStrict)), AiSafetyPreset::CloudStrict);
    assert_eq!(detect_safety_preset(&default_safety_gate(SafetyPresetKey::EnterpriseBalanced)), AiSafetyPreset::EnterpriseBalanced);
    assert_eq!(detect_safety_preset(&default_safety_gate(SafetyPresetKey::LocalTrust)), AiSafetyPreset::LocalTrust);
    // One field off any preset is custom.
    let mut tweaked = default_safety_gate(SafetyPresetKey::CloudStrict);
    tweaked.mcp_tool_policy = AiToolPermissionPolicy::RequireConfirm;
    assert_eq!(detect_safety_preset(&tweaked), AiSafetyPreset::Custom);
    let mut flipped = default_safety_gate(SafetyPresetKey::LocalTrust);
    flipped.require_sql_confirm = true;
    assert_eq!(detect_safety_preset(&flipped), AiSafetyPreset::Custom);
}

#[test]
fn is_local_endpoint_matches_reacts_six_markers_case_insensitively() {
    assert!(is_local_endpoint("http://localhost:8080/v1"));
    assert!(is_local_endpoint("HTTP://LOCALHOST/v1"));
    assert!(is_local_endpoint("http://127.0.0.1:11434/v1"));
    assert!(is_local_endpoint("http://0.0.0.0/v1"));
    assert!(is_local_endpoint("http://192.168.1.10/v1"));
    assert!(is_local_endpoint("http://10.0.0.5/v1"));
    assert!(is_local_endpoint("http://my-host:11434"));
    assert!(!is_local_endpoint("https://api.openai.com/v1"));
    // React's `if (!endpoint) return false`: an empty endpoint is remote.
    assert!(!is_local_endpoint(""));
}

// ---- the PROVIDER_PRESETS table ---------------------------------------------

#[test]
fn provider_presets_keep_the_react_metadata() {
    let anthropic = provider_preset(AiProviderType::Anthropic);
    assert_eq!(anthropic.display_name, "Anthropic (Claude)");
    assert_eq!(anthropic.default_endpoint, "https://api.anthropic.com");
    assert_eq!(anthropic.default_protocol, "anthropic_compatible");
    assert_eq!(anthropic.default_model.as_deref(), Some("claude-sonnet-4-20250514"));
    assert!(anthropic.supports_streaming && anthropic.supports_tools);

    let openai = provider_preset(AiProviderType::OpenAi);
    assert_eq!(openai.default_endpoint, "https://api.openai.com/v1");
    assert_eq!(openai.default_model.as_deref(), Some("gpt-4o"));

    let deepseek = provider_preset(AiProviderType::DeepSeek);
    assert_eq!(deepseek.default_protocol, "open_ai_responses");

    let ollama = provider_preset(AiProviderType::Ollama);
    assert_eq!(ollama.default_endpoint, "http://127.0.0.1:11434/v1");
    assert_eq!(ollama.default_model.as_deref(), Some("llama3.2"));

    // The compatible/custom presets carry empty endpoints and no model.
    for t in [AiProviderType::OpenAiCompatible, AiProviderType::AnthropicCompatible, AiProviderType::Custom] {
        let preset = provider_preset(t);
        assert_eq!(preset.default_endpoint, "");
        assert_eq!(preset.default_model, None);
    }

    // Every local CLI agent: streaming only, local_cli protocol, no endpoint.
    for t in [
        AiProviderType::ClaudeCode, AiProviderType::Codex, AiProviderType::Cursor,
        AiProviderType::OpenCode, AiProviderType::Grok, AiProviderType::Qoder,
        AiProviderType::CodeBuddy, AiProviderType::Pi,
    ] {
        let preset = provider_preset(t);
        assert!(preset.supports_streaming);
        assert!(!preset.supports_tools);
        assert_eq!(preset.default_endpoint, "");
        assert_eq!(preset.default_protocol, "local_cli");
        assert_eq!(preset.default_model, None);
    }
}

#[test]
fn provider_type_literals_round_trip_through_the_select_edge() {
    assert_eq!(PROVIDER_ORDER.len(), 18);
    for t in PROVIDER_ORDER {
        assert_eq!(provider_type_from_str(t.as_str()), Some(t));
    }
    // The deliberately-unhyphenated local-CLI spellings.
    assert_eq!(provider_type_from_str("opencode"), Some(AiProviderType::OpenCode));
    assert_eq!(provider_type_from_str("codebuddy"), Some(AiProviderType::CodeBuddy));
    assert_eq!(provider_type_from_str("minimax"), None, "not a types/ai.ts spelling");
}

// ---- normalizeAiProviders ----------------------------------------------------

#[test]
fn normalize_fills_missing_presets_and_follows_the_prd_order() {
    // Old backend: only OpenAI, with metadata.
    let raw = vec![provider(AiProviderType::OpenAi, "OpenAI", "https://api.openai.com/v1", Some("gpt-4o"))];
    let normalized = normalize_ai_providers(&raw);
    assert_eq!(normalized.len(), 18);
    assert_eq!(normalized[0].provider_type, AiProviderType::Anthropic);
    assert_eq!(normalized[1].provider_type, AiProviderType::OpenAi);
    assert_eq!(normalized[1].display_name, "OpenAI");
    assert_eq!(normalized[9].provider_type, AiProviderType::Custom);
    assert_eq!(normalized[10].provider_type, AiProviderType::ClaudeCode);
    // Missing entries are the canonical presets.
    assert_eq!(normalized[0].default_endpoint, "https://api.anthropic.com");
}

#[test]
fn normalize_keeps_the_canonical_deep_seek_and_ollama_defaults() {
    let stale = vec![
        provider(AiProviderType::DeepSeek, "DeepSeek", "https://stale.example.com", Some("old-model")),
        provider(AiProviderType::Ollama, "Ollama", "http://wrong:1", None),
    ];
    let normalized = normalize_ai_providers(&stale);
    let deep_seek = normalized.iter().find(|p| p.provider_type == AiProviderType::DeepSeek).unwrap();
    assert_eq!(deep_seek.default_endpoint, "https://api.deepseek.com");
    assert_eq!(deep_seek.default_model.as_deref(), Some("deepseek-chat"));
    let ollama = normalized.iter().find(|p| p.provider_type == AiProviderType::Ollama).unwrap();
    assert_eq!(ollama.default_endpoint, "http://127.0.0.1:11434/v1");
}

#[test]
fn normalize_merges_backend_metadata_over_the_preset() {
    let raw = vec![provider(AiProviderType::Gemini, "Google Gemini (EU)", "https://eu.example.com", Some("gemini-2.5-flash"))];
    let normalized = normalize_ai_providers(&raw);
    let gemini = normalized.iter().find(|p| p.provider_type == AiProviderType::Gemini).unwrap();
    // The backend's names win…
    assert_eq!(gemini.display_name, "Google Gemini (EU)");
    assert_eq!(gemini.default_endpoint, "https://eu.example.com");
    assert_eq!(gemini.default_model.as_deref(), Some("gemini-2.5-flash"));
    // …and empty backend strings fall back to the preset (JS `||`).
    let blank = vec![provider(AiProviderType::Gemini, "", "", None)];
    let normalized = normalize_ai_providers(&blank);
    let gemini = normalized.iter().find(|p| p.provider_type == AiProviderType::Gemini).unwrap();
    assert_eq!(gemini.display_name, "Google Gemini");
    assert_eq!(gemini.default_endpoint, "https://generativelanguage.googleapis.com/v1beta/openai/");
    assert_eq!(gemini.default_model.as_deref(), Some("gemini-2.5-pro"));
}

// ---- createEmptyProfile / reseed ---------------------------------------------

#[test]
fn create_empty_profile_seeds_from_the_open_ai_provider() {
    let providers = vec![provider(AiProviderType::OpenAi, "OpenAI", "https://api.openai.com/v1", Some("gpt-4o"))];
    let fresh = create_empty_profile("n1", &providers);
    assert_eq!(fresh.id, "n1");
    assert_eq!(fresh.name, "");
    assert_eq!(fresh.provider_type, AiProviderType::OpenAi);
    assert_eq!(fresh.endpoint.as_deref(), Some("https://api.openai.com/v1"));
    assert_eq!(fresh.model, "gpt-4o");
    assert_eq!(fresh.max_tokens, Some(DEFAULT_MAX_TOKENS));
    assert_eq!(fresh.safety_gate, default_safety_gate(SafetyPresetKey::CloudStrict));
    assert_eq!(fresh.is_default, Some(false));

    // Without providers the endpoint and model stay blank.
    let bare = create_empty_profile("n2", &[]);
    assert_eq!(bare.endpoint, None);
    assert_eq!(bare.model, "");
}

#[test]
fn reseed_from_a_profile_clones_its_fields_and_resets_the_tab() {
    let mut draft = ModelProfileDraft::default();
    draft.tab = DraftTab::Safety;
    draft.validate_ok_until = Some(10.0);
    draft.manual_model_input = true;
    let source = profile("p1", AiProviderType::OpenAi);
    draft.reseed(Some(&source), "ignored", &[]);
    assert_eq!(draft.id, "p1");
    assert_eq!(draft.name.text, "Profile p1");
    assert_eq!(draft.api_key.text, "sk-1");
    assert_eq!(draft.endpoint.text, "https://api.remote.com");
    assert_eq!(draft.model.text, "gpt-4o");
    assert_eq!(draft.max_tokens.text, "8192");
    assert_eq!(draft.is_default, false);
    assert_eq!(draft.safety_gate, source.safety_gate);
    assert_eq!(draft.tab, DraftTab::Basic);
    assert_eq!(draft.validate_ok_until, None);
    // What React's effect leaves alone, the reseed leaves alone.
    assert!(draft.manual_model_input);
}

#[test]
fn reseed_normalizes_a_custom_profiles_legacy_protocol() {
    let mut draft = ModelProfileDraft::default();
    draft.custom_protocol = "open_ai_responses".to_owned();
    let mut source = profile("p2", AiProviderType::Custom);
    source.extra = Some(serde_json::json!({ "protocol": "anthropic" }));
    draft.reseed(Some(&source), "ignored", &[]);
    assert_eq!(draft.custom_protocol, "anthropic_compatible");

    // A non-custom profile leaves customProtocol stale (React's quirk).
    let mut draft = ModelProfileDraft::default();
    draft.custom_protocol = "open_ai_responses".to_owned();
    draft.reseed(Some(&profile("p3", AiProviderType::OpenAi)), "ignored", &[]);
    assert_eq!(draft.custom_protocol, "open_ai_responses");

    // The new-profile branch does not reset it either.
    let mut draft = ModelProfileDraft::default();
    draft.custom_protocol = "anthropic_compatible".to_owned();
    draft.reseed(None, "fresh", &[]);
    assert_eq!(draft.custom_protocol, "anthropic_compatible");
    assert_eq!(draft.id, "fresh");
    assert_eq!(draft.provider_type, AiProviderType::OpenAi);
    assert_eq!(draft.name.text, "");
}

#[test]
fn reseed_seeds_a_cli_profiles_paths_from_extra() {
    let mut draft = ModelProfileDraft::default();
    let mut source = profile("p4", AiProviderType::ClaudeCode);
    source.extra = Some(serde_json::json!({
        "executablePath": "/usr/local/bin/claude",
        "workspaceDir": "/tmp/w",
        "note": "keep me"
    }));
    draft.reseed(Some(&source), "ignored", &[]);
    assert_eq!(draft.local_cli_path(), "/usr/local/bin/claude");
    assert_eq!(draft.local_cli_workspace(), "/tmp/w");
    // The unknown key survives into the effective extra.
    let extra = draft.effective_extra().unwrap();
    assert_eq!(extra.get("note"), Some(&serde_json::json!("keep me")));
    assert_eq!(extra.get("executablePath"), Some(&serde_json::json!("/usr/local/bin/claude")));
}

#[test]
fn fill_provider_defaults_fills_only_blank_fields() {
    // Editing a profile whose endpoint is blank.
    let mut draft = ModelProfileDraft::default();
    let mut source = profile("p1", AiProviderType::OpenAi);
    source.endpoint = None;
    source.model = String::new();
    draft.reseed(Some(&source), "ignored", &[]);
    assert_eq!(draft.endpoint.text, "");
    // The metadata arrives late.
    draft.fill_provider_defaults(&[provider(AiProviderType::OpenAi, "OpenAI", "https://api.openai.com/v1", Some("gpt-4o"))]);
    assert_eq!(draft.endpoint.text, "https://api.openai.com/v1");
    assert_eq!(draft.model.text, "gpt-4o");
    // What the user typed survives a second fill.
    draft.endpoint = TextEdit::new("https://proxy.example.com".to_owned());
    draft.fill_provider_defaults(&[provider(AiProviderType::OpenAi, "OpenAI", "https://api.openai.com/v1", Some("gpt-4o"))]);
    assert_eq!(draft.endpoint.text, "https://proxy.example.com");
    // A provider the list lacks changes nothing.
    draft.fill_provider_defaults(&[]);
    assert_eq!(draft.endpoint.text, "https://proxy.example.com");
}

#[test]
fn a_fresh_draft_seeds_from_the_normalized_presets_even_with_no_store_list() {
    // React's `providerDefaults` memo fills all eighteen presets from
    // PROVIDER_PRESETS, so `createEmptyProfile(providerDefaults)` finds the
    // open_ai preset even when the store's provider list is empty.
    let mut draft = ModelProfileDraft::default();
    draft.reseed(None, "n1", &[]);
    assert_eq!(draft.provider_type, AiProviderType::OpenAi);
    assert_eq!(draft.endpoint.text, "https://api.openai.com/v1");
    assert_eq!(draft.model.text, "gpt-4o");
}

// ---- handleProviderChange ----------------------------------------------------

#[test]
fn provider_change_reseeds_the_provider_dependent_fields() {
    let mut draft = ModelProfileDraft::default();
    draft.reseed(None, "n1", &[]);
    draft.manual_model_input = true;
    draft.validate_ok_until = Some(9.0);
    draft.model = TextEdit::new("typed".to_owned());

    // Ollama: the key is preset, the gate trusts the local runtime.
    draft.handle_provider_change(AiProviderType::Ollama, &[]);
    assert_eq!(draft.api_key.text, "ollama");
    assert_eq!(draft.endpoint.text, "http://127.0.0.1:11434/v1");
    assert_eq!(draft.model.text, "llama3.2");
    assert_eq!(draft.safety_gate, default_safety_gate(SafetyPresetKey::LocalTrust));
    assert!(!draft.manual_model_input);
    assert_eq!(draft.validate_ok_until, None);
    assert_eq!(draft.extra, None);

    // Custom: the protocol select's value and the {protocol} extra.
    draft.handle_provider_change(AiProviderType::Custom, &[]);
    assert_eq!(draft.api_key.text, "");
    assert_eq!(draft.endpoint.text, "");
    assert_eq!(draft.custom_protocol, "open_ai_compatible");
    assert_eq!(draft.effective_extra(), Some(serde_json::json!({ "protocol": "open_ai_compatible" })));
    assert_eq!(draft.safety_gate, default_safety_gate(SafetyPresetKey::CloudStrict));

    // A local CLI: blank endpoint, strict cloud gate, the two path keys.
    draft.handle_provider_change(AiProviderType::ClaudeCode, &[]);
    assert_eq!(draft.api_key.text, "");
    assert_eq!(draft.endpoint.text, "");
    assert_eq!(draft.local_cli_path(), "");
    assert_eq!(draft.local_cli_workspace(), "");
    assert_eq!(draft.safety_gate, default_safety_gate(SafetyPresetKey::CloudStrict));
    assert_eq!(
        draft.effective_extra(),
        Some(serde_json::json!({ "executablePath": "", "workspaceDir": "" }))
    );

    // Gemini through the store's provider list (the merge path).
    draft.handle_provider_change(
        AiProviderType::Gemini,
        &[provider(AiProviderType::Gemini, "Google Gemini", "https://generativelanguage.googleapis.com/v1beta/openai/", Some("gemini-2.5-pro"))],
    );
    assert_eq!(draft.endpoint.text, "https://generativelanguage.googleapis.com/v1beta/openai/");
    assert_eq!(draft.model.text, "gemini-2.5-pro");
    assert_eq!(draft.extra, None);
}

#[test]
fn protocol_change_replaces_the_extra_wholesale() {
    let mut draft = ModelProfileDraft::default();
    draft.handle_provider_change(AiProviderType::Custom, &[]);
    draft.handle_protocol_change("open_ai_responses");
    assert_eq!(draft.custom_protocol, "open_ai_responses");
    assert_eq!(draft.effective_extra(), Some(serde_json::json!({ "protocol": "open_ai_responses" })));
}

#[test]
fn cli_path_edits_clear_the_validate_badge_and_merge_into_extra() {
    let mut draft = ModelProfileDraft::default();
    draft.handle_provider_change(AiProviderType::OpenCode, &[]);
    draft.validate_ok_until = Some(5.0);
    draft.note_cli_path_edit();
    assert_eq!(draft.validate_ok_until, None);
    // The buffer write is the view's edit; the extra picks it up at read time.
    draft.local_cli_path = TextEdit::new("/opt/opencode/bin".to_owned());
    assert_eq!(
        draft.effective_extra(),
        Some(serde_json::json!({ "executablePath": "/opt/opencode/bin", "workspaceDir": "" }))
    );
}

// ---- fetch / validate payloads ----------------------------------------------

#[test]
fn fetch_models_guard_requires_an_endpoint_and_a_key() {
    let mut draft = ModelProfileDraft::default();
    draft.reseed(None, "n1", &[]);
    // No endpoint: nothing.
    assert_eq!(draft.fetch_models_request(&[]), None);
    // Endpoint but no key on a keyed provider: nothing.
    draft.endpoint = TextEdit::new("https://api.openai.com/v1".to_owned());
    assert_eq!(draft.fetch_models_request(&[]), None);
    // Ollama needs no key and answers 'ollama' as the key.
    draft.handle_provider_change(AiProviderType::Ollama, &[]);
    let request = draft.fetch_models_request(&[]).unwrap();
    assert_eq!(
        request,
        FetchModelsRequest {
            protocol: "open_ai_compatible".to_owned(),
            endpoint: "http://127.0.0.1:11434/v1".to_owned(),
            api_key: "ollama".to_owned(),
        }
    );
    // A keyed provider carries its key and the selected provider's protocol.
    draft.handle_provider_change(AiProviderType::DeepSeek, &[]);
    draft.api_key = TextEdit::new(" sk-2 ".to_owned());
    let request = draft.fetch_models_request(&[]).unwrap();
    assert_eq!(request.protocol, "open_ai_responses");
    assert_eq!(request.api_key, "sk-2");
    // Custom uses the chosen protocol (and still needs a key, like React).
    draft.handle_provider_change(AiProviderType::Custom, &[]);
    draft.endpoint = TextEdit::new("https://proxy.example.com".to_owned());
    draft.api_key = TextEdit::new("sk-3".to_owned());
    draft.handle_protocol_change("anthropic_compatible");
    let request = draft.fetch_models_request(&[]).unwrap();
    assert_eq!(request.protocol, "anthropic_compatible");
}

#[test]
fn validate_payload_carries_the_raw_fields() {
    let mut draft = ModelProfileDraft::default();
    draft.reseed(Some(&profile("p1", AiProviderType::OpenAi)), "ignored", &[]);
    draft.endpoint = TextEdit::new("  https://api.remote.com  ".to_owned());
    let payload = draft.validate_payload();
    assert_eq!(payload.provider_type, AiProviderType::OpenAi);
    assert_eq!(payload.api_key.as_deref(), Some("sk-1"));
    assert_eq!(payload.endpoint.as_deref(), Some("  https://api.remote.com  "));
    assert_eq!(payload.model, "gpt-4o");
    assert_eq!(payload.max_tokens, Some(8192));
    assert_eq!(payload.extra, None);

    draft.handle_provider_change(AiProviderType::Custom, &[]);
    draft.handle_protocol_change("open_ai_responses");
    assert_eq!(
        draft.validate_payload().extra,
        Some(serde_json::json!({ "protocol": "open_ai_responses" }))
    );
}

#[test]
fn note_validate_arms_the_three_second_badge() {
    let mut draft = ModelProfileDraft::default();
    assert!(!draft.validate_ok(0.0));
    draft.note_validate(true, 10.0);
    assert!(draft.validate_ok(10.0));
    assert!(draft.validate_ok(12.9));
    assert!(!draft.validate_ok(13.1), "past the 3s deadline the badge is gone");
    draft.note_validate(false, 20.0);
    assert!(!draft.validate_ok(20.0), "a failed validation never arms the badge");
}

// ---- presets, gate setters, maxTokens ---------------------------------------

#[test]
fn apply_preset_swaps_the_whole_gate_and_setters_touch_one_field() {
    let mut draft = ModelProfileDraft::default();
    draft.apply_preset(SafetyPresetKey::EnterpriseBalanced);
    assert_eq!(draft.current_preset(), AiSafetyPreset::EnterpriseBalanced);
    draft.apply_preset(SafetyPresetKey::LocalTrust);
    assert_eq!(draft.current_preset(), AiSafetyPreset::LocalTrust);

    draft.set_gate_data_egress(AiDataEgressLevel::SampleMasked);
    assert_eq!(draft.safety_gate.data_egress_level, AiDataEgressLevel::SampleMasked);
    draft.set_gate_max_sample_rows(7);
    assert_eq!(draft.safety_gate.max_sample_rows, 7);
    draft.set_gate_max_tool_rounds(12);
    assert_eq!(draft.safety_gate.max_tool_rounds, 12);
    draft.set_gate_api_retries(5);
    assert_eq!(draft.safety_gate.api_retries, 5);
    draft.set_gate_max_tool_tokens(64000);
    assert_eq!(draft.safety_gate.max_tool_tokens, 64000);
    draft.set_gate_max_tool_result_bytes(4096);
    assert_eq!(draft.safety_gate.max_tool_result_bytes, 4096);
    draft.set_gate_max_tool_duration_secs(600);
    assert_eq!(draft.safety_gate.max_tool_duration_secs, 600);
    draft.set_gate_db_tool_policy(AiToolPermissionPolicy::Unrestricted);
    assert_eq!(draft.safety_gate.db_tool_policy, AiToolPermissionPolicy::Unrestricted);
    draft.set_gate_mcp_tool_policy(AiToolPermissionPolicy::ReadOnly);
    assert_eq!(draft.safety_gate.mcp_tool_policy, AiToolPermissionPolicy::ReadOnly);
    draft.set_gate_require_sql_confirm(false);
    assert!(!draft.safety_gate.require_sql_confirm);
}

#[test]
fn max_tokens_parses_like_parse_int_with_the_200000_fallback() {
    let mut draft = ModelProfileDraft::default();
    assert_eq!(draft.max_tokens_value(), DEFAULT_MAX_TOKENS, "a blank buffer shows the fallback");
    draft.max_tokens = TextEdit::new("123456".to_owned());
    assert_eq!(draft.max_tokens_value(), 123456);
    draft.max_tokens = TextEdit::new(" 78x".to_owned());
    assert_eq!(draft.max_tokens_value(), 78, "parseInt keeps the leading digits");
    draft.max_tokens = TextEdit::new("abc".to_owned());
    assert_eq!(draft.max_tokens_value(), DEFAULT_MAX_TOKENS, "NaN falls back");
}

// ---- credentials, save payload, the high-risk gate ---------------------------

#[test]
fn has_credentials_accepts_ollama_cli_and_a_trimmed_key() {
    let mut draft = ModelProfileDraft::default();
    draft.reseed(None, "n1", &[]);
    assert!(!draft.has_credentials(), "a blank key on a keyed provider");
    draft.api_key = TextEdit::new("   ".to_owned());
    assert!(!draft.has_credentials(), "whitespace is not a key");
    draft.api_key = TextEdit::new(" sk ".to_owned());
    assert!(draft.has_credentials());
    draft.handle_provider_change(AiProviderType::Ollama, &[]);
    assert!(draft.has_credentials(), "ollama needs no key");
    draft.handle_provider_change(AiProviderType::Codex, &[]);
    assert!(draft.has_credentials(), "a local CLI needs no key");
}

#[test]
fn save_payload_walks_reacts_name_fallback_chain() {
    let mut draft = ModelProfileDraft::default();
    draft.reseed(None, "n1", &[]);
    draft.fill_provider_defaults(&[provider(AiProviderType::OpenAi, "OpenAI", "https://api.openai.com/v1", Some("gpt-4o"))]);

    // Blank name → the model.
    draft.model = TextEdit::new("gpt-4o".to_owned());
    assert_eq!(draft.save_payload(&[]).name, "gpt-4o");
    // A whitespace name is blank; the model wins over the display name.
    draft.name = TextEdit::new("   ".to_owned());
    assert_eq!(draft.save_payload(&[]).name, "gpt-4o");
    // Blank name and model → the selected provider's display name. The
    // normalized provider list always holds the preset, even with an empty
    // store list, so React's display-name arm wins here too; the trailing
    // `draft.providerType` literal stays as the unreachable-on-typed-mirror
    // fallback the React chain keeps.
    draft.model = TextEdit::new(String::new());
    assert_eq!(draft.save_payload(&[]).name, "OpenAI");
    // A real name wins, trimmed of its padding.
    draft.name = TextEdit::new("  My Profile  ".to_owned());
    assert_eq!(draft.save_payload(&[]).name, "My Profile");
}

#[test]
fn save_payload_carries_the_fields_and_the_effective_extra() {
    let mut draft = ModelProfileDraft::default();
    draft.reseed(Some(&profile("p9", AiProviderType::OpenAi)), "ignored", &[]);
    let payload = draft.save_payload(&[]);
    assert_eq!(payload.id, "p9");
    assert_eq!(payload.api_key.as_deref(), Some("sk-1"));
    assert_eq!(payload.endpoint.as_deref(), Some("https://api.remote.com"));
    assert_eq!(payload.model, "gpt-4o");
    assert_eq!(payload.max_tokens, Some(8192));
    assert_eq!(payload.is_default, Some(false));

    // Custom saves exactly {protocol}; an empty key/endpoint saves as absent.
    draft.handle_provider_change(AiProviderType::Custom, &[]);
    draft.handle_protocol_change("anthropic_compatible");
    let payload = draft.save_payload(&[]);
    assert_eq!(payload.extra, Some(serde_json::json!({ "protocol": "anthropic_compatible" })));
    assert_eq!(payload.api_key, None);
    assert_eq!(payload.endpoint, None);
}

#[test]
fn request_save_gates_unrestricted_remote_egress_behind_the_confirmation() {
    let mut draft = ModelProfileDraft::default();
    draft.reseed(Some(&profile("p1", AiProviderType::OpenAi)), "ignored", &[]);
    // cloud_strict on a remote endpoint: straight to the payload.
    assert!(matches!(draft.request_save(&[]), SaveRequest::Save(_)));
    assert!(!draft.show_high_risk_modal);

    // Unrestricted + remote: the confirmation opens, nothing is saved yet.
    draft.apply_preset(SafetyPresetKey::LocalTrust);
    assert_eq!(draft.request_save(&[]), SaveRequest::ConfirmHighRisk);
    assert!(draft.show_high_risk_modal);
    // Confirming closes it and hands over the payload.
    let SaveRequest::Save(payload) = draft.confirm_high_risk(&[]) else {
        panic!("confirm hands the payload to the save callback");
    };
    assert_eq!(payload.id, "p1");
    assert!(!draft.show_high_risk_modal);

    // Unrestricted + local endpoint: no confirmation.
    draft.endpoint = TextEdit::new("http://127.0.0.1:11434/v1".to_owned());
    assert!(matches!(draft.request_save(&[]), SaveRequest::Save(_)));

    // Unrestricted + EMPTY endpoint counts as remote (React's !endpoint).
    draft.endpoint = TextEdit::new(String::new());
    assert_eq!(draft.request_save(&[]), SaveRequest::ConfirmHighRisk);
}

#[test]
fn begin_and_end_save_flip_the_saving_flag() {
    let mut draft = ModelProfileDraft::default();
    assert!(!draft.saving);
    draft.begin_save();
    assert!(draft.saving);
    draft.end_save();
    assert!(!draft.saving);
}
