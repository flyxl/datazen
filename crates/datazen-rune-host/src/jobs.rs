//! The host's export and backup jobs (N11): real work behind the UI's
//! job-and-progress vocabulary. Each job runs on its own thread with its own
//! runtime and connection (a dump or a full-table read is not the frame
//! loop's to wait for), publishing `ProgressSnapshot`s into a shared table
//! the adapter reads.

use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen_driver_api::{ConnectionConfig, DatabaseDriver, Value};
use datazen_ui_api::ProgressSnapshot;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// One running or finished host job.
struct JobState {
    snapshot: ProgressSnapshot,
    /// Set by `stop`; the worker checks it between its steps.
    stop_requested: bool,
}

pub struct HostJobs {
    next: AtomicU64,
    states: Arc<Mutex<HashMap<u64, JobState>>>,
}

impl HostJobs {
    pub fn new() -> Self {
        Self { next: AtomicU64::new(1), states: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn begin(&self, first_log: String) -> u64 {
        let job = self.next.fetch_add(1, Ordering::SeqCst);
        self.states.lock().expect("the job lock").insert(
            job,
            JobState {
                snapshot: ProgressSnapshot {
                    job,
                    status: "running".into(),
                    percent: 0,
                    log: vec![first_log],
                },
                stop_requested: false,
            },
        );
        job
    }

    /// The job's current snapshot, if it exists.
    pub fn progress(&self, job: u64) -> Option<ProgressSnapshot> {
        self.states
            .lock()
            .expect("the job lock")
            .get(&job)
            .map(|state| state.snapshot.clone())
    }

    /// Ask a job to stop. A job that already finished answers what it did;
    /// one between steps stops before its next write.
    pub fn stop(&self, job: u64) -> Result<(), String> {
        let mut states = self.states.lock().expect("the job lock");
        match states.get_mut(&job) {
            None => Err(format!("no job {job}")),
            Some(state) => {
                if state.snapshot.status == "running" {
                    state.stop_requested = true;
                }
                Ok(())
            }
        }
    }

    fn update(&self, job: u64, percent: u32, log: String) -> Option<bool> {
        let mut states = self.states.lock().expect("the job lock");
        let state = states.get_mut(&job)?;
        Some(if state.stop_requested {
            state.snapshot.status = "stopped".into();
            state.snapshot.log.push("stopped".into());
            false
        } else {
            state.snapshot.percent = percent;
            state.snapshot.log.push(log);
            true
        })
    }

    fn finish(&self, job: u64, log: String) {
        let mut states = self.states.lock().expect("the job lock");
        if let Some(state) = states.get_mut(&job) {
            if state.snapshot.status == "stopped" {
                return;
            }
            state.snapshot.status = "completed".into();
            state.snapshot.percent = 100;
            state.snapshot.log.push(log);
        }
    }

    fn fail(&self, job: u64, error: String) {
        let mut states = self.states.lock().expect("the job lock");
        if let Some(state) = states.get_mut(&job) {
            if state.snapshot.status == "stopped" {
                return;
            }
            state.snapshot.status = "failed".into();
            state.snapshot.log.push(error);
        }
    }

    /// Start an export of one table to a file under the data directory.
    /// The range is the dialog's free text; the honest native semantics are
    /// "the whole table" — anything else is refused rather than guessed.
    pub fn spawn_export(
        &self,
        registry: Arc<DriverRegistry>,
        config: ConnectionConfig,
        table: String,
        format: String,
        range: String,
        dir: PathBuf,
    ) -> u64 {
        let job = self.begin(format!("export {table} as {format}"));
        let states = Arc::clone(&self.states);
        let _ = &self.next; // the counter lives on; the thread owns the rest
        std::thread::Builder::new()
            .name("datazen-export".into())
            .spawn(move || {
                let states_after = Arc::clone(&states);
                let run = move || -> Result<String, String> {
                    let states = states_after;
                    if !range.trim().is_empty() && range.trim() != "all" {
                        return Err(format!(
                            "the native host exports the whole table; '{range}' is not a range it can serve"
                        ));
                    }
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| format!("the export runtime did not start: {error}"))?;
                    runtime.block_on(async move {
                        let driver = registry
                            .get(&config.database_type)
                            .await
                            .ok_or_else(|| format!("no driver for '{}'", config.database_type))?;
                        update_states(&states, job, 10, "connected".into());
                        let handle = driver
                            .connect(&config)
                            .await
                            .map_err(|error| error.to_string())?;
                        let sql = format!(
                            "SELECT * FROM {}",
                            driver.quote_ident(&table)
                        );
                        let result = driver.query(&handle, &sql).await.map_err(|e| e.to_string())?;
                        update_states(&states, job, 50, format!("{} rows read", result.rows.len()));

                        let extension = match format.as_str() {
                            "csv" => "csv",
                            "json" => "json",
                            other => {
                                return Err(format!(
                                    "the native host exports csv and json; '{other}' is not wired"
                                ))
                            }
                        };
                        let path = dir.join(format!(
                            "export-{table}-{}.{extension}",
                            chrono::Utc::now().format("%Y%m%d-%H%M%S")
                        ));
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        let body = match extension {
                            "csv" => rows_to_csv(&result.columns, &result.rows),
                            _ => rows_to_json(&result.columns, &result.rows),
                        };
                        std::fs::write(&path, body).map_err(|e| e.to_string())?;
                        Ok(format!("wrote {}", path.display()))
                    })
                };
                match run() {
                    Ok(log) => finish_states(&states, job, log),
                    Err(error) => fail_states(&states, job, error),
                }
            })
            .expect("the export thread starts");
        job
    }

    /// Start a backup of the live connection's database to a file under the
    /// data directory. The driver's own dump renders the SQL.
    pub fn spawn_backup(
        &self,
        registry: Arc<DriverRegistry>,
        config: ConnectionConfig,
        dir: PathBuf,
    ) -> u64 {
        let database = config.database.clone().unwrap_or_default();
        let job = self.begin(format!("backup {database}"));
        let states = Arc::clone(&self.states);
        let states_after = Arc::clone(&states);
        std::thread::Builder::new()
            .name("datazen-backup".into())
            .spawn(move || {
                let run = move || -> Result<String, String> {
                    let states = states_after;
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| format!("the backup runtime did not start: {error}"))?;
                    runtime.block_on(async move {
                        let driver = registry
                            .get(&config.database_type)
                            .await
                            .ok_or_else(|| format!("no driver for '{}'", config.database_type))?;
                        let handle = driver
                            .connect(&config)
                            .await
                            .map_err(|error| error.to_string())?;
                        update_states(&states, job, 30, "connected".into());
                        let options = datazen_driver_api::BackupDumpOptions::default();
                        let dump = driver
                            .dump_database(&handle, &database, &options)
                            .await
                            .map_err(|error| error.to_string())?;
                        update_states(&states, job, 70, "dumped".into());
                        // The database name can be a file path (sqlite): only
                        // its safe characters may name the backup file.
                        let safe = if database.is_empty() {
                            "main".to_string()
                        } else {
                            database
                                .chars()
                                .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
                                .collect::<String>()
                        };
                        let path = dir.join(format!(
                            "backup-{}-{}.sql",
                            safe,
                            chrono::Utc::now().format("%Y%m%d-%H%M%S")
                        ));
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        std::fs::write(&path, dump).map_err(|e| e.to_string())?;
                        Ok(format!("wrote {}", path.display()))
                    })
                };
                match run() {
                    Ok(log) => finish_states(&states, job, log),
                    Err(error) => fail_states(&states, job, error),
                }
            })
            .expect("the backup thread starts");
        job
    }
}

