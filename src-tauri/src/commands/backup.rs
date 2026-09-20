use super::error::{resolve_override_path, CmdExt, CommandError, OVERRIDE_DISABLED_MSG};
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

/// Save-dialog branch of [`backup_database`]; `None` = dialog cancelled.
async fn pick_backup_save_path(
    app: &tauri::AppHandle,
    default_file_name: &str,
    filter_extension: &str,
) -> Result<Option<PathBuf>, CommandError> {
    let ext = validate_backup_filter_extension(filter_extension)?;
    super::dialog::save_file(
        app,
        ("Backup".into(), vec![ext]),
        default_file_name.to_string(),
    )
    .await
}

/// Open-dialog branch of [`restore_sql_file`]; `None` = dialog cancelled.
async fn pick_sql_open_path(app: &tauri::AppHandle) -> Result<Option<PathBuf>, CommandError> {
    super::dialog::open_file(app, vec![("SQL".into(), vec!["sql".into()])]).await
}

/// Native save dialog + database backup. Returns `true` if written.
#[tauri::command]
pub async fn backup_database(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    db_session_id: String,
    database: Option<String>,
    default_file_name: String,
    filter_extension: String,
    options: Option<Vec<String>>,
    compress: Option<bool>,
    override_path: Option<String>,
) -> Result<bool, CommandError> {
    let output_path = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Some(path),
        None => pick_backup_save_path(&app, &default_file_name, &filter_extension).await?,
    };
    let Some(output_path) = output_path else {
        return Ok(false);
    };
    backup_database_to_path(
        &state,
        db_session_id,
        database,
        output_path,
        options,
        compress,
        Some(&app),
    )
    .await?;
    Ok(true)
}

async fn backup_database_to_path(
    state: &AppState,
    db_session_id: String,
    database: Option<String>,
    output_path: PathBuf,
    options: Option<Vec<String>>,
    compress: Option<bool>,
    app: Option<&tauri::AppHandle>,
) -> Result<(), CommandError> {
    tracing::info!(%db_session_id, path = %output_path.display(), "backup_database");
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("backup_database")?;

    let (driver, _workspace_handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("backup_database")?;

    let dump_handle = driver.connect(&config).await.map_err(|e| {
        let err: CommandError = e.into();
        tracing::error!(cmd = "backup_database", error = %err, "Failed to open independent backup connection");
        err
    })?;

    let db_name = database
        .as_deref()
        .unwrap_or(config.database.as_deref().unwrap_or(""))
        .to_string();
    let opts = parse_backup_options(&options.unwrap_or_default())?;

    let mut on_progress = |progress: DumpProgress| emit_backup_progress(app, progress);
    let out = match driver
        .dump_database_with_progress(&dump_handle, &db_name, &opts, &mut on_progress)
        .await
    {
        Ok(out) => out,
        Err(e) => {
            let _ = driver.disconnect(dump_handle).await;
            let err = CommandError::from(e);
            tracing::error!(cmd = "backup_database", error = %err);
            return Err(err);
        }
    };

    let _ = driver.disconnect(dump_handle).await;

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

include!("backup_restore.rs");

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
