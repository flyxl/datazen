//! T087-S1: `stores/aiStore.ts` config, profiles, providers, remote models.

use datazen_ui::ai_store::{
    next_settings_for_delete, next_settings_for_save, AiConfigEffect, AiMemorySettings,
    AiModelProfile, AiProviderConfig, AiSettingsConfig, AiSkillsSettings, AiStore, ConfigStart,
    ModelInfo, ProfileEdit, PromptTemplate, ProviderListItem,
};

fn profile(id: &str, is_default: bool) -> AiModelProfile {
    AiModelProfile {
        id: id.into(),
        name: id.into(),
        is_default,
    }
}

fn provider(model: &str) -> AiProviderConfig {
    AiProviderConfig {
        provider_type: "openai".into(),
        model: model.into(),
        api_key: None,
        endpoint: None,
    }
}

fn settings(active: &str, profiles: Vec<AiModelProfile>) -> AiSettingsConfig {
    AiSettingsConfig {
        active_profile_id: active.into(),
        profiles,
        global_instructions: "keep me".into(),
        prompt_templates: vec![PromptTemplate {
            id: "t".into(),
            name: "n".into(),
            content: "c".into(),
            database_types: vec!["postgres".into()],
            is_default: false,
        }],
        memory: AiMemorySettings {
            enabled: true,
            auto_learn: false,
            cloud_policy: "normal_only",
            privacy_acknowledged: true,
        },
        skills: AiSkillsSettings::default(),
    }
}

fn saved(effect: AiConfigEffect) -> AiSettingsConfig {
    match effect {
        AiConfigEffect::SaveSettings(config) => config,
        other => panic!("expected SaveSettings, got {other:?}"),
    }
}

#[test]
fn a_fresh_store_matches_the_zustand_initial_object() {
    let store = AiStore::new();
    assert!(store.config.is_none());
    assert!(store.settings_config.is_none());
    assert!(!store.is_configured);
    assert!(store.providers.is_empty());
    assert!(!store.config_loading);
    assert!(store.config_error.is_none());
    assert!(!store.validating);
    assert!(!store.saving);
    assert!(store.remote_models.is_empty());
    assert!(!store.fetching_remote_models);
}

#[test]
fn load_config_skips_without_a_backend_and_applies_the_is_configured_formula() {
    let mut store = AiStore::new();
    assert_eq!(store.begin_load_config(false).0, ConfigStart::Skipped);
    assert!(!store.config_loading);

    let (start, effect) = store.begin_load_config(true);
    assert_eq!(start, ConfigStart::Started);
    assert_eq!(
        effect,
        Some(AiConfigEffect::FetchConfig { arm_listener: true })
    );
    assert!(store.config_loading);
    store.note_listener_installed(true);

    store.complete_load_config(None, Some(settings("a", vec![profile("a", true)])));
    assert!(store.is_configured);
    assert!(!store.config_loading);
    assert_eq!(
        store
            .settings_config
            .as_ref()
            .map(|cfg| cfg.active_profile_id.as_str()),
        Some("a")
    );

    let (_, effect) = store.begin_load_config(true);
    assert_eq!(
        effect,
        Some(AiConfigEffect::FetchConfig {
            arm_listener: false
        })
    );
    store.fail_load_config("boom");
    assert!(!store.config_loading);
    assert_eq!(store.config_error.as_deref(), Some("boom"));
    // A failed reload does not wipe the previous config.
    assert!(store.is_configured);
}

#[test]
fn a_missing_event_port_retries_the_listener_on_the_next_load() {
    let mut store = AiStore::new();
    let (_, effect) = store.begin_load_config(true);
    assert_eq!(
        effect,
        Some(AiConfigEffect::FetchConfig { arm_listener: true })
    );
    store.note_listener_installed(false);
    store.complete_load_config(Some(provider("m")), None);
    assert!(store.is_configured);
    let (_, effect) = store.begin_load_config(true);
    assert_eq!(
        effect,
        Some(AiConfigEffect::FetchConfig { arm_listener: true })
    );
}

#[test]
fn save_settings_keeps_saving_set_until_the_reload_finishes() {
    let mut store = AiStore::new();
    let effect = store.begin_save_settings(settings("a", vec![profile("a", true)]));
    assert!(store.saving);
    assert!(matches!(effect, AiConfigEffect::SaveSettings(_)));
    // The local copy is not written by the save itself.
    assert!(store.settings_config.is_none());

    let reload = store.ack_save_settings(true);
    assert_eq!(
        reload,
        Some(AiConfigEffect::FetchConfig { arm_listener: true })
    );
    assert!(store.saving);
    assert!(store.config_loading);
    store.fail_load_config("reload failed");
    assert!(store.finish_save_settings());
    assert!(!store.saving);
    assert_eq!(store.config_error.as_deref(), Some("reload failed"));

    let mut store = AiStore::new();
    let _ = store.begin_save_settings(settings("a", vec![]));
    assert!(!store.fail_save_settings("nope"));
    assert!(!store.saving);
    assert_eq!(store.config_error.as_deref(), Some("nope"));
    assert!(store.ack_save_settings(false).is_none());
}

