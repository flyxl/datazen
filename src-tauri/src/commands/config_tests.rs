//! Tests for the app config / settings / path commands.

use super::*;
use std::path::Path;
#[test]
fn test_get_app_executable_path_returns_valid_path() {
    let exe = get_app_executable_path_impl().expect("executable path should be resolved");
    assert!(!exe.is_empty());
    assert!(Path::new(&exe).exists());
}

#[test]
fn path_is_under_matches_prefix() {
    assert!(path_is_under(
        Path::new("/data/app/logs"),
        Path::new("/data/app")
    ));
    assert!(!path_is_under(
        Path::new("/tmp/evil"),
        Path::new("/data/app")
    ));
}

#[test]
fn require_webdriver_path_ipc_gates_without_feature() {
    let result = require_webdriver_path_ipc("Direct path connection export disabled");
    if cfg!(feature = "webdriver") {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }
}

#[tokio::test]
async fn config_store_commands_via_impl() {
    use crate::store::AppSettings;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    save_groups_impl(&test.state, vec!["alpha".into(), "beta".into()])
        .await
        .unwrap();
    assert_eq!(
        get_groups_impl(&test.state).await.unwrap(),
        vec!["alpha", "beta"]
    );

    let mut settings = AppSettings::default();
    settings.language = "zh-CN".into();
    save_settings_impl(&test.state, settings.clone())
        .await
        .unwrap();
    assert_eq!(
        get_settings_impl(&test.state).await.unwrap().language,
        "zh-CN"
    );

    let log_path = get_log_path_impl(&test.state).await.unwrap();
    assert!(log_path.contains("logs"));
}

#[tokio::test]
async fn open_path_validation_without_webdriver() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    if cfg!(feature = "webdriver") {
        return;
    }
    let err = open_path_impl(&test.state, "../etc/passwd".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("disabled") || err.to_string().contains("traversal"));
}

#[test]
fn resolve_log_and_context_dirs_via_crate_helpers() {
    let data = Path::new("/data/app");
    assert_eq!(
        crate::resolve_log_dir(data, ""),
        Path::new("/data/app/logs")
    );
    assert_eq!(crate::resolve_context_dir(data, "/ctx"), Path::new("/ctx"));
}

#[tokio::test]
async fn get_log_path_honors_custom_log_path_setting() {
    use crate::store::AppSettings;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let mut settings = AppSettings::default();
    settings.log_path = test
        ._temp
        .path()
        .join("custom-logs")
        .to_string_lossy()
        .into();
    save_settings_impl(&test.state, settings).await.unwrap();
    let log_path = get_log_path_impl(&test.state).await.unwrap();
    assert!(log_path.contains("custom-logs"));
}
