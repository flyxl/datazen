//! The host's export and backup jobs (N11): real work behind the UI's
//! job-and-progress vocabulary. Each job runs on its own thread with its own
//! runtime and connection (a dump or a full-table read is not the frame
//! loop's to wait for), publishing `ProgressSnapshot`s into a shared table
//! the adapter reads.

use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver, Value};
use datazen_ui_api::ProgressSnapshot;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tempfile::NamedTempFile;

struct FileJob(Mutex<ProgressSnapshot>);

struct JobOutput {
    path: PathBuf,
    body: String,
    sqlite_backup_source: Option<String>,
}

struct StagedOutput {
    destination: PathBuf,
    file: NamedTempFile,
    create_only: bool,
}

impl FileJob {
    fn snapshot(&self) -> MutexGuard<'_, ProgressSnapshot> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn progress(&self, percent: u32, message: String) -> Result<(), String> {
        let mut snapshot = self.snapshot();
        if snapshot.status != "running" {
            return Err("stopped".into());
        }
        snapshot.percent = percent;
        snapshot.log.push(message);
        Ok(())
    }

    fn fail(&self, error: String) {
        let mut snapshot = self.snapshot();
        if snapshot.status == "running" {
            snapshot.status = "failed".into();
            snapshot.log.push(error);
        }
    }

    fn stop(&self) {
        let mut snapshot = self.snapshot();
        if snapshot.status == "running" {
            snapshot.status = "stopped".into();
            snapshot.log.push("stopped".into());
        }
    }

    fn write(&self, output: JobOutput) -> Result<(), String> {
        if self.snapshot().status != "running" {
            return Ok(());
        }
        let source = output.sqlite_backup_source.as_deref();
        let staged = stage_output(&output.path, &output.body, source)
            .map_err(|e| e.to_string())?;
        self.publish(staged, source)
    }

    fn publish(&self, staged: StagedOutput, sqlite_backup_source: Option<&str>) -> Result<(), String> {
        // Only publication and the terminal state share the lock with Stop.
        // A stopped job drops its temporary file and leaves the destination intact.
        let mut snapshot = self.snapshot();
        if snapshot.status == "running" {
            let StagedOutput { destination, file, create_only } = staged;
            if create_only {
                file.persist_noclobber(&destination).map_err(|e| e.error.to_string())?;
            } else {
                // Existing-output backup keeps the normal overwrite semantics
                // for a distinct file. Recheck the actual resolved destination
                // before publication, after the potentially slow staging.
                verify_sqlite_backup_destination(&destination, sqlite_backup_source)
                    .map_err(|e| e.to_string())?;
                file.persist(&destination).map_err(|e| e.error.to_string())?;
            }
            snapshot.status = "completed".into();
            snapshot.percent = 100;
            let name = destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            snapshot.log.push(format!("wrote {name}"));
        }
        Ok(())
    }
}

fn verify_sqlite_backup_destination(path: &Path, source: Option<&str>) -> io::Result<()> {
    if let Some(source) = source {
        // A replacement may be a non-regular file; never try to open a pipe
        // or device merely to compare its identity.
        if !std::fs::metadata(path)?.is_file() {
            return Err(io::Error::other("Choose a writable file for this job."));
        }
        let destination = datazen_driver_sqlite::sqlite_existing_file_identity(path)
            .map_err(|_| io::Error::other("Could not verify the existing SQLite backup output file."))?;
        if destination == source {
            return Err(io::Error::other("Choose a backup file different from the SQLite source database."));
        }
    }
    Ok(())
}

fn stage_output(path: &Path, body: &str, sqlite_backup_source: Option<&str>) -> io::Result<StagedOutput> {
    // A save to an existing link still targets its file. Check write access
    // before staging: rename alone only requires access to the directory.
    let mut destination = path.to_owned();
    let permissions = match std::fs::symlink_metadata(path) {
        Ok(_) => {
            destination = path.canonicalize()?;
            let metadata = std::fs::metadata(&destination)?;
            if !metadata.is_file() || metadata.permissions().readonly() {
                return Err(io::Error::other("Choose a writable file for this job."));
            }
            verify_sqlite_backup_destination(&destination, sqlite_backup_source)?;
            std::fs::OpenOptions::new().write(true).open(&destination)?;
            Some(metadata.permissions())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file = stage_body(parent, body)?;
    let create_only = sqlite_backup_source.is_some() && permissions.is_none();
    if let Some(permissions) = permissions {
        file.as_file().set_permissions(permissions)?;
    }

    Ok(StagedOutput { destination, file, create_only })
}

fn stage_body(parent: &Path, body: &str) -> io::Result<NamedTempFile> {
    let mut staged = NamedTempFile::new_in(parent)?;
    staged.write_all(body.as_bytes())?;
    Ok(staged)
}

/// Results require a user-picked destination, and cannot replace an existing
/// file. Atomic no-clobber applies even if the path changes after selection.
pub(super) fn save_new_file(path: &Path, body: &str) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::other("missing save directory"))?;
    stage_body(parent, body)?.persist_noclobber(path).map_err(|error| error.error)?;
    Ok(())
}

