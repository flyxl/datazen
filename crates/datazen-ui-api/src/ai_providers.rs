//! Canonical AI-provider preset metadata and normalization — ported from
//! React `packages/frontend/src/lib/aiProviders.ts` (236 lines, task T251).
//!
//! The connection dialog shows one row per provider. React keeps the rows in
//! a `Record<AiProviderType, ProviderListItem>` ([`provider_preset`] here —
//! an exhaustive lookup, not a `HashMap` that could silently omit a variant),
//! plus two ordered `AiProviderType[]` lists ([`PROVIDER_ORDER`] and
//! [`LOCAL_CLI_PROVIDER_TYPES`]) and a handful of pure helpers.
//!
//! ## Coverage: all 18 variants, no fabricated entries
//!
//! React's doc comment says "the ten API presets", but the actual table has
//! **18** keys — ten API presets first, then the eight local-CLI agents — and
//! its own test pins `Object.keys(PROVIDER_PRESETS)).toHaveLength(18)`.
//! Those 18 spellings are exactly the 18 variants of
//! [`AiProviderType`](crate::ai_types::AiProviderType) (T042). There is no
//! variant the React table leaves out, so this port covers all 18 too and
//! invents no metadata for any of them. The "missing provider" behaviour in
//! this file is the *backend-input* fallback in [`normalize_ai_providers`]:
//! a preset absent from the backend's list is filled from the local table,
//! never guessed from a neighbouring provider.
//!
//! ## Mirror rules applied
//!
//! - `PROVIDER_ORDER` is a distinct PRD order (it starts with `anthropic`,
//!   not the enum's `open_ai`) and is the single source for both the
//!   `is_known_provider_type` predicate and the output order of
//!   [`normalize_ai_providers`]. Both predicates are case-sensitive exact
//!   string matches, like JS `Array.includes` on a string array.
//! - React's `defaultModel?: string` is optional. The T042 wire mirror
//!   ([`ProviderListItem`](crate::ai_types::ProviderListItem)) models it as
//!   `Option<String>`; a preset whose React literal is the empty string maps
//!   to `None` there, and `merge_provider` treats both `None` and `Some("")`
//!   as JS-falsy so the preset's value wins.
//! - JS `a || b` on a string falls back only when `a` is empty; on the
//!   optional model it falls back on `None` *and* `Some("")`. The `...preset,
//!   ...provider` spread means `supportsStreaming`/`supportsTools` come from
//!   the backend entry verbatim (only the four string fields and
//!   `providerType` are explicitly overridden).
//! - DeepSeek/Ollama are the documented exception: the backend's stale
//!   defaults are discarded wholesale and the canonical preset is used.
//! - No production `unwrap`/`expect`: the lookup is an exhaustive `match`
//!   over the closed enum, and the final table walk uses an explicit `match`
//!   on `HashMap::get` (every key was inserted one line earlier).

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::ai_types::{AiProviderType, ProviderListItem};

