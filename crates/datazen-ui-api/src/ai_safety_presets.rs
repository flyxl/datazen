//! Per-profile AI safety-gate presets — ported from React
//! `packages/frontend/src/lib/aiSafetyPresets.ts` (task T291, 95 lines).
//!
//! The settings dialog offers three named gates (`cloud_strict`,
//! `enterprise_balanced`, `local_trust`). React keeps them in a
//! `Record<AiSafetyPresetType, AiSafetyGateConfig>` and exposes three pure
//! helpers:
//!
//! | React export | Rust home |
//! | --- | --- |
//! | `AiSafetyPresetType` | [`AiSafetyPresetType`] (closed enum) |
//! | `AI_SAFETY_PRESETS[preset]` | [`safety_preset`] plus the three [`LazyLock`] rows |
//! | `getDefaultSafetyGate(preset?)` | [`get_default_safety_gate`] |
//! | `detectSafetyPreset(gate)` | [`detect_safety_preset`] |
//! | `isLocalEndpoint(endpoint?)` | [`is_local_endpoint`] |
//!
//! ## Mirror rules applied
//!
//! - The React record is an **exhaustive** lookup on a closed string union.
//!   The port is an exhaustive `match` on [`AiSafetyPresetType`]: adding a
//!   variant without adding its row is a compile error, which is the Rust
//!   twin of React's "every key has a value" property. [`preset_from_str`]
//!   carries the string-key access (unknown key → `None`, never a guess).
//! - All twelve config fields are copied literal-for-literal. The wire shape
//!   [`AiSafetyGateConfig`] already lives in `ai_types` (T042); this module
//!   reuses it rather than forking a second gate struct.
//! - Every preset has `redact_credentials: true`. **No credential, endpoint
//!   secret or raw connection string is stored or logged here**; the presets
//!   only carry policy numbers and booleans. A preset never relaxes a limit
//!   that the React literal pins — `local_trust` is the *only* one that
//!   permits unrestricted egress/tools and no SQL confirmation, exactly as
//!   the shipped file declares.
//! - `getDefaultSafetyGate` returns a fresh copy (`{ ...PRESET }` in React);
//!   Rust clones the config so a caller's edit cannot mutate the shared row.
//! - `detectSafetyPreset` examines **only four fields** (egress, db policy,
//!   mcp policy, sql confirmation) — the numeric/redaction fields are
//!   deliberately not part of identity, so a gate that differs only in a
//!   sample-row cap still detects as its named preset. `custom` is `None`.
//! - `isLocalEndpoint` is a case-insensitive **substring** test (React
//!   `endpoint.toLowerCase().includes(...)`): `10.` and `:11434` match
//!   anywhere, not just at a prefix. A missing/empty endpoint is not local.

use std::sync::LazyLock;

use crate::ai_types::{
    AiDataEgressLevel, AiSafetyGateConfig, AiToolPermissionPolicy,
};

/// React `AiSafetyPresetType` — the three keys of `AI_SAFETY_PRESETS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AiSafetyPresetType {
    CloudStrict,
    EnterpriseBalanced,
    LocalTrust,
}

impl AiSafetyPresetType {
    /// The exact React key spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CloudStrict => "cloud_strict",
            Self::EnterpriseBalanced => "enterprise_balanced",
            Self::LocalTrust => "local_trust",
        }
    }

    /// All three keys in declaration order (the dialog's display order).
    pub const ALL: [Self; 3] = [
        Self::CloudStrict,
        Self::EnterpriseBalanced,
        Self::LocalTrust,
    ];
}

/// String-key access to the React record: `None` for an unknown key, never
/// a nearest-neighbour fallback (an unknown preset must not silently select
/// a looser gate).
pub fn preset_from_str(name: &str) -> Option<AiSafetyPresetType> {
    AiSafetyPresetType::ALL
        .iter()
        .copied()
        .find(|preset| preset.as_str() == name)
}

