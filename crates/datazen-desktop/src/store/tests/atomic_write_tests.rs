//! Crash-safety of the settings and AI-config writes: a save must not leave a
//! `.tmp` file behind, and a delete must remove the file rather than blank it.
//!
//! A leftover temp file is not cosmetic — the next startup enumerates the data
//! directory and would treat it as state.

use super::super::*;
use super::fixtures::*;

#[tokio::test]
async fn save_json_file_leaves_no_tmp_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let settings = AppSettings {
        language: "fr".into(),
        ..AppSettings::default()
    };
    store.save_settings(settings).await.unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!entries.iter().any(|n| n.contains(".tmp")));
    let content = tokio::fs::read_to_string(dir.path().join("settings.json"))
        .await
        .unwrap();
    let parsed: AppSettings = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed.language, "fr");
}

#[tokio::test]
async fn save_ai_config_uses_atomic_encrypted_write() {
    use datazen::ai::{AiProviderConfig, AiProviderType};

    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let config = AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: Some("sk-test".into()),
        endpoint: None,
        model: "gpt-4o".into(),
        max_tokens: 200_000,
        extra: serde_json::Value::Null,
    };
    store.save_ai_config(&config).await.unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(entries.contains(&"ai_config.enc".to_string()));
    assert!(!entries.iter().any(|n| n.contains(".tmp")));
    let loaded = store.get_ai_config().await.unwrap();
    assert_eq!(loaded.model, "gpt-4o");
}

#[tokio::test]
async fn delete_ai_config_removes_file() {
    use datazen::ai::{AiProviderConfig, AiProviderType};

    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    let config = AiProviderConfig {
        provider_type: AiProviderType::OpenAi,
        api_key: Some("sk-x".into()),
        endpoint: None,
        model: "gpt-4o".into(),
        max_tokens: 1000,
        extra: serde_json::Value::Null,
    };
    store.save_ai_config(&config).await.unwrap();
    assert!(store.get_ai_config().await.is_some());

    store.delete_ai_config().await.unwrap();
    assert!(store.get_ai_config().await.is_none());
    assert!(!dir.path().join("ai_config.enc").exists());
}
