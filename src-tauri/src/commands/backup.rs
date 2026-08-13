use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{
    BackupDumpOptions, BackupRestoreOptions, ConnectionHandle, DatabaseDriver, DriverError,
    DumpPhase, DumpProgress, TableInfo, TableType,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, State};

fn emit_backup_progress(app: Option<&tauri::AppHandle>, progress: DumpProgress) {
    if let Some(app) = app {
        let _ = app.emit("backup-progress", &progress);
    }
}

fn emit_restore_progress(app: Option<&tauri::AppHandle>, progress: DumpProgress) {
    if let Some(app) = app {
        let _ = app.emit("restore-progress", &progress);
    }
}

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

pub(crate) fn parse_backup_options(options: &[String]) -> Result<BackupDumpOptions, CommandError> {
    if options.iter().any(|o| o == "format-custom") {
        return Err(CommandError::Validation(
            "Backup option 'format-custom' requires pg_dump custom binary format and is not supported"
                .into(),
        ));
    }
    let opts: std::collections::HashSet<String> = options.iter().cloned().collect();
    Ok(BackupDumpOptions {
        schema_only: opts.contains("schema-only") || opts.contains("no-data"),
        data_only: opts.contains("data-only") || opts.contains("no-create-info"),
        clean: opts.contains("clean") || opts.contains("add-drop-table"),
        create_database: opts.contains("create"),
        no_owner: opts.contains("no-owner"),
        single_transaction: opts.contains("single-transaction"),
        routines: opts.contains("routines"),
        triggers: opts.contains("triggers"),
    })
}

pub(crate) fn parse_restore_options(options: &[String]) -> BackupRestoreOptions {
    let opts: std::collections::HashSet<String> = options.iter().cloned().collect();
    BackupRestoreOptions {
        single_transaction: opts.contains("single-transaction"),
        overwrite: opts.contains("overwrite"),
    }
}

pub(crate) fn validate_backup_filter_extension(
    filter_extension: &str,
) -> Result<String, CommandError> {
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
        None,
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
    backup_database_to_path(
        &state,
        connection_id,
        database,
        path,
        options,
        compress,
        Some(&app),
    )
    .await?;
    Ok(true)
}

async fn backup_database_to_path(
    state: &AppState,
    connection_id: String,
    database: Option<String>,
    output_path: PathBuf,
    options: Option<Vec<String>>,
    compress: Option<bool>,
    app: Option<&tauri::AppHandle>,
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
    let opts = parse_backup_options(&options.unwrap_or_default())?;

    let mut on_progress = |progress: DumpProgress| emit_backup_progress(app, progress);
    let out = driver
        .dump_database_with_progress(&handle, &db_name, &opts, &mut on_progress)
        .await
        .map_err(|e| {
            let err = map_driver_err(e);
            tracing::error!(cmd = "backup_database", error = %err);
            err
        })?;

    emit_backup_progress(
        app,
        DumpProgress {
            current: 0,
            total: 0,
            object_name: String::new(),
            phase: DumpPhase::Writing,
        },
    );

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
    emit_backup_progress(
        app,
        DumpProgress {
            current: 0,
            total: 0,
            object_name: String::new(),
            phase: DumpPhase::Done,
        },
    );
    tracing::info!(path = %output_path.display(), "backup_database OK");
    Ok(())
}

#[tauri::command]
pub async fn restore_database(
    state: State<'_, AppState>,
    connection_id: String,
    input_path: String,
    options: Option<Vec<String>>,
    database: Option<String>,
) -> Result<(), CommandError> {
    require_webdriver_path_ipc("Direct path restore disabled; use restore_database_with_dialog")?;
    restore_database_from_path(
        &state,
        None,
        connection_id,
        database,
        PathBuf::from(input_path),
        options,
    )
    .await
}

/// Native open dialog + restore. Returns `true` if restored.
#[tauri::command]
pub async fn restore_database_with_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    options: Option<Vec<String>>,
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
    restore_database_from_path(&state, Some(&app), connection_id, database, path, options).await?;
    Ok(true)
}

fn qualify_restore_ident(driver: &dyn DatabaseDriver, table: &TableInfo) -> String {
    match table.schema.as_deref().filter(|s| !s.is_empty()) {
        Some(schema) => format!(
            "{}.{}",
            driver.quote_ident(schema),
            driver.quote_ident(&table.name)
        ),
        None => driver.quote_ident(&table.name),
    }
}