/// React `AI_SAFETY_PRESETS[preset]` — the canonical row for one key. The
/// match is exhaustive over the closed enum, so no preset can be missing.
pub fn safety_preset(preset: AiSafetyPresetType) -> AiSafetyGateConfig {
    match preset {
        AiSafetyPresetType::CloudStrict => AiSafetyGateConfig {
            redact_credentials: true,
            data_egress_level: AiDataEgressLevel::Strict,
            max_sample_rows: 0,
            db_tool_policy: AiToolPermissionPolicy::ReadOnly,
            mcp_tool_policy: AiToolPermissionPolicy::Disabled,
            require_sql_confirm: true,
            max_context_bytes: 32_000,
            max_tool_rounds: 30,
            max_tool_tokens: 48_000,
            max_tool_result_bytes: 8_192,
            max_tool_duration_secs: 300,
            api_retries: 2,
        },
        AiSafetyPresetType::EnterpriseBalanced => AiSafetyGateConfig {
            redact_credentials: true,
            data_egress_level: AiDataEgressLevel::SampleMasked,
            max_sample_rows: 3,
            db_tool_policy: AiToolPermissionPolicy::RequireConfirm,
            mcp_tool_policy: AiToolPermissionPolicy::RequireConfirm,
            require_sql_confirm: true,
            max_context_bytes: 32_000,
            max_tool_rounds: 30,
            max_tool_tokens: 48_000,
            max_tool_result_bytes: 8_192,
            max_tool_duration_secs: 300,
            api_retries: 2,
        },
        AiSafetyPresetType::LocalTrust => AiSafetyGateConfig {
            redact_credentials: true,
            data_egress_level: AiDataEgressLevel::Unrestricted,
            max_sample_rows: 10,
            db_tool_policy: AiToolPermissionPolicy::Unrestricted,
            mcp_tool_policy: AiToolPermissionPolicy::Unrestricted,
            require_sql_confirm: false,
            max_context_bytes: 64_000,
            max_tool_rounds: 30,
            max_tool_tokens: 48_000,
            max_tool_result_bytes: 8_192,
            max_tool_duration_secs: 300,
            api_retries: 2,
        },
    }
}

/// The three named rows of the React record, kept as the exact exported
/// names. `LazyLock` owns the `String`-free config (the struct itself has no
/// heap fields, but a `static` of it still needs interior initialisation to
/// keep the single source of truth in [`safety_preset`]).
pub static CLOUD_STRICT_PRESET: LazyLock<AiSafetyGateConfig> =
    LazyLock::new(|| safety_preset(AiSafetyPresetType::CloudStrict));
pub static ENTERPRISE_BALANCED_PRESET: LazyLock<AiSafetyGateConfig> =
    LazyLock::new(|| safety_preset(AiSafetyPresetType::EnterpriseBalanced));
pub static LOCAL_TRUST_PRESET: LazyLock<AiSafetyGateConfig> =
    LazyLock::new(|| safety_preset(AiSafetyPresetType::LocalTrust));

/// React `getDefaultSafetyGate(preset = 'cloud_strict')`. `None` is the
/// omitted default argument; the returned config is a fresh copy, so a
/// caller cannot edit the shared preset through it.
pub fn get_default_safety_gate(preset: Option<AiSafetyPresetType>) -> AiSafetyGateConfig {
    safety_preset(preset.unwrap_or(AiSafetyPresetType::CloudStrict))
}

