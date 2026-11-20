//! Independent audit of T087-S1 (`stores/aiStore.ts` config surface).
//!
//! Expectations are the React branches, checked through the store's
//! effects rather than the coder's fixtures:
//!
//! * `isConfigured` is true when the provider config is null but the
//!   settings object has at least one profile, and false when both are
//!   empty.
//! * `saveProfile` with an empty `activeProfileId` adopts the new id
//!   (`current.activeProfileId || profile.id`) even when `isDefault` is
//!   false, and a new default clears `isDefault` on every sibling.
//! * `deleteProfile` of the active id falls back to `remaining[0].id`,
//!   or `""` when nothing remains. Deleting a non-active id keeps the
//!   current active id. Neither path writes the store until reload.
//! * `deleteConfig` forces `isConfigured` false even if profiles remain.
//! * A provider-list failure keeps the previous list.

use crate::ai_store::{
    AiConfigEffect, AiMemorySettings, AiModelProfile, AiProviderConfig, AiSettingsConfig,
    AiSkillsSettings, AiStore, ConfigStart, ProfileEdit, ProviderListItem,
};

fn profile(id: &str, is_default: bool) -> AiModelProfile {
    AiModelProfile {
        id: id.into(),
        name: format!("name-{id}"),
        is_default,
    }
}

fn settings(active: &str, profiles: Vec<AiModelProfile>) -> AiSettingsConfig {
    AiSettingsConfig {
        active_profile_id: active.into(),
        profiles,
        global_instructions: "do-not-drop".into(),
        prompt_templates: Vec::new(),
        memory: AiMemorySettings::default(),
        skills: AiSkillsSettings {
            enabled: true,
            custom_dir: Some("/skills".into()),
            auto_use: false,
            selected: vec!["sql".into()],
        },
    }
}

fn saved(effect: AiConfigEffect) -> AiSettingsConfig {
    match effect {
        AiConfigEffect::SaveSettings(config) => config,
        other => panic!("expected SaveSettings, got {other:?}"),
    }
}

#[test]
fn audit_profiles_alone_count_as_configured_and_delete_config_clears_the_flag() {
    let mut store = AiStore::new();
    store.complete_load_config(None, Some(settings("", vec![])));
    assert!(!store.is_configured, "no config and no profiles");

    store.complete_load_config(None, Some(settings("p", vec![profile("p", false)])));
    assert!(store.is_configured);
    assert!(store.config.is_none());

    store.complete_load_config(None, None);
    assert!(!store.is_configured);

    store.complete_load_settings(None, settings("p", vec![profile("p", true)]));
    assert!(store.is_configured);
    store.complete_delete_config();
    assert!(store.config.is_none());
    assert!(
        !store.is_configured,
        "deleteConfig forces false and does not look at profiles"
    );
    assert_eq!(
        store.settings_config.as_ref().map(|cfg| cfg.profiles.len()),
        Some(1)
    );
}

#[test]
fn audit_save_profile_adopts_an_empty_active_id_and_clears_sibling_defaults() {
    let mut store = AiStore::new();
    store.settings_config = Some(settings(
        "",
        vec![profile("alpha", true), profile("beta", false)],
    ));
    let next = saved(store.save_profile(profile("beta", false)));
    assert_eq!(next.active_profile_id, "beta");
    assert!(
        next.profiles[0].is_default,
        "not a new default, sibling stays"
    );
    assert!(!next.profiles[1].is_default);
    assert_eq!(next.global_instructions, "do-not-drop");
    assert_eq!(
        next.skills.custom_dir.as_deref(),
        Some("/skills"),
        "untouched skills round-trip into the save payload"
    );
    assert_eq!(
        store
            .settings_config
            .as_ref()
            .map(|cfg| cfg.active_profile_id.as_str()),
        Some(""),
        "the store waits for the reload"
    );
    assert!(store.saving);

    let next = saved(store.save_profile(profile("beta", true)));
    assert!(next.profiles.iter().filter(|item| item.is_default).count() == 1);
    assert!(next
        .profiles
        .iter()
        .any(|item| item.id == "beta" && item.is_default));
    assert!(next
        .profiles
        .iter()
        .any(|item| item.id == "alpha" && !item.is_default));
}

#[test]
fn audit_delete_active_falls_back_and_a_provider_failure_keeps_the_list() {
    let mut store = AiStore::new();
    store.settings_config = Some(settings(
        "alpha",
        vec![
            profile("alpha", true),
            profile("beta", false),
            profile("gamma", false),
        ],
    ));
    let ProfileEdit::Save(effect) = store.delete_profile("beta") else {
        panic!("non-active delete still saves");
    };
    let next = saved(effect);
    assert_eq!(next.active_profile_id, "alpha");
    assert_eq!(
        next.profiles
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "gamma"]
    );

    store.settings_config = Some(next);
    let ProfileEdit::Save(effect) = store.delete_profile("alpha") else {
        panic!("active delete saves");
    };
    let next = saved(effect);
    assert_eq!(next.active_profile_id, "gamma");

    store.settings_config = Some(settings("only", vec![profile("only", true)]));
    let ProfileEdit::Save(effect) = store.delete_profile("only") else {
        panic!("last profile still saves");
    };
    assert_eq!(saved(effect).active_profile_id, "");

    assert_eq!(store.begin_load_providers(true), ConfigStart::Started);
    store.complete_load_providers(vec![ProviderListItem {
        provider_type: "ollama".into(),
        display_name: "Ollama".into(),
    }]);
    store.fail_load_providers();
    assert_eq!(store.providers.len(), 1);
    assert!(store.config_error.is_none());
}

#[test]
fn audit_unbound_reload_does_not_clear_saving_by_itself() {
    let mut store = AiStore::new();
    let _ = store.begin_save_settings(settings("a", vec![profile("a", false)]));
    assert!(store.ack_save_settings(false).is_none());
    assert!(store.saving);
    assert!(!store.config_loading);
    assert!(store.finish_save_settings());
    assert!(!store.saving);

    let _ = store.begin_validate(AiProviderConfig {
        provider_type: "custom".into(),
        model: "m".into(),
        api_key: Some("secret".into()),
        endpoint: Some("https://llm".into()),
    });
    assert!(store.validating);
    assert!(store.complete_validate());
    assert!(store.config.is_none(), "validate does not store the draft");
}
