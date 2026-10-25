//! The encrypted desktop adapter uses the shared profile fallback on save and reload.
use super::fixtures::init_store_for_test;
use datazen_modules::ai::{AiModelProfile, AiProviderConfig, AiProviderType, AiSettingsConfig};

fn profile(id: &str, is_default: bool) -> AiModelProfile {
    let mut profile = AiModelProfile::from(&AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: None,
        endpoint: None,
        model: "fixture".into(),
        max_tokens: 1000,
        extra: serde_json::Value::Null,
    });
    profile.id = id.into();
    profile.model = format!("model-{id}");
    profile.is_default = is_default;
    profile
}

#[tokio::test]
async fn save_and_reload_use_shared_selection_and_preserve_normalization_rules() {
    for (active, has_default, expected) in [
        ("active", true, "active"),
        ("missing", true, "default"),
        ("", true, "default"),
        ("missing", false, "first"),
        ("", false, "first"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let config = AiSettingsConfig {
            active_profile_id: active.into(),
            profiles: vec![
                profile("first", false),
                profile("default", has_default),
                profile("active", false),
            ],
        };
        assert_eq!(config.selected_profile().unwrap().id, expected);
        store.save_ai_settings_config(&config).await.unwrap();
        assert_eq!(
            store.get_ai_config().await.unwrap().model,
            format!("model-{expected}")
        );
        // Save retains the supplied selection; loading normalizes a stale id.
        assert_eq!(
            store.get_ai_settings_config().await.active_profile_id,
            active
        );
        drop(store);
        let reloaded = init_store_for_test(dir.path()).await;
        assert_eq!(
            reloaded.get_ai_config().await.unwrap().model,
            format!("model-{expected}")
        );
        assert_eq!(
            reloaded.get_ai_settings_config().await.active_profile_id,
            expected
        );
    }
}

#[tokio::test]
async fn empty_profiles_have_no_provider_before_or_after_reload() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let config = AiSettingsConfig {
        active_profile_id: "missing".into(),
        profiles: vec![],
    };
    assert!(config.selected_profile().is_none());
    store.save_ai_settings_config(&config).await.unwrap();
    assert!(store.get_ai_config().await.is_none());
    assert!(store.get_ai_settings_config().await.profiles.is_empty());
    drop(store);
    let reloaded = init_store_for_test(dir.path()).await;
    assert!(reloaded.get_ai_config().await.is_none());
    assert!(reloaded.get_ai_settings_config().await.profiles.is_empty());
}
