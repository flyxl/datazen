//! Shared backup and SQL restore use cases. Paths and progress delivery are supplied by hosts.
use crate::error::FeatureError as CommandError;
use datazen_driver_api::{
    BackupDumpOptions, BackupRestoreOptions, ConnectionConfig, ConnectionHandle, DatabaseDriver,
    DriverError, DumpPhase, DumpProgress, RestoreSession, SqlTarget, TableInfo, TableType,
    Utf8ChunkDecoder,
};
use std::{
    io::Read,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
#[async_trait::async_trait]
pub trait BackupSource: Send + Sync {
    async fn resolve_source(
        &self,
        session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionConfig), CommandError>;
    async fn safe_mode(&self) -> bool;
}
pub trait BackupEvents: Send + Sync {
    fn backup_progress(&self, progress: DumpProgress);
    fn restore_progress(&self, progress: DumpProgress);
}
// Progress events are fire-and-forget by construction: `emit` fails only when
// the application is shutting down and there is no window left to receive the
// event, and the backup/restore result itself is returned through the command
// channel, not through these events. Reporting a "no window to report to"
// error to the user would replace a real backup outcome with a shutdown
// artifact. Not logged either — these fire per object/statement, so a log line
// would flood for the rest of a large dump. The backup/restore failures
// themselves are *not* swallowed; see `backup_database_to_path` below.
fn emit_backup_progress(app: Option<&dyn BackupEvents>, progress: DumpProgress) {
    if let Some(app) = app {
        app.backup_progress(progress);
    }
}

fn emit_restore_progress(app: Option<&dyn BackupEvents>, progress: DumpProgress) {
    if let Some(app) = app {
        app.restore_progress(progress);
    }
}

/// Coalesce per-statement restore events so the webview is not flooded
/// (one INSERT per row previously froze the UI halfway through a dump).
struct ThrottledRestoreProgress<'a> {
    app: Option<&'a dyn BackupEvents>,
    last: Instant,
    pending: Option<DumpProgress>,
    interval: Duration,
}