// The thread closures cannot borrow `self` (the adapter owns it); they take
// the states map directly. These helpers are the same operations as the
// methods above, on a bare map.
fn update_states(states: &Arc<Mutex<HashMap<u64, JobState>>>, job: u64, percent: u32, log: String) -> bool {
    let mut states = states.lock().expect("the job lock");
    match states.get_mut(&job) {
        None => false,
        Some(state) => {
            if state.stop_requested {
                state.snapshot.status = "stopped".into();
                state.snapshot.log.push("stopped".into());
                false
            } else {
                state.snapshot.percent = percent;
                state.snapshot.log.push(log);
                true
            }
        }
    }
}

fn finish_states(states: &Arc<Mutex<HashMap<u64, JobState>>>, job: u64, log: String) {
    let mut states = states.lock().expect("the job lock");
    if let Some(state) = states.get_mut(&job) {
        if state.snapshot.status == "stopped" {
            return;
        }
        state.snapshot.status = "completed".into();
        state.snapshot.percent = 100;
        state.snapshot.log.push(log);
    }
}

fn fail_states(states: &Arc<Mutex<HashMap<u64, JobState>>>, job: u64, error: String) {
    let mut states = states.lock().expect("the job lock");
    if let Some(state) = states.get_mut(&job) {
        if state.snapshot.status == "stopped" {
            return;
        }
        state.snapshot.status = "failed".into();
        state.snapshot.log.push(error);
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

fn rows_to_csv(
    columns: &[datazen_driver_api::ColumnInfo],
    rows: &[Vec<Option<Value>>],
) -> String {
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

fn rows_to_json(
    columns: &[datazen_driver_api::ColumnInfo],
    rows: &[Vec<Option<Value>>],
) -> String {
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