pub struct HostJobs {
    next: AtomicU64,
    states: Mutex<HashMap<u64, Arc<FileJob>>>,
}

impl HostJobs {
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
            states: Mutex::default(),
        }
    }

    fn job(&self, id: u64) -> Option<Arc<FileJob>> {
        self.states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&id)
            .cloned()
    }

    pub fn progress(&self, id: u64) -> Option<ProgressSnapshot> {
        Some(self.job(id)?.snapshot().clone())
    }

    pub fn stop(&self, id: u64) -> Result<(), String> {
        self.job(id).ok_or_else(|| format!("no job {id}"))?.stop();
        Ok(())
    }

    fn spawn(
        &self,
        registry: Arc<DriverRegistry>,
        config: ConnectionConfig,
        name: &'static str,
        first_log: String,
        read: impl AsyncFnOnce(
                &dyn DatabaseDriver,
                &ConnectionHandle,
                &FileJob,
            ) -> Result<JobOutput, String>
            + Send
            + 'static,
    ) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let job = Arc::new(FileJob(Mutex::new(ProgressSnapshot {
            job: id,
            status: "running".into(),
            percent: 0,
            log: vec![first_log],
        })));
        self.states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, Arc::clone(&job));
        let worker = Arc::clone(&job);
        let thread = std::thread::Builder::new()
            .name(format!("datazen-{name}"))
            .spawn(move || {
                let run = || -> Result<(), String> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| format!("the {name} runtime did not start: {error}"))?;
                    let output = runtime.block_on(async {
                        let driver = registry
                            .get(&config.database_type)
                            .await
                            .ok_or_else(|| format!("no driver for '{}'", config.database_type))?;
                        let handle = driver.connect(&config).await.map_err(|e| e.to_string())?;
                        let result = read(driver.as_ref(), &handle, &worker).await;
                        let closed = driver.disconnect(handle).await;
                        let output = result?;
                        closed.map_err(|e| e.to_string())?;
                        Ok::<_, String>(output)
                    })?;
                    worker.write(output)
                };
                if let Err(error) = run() {
                    worker.fail(error);
                }
            });
        if let Err(error) = thread {
            job.fail(format!("the {name} thread did not start: {error}"));
        }
        id
    }

    pub fn spawn_export(
        &self,
        registry: Arc<DriverRegistry>,
        config: ConnectionConfig,
        table: String,
        format: String,
        range: String,
        dir: PathBuf,
        picked: Option<PathBuf>,
    ) -> u64 {
        let first_log = format!("export {table} as {format}");
        self.spawn(
            registry,
            config,
            "export",
            first_log,
            async move |driver, handle, job| {
                job.progress(10, "connected".into())?;
                if !range.trim().is_empty() && range.trim() != "all" {
                    return Err(format!(
                        "the native host exports the whole table; '{range}' is not a range it can serve"
                    ));
                }
                if !matches!(format.as_str(), "csv" | "json") {
                    return Err(format!("the native host exports csv and json; '{format}' is not wired"));
                }
                let sql = format!("SELECT * FROM {}", driver.quote_ident(&table));
                let result = driver.query(handle, &sql).await.map_err(|e| e.to_string())?;
                job.progress(50, format!("{} rows read", result.rows.len()))?;
                let body = if format == "csv" {
                    rows_to_csv(&result.columns, &result.rows)
                } else {
                    rows_to_json(&result.columns, &result.rows)
                };
                let path = picked.unwrap_or_else(|| {
                    let table = table.replace(['/', '\\'], "-");
                    dir.join(format!("export-{table}-{}.{format}", chrono::Utc::now().format("%Y%m%d-%H%M%S")))
                });
                Ok(JobOutput { path, body, sqlite_backup_source: None })
            },
        )
    }

    pub fn spawn_backup(
        &self,
        registry: Arc<DriverRegistry>,
        config: ConnectionConfig,
        dir: PathBuf,
        picked: Option<PathBuf>,
    ) -> u64 {
        let database = config.database.clone().unwrap_or_default();
        let first_log = format!("backup {}", config.name);
        self.spawn(
            registry,
            config,
            "backup",
            first_log,
            async move |driver, handle, job| {
                job.progress(30, "connected".into())?;
                let sqlite_backup_source = if driver.driver_type().as_str() == "sqlite" {
                    Some(driver.physical_database_identity(handle, "main").await
                        .ok().flatten()
                        .ok_or_else(|| "Could not verify the SQLite backup source file.".to_owned())?)
                } else {
                    None
                };
                let dump = driver
                    .dump_database(handle, &database, &Default::default())
                    .await
                    .map_err(|e| e.to_string())?;
                job.progress(70, "dumped".into())?;
                let path = picked.unwrap_or_else(|| {
                    let safe: String = if database.is_empty() {
                        "main"
                    } else {
                        &database
                    }
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                    dir.join(format!(
                        "backup-{safe}-{}.sql",
                        chrono::Utc::now().format("%Y%m%d-%H%M%S")
                    ))
                });
                Ok(JobOutput { path, body: dump, sqlite_backup_source })
            },
        )
    }
}