impl<'a> ThrottledRestoreProgress<'a> {
    fn new(app: Option<&'a dyn BackupEvents>) -> Self {
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

pub fn parse_backup_options(options: &[String]) -> Result<BackupDumpOptions, CommandError> {
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

pub fn parse_restore_options(options: &[String]) -> BackupRestoreOptions {
    let opts: std::collections::HashSet<String> = options.iter().cloned().collect();
    BackupRestoreOptions {
        single_transaction: opts.contains("single-transaction"),
        overwrite: opts.contains("overwrite"),
    }
}

pub fn validate_backup_filter_extension(filter_extension: &str) -> Result<String, CommandError> {
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
pub async fn backup_database_to_path(
    state: &dyn BackupSource,
    db_session_id: String,
    database: Option<String>,
    output_path: PathBuf,
    options: Option<Vec<String>>,
    compress: Option<bool>,
    app: Option<&dyn BackupEvents>,
) -> Result<(), CommandError> {
    tracing::info!(%db_session_id, path = %output_path.display(), "backup_database");
    // Reject unsupported options before opening a driver connection. The driver
    // owns a connection pool keyed by this handle; a parse error after connect
    // would otherwise bypass every explicit disconnect path below.
    let opts = parse_backup_options(options.as_deref().unwrap_or_default())?;
    let (driver, config) = state.resolve_source(&db_session_id).await?;

    let dump_handle = driver.connect(&config).await.map_err(|e| {
        let err: CommandError = e.into();
        tracing::error!(cmd = "backup_database", error = %err, "Failed to open independent backup connection");
        err
    })?;

    let db_name = database
        .as_deref()
        .unwrap_or(config.database.as_deref().unwrap_or(""))
        .to_string();

    let mut on_progress = |progress: DumpProgress| emit_backup_progress(app, progress);
    let out = match driver
        .dump_database_with_progress(&dump_handle, &db_name, &opts, &mut on_progress)
        .await
    {
        Ok(out) => out,
        Err(e) => {
            // The dump has already failed and `e` is returned below, so a
            // failure to close `dump_handle` here is a *second, subordinate*
            // teardown error on a connection the dump itself broke. It is
            // logged rather than propagated on purpose: returning it instead
            // of `e` would replace the real cause of the backup failure with
            // "could not close the backup connection", which is exactly the
            // masking the ordering below prevents. `dump_handle` is consumed
            // by the call, so the retry problem noted in
            // `ConnectionManager::disconnect` applies here too — this
            // connection is not referenced by the session tables at all, it is
            // a short-lived connection opened only for the dump.
            if let Err(teardown) = driver.disconnect(dump_handle).await {
                tracing::warn!(
                    cmd = "backup_database",
                    error = %teardown,
                    "Failed to close the independent backup connection after the dump failed",
                );
            }
            let err = CommandError::from(e);
            tracing::error!(cmd = "backup_database", error = %err);
            return Err(err);
        }
    };

    // The dump **succeeded** here. A teardown failure must not turn a backup
    // that was produced and is about to be written to disk into a reported
    // failure — the user's data is already in hand, and the only cost of
    // proceeding is one connection the driver says it could not close. Logged,
    // not swallowed silently, and deliberately not propagated.
    if let Err(e) = driver.disconnect(dump_handle).await {
        tracing::warn!(
            cmd = "backup_database",
            error = %e,
            "Backup finished but the independent backup connection could not be closed",
        );
    }

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
        tokio::fs::write(&output_path, data).await?;
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

pub async fn drop_existing_restore_targets(
    driver: &Arc<dyn DatabaseDriver>,
    handle: &ConnectionHandle,
    database: &str,
    app: Option<&dyn BackupEvents>,
) -> Result<(), CommandError> {
    let tables = driver.get_tables(handle, database, None).await?;
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

    let target = SqlTarget::new(Some(database), None);
    // `SET FOREIGN_KEY_CHECKS` is MySQL-family syntax. This function runs for
    // every dialect, so on PostgreSQL / SQLite / … the statement is *expected*
    // to fail. Swallowing it is therefore not "losing a user-visible failure":
    // the failure being discarded is the dialect saying "I do not have this
    // switch", and there is nothing for the user to act on. Dropping the
    // existing objects is guarded by the `CASCADE` variants below instead.
    let _ = driver
        .execute_at(handle, "SET FOREIGN_KEY_CHECKS=0", target)
        .await;
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
        // The five forms are an **alternatives set, not five required
        // statements**. `get_tables` (line 48) reported this object as a view,
        // a materialized view or a table, and a database rejects the `DROP`
        // written for the other two. At most one of the five can apply, so
        // propagating any single error would fail every restore on every
        // dialect — this is the tolerable swallow, not a masked defect.
        //
        // What is *not* tolerable is the all-five-failed case: the objects
        // came from the target database, so at least one form must succeed
        // unless the statement is being rejected for a reason the user does
        // need to see (no DROP privilege, broken connection, wrong target
        // schema). Previously that restore continued silently against a table
        // it never dropped. Counting the successes converts a blind tolerance
        // into one warning.
        let mut dropped = 0usize;
        for sql in [
            format!("DROP VIEW IF EXISTS {ident} CASCADE"),
            format!("DROP MATERIALIZED VIEW IF EXISTS {ident} CASCADE"),
            format!("DROP TABLE IF EXISTS {ident} CASCADE"),
            format!("DROP VIEW IF EXISTS {ident}"),
            format!("DROP TABLE IF EXISTS {ident}"),
        ] {
            if driver.execute_at(handle, &sql, target).await.is_ok() {
                dropped += 1;
            }
        }
        if dropped == 0 {
            tracing::warn!(
                cmd = "restore_database",
                object = %ident,
                database = %database,
                "No DROP variant applied to an object that get_tables reported; \
                 the restore is about to write into a target that was not cleared",
            );
        }
    }
    // Re-enabling is best effort for the same dialect reason as the disable
    // above, and the residual is harmless here: `restore_handle` is a
    // short-lived connection created for this restore and disconnected right
    // after, so a session-scoped setting that failed to switch back dies with
    // it. The restore result itself is returned separately below.
    let _ = driver
        .execute_at(handle, "SET FOREIGN_KEY_CHECKS=1", target)
        .await;
    Ok(())
}

pub async fn restore_database_from_path(
    state: &dyn BackupSource,
    app: Option<&dyn BackupEvents>,
    db_session_id: String,
    database: Option<String>,
    input_path: PathBuf,
    options: Option<Vec<String>>,
) -> Result<(), CommandError> {
    tracing::info!(%db_session_id, path = %input_path.display(), "restore_database");
    emit_restore_progress(
        app,
        DumpProgress {
            current: 0,
            total: 0,
            object_name: String::new(),
            phase: DumpPhase::Object,
        },
    );

    let (driver, config) = state.resolve_source(&db_session_id).await?;
    if config.read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; restore / execute SQL file is not allowed".into(),
        ));
    }
    if state.safe_mode().await {
        return Err(CommandError::Validation(
            "Safe mode is enabled; restore / execute SQL file is not allowed".into(),
        ));
    }

    let restore_handle = driver.connect(&config).await.map_err(|e| {
        let err: CommandError = e.into();
        tracing::error!(cmd = "restore_database", error = %err, "Failed to open independent restore connection");
        err
    })?;

    let restore_opts = parse_restore_options(&options.unwrap_or_default());
    tracing::info!(
        %db_session_id,
        overwrite = restore_opts.overwrite,
        "restore_database streaming"
    );

    let restore_result: Result<(), CommandError> = async {
        if restore_opts.overwrite {
            let db_name = if let Some(name) = database.as_deref().filter(|s| !s.is_empty()) {
                name.to_string()
            } else {
                config.database.unwrap_or_default()
            };
            if !db_name.is_empty() {
                drop_existing_restore_targets(&driver, &restore_handle, &db_name, app).await?;
            }
        }

        let mut throttle = ThrottledRestoreProgress::new(app);
        {
            let mut on_progress = |progress: DumpProgress| throttle.emit(progress);
            if driver.uses_sql_restore_pipeline() {
                stream_sql_file_into_session(
                    &input_path,
                    driver.as_ref(),
                    &restore_handle,
                    &restore_opts,
                    config.read_only,
                    state.safe_mode().await,
                    &mut on_progress,
                )
                .await?;
            } else {
                driver
                    .restore_sql_with_progress(
                        &restore_handle,
                        "",
                        Some(&restore_opts),
                        &mut on_progress,
                    )
                    .await?;
            }
        }
        throttle.flush();
        Ok(())
    }
    .await;

    // `restore_handle` is a short-lived connection opened only for this
    // restore (line 152), and it is closed **before** `restore_result` is
    // returned below. A teardown failure is therefore a second, subordinate
    // error on a connection the restore has already finished with; returning
    // it would let "could not close the restore connection" mask the actual
    // restore failure that `restore_result` is holding. Logged, never
    // propagated — and, as in `backup_database_to_path`, not retryable once
    // consumed, which is acceptable for a handle whose whole lifetime was
    // this one call.
    if let Err(e) = driver.disconnect(restore_handle).await {
        tracing::warn!(
            cmd = "restore_database",
            error = %e,
            "The independent restore connection could not be closed",
        );
    }

    restore_result?;

    emit_restore_progress(
        app,
        DumpProgress {
            current: 0,
            total: 0,
            object_name: String::new(),
            phase: DumpPhase::Done,
        },
    );
    tracing::info!(%db_session_id, "restore_database OK");
    Ok(())
}

const SQL_STREAM_CHUNK: usize = 64 * 1024;

/// Read a `.sql` / `.sql.gz` file in chunks and feed the driver's restore pipeline.
pub async fn stream_sql_file_into_session(
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
            datazen_runtime::sql_guard::check_sql(stmt, read_only, safe_mode)
                .map_err(DriverError::QueryFailed)
        }));
    let mut utf8 = Utf8ChunkDecoder::new();
    while let Some(item) = rx.recv().await {
        let bytes = item.map_err(CommandError::Validation)?;
        let text = utf8.push(&bytes).map_err(CommandError::Validation)?;
        if !text.is_empty() {
            session.feed(&text, on_progress).await?;
        }
    }
    reader_task
        .await
        .map_err(|e| CommandError::Internal(format!("sql file reader: {e}")))?
        .map_err(CommandError::Validation)?;
    let tail = utf8.finish().map_err(CommandError::Validation)?;
    if !tail.is_empty() {
        session.feed(&tail, on_progress).await?;
    }
    session.finish(on_progress).await.map_err(Into::into)
}
