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

    let mut shared = get_shared_settings_impl(&test.state).await.unwrap();
    shared.preferences.language = "ja-JP".into();
    let saved_shared = save_shared_settings_impl(&test.state, shared.clone())
        .await
        .unwrap();
    assert_eq!(saved_shared, shared);
    assert_eq!(get_shared_settings_impl(&test.state).await.unwrap(), shared);

    let mut invalid = shared.clone();
    invalid.preferences.theme.mode = "neon".into();
    let error = save_shared_settings_impl(&test.state, invalid)
        .await
        .expect_err("invalid enum must fail at the command boundary");
    assert!(
        matches!(error, CommandError::Validation(message) if message == "Invalid settings enum value")
    );
    assert_eq!(get_shared_settings_impl(&test.state).await.unwrap(), shared);

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

/// §5.1.2 设置命令族的验收断言：设置与分组只走应用层聚合。
///
/// 这两条命令原来自行拼服务——`SettingsService::new(state.store.as_ref())`。
/// 那种写法让「设置用例归位」在源码上根本没发生：命令仍然是服务的
/// 构造者与持有者。
///
/// D1 收尾后设置用例由 `datazen-modules::settings` 拥有，宿主 adapter 持有
/// 服务实例，命令经 `platform::ipc::require_settings_service` 取用例。
/// 宿主 store 只作为端口实现被注入，不再出现在调用点。
/// **这条断言盯的就是「不再出现」**：只要有人
/// 为了「装配失败时也能读设置」把直连加回来，它就红。
#[test]
fn shared_settings_commands_read_the_aggregate_not_the_host_store() {
    let source = include_str!("config.rs");
    let production = source
        .split("#[cfg(test)]")
        .next()
        .expect("config.rs 必有生产半边");
    assert!(
        !production.contains("SettingsService::new("),
        "设置命令不得自己拼 SettingsService：应当经 platform::ipc::require_settings_service 取宿主装配的用例"
    );
    assert!(
        !production.contains("state.store.as_ref()"),
        "设置命令不得直连宿主 store：设置仓储由模块服务经端口持有"
    );
    assert_eq!(
        production.matches("settings_service()").count(),
        4,
        "完整设置读写与共享设置读写都必须经聚合取用例"
    );
    assert!(
        !production.contains("state.store.get_settings")
            && !production.contains("state.store.get_groups")
            && !production.contains("state.store.save_groups"),
        "设置与分组不得直读/直写宿主 store"
    );
    // `data_dir()` 仍是 §5.1.3 的桌面路径能力，本族只把设置内容本身移入聚合。
    assert_eq!(
        production.matches("state.store").count(),
        4,
        "config.rs 里只允许保留 4 处桌面 data_dir() 路径访问"
    );
}