/// React `detectSafetyPreset(gate)`. Only the four identity fields matter;
/// `None` is the React `'custom'` arm. Comparison uses strict equality on
/// the enum variants (the Rust twin of a TS string `===`).
pub fn detect_safety_preset(gate: &AiSafetyGateConfig) -> Option<AiSafetyPresetType> {
    let strict = gate.data_egress_level == AiDataEgressLevel::Strict
        && gate.db_tool_policy == AiToolPermissionPolicy::ReadOnly
        && gate.mcp_tool_policy == AiToolPermissionPolicy::Disabled
        && gate.require_sql_confirm;
    if strict {
        return Some(AiSafetyPresetType::CloudStrict);
    }
    let balanced = gate.data_egress_level == AiDataEgressLevel::SampleMasked
        && gate.db_tool_policy == AiToolPermissionPolicy::RequireConfirm
        && gate.mcp_tool_policy == AiToolPermissionPolicy::RequireConfirm
        && gate.require_sql_confirm;
    if balanced {
        return Some(AiSafetyPresetType::EnterpriseBalanced);
    }
    let local = gate.data_egress_level == AiDataEgressLevel::Unrestricted
        && gate.db_tool_policy == AiToolPermissionPolicy::Unrestricted
        && gate.mcp_tool_policy == AiToolPermissionPolicy::Unrestricted
        && !gate.require_sql_confirm;
    if local {
        return Some(AiSafetyPresetType::LocalTrust);
    }
    None
}

