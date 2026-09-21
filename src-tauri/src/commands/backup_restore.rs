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
    restore_database_from_path(
        &state,
        Some(&app),
        db_session_id,
        database,
        input_path,
        options,
    )
    .await?;
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
        .get_tables(handle, database, None)
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

    let target = SqlTarget::new(Some(database), None);
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
        for sql in [
            format!("DROP VIEW IF EXISTS {ident} CASCADE"),
            format!("DROP MATERIALIZED VIEW IF EXISTS {ident} CASCADE"),
            format!("DROP TABLE IF EXISTS {ident} CASCADE"),
            format!("DROP VIEW IF EXISTS {ident}"),
            format!("DROP TABLE IF EXISTS {ident}"),
        ] {
            let _ = driver.execute_at(handle, &sql, target).await;
        }
    }
    let _ = driver
        .execute_at(handle, "SET FOREIGN_KEY_CHECKS=1", target)
        .await;
    Ok(())
}

async fn restore_database_from_path(
    state: &AppState,
    app: Option<&tauri::AppHandle>,
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

    let (driver, _workspace_handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("restore_database")?;
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
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
                    state.store.get_settings().await.safe_mode,
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

    let _ = driver.disconnect(restore_handle).await;

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
