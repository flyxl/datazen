//! Tests for the app config / settings / path commands.
//!
//! B2 后命令处理函数直接持有接线（无 `_impl` 转发层），行为测试经真实命令
//! 入口驱动（`host_command_app` 的 mock 宿主，见
//! `dashboard_command_test_host.rs`——本刀同时拥有 dashboard 与 config 两域，
//! 共用一个宿主）。留在宿主的是 C1–C4：可执行文件路径（C3）、路径门与
//! webdriver 门（C3）、设置读写适配器烟测（C4）、聚合装配守卫（C2）。
//! 纯设置规则（枚举校验、归一化）在 `modules/src/settings` 已有覆盖。

use super::*;
use std::path::Path;
#[test]
fn test_get_app_executable_path_returns_valid_path() {
    let exe = get_app_executable_path().expect("executable path should be resolved");
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

/// 设置/分组命令族的适配器烟测（C4）：真实命令入口 → 宿主装配的设置/
/// 连接用例 → 临时 store。共享域的深度行为（枚举校验、归一化、仓储失败
/// 语义）在 `modules/src/settings/application.rs` 与连接域各有覆盖；这里
/// 钉的是「同一用例经宿主端口装配后端到端走通」与命令边界的错误文案。
#[tokio::test]
async fn config_store_commands_roundtrip() {
    use crate::commands::dashboard::command_test_host::host_command_app;
    use crate::store::AppSettings;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let app = host_command_app(test);
    save_groups(app.state(), vec!["alpha".into(), "beta".into()])
        .await
        .unwrap();
    assert_eq!(
        get_groups(app.state()).await.unwrap(),
        vec!["alpha", "beta"]
    );

    let mut settings = AppSettings::default();
    settings.language = "zh-CN".into();
    save_host_settings(app.state(), settings.clone())
        .await
        .unwrap();
    assert_eq!(
        get_host_settings(app.state()).await.unwrap().language,
        "zh-CN"
    );

    let mut shared = get_settings(app.state()).await.unwrap();
    shared.preferences.language = "ja-JP".into();
    let saved_shared = save_settings(
        app.state(),
        SaveSettingsRequest {
            settings: shared.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(saved_shared, shared);
    assert_eq!(get_settings(app.state()).await.unwrap(), shared);

    let mut invalid = shared.clone();
    invalid.preferences.theme.mode = "neon".into();
    let error = save_settings(app.state(), SaveSettingsRequest { settings: invalid })
        .await
        .expect_err("invalid enum must fail at the command boundary");
    assert!(
        matches!(error, CommandError::Validation(message) if message == "Invalid settings enum value")
    );
    assert_eq!(get_settings(app.state()).await.unwrap(), shared);

    let log_path = get_log_path(app.state()).await.unwrap();
    assert!(log_path.contains("logs"));
}

#[tokio::test]
async fn open_path_validation_without_webdriver() {
    use crate::commands::dashboard::command_test_host::host_command_app;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let app = host_command_app(test);
    if cfg!(feature = "webdriver") {
        return;
    }
    let err = open_path(app.state(), "../etc/passwd".into())
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
    use crate::commands::dashboard::command_test_host::host_command_app;
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
    // `log_path` 是 host 专属设置（F4a 分域后不在 SharedSettings 里），保存
    // 走宿主 store 的全量写路径，而不是 save_settings 命令的共享域通道；
    // 读取侧 get_log_path 命令经 settings 服务读回同一份 host 文档。
    let app = host_command_app(test);
    app.keep
        .store
        .save_settings(settings)
        .await
        .expect("host settings persist");
    let log_path = get_log_path(app.state()).await.unwrap();
    assert!(log_path.contains("custom-logs"));
}

/// §5.1.2 设置命令族的验收断言：设置与分组只走应用层聚合。
///
/// 这两条命令原来自行拼服务——`SettingsService::new(state.store.as_ref())`。
/// 那种写法让「设置用例归位」在源码上根本没发生：命令仍然是服务的
/// 构造者与持有者。
///
/// D1 收尾后设置用例由 `datazen::settings` 拥有，宿主 adapter 持有
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
    // D2 起调用形态统一为经 `platform::ipc` 取模块服务；B2 后接线住在命令
    // 处理函数里（`&state` 解引用，私有 read_host_settings 辅助同形），
    // 守卫盯同一约定（四条设置读写命令各取一次模块服务）。
    assert_eq!(
        production.matches("require_settings_service(").count(),
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
