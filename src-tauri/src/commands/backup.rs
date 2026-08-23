use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{
    BackupDumpOptions, BackupRestoreOptions, ConnectionHandle, DatabaseDriver, DriverError,
    DumpPhase, DumpProgress, RestoreSession, TableInfo, TableType, Utf8ChunkDecoder,
};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
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

/// Coalesce per-statement restore events so the webview is not flooded
/// (one INSERT per row previously froze the UI halfway through a dump).
struct ThrottledRestoreProgress<'a> {
    app: Option<&'a tauri::AppHandle>,
    last: Instant,
    pending: Option<DumpProgress>,
    interval: Duration,
}

impl<'a> ThrottledRestoreProgress<'a> {
    fn new(app: Option<&'a tauri::AppHandle>) -> Self {
        Self {
            app,
            last: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            pending: None,
            interval: Duration::from_millis(80),
        }
    }

    fn emit(&mut self, progress: DumpProgress) {
        let force = !matches!(progress.phase, DumpPhase::Object);
        if force || self.last.elapsed() >= self.interval {
            emit_restore_progress(self.app, progress);
            self.last = Instant::now();
            self.pending = None;
        } else {
            self.pending = Some(progress);
        }
    }

    fn flush(&mut self) {
        if let Some(progress) = self.pending.take() {
            emit_restore_progress(self.app, progress);
        }
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
    sql_file_with_dialog(&app, &state, connection_id, database, options).await
}

/// Native open dialog + execute a `.sql` file against the current connection.
/// Shares the same streaming restore pipeline as `restore_database_with_dialog`.
#[tauri::command]
pub async fn execute_sql_file_with_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    options: Option<Vec<String>>,
) -> Result<bool, CommandError> {
    sql_file_with_dialog(&app, &state, connection_id, database, options).await
}
/// Direct-path `.sql` file execution (no dialog). Available only in webdriver
/// builds so E2E can drive the streaming pipeline without a native picker.
#[tauri::command]
pub async fn execute_sql_file(
    state: State<'_, AppState>,
    connection_id: String,
    input_path: String,
    options: Option<Vec<String>>,
    database: Option<String>,
) -> Result<bool, CommandError> {
    require_webdriver_path_ipc(
        "Direct path sql file execution disabled; use execute_sql_file_with_dialog",
    )?;
    restore_database_from_path(
        &state,
        None,
        connection_id,
        database,
        PathBuf::from(input_path),
        options,
    )
    .await?;
    Ok(true)
}

async fn sql_file_with_dialog(
    app: &tauri::AppHandle,
    state: &AppState,
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
    restore_database_from_path(state, Some(app), connection_id, database, path, options).await?;
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
        emit_restore_progress(
            app,
            DumpProgress {
                current: (i as u32) + 1,
                total,
                object_name: format!("DROP {ident}"),
                phase: DumpPhase::Object,
            },
        );
        // PG: DROP TABLE on a view (or the reverse) errors even with IF EXISTS.
        // Try every kind so leftover catalog rows cannot block later CREATE.
        for sql in [
            format!("DROP VIEW IF EXISTS {ident} CASCADE"),
            format!("DROP MATERIALIZED VIEW IF EXISTS {ident} CASCADE"),
            format!("DROP TABLE IF EXISTS {ident} CASCADE"),
            format!("DROP VIEW IF EXISTS {ident}"),
            format!("DROP TABLE IF EXISTS {ident}"),
        ] {
            let _ = driver.execute(handle, &sql).await;
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

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("restore_database")?;
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("restore_database")?;
    if config.read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; restore / execute SQL file is not allowed".into(),
        ));
    }
    if state.store.get_settings().await.safe_mode {
        return Err(CommandError::Validation(
            "Safe mode is enabled; restore / execute SQL file is not allowed".into(),
        ));
    }

    let restore_opts = parse_restore_options(&options.unwrap_or_default());
    tracing::info!(
        %connection_id,
        overwrite = restore_opts.overwrite,
        "restore_database streaming"
    );
    if restore_opts.overwrite {
        let db_name = if let Some(name) = database.as_deref().filter(|s| !s.is_empty()) {
            name.to_string()
        } else {
            config.database.unwrap_or_default()
        };
        if !db_name.is_empty() {
            drop_existing_restore_targets(&driver, &handle, &db_name, app).await?;
        }
    }

    let mut throttle = ThrottledRestoreProgress::new(app);
    {
        let mut on_progress = |progress: DumpProgress| throttle.emit(progress);
        if driver.uses_sql_restore_pipeline() {
            stream_sql_file_into_session(
                &input_path,
                driver.as_ref(),
                &handle,
                &restore_opts,
                config.read_only,
                state.store.get_settings().await.safe_mode,
                &mut on_progress,
            )
            .await?;
        } else {
            driver
                .restore_sql_with_progress(&handle, "", Some(&restore_opts), &mut on_progress)
                .await
                .map_err(map_driver_err)?;
        }
    }
    throttle.flush();

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

const SQL_STREAM_CHUNK: usize = 64 * 1024;

/// Read a `.sql` / `.sql.gz` file in chunks and feed the driver's restore pipeline.
async fn stream_sql_file_into_session(
    path: &std::path::Path,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    opts: &BackupRestoreOptions,
    read_only: bool,
    safe_mode: bool,
    on_progress: &mut (dyn FnMut(DumpProgress) + Send),
) -> Result<(), CommandError> {
    let gz = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, String>>(8);
    let path = path.to_path_buf();
    let reader_task = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut reader: Box<dyn Read> = if gz {
            Box::new(flate2::read::GzDecoder::new(file))
        } else {
            Box::new(std::io::BufReader::with_capacity(SQL_STREAM_CHUNK, file))
        };
        let mut buf = vec![0u8; SQL_STREAM_CHUNK];
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if tx.blocking_send(Ok(buf[..n].to_vec())).is_err() {
                break;
            }
        }
        Ok::<(), String>(())
    });

    let mut session = RestoreSession::new(driver, handle, driver.new_sql_scanner(), Some(opts))
        .with_statement_guard(Box::new(move |stmt| {
            crate::sql_guard::check_sql(stmt, read_only, safe_mode)
                .map_err(DriverError::QueryFailed)
        }));
    let mut utf8 = Utf8ChunkDecoder::new();
    while let Some(item) = rx.recv().await {
        let bytes = item.map_err(CommandError::Validation)?;
        let text = utf8.push(&bytes).map_err(CommandError::Validation)?;
        if !text.is_empty() {
            session
                .feed(&text, on_progress)
                .await
                .map_err(map_driver_err)?;
        }
    }
    match reader_task.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return Err(CommandError::Io(std::io::Error::other(e)));
        }
        Err(e) => return Err(CommandError::Internal(e.to_string())),
    }
    let tail = utf8.finish().map_err(CommandError::Validation)?;
    if !tail.is_empty() {
        session
            .feed(&tail, on_progress)
            .await
            .map_err(map_driver_err)?;
    }
    session.finish(on_progress).await.map_err(map_driver_err)
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

    #[tokio::test]
    async fn restore_database_rejects_read_only_connection() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::with_tables().await;
        let mut cfg = test.save_connection("backup-ro").await;
        cfg.read_only = true;
        test.state.store.save_connection(cfg).await.unwrap();
        let conn_id = test.connect_config("backup-ro").await;
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

        let test = TestAppState::with_tables().await;
        let mut settings = test.state.store.get_settings().await;
        settings.safe_mode = true;
        test.state.store.save_settings(settings).await.unwrap();

        let (_, conn_id) = test.save_and_connect("backup-safe").await;
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
}