/// One cell as CSV text, quoted when it must be.
fn csv_cell(cell: &Option<Value>) -> String {
    let text = match cell {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Integer(value)) => value.to_string(),
        Some(Value::Float(value)) => value.to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        Some(Value::Timestamp(value)) => value.clone(),
        Some(Value::Json(value)) => value.to_string(),
    };
    if text.contains(',') || text.contains('"') || text.contains('\n') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

fn rows_to_csv(columns: &[datazen_driver_api::ColumnInfo], rows: &[Vec<Option<Value>>]) -> String {
    let mut out = String::new();
    out.push_str(
        &columns
            .iter()
            .map(|column| csv_cell(&Some(Value::String(column.name.clone()))))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        out.push_str(&row.iter().map(csv_cell).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

fn rows_to_json(columns: &[datazen_driver_api::ColumnInfo], rows: &[Vec<Option<Value>>]) -> String {
    let entries: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::Map::from_iter(
                columns
                    .iter()
                    .zip(row.iter())
                    .map(|(column, cell)| (column.name.clone(), cell_json(cell))),
            )
            .into()
        })
        .collect();
    serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".into())
}

fn cell_json(cell: &Option<Value>) -> serde_json::Value {
    match cell {
        None | Some(Value::Null) => serde_json::Value::Null,
        Some(Value::Bool(value)) => (*value).into(),
        Some(Value::Integer(value)) => (*value).into(),
        Some(Value::Float(value)) => (*value).into(),
        Some(Value::String(value)) => value.clone().into(),
        Some(Value::Bytes(bytes)) => {
            serde_json::Value::String(String::from_utf8_lossy(bytes).into_owned())
        }
        Some(Value::Timestamp(value)) => value.clone().into(),
        Some(Value::Json(value)) => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> FileJob {
        FileJob(Mutex::new(ProgressSnapshot {
            job: 1,
            status: "running".into(),
            percent: 70,
            log: vec!["read".into()],
        }))
    }

    fn ordinary_output(path: &Path, body: &str) -> JobOutput {
        JobOutput { path: path.to_owned(), body: body.to_owned(), sqlite_backup_source: None }
    }

    #[cfg(unix)]
    #[test]
    fn overwriting_a_picked_link_respects_its_destination_and_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("picked");
        std::fs::write(&target, "original").unwrap();
        symlink(&target, &link).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert!(running().write(ordinary_output(&link, "rejected")).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");

        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        running().write(ordinary_output(&link, "saved")).unwrap();
        assert!(link.is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "saved");
        assert_eq!(
            target.metadata().unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn new_sqlite_backup_never_overwrites_an_entry_that_appears_before_publish() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("live.db");
        let chosen = dir.path().join("new.sql");
        std::fs::write(&source, "source file identity fixture").unwrap();
        let identity = datazen_driver_sqlite::sqlite_existing_file_identity(&source).unwrap();
        let staged = stage_output(&chosen, "dump", Some(&identity)).unwrap();
        std::fs::write(&chosen, "an intervening file").unwrap();

        assert!(running().publish(staged, Some(&identity)).is_err());
        assert_eq!(std::fs::read_to_string(&chosen).unwrap(), "an intervening file");
    }

    #[test]
    fn existing_sqlite_backup_rechecks_aliases_before_publish() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("live.db");
        let chosen = dir.path().join("old.sql");
        std::fs::write(&source, "source file identity fixture").unwrap();
        std::fs::write(&chosen, "a previous dump").unwrap();
        let identity = datazen_driver_sqlite::sqlite_existing_file_identity(&source).unwrap();
        let staged = stage_output(&chosen, "dump", Some(&identity)).unwrap();
        std::fs::remove_file(&chosen).unwrap();
        std::fs::hard_link(&source, &chosen).unwrap();

        let error = running().publish(staged, Some(&identity)).unwrap_err();
        assert!(error.contains("different from the SQLite source"));
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "source file identity fixture");
        assert_eq!(std::fs::read_to_string(&chosen).unwrap(), "source file identity fixture");
    }

    #[test]
    fn stopping_before_or_after_publication_preserves_the_winning_result() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("chosen.csv");
        std::fs::write(&file, "original").unwrap();

        let stopped = running();
        let staged = stage_output(&file, "discarded", None).unwrap();
        stopped.stop();
        stopped.publish(staged, None).unwrap();
        stopped.fail("late failure".into());
        assert_eq!(stopped.snapshot().status, "stopped");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        let completed = running();
        completed.write(ordinary_output(&file, "published")).unwrap();
        completed.stop();
        assert_eq!(completed.snapshot().status, "completed");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "published");
    }
}
