//! Port of `packages/frontend/src/lib/aiMemorySettings.ts` (T228) — the
//! memory-settings normalizers: backfill for configs written before Memory
//! settings existed, the whole-config wrapper, and the `off`/`auto` mode
//! probe the chat request consults.
//!
//! React's `Partial<AiMemorySettings> | null | undefined` argument is the
//! persistence layer's JSON — the port takes the [`serde_json::Value`] it
//! always parsed from (the `theme.ts` ruling), so a stored `bogus` policy
//! survives far enough to hit the fallback the React suite pins. The three
//! members:
//!
//! - [`normalize_memory_settings`]: the defaults carry, the booleans only
//!   trust `true`, and an unrecognised `cloudPolicy` string falls back to
//!   the default policy (the `CLOUD_POLICIES` membership test);
//! - [`normalize_ai_settings_config`]: the whole-config wrapper — the
//!   memory half through [`normalize_memory_settings`], the skills half
//!   through [`crate::ai_types::skills::normalize_ai_skills_settings`]
//!   (the `lib/aiSkills.ts` dependency leg, ported beside its types), both
//!   legs rewritten on the raw config JSON before the typed decode, which
//!   then sees only canonical shapes;
//! - [`memory_mode_for`]: memory is opt-in; an unset/disabled config must
//!   send `off`.
//!
//! The React suite (`__tests__/aiMemorySettings.test.ts`) ports below.

use serde_json::Value;

use crate::ai_types::{
    AiMemoryCloudPolicy, AiMemorySettings, AiSettingsConfig, DEFAULT_AI_MEMORY_SETTINGS,
    normalize_ai_skills_settings,
};

/// React's `CLOUD_POLICIES` — the three strings a stored `cloudPolicy` may
/// be; anything else falls back to the default policy.
const CLOUD_POLICIES: [AiMemoryCloudPolicy; 3] = [
    AiMemoryCloudPolicy::NormalOnly,
    AiMemoryCloudPolicy::AllowInternal,
    AiMemoryCloudPolicy::DisableCloud,
];

fn boolean_leg_is_true(obj: Option<&serde_json::Map<String, Value>>, key: &str) -> bool {
    obj.and_then(|object| object.get(key)) == Some(&Value::Bool(true))
}

fn cloud_policy_of(stored: Option<&Value>) -> AiMemoryCloudPolicy {
    stored
        .and_then(Value::as_str)
        .and_then(|stored| {
            CLOUD_POLICIES
                .into_iter()
                .find(|policy| policy.as_str() == stored)
        })
        .unwrap_or(DEFAULT_AI_MEMORY_SETTINGS.cloud_policy)
}

/// React `normalizeMemorySettings` — fill fields omitted by configs
/// written before Memory settings existed: the defaults carry, the
/// booleans only trust `true` (an omitted leg stays `false`), and the
/// policy leg falls back to the default when the stored string is not one
/// of the three.
pub fn normalize_memory_settings(memory: Option<&Value>) -> AiMemorySettings {
    let obj = memory.and_then(Value::as_object);
    AiMemorySettings {
        enabled: boolean_leg_is_true(obj, "enabled"),
        auto_learn: boolean_leg_is_true(obj, "autoLearn"),
        cloud_policy: cloud_policy_of(obj.and_then(|object| object.get("cloudPolicy"))),
        privacy_acknowledged: boolean_leg_is_true(obj, "privacyAcknowledged"),
    }
}