/// React `isLocalEndpoint(endpoint?)`. Lowercase, then substring-match each
/// of the six local markers. `None` (an omitted argument) is not local.
pub fn is_local_endpoint(endpoint: Option<&str>) -> bool {
    let Some(endpoint) = endpoint else {
        return false;
    };
    let lower = endpoint.to_lowercase();
    lower.contains("localhost")
        || lower.contains("127.0.0.1")
        || lower.contains("0.0.0.0")
        || lower.contains("192.168.")
        || lower.contains("10.")
        || lower.contains(":11434")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::AiSafetyGateConfig as Gate;

    /// One row of React's literal table, pinning all twelve fields.
    fn expected(preset: AiSafetyPresetType) -> Gate {
        match preset {
            AiSafetyPresetType::CloudStrict => Gate {
                redact_credentials: true,
                data_egress_level: AiDataEgressLevel::Strict,
                max_sample_rows: 0,
                db_tool_policy: AiToolPermissionPolicy::ReadOnly,
                mcp_tool_policy: AiToolPermissionPolicy::Disabled,
                require_sql_confirm: true,
                max_context_bytes: 32_000,
                max_tool_rounds: 30,
                max_tool_tokens: 48_000,
                max_tool_result_bytes: 8_192,
                max_tool_duration_secs: 300,
                api_retries: 2,
            },
            AiSafetyPresetType::EnterpriseBalanced => Gate {
                redact_credentials: true,
                data_egress_level: AiDataEgressLevel::SampleMasked,
                max_sample_rows: 3,
                db_tool_policy: AiToolPermissionPolicy::RequireConfirm,
                mcp_tool_policy: AiToolPermissionPolicy::RequireConfirm,
                require_sql_confirm: true,
                max_context_bytes: 32_000,
                max_tool_rounds: 30,
                max_tool_tokens: 48_000,
                max_tool_result_bytes: 8_192,
                max_tool_duration_secs: 300,
                api_retries: 2,
            },
            AiSafetyPresetType::LocalTrust => Gate {
                redact_credentials: true,
                data_egress_level: AiDataEgressLevel::Unrestricted,
                max_sample_rows: 10,
                db_tool_policy: AiToolPermissionPolicy::Unrestricted,
                mcp_tool_policy: AiToolPermissionPolicy::Unrestricted,
                require_sql_confirm: false,
                max_context_bytes: 64_000,
                max_tool_rounds: 30,
                max_tool_tokens: 48_000,
                max_tool_result_bytes: 8_192,
                max_tool_duration_secs: 300,
                api_retries: 2,
            },
        }
    }

    #[test]
    fn every_preset_pins_all_twelve_react_fields() {
        for preset in AiSafetyPresetType::ALL {
            assert_eq!(safety_preset(preset), expected(preset), "{}", preset.as_str());
        }
        assert_eq!(AiSafetyPresetType::ALL.len(), 3);
    }

    #[test]
    fn preset_keys_keep_the_react_spellings_and_unknown_keys_never_fallback() {
        assert_eq!(AiSafetyPresetType::CloudStrict.as_str(), "cloud_strict");
        assert_eq!(
            AiSafetyPresetType::EnterpriseBalanced.as_str(),
            "enterprise_balanced"
        );
        assert_eq!(AiSafetyPresetType::LocalTrust.as_str(), "local_trust");
        for preset in AiSafetyPresetType::ALL {
            assert_eq!(preset_from_str(preset.as_str()), Some(preset));
        }
        for unknown in ["", "CLOUD_STRICT", " cloud_strict", "local", "strict"] {
            assert_eq!(preset_from_str(unknown), None, "{unknown:?}");
        }
    }

    // Safety invariant: no preset weakens credential redaction, and only the
    // explicitly-local preset opens egress/tools or drops SQL confirmation.
    #[test]
    fn redaction_is_always_on_and_only_local_trust_relaxes_policy() {
        for preset in AiSafetyPresetType::ALL {
            assert!(safety_preset(preset).redact_credentials, "{}", preset.as_str());
        }
        let cloud = safety_preset(AiSafetyPresetType::CloudStrict);
        assert_eq!(cloud.data_egress_level, AiDataEgressLevel::Strict);
        assert_eq!(cloud.db_tool_policy, AiToolPermissionPolicy::ReadOnly);
        assert_eq!(cloud.mcp_tool_policy, AiToolPermissionPolicy::Disabled);
        assert!(cloud.require_sql_confirm);
        assert_eq!(cloud.max_sample_rows, 0);

        let balanced = safety_preset(AiSafetyPresetType::EnterpriseBalanced);
        assert_eq!(balanced.data_egress_level, AiDataEgressLevel::SampleMasked);
        assert_eq!(balanced.db_tool_policy, AiToolPermissionPolicy::RequireConfirm);
        assert_eq!(balanced.mcp_tool_policy, AiToolPermissionPolicy::RequireConfirm);
        assert!(balanced.require_sql_confirm);

        let local = safety_preset(AiSafetyPresetType::LocalTrust);
        assert_eq!(local.data_egress_level, AiDataEgressLevel::Unrestricted);
        assert_eq!(local.db_tool_policy, AiToolPermissionPolicy::Unrestricted);
        assert_eq!(local.mcp_tool_policy, AiToolPermissionPolicy::Unrestricted);
        assert!(!local.require_sql_confirm);
    }

    #[test]
    fn the_named_row_statics_equal_their_preset_lookup() {
        assert_eq!(*CLOUD_STRICT_PRESET, safety_preset(AiSafetyPresetType::CloudStrict));
        assert_eq!(
            *ENTERPRISE_BALANCED_PRESET,
            safety_preset(AiSafetyPresetType::EnterpriseBalanced)
        );
        assert_eq!(*LOCAL_TRUST_PRESET, safety_preset(AiSafetyPresetType::LocalTrust));
    }

    #[test]
    fn default_gate_is_cloud_strict_and_returns_a_fresh_copy() {
        let mut gate = get_default_safety_gate(None);
        assert_eq!(gate, expected(AiSafetyPresetType::CloudStrict));
        // The caller's edit must not reach the shared preset row.
        gate.max_sample_rows = 999;
        gate.data_egress_level = AiDataEgressLevel::Unrestricted;
        assert_eq!(gate.max_sample_rows, 999);
        assert_eq!(gate.data_egress_level, AiDataEgressLevel::Unrestricted);
        assert_eq!(get_default_safety_gate(None).max_sample_rows, 0);
        assert_eq!(
            get_default_safety_gate(None).data_egress_level,
            AiDataEgressLevel::Strict
        );
        for preset in AiSafetyPresetType::ALL {
            assert_eq!(get_default_safety_gate(Some(preset)), expected(preset));
        }
    }

    #[test]
    fn detection_finds_each_named_preset_from_exact_fields() {
        for preset in AiSafetyPresetType::ALL {
            assert_eq!(
                detect_safety_preset(&safety_preset(preset)),
                Some(preset),
                "{}",
                preset.as_str()
            );
        }
    }

    #[test]
    fn detection_ignores_fields_that_are_not_part_of_identity() {
        let mut gate = safety_preset(AiSafetyPresetType::CloudStrict);
        gate.max_context_bytes = 1;
        gate.max_tool_rounds = 2;
        gate.max_tool_tokens = 3;
        gate.max_tool_result_bytes = 4;
        gate.max_tool_duration_secs = 5;
        gate.api_retries = 6;
        assert_eq!(
            detect_safety_preset(&gate),
            Some(AiSafetyPresetType::CloudStrict),
            "policy caps are not part of preset identity"
        );
        // Redaction is likewise not an identity field (all presets share it).
        gate.redact_credentials = false;
        assert_eq!(detect_safety_preset(&gate), Some(AiSafetyPresetType::CloudStrict));
    }

    #[test]
    fn detection_returns_custom_when_any_identity_field_differs() {
        for preset in AiSafetyPresetType::ALL {
            let mut gate = safety_preset(preset);
            gate.data_egress_level = match gate.data_egress_level {
                AiDataEgressLevel::Strict => AiDataEgressLevel::SampleMasked,
                _ => AiDataEgressLevel::Strict,
            };
            assert_eq!(detect_safety_preset(&gate), None, "{}", preset.as_str());
        }
        // `require_sql_confirm` is the discriminator between cloud/balanced.
        let mut cloud = safety_preset(AiSafetyPresetType::CloudStrict);
        cloud.require_sql_confirm = false;
        assert_eq!(detect_safety_preset(&cloud), None);
        let mut local = safety_preset(AiSafetyPresetType::LocalTrust);
        local.require_sql_confirm = true;
        assert_eq!(detect_safety_preset(&local), None);
        // A mixed gate (cloud egress, local tools) is custom, not a preset.
        let mut mixed = safety_preset(AiSafetyPresetType::CloudStrict);
        mixed.mcp_tool_policy = AiToolPermissionPolicy::Unrestricted;
        assert_eq!(detect_safety_preset(&mixed), None);
    }

    #[test]
    fn local_endpoint_matches_each_marker_case_insensitively_as_a_substring() {
        for endpoint in [
            "http://localhost:8080",
            "HTTP://LOCALHOST",
            "http://127.0.0.1:11434",
            "http://0.0.0.0",
            "192.168.1.20",
            "10.0.0.5",
            "http://ollama:11434/v1",
            "prefix-10.suffix",
            "MYLOCALHOST-DEVICE",
        ] {
            assert!(is_local_endpoint(Some(endpoint)), "{endpoint:?}");
        }
    }

    #[test]
    fn local_endpoint_rejects_absent_empty_and_remote_addresses() {
        assert!(!is_local_endpoint(None));
        assert!(!is_local_endpoint(Some("")));
        for endpoint in [
            "https://api.openai.com/v1",
            "https://api.anthropic.com",
            "1.2.3.4",
            "172.16.0.1",
            "11.0.0.1",
            "https://example.com/v1",
        ] {
            assert!(!is_local_endpoint(Some(endpoint)), "{endpoint:?}");
        }
    }

    #[test]
    fn the_react_surface_is_reexported_at_the_crate_root() {
        use crate::{
            detect_safety_preset, get_default_safety_gate, is_local_endpoint, preset_from_str,
            safety_preset, AiSafetyPresetType,
        };
        assert_eq!(get_default_safety_gate(None), safety_preset(AiSafetyPresetType::CloudStrict));
        assert_eq!(
            detect_safety_preset(&safety_preset(AiSafetyPresetType::LocalTrust)),
            Some(AiSafetyPresetType::LocalTrust)
        );
        assert!(is_local_endpoint(Some("127.0.0.1")));
        assert_eq!(preset_from_str("cloud_strict"), Some(AiSafetyPresetType::CloudStrict));
    }
}