/// React `PROVIDER_ORDER`: ten API presets then the eight local-CLI agents,
/// in the PRD order the connection dialog shows. Unlike the enum's
/// declaration order this begins with `anthropic`.
pub const PROVIDER_ORDER: [AiProviderType; 18] = [
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

/// React `LOCAL_CLI_PROVIDER_TYPES`: the agents advertised after the API
/// presets, each using the `local_cli` protocol and no tool support.
pub const LOCAL_CLI_PROVIDER_TYPES: [AiProviderType; 8] = [
    AiProviderType::ClaudeCode,
    AiProviderType::Codex,
    AiProviderType::Cursor,
    AiProviderType::OpenCode,
    AiProviderType::Grok,
    AiProviderType::Qoder,
    AiProviderType::CodeBuddy,
    AiProviderType::Pi,
];

/// One API preset. `default_model == ""` in React maps to `None`; every API
/// preset advertises both streaming and tools (`supportsBoth`).
fn api_preset(
    provider_type: AiProviderType,
    display_name: &str,
    default_endpoint: &str,
    default_protocol: &str,
    default_model: &str,
) -> ProviderListItem {
    ProviderListItem {
        provider_type,
        display_name: display_name.to_owned(),
        supports_streaming: true,
        supports_tools: true,
        default_endpoint: default_endpoint.to_owned(),
        default_protocol: default_protocol.to_owned(),
        default_model: if default_model.is_empty() {
            None
        } else {
            Some(default_model.to_owned())
        },
    }
}

/// One local-CLI preset: streaming on, tools off, empty endpoint/model,
/// `local_cli` protocol (React's three-field pin for these entries).
fn local_cli_preset(provider_type: AiProviderType, display_name: &str) -> ProviderListItem {
    ProviderListItem {
        provider_type,
        display_name: display_name.to_owned(),
        supports_streaming: true,
        supports_tools: false,
        default_endpoint: String::new(),
        default_protocol: "local_cli".to_owned(),
        default_model: None,
    }
}

/// React `PROVIDER_PRESETS[providerType]` — the canonical metadata for all
/// 18 provider types. The match is exhaustive: adding a new enum variant is
/// a compile error here, which is exactly the "no silently missing preset"
/// guarantee the React record gives at runtime.
pub fn provider_preset(provider_type: AiProviderType) -> ProviderListItem {
    match provider_type {
        AiProviderType::Anthropic => api_preset(
            provider_type,
            "Anthropic (Claude)",
            "https://api.anthropic.com",
            "anthropic_compatible",
            "claude-sonnet-4-20250514",
        ),
        AiProviderType::OpenAi => api_preset(
            provider_type,
            "OpenAI",
            "https://api.openai.com/v1",
            "open_ai_compatible",
            "gpt-4o",
        ),
        AiProviderType::Gemini => api_preset(
            provider_type,
            "Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai/",
            "open_ai_compatible",
            "gemini-2.5-pro",
        ),
        AiProviderType::DeepSeek => api_preset(
            provider_type,
            "DeepSeek",
            "https://api.deepseek.com",
            "open_ai_responses",
            "deepseek-chat",
        ),
        AiProviderType::Qwen => api_preset(
            provider_type,
            "Qwen",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "open_ai_compatible",
            "qwen-plus",
        ),
        AiProviderType::MiniMax => api_preset(
            provider_type,
            "MiniMax",
            "https://api.minimax.io/v1",
            "open_ai_compatible",
            "MiniMax-M3.1-Flash",
        ),
        AiProviderType::Ollama => api_preset(
            provider_type,
            "Ollama",
            "http://127.0.0.1:11434/v1",
            "open_ai_compatible",
            "llama3.2",
        ),
        AiProviderType::OpenAiCompatible => api_preset(
            provider_type,
            "OpenAI Compatible",
            "",
            "open_ai_compatible",
            "",
        ),
        AiProviderType::AnthropicCompatible => api_preset(
            provider_type,
            "Anthropic Compatible",
            "",
            "anthropic_compatible",
            "",
        ),
        AiProviderType::Custom => api_preset(provider_type, "Custom", "", "open_ai_compatible", ""),
        AiProviderType::ClaudeCode => local_cli_preset(provider_type, "Claude Code"),
        AiProviderType::Codex => local_cli_preset(provider_type, "Codex"),
        AiProviderType::Cursor => local_cli_preset(provider_type, "Cursor"),
        AiProviderType::OpenCode => local_cli_preset(provider_type, "OpenCode"),
        AiProviderType::Grok => local_cli_preset(provider_type, "Grok"),
        AiProviderType::Qoder => local_cli_preset(provider_type, "Qoder"),
        AiProviderType::CodeBuddy => local_cli_preset(provider_type, "CodeBuddy"),
        AiProviderType::Pi => local_cli_preset(provider_type, "Pi"),
    }
}

/// React's `DEEP_SEEK_PROVIDER` / `OLLAMA_PROVIDER` const aliases to their
/// presets. `LazyLock` keeps the exact exported names while owning `String`
/// fields (a Rust `const` cannot allocate).
pub static DEEP_SEEK_PROVIDER: LazyLock<ProviderListItem> =
    LazyLock::new(|| provider_preset(AiProviderType::DeepSeek));
pub static OLLAMA_PROVIDER: LazyLock<ProviderListItem> =
    LazyLock::new(|| provider_preset(AiProviderType::Ollama));

/// React `isLocalCliProvider(value)` — `LOCAL_CLI_PROVIDER_TYPES.includes`.
/// Case-sensitive: `"Codex"` is not the `"codex"` agent.
pub fn is_local_cli_provider(value: &str) -> bool {
    LOCAL_CLI_PROVIDER_TYPES
        .iter()
        .any(|kind| kind.as_str() == value)
}

/// React `isKnownProviderType(value)` — `PROVIDER_ORDER.includes`. The
/// argument is a plain string in React (hence the type guard); here the
/// `&str` signature carries the same "caller may pass anything" contract.
pub fn is_known_provider_type(value: &str) -> bool {
    PROVIDER_ORDER.iter().any(|kind| kind.as_str() == value)
}

/// JS `a || b` for a required string field: only the empty string is falsy.
fn string_or<'a>(provider_value: &'a str, preset_value: &'a str) -> &'a str {
    if provider_value.is_empty() {
        preset_value
    } else {
        provider_value
    }
}

/// React `mergeProvider(provider, preset)`: the spread takes every field
/// from `provider`, then the four string fields fall back to the preset on
/// an empty value, and `providerType` is pinned to the preset's value.
/// `supportsStreaming`/`supportsTools` are *not* overridden — the backend's
/// capability flags win, exactly as the JS spread order dictates.
fn merge_provider(provider: &ProviderListItem, preset: &ProviderListItem) -> ProviderListItem {
    ProviderListItem {
        provider_type: preset.provider_type,
        display_name: string_or(&provider.display_name, &preset.display_name).to_owned(),
        supports_streaming: provider.supports_streaming,
        supports_tools: provider.supports_tools,
        default_endpoint: string_or(&provider.default_endpoint, &preset.default_endpoint).to_owned(),
        default_protocol: string_or(&provider.default_protocol, &preset.default_protocol).to_owned(),
        default_model: match &provider.default_model {
            // JS `provider.defaultModel || preset.defaultModel`: `None` and
            // `Some("")` are both falsy.
            Some(value) if !value.is_empty() => Some(value.clone()),
            _ => preset.default_model.clone(),
        },
    }
}

/// React `normalizeAiProviders(providers)`: fold the backend list into the
/// canonical presets, then emit exactly [`PROVIDER_ORDER`] — every preset
/// present once, in PRD order, whether or not the backend sent it. Unknown
/// provider ids are dropped (React's `isKnownProviderType` guard); duplicate
/// ids take the last entry (JS `Map.set` overwrites).
pub fn normalize_ai_providers(providers: &[ProviderListItem]) -> Vec<ProviderListItem> {
    let mut by_type: HashMap<AiProviderType, ProviderListItem> = HashMap::new();

    for provider in providers {
        if !is_known_provider_type(provider.provider_type.as_str()) {
            continue;
        }
        let preset = provider_preset(provider.provider_type);
        // DeepSeek/Ollama historically shipped stale backend defaults; keep
        // the frontend canonical endpoints so existing profiles migrate
        // predictably (React's explicit early-continue).
        if matches!(
            provider.provider_type,
            AiProviderType::DeepSeek | AiProviderType::Ollama
        ) {
            by_type.insert(provider.provider_type, preset);
            continue;
        }
        by_type.insert(provider.provider_type, merge_provider(provider, &preset));
    }

    for &kind in &PROVIDER_ORDER {
        by_type.entry(kind).or_insert_with(|| provider_preset(kind));
    }

    PROVIDER_ORDER
        .iter()
        .map(|&kind| match by_type.get(&kind) {
            Some(item) => item.clone(),
            // Unreachable after the fill loop above, but explicit rather
            // than an `unwrap`: a future change to PROVIDER_ORDER cannot
            // turn this into a panic.
            None => provider_preset(kind),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full preset table as `(type, display_name, streaming, tools,
    /// endpoint, protocol, default_model)` rows in PRD order — every field
    /// literal from `aiProviders.ts`, pinned per provider.
    fn expected_presets() -> Vec<(
        AiProviderType,
        &'static str,
        bool,
        bool,
        &'static str,
        &'static str,
        Option<&'static str>,
    )> {
        vec![
            (AiProviderType::Anthropic, "Anthropic (Claude)", true, true, "https://api.anthropic.com", "anthropic_compatible", Some("claude-sonnet-4-20250514")),
            (AiProviderType::OpenAi, "OpenAI", true, true, "https://api.openai.com/v1", "open_ai_compatible", Some("gpt-4o")),
            (AiProviderType::Gemini, "Google Gemini", true, true, "https://generativelanguage.googleapis.com/v1beta/openai/", "open_ai_compatible", Some("gemini-2.5-pro")),
            (AiProviderType::DeepSeek, "DeepSeek", true, true, "https://api.deepseek.com", "open_ai_responses", Some("deepseek-chat")),
            (AiProviderType::Qwen, "Qwen", true, true, "https://dashscope.aliyuncs.com/compatible-mode/v1", "open_ai_compatible", Some("qwen-plus")),
            (AiProviderType::MiniMax, "MiniMax", true, true, "https://api.minimax.io/v1", "open_ai_compatible", Some("MiniMax-M3.1-Flash")),
            (AiProviderType::Ollama, "Ollama", true, true, "http://127.0.0.1:11434/v1", "open_ai_compatible", Some("llama3.2")),
            (AiProviderType::OpenAiCompatible, "OpenAI Compatible", true, true, "", "open_ai_compatible", None),
            (AiProviderType::AnthropicCompatible, "Anthropic Compatible", true, true, "", "anthropic_compatible", None),
            (AiProviderType::Custom, "Custom", true, true, "", "open_ai_compatible", None),
            (AiProviderType::ClaudeCode, "Claude Code", true, false, "", "local_cli", None),
            (AiProviderType::Codex, "Codex", true, false, "", "local_cli", None),
            (AiProviderType::Cursor, "Cursor", true, false, "", "local_cli", None),
            (AiProviderType::OpenCode, "OpenCode", true, false, "", "local_cli", None),
            (AiProviderType::Grok, "Grok", true, false, "", "local_cli", None),
            (AiProviderType::Qoder, "Qoder", true, false, "", "local_cli", None),
            (AiProviderType::CodeBuddy, "CodeBuddy", true, false, "", "local_cli", None),
            (AiProviderType::Pi, "Pi", true, false, "", "local_cli", None),
        ]
    }

    fn item(
        provider_type: AiProviderType,
        display_name: &str,
        supports_streaming: bool,
        supports_tools: bool,
        default_endpoint: &str,
        default_protocol: &str,
        default_model: Option<&str>,
    ) -> ProviderListItem {
        ProviderListItem {
            provider_type,
            display_name: display_name.to_owned(),
            supports_streaming,
            supports_tools,
            default_endpoint: default_endpoint.to_owned(),
            default_protocol: default_protocol.to_owned(),
            default_model: default_model.map(str::to_owned),
        }
    }

    // React `isKnownProviderType`: every PRD preset accepted, legacy
    // anthropic included; unknown/case variants rejected.
    #[test]
    fn is_known_provider_type_accepts_every_prd_preset_and_rejects_others() {
        for &kind in &PROVIDER_ORDER {
            assert!(is_known_provider_type(kind.as_str()), "{}", kind.as_str());
        }
        assert!(is_known_provider_type("anthropic"));
        for unknown in [
            "not_a_provider",
            "",
            "OPEN_AI",
            "Open_AI",
            " minimax",
            "open-code",
        ] {
            assert!(!is_known_provider_type(unknown), "{unknown:?}");
        }
    }

    // React `isLocalCliProvider`: only the eight agents.
    #[test]
    fn is_local_cli_provider_matches_only_the_eight_agents() {
        for &kind in &LOCAL_CLI_PROVIDER_TYPES {
            assert!(is_local_cli_provider(kind.as_str()));
            assert!(is_known_provider_type(kind.as_str()));
        }
        for &kind in &PROVIDER_ORDER {
            if !LOCAL_CLI_PROVIDER_TYPES.contains(&kind) {
                assert!(!is_local_cli_provider(kind.as_str()), "{}", kind.as_str());
            }
        }
        assert!(!is_local_cli_provider("Codex"));
        assert!(!is_local_cli_provider("not_a_provider"));
    }

    // React test: "exposes ten API presets plus eight local CLI agents in
    // stable order" — the PRD order is exact, and PROVIDER_ORDER = the 18
    // preset keys, so there is no variant the table leaves out.
    #[test]
    fn provider_order_is_the_exact_prd_order_and_covers_all_18_variants() {
        assert_eq!(PROVIDER_ORDER.len(), 18);
        assert_eq!(LOCAL_CLI_PROVIDER_TYPES.len(), 8);
        let literals = [
            "anthropic",
            "open_ai",
            "gemini",
            "deep_seek",
            "qwen",
            "mini_max",
            "ollama",
            "open_ai_compatible",
            "anthropic_compatible",
            "custom",
            "claude_code",
            "codex",
            "cursor",
            "opencode",
            "grok",
            "qoder",
            "codebuddy",
            "pi",
        ];
        let expected: Vec<AiProviderType> = expected_presets()
            .into_iter()
            .map(|row| row.0)
            .collect();
        assert_eq!(PROVIDER_ORDER.to_vec(), expected);
        for (kind, literal) in PROVIDER_ORDER.iter().zip(literals) {
            assert_eq!(kind.as_str(), literal);
            assert!(is_known_provider_type(literal));
        }
        // The eight agents ride at the tail, in order.
        assert_eq!(&PROVIDER_ORDER[10..], LOCAL_CLI_PROVIDER_TYPES.as_slice());
    }

    // Every field of every preset, literal-for-literal from the React table.
    #[test]
    fn every_preset_pins_its_six_literal_fields() {
        for (kind, name, streaming, tools, endpoint, protocol, model) in expected_presets() {
            let preset = provider_preset(kind);
            assert_eq!(preset.provider_type, kind);
            assert_eq!(preset.display_name, name, "{}", kind.as_str());
            assert_eq!(preset.supports_streaming, streaming, "{}", kind.as_str());
            assert_eq!(preset.supports_tools, tools, "{}", kind.as_str());
            assert_eq!(preset.default_endpoint, endpoint, "{}", kind.as_str());
            assert_eq!(preset.default_protocol, protocol, "{}", kind.as_str());
            assert_eq!(
                preset.default_model.as_deref(),
                model,
                "{}",
                kind.as_str()
            );
        }
    }

    // The three-field contract the React suite pins for local CLI agents:
    // `defaultProtocol === 'local_cli'`, `supportsTools === false`,
    // `defaultEndpoint === ''` — plus streaming true and no model.
    #[test]
    fn local_cli_presets_share_the_three_field_signature() {
        for &kind in &LOCAL_CLI_PROVIDER_TYPES {
            let preset = provider_preset(kind);
            assert_eq!(preset.default_protocol, "local_cli", "{}", kind.as_str());
            assert!(!preset.supports_tools, "{}", kind.as_str());
            assert!(preset.supports_streaming, "{}", kind.as_str());
            assert_eq!(preset.default_endpoint, "", "{}", kind.as_str());
            assert_eq!(preset.default_model, None, "{}", kind.as_str());
        }
    }

    // React test: "uses the real compatible endpoints and protocol for each
    // cloud preset" — pinned again against distinct literal values, and the
    // custom preset's empty endpoint.
    #[test]
    fn cloud_presets_keep_their_real_endpoints_and_protocols() {
        let gemini = provider_preset(AiProviderType::Gemini);
        assert_eq!(
            gemini.default_endpoint,
            "https://generativelanguage.googleapis.com/v1beta/openai/"
        );
        assert_eq!(gemini.default_protocol, "open_ai_compatible");
        assert_eq!(
            provider_preset(AiProviderType::Qwen).default_endpoint,
            "https://dashscope.aliyuncs.com/compatible-mode/v1"
        );
        assert_eq!(
            provider_preset(AiProviderType::MiniMax).default_endpoint,
            "https://api.minimax.io/v1"
        );
        assert_eq!(
            provider_preset(AiProviderType::Anthropic).default_protocol,
            "anthropic_compatible"
        );
        assert_eq!(
            provider_preset(AiProviderType::AnthropicCompatible).default_protocol,
            "anthropic_compatible"
        );
        assert_eq!(
            provider_preset(AiProviderType::OpenAiCompatible).default_protocol,
            "open_ai_compatible"
        );
        assert_eq!(provider_preset(AiProviderType::Custom).default_endpoint, "");
    }

    // The two exported aliases must equal the full preset rows.
    #[test]
    fn deep_seek_and_ollama_aliases_are_their_presets() {
        assert_eq!(*DEEP_SEEK_PROVIDER, provider_preset(AiProviderType::DeepSeek));
        assert_eq!(*OLLAMA_PROVIDER, provider_preset(AiProviderType::Ollama));
        assert_eq!(DEEP_SEEK_PROVIDER.default_model.as_deref(), Some("deepseek-chat"));
        assert_eq!(OLLAMA_PROVIDER.default_model.as_deref(), Some("llama3.2"));
    }

    // Empty input still emits all 18 presets in PRD order: the fill loop is
    // the "form never silently loses a preset" guarantee.
    #[test]
    fn an_empty_backend_list_fills_every_preset_in_order() {
        let result = normalize_ai_providers(&[]);
        let expected: Vec<ProviderListItem> =
            PROVIDER_ORDER.iter().map(|&kind| provider_preset(kind)).collect();
        assert_eq!(result, expected);
    }

    // React test: "keeps legacy anthropic visible and fills every missing
    // preset". Three backend entries (open_ai, anthropic, custom) -> all 18
    // rows, with the missing gemini row carrying its canonical model.
    #[test]
    fn normalize_keeps_anthropic_and_fills_missing_presets() {
        let open_ai = item(
            AiProviderType::OpenAi,
            "OpenAI",
            true,
            true,
            "https://api.openai.com/v1",
            "open_ai_compatible",
            None,
        );
        let anthropic = item(
            AiProviderType::Anthropic,
            "Anthropic (Claude)",
            true,
            true,
            "https://api.anthropic.com",
            "anthropic_compatible",
            None,
        );
        let custom = item(
            AiProviderType::Custom,
            "Custom",
            true,
            true,
            "",
            "open_ai_compatible",
            None,
        );
        let result = normalize_ai_providers(&[open_ai, anthropic, custom]);
        assert_eq!(
            result.iter().map(|p| p.provider_type).collect::<Vec<_>>(),
            PROVIDER_ORDER.to_vec()
        );
        let found = result
            .iter()
            .find(|p| p.provider_type == AiProviderType::Anthropic)
            .cloned();
        assert_eq!(found.map(|p| p.display_name).as_deref(), Some("Anthropic (Claude)"));
        let gemini = result
            .iter()
            .find(|p| p.provider_type == AiProviderType::Gemini)
            .cloned();
        assert_eq!(gemini.and_then(|p| p.default_model).as_deref(), Some("gemini-2.5-pro"));
    }

    // React test: "applies frontend DeepSeek/Ollama defaults over old
    // backend entries" — the backend's capability flags, endpoint and
    // protocol are all discarded, not just the endpoint.
    #[test]
    fn normalize_discards_stale_deep_seek_and_ollama_backend_defaults() {
        let backend_deep_seek = item(
            AiProviderType::DeepSeek,
            "DeepSeek Backend",
            false,
            false,
            "https://old.example.com",
            "open_ai_compatible",
            None,
        );
        let backend_ollama = item(
            AiProviderType::Ollama,
            "Old Ollama",
            false,
            false,
            "http://old.example.com",
            "open_ai_compatible",
            None,
        );
        let result = normalize_ai_providers(&[backend_deep_seek, backend_ollama]);
        assert_eq!(
            result
                .iter()
                .find(|p| p.provider_type == AiProviderType::DeepSeek)
                .cloned(),
            Some(provider_preset(AiProviderType::DeepSeek))
        );
        assert_eq!(
            result
                .iter()
                .find(|p| p.provider_type == AiProviderType::Ollama)
                .cloned(),
            Some(provider_preset(AiProviderType::Ollama))
        );
    }

    // The JS `||` fallback: empty strings (and `None`/`Some("")` models)
    // yield to the preset; backend capability flags ride through verbatim.
    #[test]
    fn merge_falls_back_on_empty_strings_but_keeps_backend_capability_flags() {
        let sparse = item(
            AiProviderType::OpenAi,
            "",
            false,
            false,
            "",
            "",
            Some(""),
        );
        let result = normalize_ai_providers(&[sparse]);
        let open_ai = result
            .iter()
            .find(|p| p.provider_type == AiProviderType::OpenAi)
            .cloned()
            .unwrap_or_else(|| provider_preset(AiProviderType::OpenAi));
        assert_eq!(open_ai.display_name, "OpenAI");
        assert_eq!(open_ai.default_endpoint, "https://api.openai.com/v1");
        assert_eq!(open_ai.default_protocol, "open_ai_compatible");
        assert_eq!(open_ai.default_model.as_deref(), Some("gpt-4o"));
        // Unlike the four string fields, these come from the backend entry.
        assert!(!open_ai.supports_streaming);
        assert!(!open_ai.supports_tools);

        // Non-empty backend strings win over the preset.
        let custom = item(
            AiProviderType::Custom,
            "My GPT",
            false,
            true,
            "https://llm.local/v1",
            "open_ai_responses",
            Some("my-model"),
        );
        let result = normalize_ai_providers(&[custom]);
        let custom = result
            .iter()
            .find(|p| p.provider_type == AiProviderType::Custom)
            .cloned();
        assert_eq!(custom, Some(item(
            AiProviderType::Custom,
            "My GPT",
            false,
            true,
            "https://llm.local/v1",
            "open_ai_responses",
            Some("my-model"),
        )));
    }

    // A single backend entry never loses the other 17 presets: each input
    // lands at its PRD index and equals its canonical row. (An unknown id
    // cannot be constructed in Rust — `AiProviderType` is a closed enum and
    // serde rejects unknown strings — so React's runtime guard is exercised
    // through `is_known_provider_type` in the predicate test above.)
    #[test]
    fn a_single_backend_entry_still_emits_all_18_rows() {
        for (kind, name, streaming, tools, endpoint, protocol, model) in expected_presets() {
            let backend = item(kind, name, streaming, tools, endpoint, protocol, model);
            let result = normalize_ai_providers(&[backend]);
            assert_eq!(
                result.iter().map(|p| p.provider_type).collect::<Vec<_>>(),
                PROVIDER_ORDER.to_vec(),
                "{}",
                kind.as_str()
            );
            let index = PROVIDER_ORDER.iter().position(|&k| k == kind);
            assert!(index.is_some(), "{}", kind.as_str());
            if let Some(index) = index {
                assert_eq!(result[index], provider_preset(kind), "{}", kind.as_str());
            }
        }
    }

    // Duplicates: JS `Map.set` overwrites, so the last backend entry wins.
    #[test]
    fn normalize_takes_the_last_duplicate_entry() {
        let first = item(
            AiProviderType::Qwen,
            "First Qwen",
            true,
            true,
            "https://first.example",
            "open_ai_compatible",
            None,
        );
        let last = item(
            AiProviderType::Qwen,
            "Last Qwen",
            true,
            true,
            "https://last.example",
            "open_ai_compatible",
            None,
        );
        let result = normalize_ai_providers(&[first, last]);
        let qwen = result
            .iter()
            .find(|p| p.provider_type == AiProviderType::Qwen)
            .cloned();
        assert_eq!(
            qwen,
            Some(item(
                AiProviderType::Qwen,
                "Last Qwen",
                true,
                true,
                "https://last.example",
                "open_ai_compatible",
                // JS `provider.defaultModel || preset.defaultModel`: the
                // backend omitted it, so the preset's model wins.
                Some("qwen-plus"),
            ))
        );
    }

    // The React surface must stay reachable from the crate root — the
    // `pub use ai_providers::{…}` block in lib.rs is a shipped export, not
    // an internal detail. This pins every re-export against removal.
    #[test]
    fn the_react_surface_is_reexported_at_the_crate_root() {
        use crate::{
            DEEP_SEEK_PROVIDER, LOCAL_CLI_PROVIDER_TYPES, OLLAMA_PROVIDER, PROVIDER_ORDER,
            is_known_provider_type, is_local_cli_provider, normalize_ai_providers,
            provider_preset,
        };
        assert_eq!(PROVIDER_ORDER.len(), 18);
        assert_eq!(LOCAL_CLI_PROVIDER_TYPES.len(), 8);
        assert!(is_known_provider_type("anthropic"));
        assert!(is_local_cli_provider("codex"));
        assert_eq!(normalize_ai_providers(&[]).len(), PROVIDER_ORDER.len());
        assert_eq!(provider_preset(AiProviderType::Pi).display_name, "Pi");
        assert_eq!(DEEP_SEEK_PROVIDER.provider_type, AiProviderType::DeepSeek);
        assert_eq!(OLLAMA_PROVIDER.provider_type, AiProviderType::Ollama);
    }
}