#[test]
fn save_profile_merges_and_preserves_untouched_settings_fields() {
    let current = settings("old", vec![profile("old", true), profile("keep", false)]);
    let mut store = AiStore::new();
    store.settings_config = Some(current);
    let effect = store.save_profile(profile("keep", true));
    let next = saved(effect);
    assert_eq!(next.active_profile_id, "keep");
    assert_eq!(next.global_instructions, "keep me");
    assert!(next.memory.enabled);
    assert_eq!(next.prompt_templates.len(), 1);
    assert!(!next.profiles[0].is_default);
    assert!(next.profiles[1].is_default);
    assert_eq!(next.profiles[1].id, "keep");
    // Not applied locally.
    assert_eq!(
        store.settings_config.as_ref().unwrap().active_profile_id,
        "old"
    );

    let fresh = next_settings_for_save(None, profile("new", false));
    assert_eq!(fresh.active_profile_id, "new");
    assert_eq!(fresh.profiles.len(), 1);
    assert!(fresh.global_instructions.is_empty());
    assert!(!fresh.memory.enabled);
    assert!(!fresh.skills.enabled);
}

#[test]
fn delete_profile_rejects_without_settings_and_falls_back_when_active() {
    let mut store = AiStore::new();
    assert_eq!(store.delete_profile("a"), ProfileEdit::Rejected);
    assert!(!store.saving);

    store.settings_config = Some(settings("a", vec![profile("a", true), profile("b", false)]));
    let ProfileEdit::Save(effect) = store.delete_profile("a") else {
        panic!("expected a save");
    };
    let next = saved(effect);
    assert_eq!(next.active_profile_id, "b");
    assert_eq!(next.profiles.len(), 1);
    assert!(!next.profiles[0].is_default);
    assert_eq!(next.global_instructions, "keep me");

    let only =
        next_settings_for_delete(Some(&settings("solo", vec![profile("solo", true)])), "solo");
    assert_eq!(only.unwrap().active_profile_id, "");
}

#[test]
fn providers_remote_models_validate_save_and_delete() {
    let mut store = AiStore::new();
    assert_eq!(store.begin_load_providers(false), ConfigStart::Skipped);
    assert_eq!(store.begin_load_providers(true), ConfigStart::Started);
    store.complete_load_providers(vec![ProviderListItem {
        provider_type: "openai".into(),
        display_name: "OpenAI".into(),
    }]);
    store.config_error = Some("leave me".into());
    store.fail_load_providers();
    assert_eq!(store.providers.len(), 1);
    assert_eq!(store.config_error.as_deref(), Some("leave me"));

    let effect = store.begin_fetch_remote_models("openai", "https://example", "k");
    assert!(store.fetching_remote_models);
    assert!(store.config_error.is_none());
    assert!(matches!(effect, AiConfigEffect::FetchRemoteModels { .. }));
    let models = store.complete_fetch_remote_models(vec![ModelInfo {
        id: "gpt".into(),
        display_name: "GPT".into(),
    }]);
    assert_eq!(models.len(), 1);
    assert!(!store.fetching_remote_models);
    assert!(store.fail_fetch_remote_models("remote").is_empty());
    assert_eq!(store.config_error.as_deref(), Some("remote"));
    // Failure does not clear the list already stored.
    assert_eq!(store.remote_models.len(), 1);

    let old = provider("old");
    store.config = Some(old.clone());
    assert!(store.complete_validate());
    let _ = store.begin_validate(provider("draft"));
    assert!(store.validating);
    assert!(!store.fail_validate("bad key"));
    assert_eq!(
        store.config.as_ref().map(|cfg| cfg.model.as_str()),
        Some("old")
    );

    let _ = store.begin_save_config(provider("new"));
    assert!(store.saving);
    assert!(!store.fail_save_config("disk"));
    assert_eq!(
        store.config.as_ref().map(|cfg| cfg.model.as_str()),
        Some("old")
    );
    let _ = store.begin_save_config(provider("new"));
    assert!(store.complete_save_config(provider("new")));
    assert!(store.is_configured);
    assert!(!store.saving);

    store.settings_config = Some(settings("a", vec![profile("a", true)]));
    assert_eq!(store.begin_delete_config(), AiConfigEffect::DeleteProvider);
    store.complete_delete_config();
    assert!(store.config.is_none());
    assert!(!store.is_configured);
    assert!(store.settings_config.is_some());
    store.fail_delete_config("denied");
    assert_eq!(store.config_error.as_deref(), Some("denied"));
    store.clear_error();
    assert!(store.config_error.is_none());
    assert!(store.settings_config.is_some());
}

#[test]
fn set_active_profile_clears_loading_only_after_the_reload() {
    let mut store = AiStore::new();
    let effect = store.begin_set_active_profile("p2");
    assert_eq!(effect, AiConfigEffect::SetActiveProfile("p2".into()));
    assert!(store.config_loading);
    assert!(store.ack_set_active_profile(false).is_none());
    // No backend: loadConfig returns immediately, then loading is cleared.
    assert!(store.config_loading);
    assert!(store.finish_set_active_profile());
    assert!(!store.config_loading);

    let _ = store.begin_set_active_profile("p3");
    assert!(!store.fail_set_active_profile("missing"));
    assert!(!store.config_loading);
    assert_eq!(store.config_error.as_deref(), Some("missing"));
}

#[test]
fn load_settings_does_not_toggle_the_loading_flag() {
    let mut store = AiStore::new();
    assert_eq!(store.begin_load_settings(false), ConfigStart::Skipped);
    assert_eq!(store.begin_load_settings(true), ConfigStart::Started);
    assert!(!store.config_loading);
    store.complete_load_settings(None, settings("", vec![]));
    assert!(!store.is_configured);
    store.fail_load_settings("settings down");
    assert_eq!(store.config_error.as_deref(), Some("settings down"));
    assert!(store.settings_config.is_some());
}