/// React `normalizeAiSettingsConfig` — normalize a settings config
/// returned by an older host. The raw config JSON's memory and skills legs
/// are rewritten to their canonical shapes first (the skills leg the
/// pre-T17 host omits lands as the defaults), then the typed decode runs —
/// every leg it sees is already the one definition.
pub fn normalize_ai_settings_config(config: &Value) -> AiSettingsConfig {
    let mut wire = config.clone();
    if let Some(object) = wire.as_object_mut() {
        let memory = normalize_memory_settings(object.get("memory"));
        object.insert(
            "memory".to_owned(),
            serde_json::to_value(&memory).expect("a plain camelCase object serializes"),
        );
        let skills = normalize_ai_skills_settings(object.get("skills"));
        object.insert(
            "skills".to_owned(),
            serde_json::to_value(&skills).expect("a plain camelCase object serializes"),
        );
    }
    serde_json::from_value(wire)
        .expect("the normalized legs are canonical; the rest of the config is the typed surface React spreads as-is")
}

/// React `memoryModeFor` — memory is opt-in; an unset/disabled config must
/// send `off`.
pub fn memory_mode_for(settings: Option<&Value>) -> &'static str {
    if normalize_memory_settings(settings).enabled {
        "auto"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The React suite's `base` config — the four pre-memory legs as raw
    /// JSON (the shape an older host returns).
    fn base() -> Value {
        serde_json::json!({
            "activeProfileId": "p1",
            "profiles": [],
            "globalInstructions": "",
            "promptTemplates": [],
        })
    }

    #[test]
    fn defaults_old_settings_to_disabled_normal_only_unacknowledged_memory() {
        let normalized = normalize_ai_settings_config(&base());
        assert_eq!(normalized.memory, DEFAULT_AI_MEMORY_SETTINGS);
        assert_eq!(
            normalized.skills,
            Some(crate::ai_types::DEFAULT_AI_SKILLS_SETTINGS),
            "the pre-T17 host's missing skills leg lands as the defaults"
        );
        assert_eq!(
            memory_mode_for(Some(&serde_json::to_value(&normalized.memory).unwrap())),
            "off"
        );
    }

    #[test]
    fn selects_auto_only_when_memory_is_enabled_and_tolerates_bad_stored_values() {
        let mut config = base();
        config["memory"] = serde_json::json!({
            "enabled": true,
            "autoLearn": true,
            "cloudPolicy": "bogus",
        });
        let normalized = normalize_ai_settings_config(&config);
        assert_eq!(
            normalized.memory,
            AiMemorySettings {
                enabled: true,
                auto_learn: true,
                cloud_policy: AiMemoryCloudPolicy::NormalOnly,
                privacy_acknowledged: false,
            }
        );
        assert_eq!(
            memory_mode_for(Some(&serde_json::to_value(&normalized.memory).unwrap())),
            "auto"
        );

        let stored = serde_json::json!({
            "enabled": false,
            "autoLearn": true,
            "cloudPolicy": "disable_cloud",
        });
        assert_eq!(
            normalize_memory_settings(Some(&stored)),
            AiMemorySettings {
                enabled: false,
                auto_learn: true,
                cloud_policy: AiMemoryCloudPolicy::DisableCloud,
                privacy_acknowledged: false,
            }
        );
        assert_eq!(memory_mode_for(Some(&stored)), "off");
    }

    /// Beyond the React suite: the null/absent arms and the boolean legs'
    /// junk tolerance (only `true` is trusted).
    #[test]
    fn null_and_absent_normalize_to_the_defaults_and_junk_booleans_die() {
        assert_eq!(normalize_memory_settings(None), DEFAULT_AI_MEMORY_SETTINGS);
        assert_eq!(
            normalize_memory_settings(Some(&serde_json::Value::Null)),
            DEFAULT_AI_MEMORY_SETTINGS
        );
        assert_eq!(memory_mode_for(None), "off");
        let junk = serde_json::json!({
            "enabled": "yes",
            "autoLearn": 1,
            "cloudPolicy": "allow_internal",
        });
        assert_eq!(
            normalize_memory_settings(Some(&junk)),
            AiMemorySettings {
                enabled: false,
                auto_learn: false,
                cloud_policy: AiMemoryCloudPolicy::AllowInternal,
                privacy_acknowledged: false,
            }
        );
    }
}
