//! Native dialogs and local runtime bindings for shared backup/restore services.
use super::AppState;
use super::error::{CommandError, OVERRIDE_DISABLED_MSG, resolve_override_path};
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
pub use datazen_feature_core::backup::parse_restore_options;
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, State};
pub(crate) fn parse_backup_options(
    options: &[String],
) -> Result<datazen_driver_api::BackupDumpOptions, CommandError> {
    datazen_feature_core::backup::parse_backup_options(options).map_err(Into::into)
}
pub(crate) fn validate_backup_filter_extension(extension: &str) -> Result<String, CommandError> {
    datazen_feature_core::backup::validate_backup_filter_extension(extension).map_err(Into::into)
}
#[derive(Clone)]
struct DesktopBackupSource {
    platform: crate::platform::PlatformEntry,
    registry: Arc<crate::db::DriverRegistry>,
    store: Arc<crate::store::Store>,
}
impl DesktopBackupSource {
    fn new(state: &AppState) -> Self {
        Self {
            platform: state.platform.clone(),
            registry: state.driver_registry.clone(),
            store: state.store.clone(),
        }
    }
}
#[async_trait::async_trait]
impl datazen_feature_core::backup::BackupSource for DesktopBackupSource {
    async fn resolve_source(
        &self,
        id: &str,
    ) -> Result<
        (Arc<dyn DatabaseDriver>, ConnectionConfig),
        datazen_feature_core::error::FeatureError,
    > {
        let adapter = self.platform.require().map_err(|error| {
            datazen_feature_core::error::FeatureError::Internal(error.to_string())
        })?;
        let runtime = adapter.runtime().map_err(|error| {
            datazen_feature_core::error::FeatureError::Internal(error.to_string())
        })?;
        let owner = runtime
            .connection_id_for_session(&datazen_runtime::connection::DbSessionId::new(id))
            .ok_or_else(|| {
                datazen_feature_core::error::FeatureError::Validation(
                    "Backup session is unavailable".into(),
                )
            })?;
        let (config, _) = self
            .store
            .platform_profile(owner.as_str())
            .await
            .filter(|(_, metadata)| metadata.enabled)
            .ok_or_else(|| {
                datazen_feature_core::error::FeatureError::Validation(
                    "Backup profile unavailable".into(),
                )
            })?;
        let driver = self
            .registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| {
                datazen_feature_core::error::FeatureError::Validation(
                    "Backup driver unavailable".into(),
                )
            })?;
        Ok((driver, config))
    }
    async fn safe_mode(&self) -> bool {
        self.store.execution_settings().await.safe_mode
    }
}
struct DesktopBackupEvents(tauri::AppHandle);
impl datazen_feature_core::backup::BackupEvents for DesktopBackupEvents {
    fn backup_progress(&self, progress: datazen_driver_api::DumpProgress) {
        let _ = self.0.emit("backup-progress", progress);
    }
    fn restore_progress(&self, progress: datazen_driver_api::DumpProgress) {
        let _ = self.0.emit("restore-progress", progress);
    }
}
async fn backup_database_to_path(
    state: &AppState,
    session_id: String,
    database: Option<String>,
    output_path: PathBuf,
    options: Option<Vec<String>>,
    compress: Option<bool>,
    app: Option<&tauri::AppHandle>,
) -> Result<(), CommandError> {
    let events = app.cloned().map(DesktopBackupEvents);
    datazen_feature_core::backup::backup_database_to_path(
        &DesktopBackupSource::new(state),
        session_id,
        database,
        output_path,
        options,
        compress,
        events
            .as_ref()
            .map(|events| events as &dyn datazen_feature_core::backup::BackupEvents),
    )
    .await
    .map_err(Into::into)
}
#[cfg(test)]
async fn restore_database_from_path(
    state: &AppState,
    app: Option<&tauri::AppHandle>,
    session_id: String,
    database: Option<String>,
    input_path: PathBuf,
    options: Option<Vec<String>>,
) -> Result<(), CommandError> {
    let events = app.cloned().map(DesktopBackupEvents);
    datazen_feature_core::backup::restore_database_from_path(
        &DesktopBackupSource::new(state),
        events
            .as_ref()
            .map(|events| events as &dyn datazen_feature_core::backup::BackupEvents),
        session_id,
        database,
        input_path,
        options,
    )
    .await
    .map_err(Into::into)
}

struct BackupFileOperation {
    source: DesktopBackupSource,
    app: tauri::AppHandle,
    session_id: String,
    database: Option<String>,
    options: Option<Vec<String>>,
    compress: bool,
    staging: tempfile::TempDir,
    extension: String,
}

#[async_trait::async_trait]
impl datazen_feature_core::file_job::FileJobOperation for BackupFileOperation {
    async fn run(
        &self,
        cancel: &datazen_runtime::job::CancelToken,
    ) -> Result<
        datazen_feature_core::file_job::FileJobOutput,
        datazen_feature_core::error::FeatureError,
    > {
        if cancel.is_cancelled() {
            return Err(datazen_feature_core::error::FeatureError::Validation(
                "fileJobCancelled".into(),
            ));
        }
        let output = self
            .staging
            .path()
            .join(format!("backup.{}", self.extension));
        let events = DesktopBackupEvents(self.app.clone());
        datazen_feature_core::backup::backup_database_to_path(
            &self.source,
            self.session_id.clone(),
            self.database.clone(),
            output.clone(),
            self.options.clone(),
            Some(self.compress),
            Some(&events),
        )
        .await?;
        if cancel.is_cancelled() {
            let _ = tokio::fs::remove_file(output).await;
            return Err(datazen_feature_core::error::FeatureError::Validation(
                "fileJobCancelled".into(),
            ));
        }
        Ok(datazen_feature_core::file_job::FileJobOutput {
            path: output,
            rows: 0,
        })
    }
}
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
    let extension = output_path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CommandError::Validation("backup file extension unavailable".into()))?
        .to_owned();
    let extension = validate_backup_filter_extension(&extension)?;
    let operation = BackupFileOperation {
        source: DesktopBackupSource::new(&state),
        app: app.clone(),
        session_id: db_session_id.clone(),
        database: database.clone(),
        options,
        compress: compress.unwrap_or(false),
        staging: tempfile::tempdir()?,
        extension,
    };
    super::file_jobs::run_output_job(
        &state,
        &app,
        &db_session_id,
        database.as_deref(),
        "databaseBackup",
        Arc::new(operation),
        output_path,
    )
    .await?;
    Ok(true)
}

/// Native open dialog + streaming `.sql` execution against one database.
/// Returns `true` if executed, `false` when the dialog is dismissed.
#[tauri::command]
pub async fn restore_sql_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    db_session_id: String,
    database: Option<String>,
    options: Option<Vec<String>>,
    override_path: Option<String>,
) -> Result<bool, CommandError> {
    let input_path = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Some(path),
        None => pick_sql_open_path(&app).await?,
    };
    let Some(input_path) = input_path else {
        return Ok(false);
    };
    super::restore_job::restore_from_native_file(
        &state,
        &app,
        &db_session_id,
        database,
        input_path,
        options,
    )
    .await?;
    Ok(true)
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