async fn drop_existing_restore_targets(
    driver: &Arc<dyn DatabaseDriver>,
    handle: &ConnectionHandle,
    database: &str,
    app: Option<&tauri::AppHandle>,
) -> Result<(), CommandError> {
    let tables = driver
        .get_tables(handle, database)
        .await
        .cmd_err("restore_database")?;
    if tables.is_empty() {
        return Ok(());
    }

    let mut views = Vec::new();
    let mut rest = Vec::new();
    for table in tables {
        if matches!(
            table.table_type,
            TableType::View | TableType::MaterializedView
        ) {
            views.push(table);
        } else {
            rest.push(table);
        }
    }
    let ordered: Vec<TableInfo> = views.into_iter().chain(rest).collect();
    let total = ordered.len() as u32;

    emit_restore_progress(
        app,
        DumpProgress {
            current: 0,
            total,
            object_name: String::new(),
            phase: DumpPhase::Object,
        },
    );

    let _ = driver.execute(handle, "SET FOREIGN_KEY_CHECKS=0").await;
    for (i, table) in ordered.iter().enumerate() {
        let ident = qualify_restore_ident(driver.as_ref(), table);
        let keyword = if matches!(
            table.table_type,
            TableType::View | TableType::MaterializedView
        ) {
            "VIEW"
        } else {
            "TABLE"
        };
        emit_restore_progress(
            app,
            DumpProgress {
                current: (i as u32) + 1,
                total,
                object_name: format!("DROP {keyword} {ident}"),
                phase: DumpPhase::Object,
            },
        );
        let cascade = format!("DROP {keyword} IF EXISTS {ident} CASCADE");
        if driver.execute(handle, &cascade).await.is_err() {
            driver
                .execute(handle, &format!("DROP {keyword} IF EXISTS {ident}"))
                .await
                .map_err(|e| {
                    tracing::error!(
                        cmd = "restore_database",
                        table = %table.name,
                        error = %e
                    );
                    map_driver_err(e)
                })?;
        }
    }
    let _ = driver.execute(handle, "SET FOREIGN_KEY_CHECKS=1").await;
    Ok(())
}

async fn restore_database_from_path(
    state: &AppState,
    app: Option<&tauri::AppHandle>,
    connection_id: String,
    database: Option<String>,
    input_path: PathBuf,
    options: Option<Vec<String>>,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, path = %input_path.display(), "restore_database");
    emit_restore_progress(
        app,
        DumpProgress {
            current: 0,
            total: 0,
            object_name: String::new(),
            phase: DumpPhase::Object,
        },
    );

    let sql = tokio::fs::read_to_string(&input_path)
        .await
        .cmd_err("restore_database")?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("restore_database")?;

    let restore_opts = parse_restore_options(&options.unwrap_or_default());
    tracing::info!(
        %connection_id,
        bytes = sql.len(),
        overwrite = restore_opts.overwrite,
        "restore_database file loaded"
    );
    if restore_opts.overwrite {
        let db_name = if let Some(name) = database.as_deref().filter(|s| !s.is_empty()) {
            name.to_string()
        } else {
            let config = state
                .connection_manager
                .get_connection_config(&connection_id)
                .await
                .cmd_err("restore_database")?;
            config.database.unwrap_or_default()
        };
        if !db_name.is_empty() {
            drop_existing_restore_targets(&driver, &handle, &db_name, app).await?;
        }
    }

    let mut on_progress = |progress: DumpProgress| emit_restore_progress(app, progress);
    driver
        .restore_sql_with_progress(&handle, &sql, Some(&restore_opts), &mut on_progress)
        .await
        .map_err(|e| {
            let err = map_driver_err(e);
            tracing::error!(cmd = "restore_database", error = %err);
            err
        })?;

    emit_restore_progress(
        app,
        DumpProgress {
            current: 0,
            total: 0,
            object_name: String::new(),
            phase: DumpPhase::Done,
        },
    );
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
            "no-owner".into(),
            "single-transaction".into(),
            "routines".into(),
            "triggers".into(),
        ])
        .unwrap();
        assert_eq!(
            opts,
            BackupDumpOptions {
                schema_only: true,
                data_only: false,
                clean: true,
                create_database: true,
                no_owner: true,
                single_transaction: true,
                routines: true,
                triggers: true,
            }
        );
        let opts = parse_backup_options(&["no-data".into(), "no-create-info".into()]).unwrap();
        assert!(opts.schema_only);
        assert!(opts.data_only);
    }

    #[test]
    fn parse_backup_options_rejects_format_custom() {
        let err = parse_backup_options(&["format-custom".into()]).unwrap_err();
        assert!(matches!(err, CommandError::Validation(msg) if msg.contains("format-custom")));
    }

    #[test]
    fn parse_restore_options_recognizes_single_transaction() {
        let opts = parse_restore_options(&["single-transaction".into()]);
        assert!(opts.single_transaction);
        assert!(!parse_restore_options(&[]).single_transaction);
        assert!(!opts.overwrite);
        assert!(parse_restore_options(&["overwrite".into()]).overwrite);
    }

    #[test]
    fn validate_backup_filter_extension_accepts_sql_gz_dump() {
        assert_eq!(validate_backup_filter_extension("sql").unwrap(), "sql");
        assert_eq!(validate_backup_filter_extension(".GZ").unwrap(), "gz");
        assert!(validate_backup_filter_extension("exe").is_err());
    }

    #[test]
    fn require_webdriver_path_ipc_gates_without_feature() {
        let result = require_webdriver_path_ipc(
            "Direct path backup disabled; use backup_database_with_dialog",
        );
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
            None,
        )
        .await
        .unwrap();

        let sql = std::fs::read_to_string(&backup_path).unwrap();
        assert!(sql.contains("INSERT INTO"));

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
        assert!(backup_database_to_path(
            &test.state,
            "missing".into(),
            None,
            path,
            None,
            None,
            None,
        )
        .await
        .is_err());
    }
}
