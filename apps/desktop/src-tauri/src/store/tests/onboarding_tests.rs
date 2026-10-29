//! The first-run wizard's persisted state and the three ways an install can
//! arrive at it: brand new, upgraded from a version that predates onboarding, or
//! already finished.
//!
//! The upgrade case is the load-bearing one — it must not re-show the wizard to
//! users who already have a settings file.

use super::super::*;
use super::fixtures::*;
use crate::store::settings::ONBOARDING_VERSION;

#[test]
fn onboarding_state_roundtrip_and_legacy_compat() {
    // roundtrip: completed + version survive serde
    let settings = AppSettings {
        onboarding: Some(OnboardingState {
            completed: true,
            version: 1,
        }),
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert!(json.contains("onboarding"));
    let parsed: AppSettings = serde_json::from_str(&json).unwrap();
    let state = parsed.onboarding.as_ref().unwrap();
    assert!(state.completed);
    assert_eq!(state.version, 1);

    // legacy compat: old settings.json without the key → None (treated as an upgrade)
    let mut legacy = serde_json::to_value(AppSettings::default()).unwrap();
    legacy.as_object_mut().unwrap().remove("onboarding");
    let parsed: AppSettings = serde_json::from_value(legacy).unwrap();
    assert!(parsed.onboarding.is_none());
}

#[tokio::test]
async fn fresh_install_materializes_uncompleted_onboarding() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    let onboarding = store
        .get_settings()
        .await
        .onboarding
        .expect("fresh install must materialize onboarding state");
    assert!(!onboarding.completed, "fresh install shows the journey");
    assert_eq!(onboarding.version, ONBOARDING_VERSION);

    // Materialized on disk so a settings write cannot make the next launch
    // look like an upgrade.
    assert!(dir.path().join("settings.json").exists());
    let reloaded = init_store_for_test(dir.path()).await;
    let onboarding = reloaded.get_settings().await.onboarding.unwrap();
    assert!(
        !onboarding.completed,
        "an unfinished fresh install stays uncompleted after a restart"
    );
}

#[tokio::test]
async fn existing_settings_without_onboarding_key_is_an_upgrade() {
    let dir = tempfile::tempdir().unwrap();

    // Settings written by a build that predates the wizard.
    let mut legacy = serde_json::to_value(AppSettings::default()).unwrap();
    legacy.as_object_mut().unwrap().remove("onboarding");
    legacy["language"] = serde_json::json!("en");
    std::fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string_pretty(&legacy).unwrap(),
    )
    .unwrap();

    let store = init_store_for_test(dir.path()).await;
    let onboarding = store
        .get_settings()
        .await
        .onboarding
        .expect("upgrade must resolve an onboarding state");
    assert!(
        onboarding.completed,
        "upgrading users must not see the journey"
    );
    assert_eq!(onboarding.version, ONBOARDING_VERSION);

    // The resolved state is persisted so the decision is not re-derived.
    let persisted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        persisted["onboarding"]["completed"],
        serde_json::json!(true)
    );
}

#[tokio::test]
async fn existing_onboarding_state_is_respected() {
    let dir = tempfile::tempdir().unwrap();
    let mut settings = AppSettings::default_for_first_run();
    settings.onboarding = Some(OnboardingState {
        completed: false,
        version: 1,
    });
    std::fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();

    let store = init_store_for_test(dir.path()).await;
    let onboarding = store.get_settings().await.onboarding.unwrap();
    assert!(
        !onboarding.completed,
        "an explicit unfinished state survives restart (mid-journey quit)"
    );
}
