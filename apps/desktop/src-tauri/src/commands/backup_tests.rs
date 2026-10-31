//! Host-side tests for the backup/restore IPC surface.
//!
//! 2026-10-31 拆分：选项解析/过滤校验的纯逻辑钉子已随逻辑归位
//! `modules/src/backup/options_tests.rs`；这里只留宿主绑定——
//! 错误信封映射（`From<DriverError>`）、bootstrap 注册守卫、
//! IPC 参数形状守卫，以及经真实命令入口的备份/恢复往返。

use super::*;
use datazen_driver_api::DriverError;

// Registered IPC signatures now live together in the native backup adapter.
const SOURCE: &str = include_str!("backup.rs");
const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");

#[test]
fn bootstrap_rs_registers_merged_commands_only() {
    assert!(
        BOOTSTRAP_RS.contains("commands::backup_database,"),
        "merged `backup_database` must stay registered"
    );
    assert!(
        BOOTSTRAP_RS.contains("commands::restore_sql_file,"),
        "`restore_sql_file` must be registered"
    );
    for gone in [
        "commands::backup_database_with_dialog,",
        "commands::restore_database,",
        "commands::restore_database_with_dialog,",
        "commands::execute_sql_file,",
        "commands::execute_sql_file_with_dialog,",
    ] {
        assert!(
            !BOOTSTRAP_RS.contains(gone),
            "`{gone}` must no longer be registered in bootstrap.rs"
        );
    }
}

#[test]
fn driver_err_not_supported_is_validation() {
    let err = CommandError::from(DriverError::NotSupported("create".into()));
    assert!(matches!(err, CommandError::Validation(msg) if msg == "create"));
}

#[test]
fn driver_err_other_stays_driver() {
    let err = CommandError::from(DriverError::QueryFailed("boom".into()));
    assert!(matches!(err, CommandError::Driver(_)));
}

