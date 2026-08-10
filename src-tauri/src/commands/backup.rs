use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{BackupDumpOptions, DriverError};
use std::path::PathBuf;
use tauri::State;

fn require_webdriver_path_ipc(disabled_msg: &'static str) -> Result<(), CommandError> {
    if !cfg!(feature = "webdriver") {
        return Err(CommandError::Validation(disabled_msg.into()));
    }
    Ok(())
}

fn map_driver_err(e: DriverError) -> CommandError {
    match e {
        DriverError::NotSupported(msg) => CommandError::Validation(msg),
        other => CommandError::Driver(other),
    }
}

pub(crate) fn parse_backup_options(options: &[String]) -> BackupDumpOptions {
    let opts: std::collections::HashSet<String> = options.iter().cloned().collect();
    BackupDumpOptions {
        schema_only: opts.contains("schema-only") || opts.contains("no-data"),
        data_only: opts.contains("data-only") || opts.contains("no-create-info"),
        clean: opts.contains("clean") || opts.contains("add-drop-table"),
        create_database: opts.contains("create"),
    }
}

pub(crate) fn validate_backup_filter_extension(filter_extension: &str) -> Result<String, CommandError> {
    let ext = filter_extension.trim_start_matches('.').to_lowercase();
    let allowed = ["sql", "gz", "dump"];
    if !allowed.contains(&ext.as_str()) {
        return Err(CommandError::Validation(format!(
            "File extension '.{ext}' not allowed"
        )));
    }
    Ok(ext)
}

#[tauri::command]
pub async fn backup_database(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    output_path: String,
    options: Option<Vec<String>>,
    compress: Option<bool>,
) -> Result<(), CommandError> {
    require_webdriver_path_ipc("Direct path backup disabled; use backup_database_with_dialog")?;
    backup_database_to_path(
        &state,
        connection_id,
        database,
        PathBuf::from(output_path),
        options,
        compress,
    )
    .await
}

/// Native save dialog + database backup. Returns `true` if written.
#[tauri::command]
pub async fn backup_database_with_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    default_file_name: String,
    filter_extension: String,
    options: Option<Vec<String>>,
    compress: Option<bool>,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let ext = validate_backup_filter_extension(&filter_extension)?;

    let picked = app
        .dialog()
        .file()
        .add_filter("Backup", &[ext.as_str()])
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let path = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;
    backup_database_to_path(&state, connection_id, database, path, options, compress).await?;
    Ok(true)
}

async fn backup_database_to_path(
    state: &AppState,
    connection_id: String,
    database: Option<String>,
    output_path: PathBuf,
    options: Option<Vec<String>>,
    compress: Option<bool>,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, path = %output_path.display(), "backup_database");
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("backup_database")?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("backup_database")?;

    let db_name = database
        .as_deref()
        .unwrap_or(config.database.as_deref().unwrap_or(""))
        .to_string();
    let opts = parse_backup_options(&options.unwrap_or_default());

    let out = driver
        .dump_database(&handle, &db_name, &opts)
        .await
        .map_err(|e| {
            let err = map_driver_err(e);
            tracing::error!(cmd = "backup_database", error = %err);
            err
        })?;

    let data = out.as_bytes();
    if compress.unwrap_or(false) {
        use std::io::Write;
        let file = std::fs::File::create(&output_path).map_err(|e| {
            tracing::error!(cmd = "backup_database", error = %e);
            CommandError::Io(e)
        })?;
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder.write_all(data).map_err(|e| {
            tracing::error!(cmd = "backup_database", error = %e);
            CommandError::Io(e)
        })?;
        encoder.finish().map_err(|e| {
            tracing::error!(cmd = "backup_database", error = %e);
            CommandError::Io(e)
        })?;
    } else {
        tokio::fs::write(&output_path, data)
            .await
            .cmd_err("backup_database")?;
    }
    tracing::info!(path = %output_path.display(), "backup_database OK");
    Ok(())
}

#[tauri::command]
pub async fn restore_database(
    state: State<'_, AppState>,
    connection_id: String,
    input_path: String,
) -> Result<(), CommandError> {
    require_webdriver_path_ipc("Direct path restore disabled; use restore_database_with_dialog")?;
    restore_database_from_path(&state, connection_id, PathBuf::from(input_path)).await
}

/// Native open dialog + restore. Returns `true` if restored.
#[tauri::command]
pub async fn restore_database_with_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .add_filter("SQL", &["sql"])
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let path = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;
    restore_database_from_path(&state, connection_id, path).await?;
    Ok(true)
}

async fn restore_database_from_path(
    state: &AppState,
    connection_id: String,
    input_path: PathBuf,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, path = %input_path.display(), "restore_database");
    let sql = tokio::fs::read_to_string(&input_path)
        .await
        .cmd_err("restore_database")?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("restore_database")?;

    driver
        .restore_sql(&handle, &sql)
        .await
        .map_err(|e| {
            let err = map_driver_err(e);
            tracing::error!(cmd = "restore_database", error = %err);
            err
        })?;

    tracing::info!(%connection_id, "restore_database OK");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backup_options_recognizes_aliases() {
        let opts = parse_backup_options(&[
            "schema-only".into(),
            "clean".into(),
            "create".into(),
        ]);
        assert_eq!(
            opts,
            BackupDumpOptions {
                schema_only: true,
                data_only: false,
                clean: true,
                create_database: true,
            }
        );
        let opts = parse_backup_options(&["no-data".into(), "no-create-info".into()]);
        assert!(opts.schema_only);
        assert!(opts.data_only);
    }

    #[test]
    fn validate_backup_filter_extension_accepts_sql_gz_dump() {
        assert_eq!(validate_backup_filter_extension("sql").unwrap(), "sql");
        assert_eq!(validate_backup_filter_extension(".GZ").unwrap(), "gz");
        assert!(validate_backup_filter_extension("exe").is_err());
    }

    #[test]
    fn require_webdriver_path_ipc_gates_without_feature() {
        let result =
            require_webdriver_path_ipc("Direct path backup disabled; use backup_database_with_dialog");
        if cfg!(feature = "webdriver") {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("disabled"));
        }
    }

    #[test]
    fn map_driver_err_not_supported_is_validation() {
        let err = map_driver_err(DriverError::NotSupported("create".into()));
        assert!(matches!(err, CommandError::Validation(msg) if msg == "create"));
    }

    #[test]
    fn map_driver_err_other_stays_driver() {
        let err = map_driver_err(DriverError::QueryFailed("boom".into()));
        assert!(matches!(err, CommandError::Driver(_)));
    }

    #[tokio::test]
    async fn backup_and_restore_roundtrip() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("backup-cfg").await;
        let backup_path = test._temp.path().join("backup.sql");

        backup_database_to_path(
            &test.state,
            conn_id.clone(),
            Some("app".into()),
            backup_path.clone(),
            None,
            None,
        )
        .await
        .unwrap();

        let sql = std::fs::read_to_string(&backup_path).unwrap();
        assert!(sql.contains("INSERT INTO"));

        restore_database_from_path(&test.state, conn_id, backup_path)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn backup_database_errors_when_not_connected() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::with_tables().await;
        let path = test._temp.path().join("fail.sql");
        assert!(
            backup_database_to_path(
                &test.state,
                "missing".into(),
                None,
                path,
                None,
                None,
            )
            .await
            .is_err()
        );
    }
}