#[tokio::test]
async fn backup_and_restore_roundtrip() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_tables_and_resource_provider().await;
    let conn_id = test.save_and_open_runtime_session("backup-cfg").await;
    let backup_path = test._temp.path().join("backup.sql");

    backup_database_to_path(
        &test.state,
        conn_id.clone(),
        Some("app".into()),
        backup_path.clone(),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let sql = std::fs::read_to_string(&backup_path).unwrap();
    assert!(sql.contains("INSERT INTO"));

    let mut settings = test.state.store.get_settings().await;
    settings.safe_mode = false;
    test.state.store.save_settings(settings).await.unwrap();

    restore_database_from_path(
        &test.state,
        None,
        conn_id,
        Some("app".into()),
        backup_path,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn backup_database_errors_when_not_connected() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_tables().await;
    let path = test._temp.path().join("fail.sql");
    assert!(
        backup_database_to_path(&test.state, "missing".into(), None, path, None, None, None,)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unsupported_backup_option_is_rejected_before_driver_connection() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let path = test._temp.path().join("unsupported-format.dump");
    let error = backup_database_to_path(
        &test.state,
        "missing".into(),
        None,
        path.clone(),
        Some(vec!["format-custom".into()]),
        None,
        None,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("format-custom"));
    assert_eq!(test.mock.connect_calls(), 0);
    assert_eq!(test.mock.disconnect_calls(), 0);
    assert!(!path.exists());
}

#[tokio::test]
async fn restore_database_rejects_read_only_connection() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_tables_and_resource_provider().await;
    let mut cfg = test.save_connection("backup-ro").await;
    cfg.read_only = true;
    test.state.store.save_connection(cfg).await.unwrap();
    let conn_id = test.open_runtime_session("backup-ro").await;
    let backup_path = test._temp.path().join("readonly.sql");
    std::fs::write(&backup_path, "SELECT 1;").unwrap();

    let err = restore_database_from_path(
        &test.state,
        None,
        conn_id,
        Some("app".into()),
        backup_path,
        None,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Connection is read-only"));
}

#[tokio::test]
async fn restore_database_rejects_safe_mode() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_tables_and_resource_provider().await;
    let mut settings = test.state.store.get_settings().await;
    settings.safe_mode = true;
    test.state.store.save_settings(settings).await.unwrap();

    let conn_id = test.save_and_open_runtime_session("backup-safe").await;
    let backup_path = test._temp.path().join("safe-mode.sql");
    std::fs::write(&backup_path, "SELECT 1;").unwrap();

    let err = restore_database_from_path(
        &test.state,
        None,
        conn_id,
        Some("app".into()),
        backup_path,
        None,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Safe mode is enabled"));
}

mod ipc_contract_guards {
    use super::*;

    /// Extracts the parameter list of a `pub async fn <command>(...)`.
    fn command_params(command: &str) -> String {
        let needle = format!("pub async fn {command}(");
        let start = SOURCE
            .find(&needle)
            .unwrap_or_else(|| panic!("command `{command}` not found in backup.rs"));
        let rest = &SOURCE[start + needle.len()..];
        let end = rest.find(')').expect("unterminated parameter list");
        rest[..end].to_string()
    }

    #[test]
    fn session_semantics_commands_take_db_session_id() {
        for cmd in ["backup_database", "restore_sql_file"] {
            let params = command_params(cmd);
            assert!(
                params.contains("db_session_id"),
                "`{cmd}` must take `db_session_id` (runtime session semantics); got: {params}"
            );
            let without_new = params.replace("db_session_id", "");
            assert!(
                !without_new.contains("connection_id"),
                "`{cmd}` must not take (or also take) `connection_id`; got: {params}"
            );
        }
    }

    #[test]
    fn merged_backup_database_takes_override_path_not_raw_output_path() {
        let params = command_params("backup_database");
        assert!(
            params.contains("override_path: Option<String>"),
            "`backup_database` must expose the webdriver-only override; got: {params}"
        );
        assert!(
            !params.contains("output_path"),
            "raw `output_path` must not return after the dialog/path merge; got: {params}"
        );
        // Dialog-era wire parameters stay compatible for production callers.
        for param in ["default_file_name", "filter_extension", "options"] {
            assert!(
                params.contains(param),
                "`backup_database` must keep dialog param `{param}`; got: {params}"
            );
        }
    }

    #[test]
    fn restore_sql_file_maps_four_former_commands_params() {
        let params = command_params("restore_sql_file");
        assert!(
            params.contains("override_path: Option<String>"),
            "`override_path` replaces both raw-path variants' `input_path`; got: {params}"
        );
        assert!(
            !params.contains("input_path"),
            "raw `input_path` must not return after the four-way merge; got: {params}"
        );
        // Compat mapping:
        //   restore_database(db_session_id, input_path, options, database)
        //   execute_sql_file(db_session_id, input_path, options, database)
        //   restore_database_with_dialog(db_session_id, database, options)
        //   execute_sql_file_with_dialog(db_session_id, database, options)
        // → restore_sql_file(db_session_id, database, options, override_path)
        assert!(params.contains("database: Option<String>"));
        assert!(params.contains("options: Option<Vec<String>>"));
        // The old entry points must be gone from this module. Needles are
        // assembled at runtime so this test's own source never contains them
        // (mirroring strict_session_lookups… below).
        let fn_prefix = "pub async fn ";
        let with_dialog = "_with_";
        let dialog_kind = "dialog";
        for name in ["restore_database", "execute_sql_file"] {
            let plain = format!("{fn_prefix}{name}(");
            assert!(!SOURCE.contains(&plain), "stale raw-path command `{plain}`");
            let variant = format!("{fn_prefix}{name}{with_dialog}{dialog_kind}(");
            assert!(
                !SOURCE.contains(&variant),
                "stale dialog command `{variant}`"
            );
        }
    }

    #[test]
    fn strict_session_lookups_are_never_fed_a_connection_id_binding() {
        // Body-level guard mirroring the signature guards above. The needles are
        // assembled at runtime so this test module never contains them.
        let conn = "connection_";
        let id = "id";
        let conn_id = format!("{conn}{id}");
        assert!(!SOURCE.contains(&format!(".get_session(&{conn_id})")));
        assert!(!SOURCE.contains(&format!(".get_session({conn_id})")));
        assert!(!SOURCE.contains(&format!(".get_session_config(&{conn_id})")));
        assert!(!SOURCE.contains(&format!(".get_session_config({conn_id})")));
    }
}
