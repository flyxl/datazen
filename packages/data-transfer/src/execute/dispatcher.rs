use super::*;

pub async fn execute_transfer_data_with_resume_checkpoint(
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    formatter: &ValueFormatter<'_>,
    drop_create: Option<&DropCreateContext<'_>>,
    target_read_only: bool,
    cancelled: Option<Arc<AtomicBool>>,
    completed_tables: Option<&HashSet<String>>,
    write_started: Option<&AtomicBool>,
    mut resume_checkpoint: Option<&mut dyn super::super::resume::TransferResumeCheckpoint>,
) -> Result<TransferExecutionResult, TransferError> {
    let target = job.database_target()?;
    if target_read_only {
        return Err(TransferError::validation(
            "target connection is read-only; Data Transfer cannot execute",
        ));
    }

    if !matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    ) {
        return Ok(TransferExecutionResult {
            tables: Vec::new(),
            rows_inserted: 0,
            cancelled: false,
            partial: false,
            resume_token: None,
        });
    }

    if job.write_mode.is_destructive() && !job.options.confirmed_destructive {
        return Err(TransferError::validation(
            "destructive write mode requires confirmedDestructive",
        ));
    }

    job.options.validate()?;

    let src_quote = src_driver.quote_char();
    let tgt_quote = tgt_driver.quote_char();
    let src_family = src_driver.driver_type();
    let tgt_family = tgt_driver.driver_type();
    let mut tables_out = Vec::new();
    let mut total_rows = 0u64;
    let mut partial = false;

    let transfer_tables: Vec<_> = inspected
        .iter()
        .filter(|table| table_eligible_for_data(table, job))
        .collect();
    validate_no_self_table_overwrite(job, inspected)?;
    for (table_index, table) in transfer_tables.iter().enumerate() {
        if let Some(flag) = &cancelled {
            if flag.load(Ordering::SeqCst) {
                return Ok(TransferExecutionResult {
                    tables: tables_out,
                    rows_inserted: total_rows,
                    cancelled: true,
                    partial: true,
                    resume_token: None,
                });
            }
        }

        if completed_tables.is_some_and(|completed| completed.contains(&table.source_table)) {
            continue;
        }

        // In Structure+Data, a successful create is an earlier, separately
        // committed target write. Drop+Create is handled below as its own
        // preamble and is skipped by the structure phase.
        let mut known_preamble_applied = drop_create
            .is_some_and(|context| context.structure_precreated)
            || (job.mode == TransferMode::StructureAndData
                && table.status == super::super::model::TableMappingStatus::CreateNew
                && job.write_mode != WriteMode::DropCreateInsert);

        let columns = active_column_mappings(&table.column_mappings);
        if columns.is_empty() {
            tables_out.push(TableExecutionResult::database(
                &table.source_table,
                &table.target_table,
                Some(0),
                outcome_before_data_transaction(known_preamble_applied),
                Some("no column mappings".into()),
            ));
            partial = true;
            if job.options.stop_on_error {
                break;
            }
            continue;
        }

        let max_parameters = tgt_driver.max_bound_parameters();
        let max_batch_rows = max_parameters / columns.len();
        if max_batch_rows == 0 {
            tables_out.push(TableExecutionResult::database(
                &table.source_table,
                &table.target_table,
                Some(0),
                outcome_before_data_transaction(known_preamble_applied),
                Some(format!(
                    "target mapping has {} columns, exceeding the driver's {max_parameters}-parameter statement limit",
                    columns.len()
                )),
            ));
            partial = true;
            if job.options.stop_on_error {
                break;
            }
            continue;
        }
        let batch = (job.options.batch_size as usize).min(max_batch_rows);

        let Some(src_schema) = source_schemas.get(&table.source_table) else {
            tables_out.push(TableExecutionResult::database(
                &table.source_table,
                &table.target_table,
                Some(0),
                outcome_before_data_transaction(known_preamble_applied),
                Some("source schema not loaded".into()),
            ));
            partial = true;
            if job.options.stop_on_error {
                break;
            }
            continue;
        };

        let src_table_ref = qualify_relation_sql(
            &src_family,
            Some(&job.source.database),
            job.source.schema.as_deref(),
            &table.source_table,
            src_quote,
        );
        let tgt_table_ref = qualify_relation_sql(
            &tgt_family,
            Some(&target.database),
            target.schema.as_deref(),
            &table.target_table,
            tgt_quote,
        );

        let select_cols: Vec<String> = columns
            .iter()
            .map(|c| quote_ident_sql(&c.source_column, src_quote))
            .collect();
        let mut base_sql = format!("SELECT {} FROM {}", select_cols.join(", "), src_table_ref);
        let mapping = job
            .tables
            .iter()
            .find(|mapping| mapping.source_table == table.source_table);
        let source_scope = match super::super::recordset::build_source_scope(
            src_schema,
            mapping.and_then(|mapping| mapping.source_filter.as_ref()),
            mapping.and_then(|mapping| mapping.recordset.as_ref()),
            src_quote,
            &src_family,
            |index, data_type| {
                src_driver
                    .parameter_placeholder(index, data_type)
                    .map_err(|error| TransferError::unsupported(error.to_string()))
            },
            |column| {
                src_schema
                    .columns
                    .iter()
                    .find(|candidate| candidate.name == column)
                    .map(|candidate| candidate.data_type.clone())
            },
        ) {
            Ok(scope) => scope,
            Err(error) => {
                tables_out.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    Some(0),
                    outcome_before_data_transaction(known_preamble_applied),
                    Some(error.to_string()),
                ));
                partial = true;
                if job.options.stop_on_error {
                    break;
                }
                continue;
            }
        };
        source_scope.append_to(&mut base_sql);

        // Resolve capability and scan once before any destructive target operation.
        if let Err(error) = tgt_driver.parameter_placeholder(1, None) {
            tables_out.push(TableExecutionResult::database(
                &table.source_table,
                &table.target_table,
                Some(0),
                outcome_before_data_transaction(known_preamble_applied),
                Some(error.to_string()),
            ));
            partial = true;
            if job.options.stop_on_error {
                break;
            }
            continue;
        }

        macro_rules! resume_chunk_context {
            ($checkpoint:expr) => {
                super::super::resume_dispatch::ResumeChunkContext {
                    source_driver: src_driver,
                    source_handle: src_handle,
                    target_driver: tgt_driver,
                    target_handle: tgt_handle,
                    job,
                    table,
                    source_schema: src_schema,
                    source_scope: &source_scope,
                    source_table_ref: &src_table_ref,
                    target_table_ref: &tgt_table_ref,
                    source_quote: src_quote,
                    target_session_id: &target.db_session_id,
                    source_family: &src_family,
                    target_family: &tgt_family,
                    recordset: mapping.and_then(|mapping| mapping.recordset.as_ref()),
                    columns: &columns,
                    formatter,
                    cancelled: cancelled.clone(),
                    write_started,
                    checkpoint: $checkpoint,
                }
            };
        }
        let resume_attempt = match resume_checkpoint.as_mut() {
            Some(checkpoint) => {
                super::super::resume_dispatch::attempt_resume_chunk(resume_chunk_context!(Some(
                    &mut **checkpoint
                )))
                .await
            }
            None => {
                super::super::resume_dispatch::attempt_resume_chunk(resume_chunk_context!(None))
                    .await
            }
        };
        match resume_attempt {
            super::super::resume_dispatch::ResumeChunkDispatch::NotApplicable => {}
            super::super::resume_dispatch::ResumeChunkDispatch::Rejected {
                result,
                later_tables_reason,
            } => {
                if let Some(checkpoint) = resume_checkpoint
                    .as_mut()
                    .map(|checkpoint| &mut **checkpoint)
                {
                    checkpoint.invalidate();
                }
                tables_out.push(result);
                append_not_started_after_stop(
                    &mut tables_out,
                    &transfer_tables[table_index + 1..],
                    completed_tables,
                    later_tables_reason,
                );
                return Ok(TransferExecutionResult {
                    tables: tables_out,
                    rows_inserted: total_rows,
                    cancelled: false,
                    partial: true,
                    resume_token: None,
                });
            }
            super::super::resume_dispatch::ResumeChunkDispatch::Executed(chunked) => {
                let outcome = chunked
                    .result
                    .outcome
                    .unwrap_or(TableExecutionOutcome::NotStarted);
                total_rows = total_rows.saturating_add(chunked.confirmed_rows);
                if outcome == TableExecutionOutcome::Unknown {
                    if let Some(checkpoint) = resume_checkpoint
                        .as_mut()
                        .map(|checkpoint| &mut **checkpoint)
                    {
                        checkpoint.invalidate();
                    }
                    tables_out.push(chunked.result);
                    append_not_started_after_unknown(
                        &mut tables_out,
                        &transfer_tables[table_index + 1..],
                        completed_tables,
                    );
                    return Ok(TransferExecutionResult {
                        tables: tables_out,
                        rows_inserted: total_rows,
                        cancelled: false,
                        partial: true,
                        resume_token: None,
                    });
                }
                let stop_later_tables_reason = chunked.stop_later_tables_reason;
                let cancelled_chunk = chunked.cancelled;
                partial |= !chunked.result.success;
                tables_out.push(chunked.result);
                if let Some(reason) = stop_later_tables_reason {
                    append_not_started_after_stop(
                        &mut tables_out,
                        &transfer_tables[table_index + 1..],
                        completed_tables,
                        reason,
                    );
                    return Ok(TransferExecutionResult {
                        tables: tables_out,
                        rows_inserted: total_rows,
                        cancelled: cancelled_chunk,
                        partial: true,
                        resume_token: None,
                    });
                }
                if cancelled_chunk {
                    return Ok(TransferExecutionResult {
                        tables: tables_out,
                        rows_inserted: total_rows,
                        cancelled: true,
                        partial: true,
                        resume_token: None,
                    });
                }
                if partial && job.options.stop_on_error {
                    break;
                }
                continue;
            }
        }

        let mut scan = match super::super::scan::scan_rows_with_params(
            src_driver,
            src_handle,
            &base_sql,
            &source_scope.params,
            columns.iter().map(|c| c.source_column.clone()).collect(),
            cancelled.clone(),
        )
        .await
        {
            Ok(scan) => scan,
            Err(error) => {
                tables_out.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    Some(0),
                    outcome_before_data_transaction(known_preamble_applied),
                    Some(error.to_string()),
                ));
                partial = true;
                if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
                    return Ok(TransferExecutionResult {
                        tables: tables_out,
                        rows_inserted: total_rows,
                        cancelled: true,
                        partial,
                        resume_token: None,
                    });
                }
                if job.options.stop_on_error {
                    break;
                }
                continue;
            }
        };

        if job.write_mode == WriteMode::DropCreateInsert {
            let Some(ctx) = drop_create else {
                return Err(TransferError::validation(
                    "drop+create requires IR adapters",
                ));
            };
            if !ctx.structure_precreated {
                if let Err(e) = drop_and_recreate_table(
                    ctx.src_adapter,
                    ctx.tgt_adapter,
                    ctx.src_driver,
                    ctx.src_handle,
                    ctx.tgt_driver,
                    ctx.tgt_handle,
                    table,
                    job,
                    ctx.source_schemas,
                    write_started,
                )
                .await
                {
                    let (outcome, error) = match e {
                        super::super::structure::DropCreateFailure::NotStarted(error) => (
                            outcome_before_data_transaction(known_preamble_applied),
                            error.to_string(),
                        ),
                        super::super::structure::DropCreateFailure::Unknown(error) => {
                            (TableExecutionOutcome::Unknown, error.to_string())
                        }
                    };
                    tables_out.push(TableExecutionResult::database(
                        &table.source_table,
                        &table.target_table,
                        (outcome != TableExecutionOutcome::Unknown).then_some(0),
                        outcome,
                        Some(error),
                    ));
                    partial = true;
                    if outcome == TableExecutionOutcome::Unknown {
                        append_not_started_after_unknown(
                            &mut tables_out,
                            &transfer_tables[table_index + 1..],
                            completed_tables,
                        );
                        return Ok(TransferExecutionResult {
                            tables: tables_out,
                            rows_inserted: total_rows,
                            cancelled: false,
                            partial: true,
                            resume_token: None,
                        });
                    }
                    if job.options.stop_on_error {
                        break;
                    }
                    continue;
                }
            }
            known_preamble_applied = true;
        } else if job.write_mode == WriteMode::TruncateInsert {
            let tgt_table_ref = qualify_relation_sql(
                &tgt_family,
                Some(&target.database),
                target.schema.as_deref(),
                &table.target_table,
                tgt_quote,
            );
            let truncate_sql = build_truncate_sql_ref(&tgt_table_ref);
            if let Some(write_started) = write_started {
                write_started.store(true, Ordering::SeqCst);
            }
            if let Err(e) = tgt_driver
                .execute(tgt_handle, &truncate_sql)
                .await
                .map_err(|e| TransferError::validation(e.to_string()))
            {
                tables_out.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    None,
                    TableExecutionOutcome::Unknown,
                    Some(format!("truncate failed; outcome UNKNOWN: {e}")),
                ));
                append_not_started_after_unknown(
                    &mut tables_out,
                    &transfer_tables[table_index + 1..],
                    completed_tables,
                );
                return Ok(TransferExecutionResult {
                    tables: tables_out,
                    rows_inserted: total_rows,
                    cancelled: false,
                    partial: true,
                    resume_token: None,
                });
            }
            known_preamble_applied = true;
        }

        let target_schema = match super::super::metadata::load_table_schema(
            tgt_driver,
            tgt_handle,
            target,
            &table.target_table,
        )
        .await
        {
            Ok(schema) => schema,
            Err(error) => {
                tables_out.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    Some(0),
                    outcome_before_data_transaction(known_preamble_applied),
                    Some(error.to_string()),
                ));
                partial = true;
                if job.options.stop_on_error {
                    break;
                }
                continue;
            }
        };
        let explicit_identity_columns = columns
            .iter()
            .filter(|mapping| {
                target_schema
                    .columns
                    .iter()
                    .any(|column| column.name == mapping.target_column && column.is_auto_increment)
            })
            .map(|mapping| mapping.target_column.clone())
            .collect::<Vec<_>>();
        let identity_insert_requires_toggle = !explicit_identity_columns.is_empty()
            && tgt_driver.explicit_identity_insert_requires_session_toggle();
        let tx = match tgt_driver.begin_transaction(tgt_handle).await {
            Ok(tx) => tx,
            Err(error) => {
                tables_out.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    Some(0),
                    outcome_before_data_transaction(known_preamble_applied),
                    Some(format!("cannot start data transaction: {error}")),
                ));
                partial = true;
                if job.options.stop_on_error {
                    break;
                }
                continue;
            }
        };
        let mut table_rows = 0u64;
        let mut table_error: Option<String> = None;
        let mut was_cancelled = false;
        let mut successful_data_batches = false;
        let mut identity_insert_may_be_enabled = false;
        if identity_insert_requires_toggle {
            // Treat ON as potentially applied even when the call fails: the
            // server may have changed session state before the client saw an
            // error, so every attempted ON must be followed by an OFF attempt.
            identity_insert_may_be_enabled = true;
            if let Err(error) = tgt_driver
                .set_identity_insert(
                    tgt_handle,
                    &target.database,
                    target.schema.as_deref(),
                    &table.target_table,
                    true,
                )
                .await
            {
                table_error = Some(format!(
                    "cannot enable explicit identity insertion: {error}"
                ));
            }
        }
        'batches: loop {
            if table_error.is_some() {
                break;
            }
            if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
                was_cancelled = true;
                table_error =
                    Some("transfer cancelled; current data transaction rolled back".into());
                break;
            }
            let rows = match scan.next_batch(batch) {
                Ok(rows) => rows,
                Err(error) => {
                    table_error = Some(error.to_string());
                    break;
                }
            };
            if rows.is_empty() {
                break;
            }
            let projected_rows = rows
                .iter()
                .map(|row| map_row_values(row, src_schema, &columns))
                .collect::<Result<Vec<_>, _>>();
            let projected_rows = match projected_rows {
                Ok(rows) => rows,
                Err(error) => {
                    table_error = Some(error.to_string());
                    break 'batches;
                }
            };
            if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
                was_cancelled = true;
                table_error =
                    Some("transfer cancelled; current data transaction rolled back".into());
                break 'batches;
            }
            let statement = super::super::writer::bound_insert_batch(
                tgt_driver,
                &table.source_table,
                &tgt_table_ref,
                &columns,
                &target_schema,
                &projected_rows,
                formatter,
            );
            let (sql, parameters) = match statement {
                Ok(statement) => statement,
                Err(error) => {
                    table_error = Some(error.to_string());
                    break 'batches;
                }
            };
            if let Some(write_started) = write_started {
                write_started.store(true, Ordering::SeqCst);
            }
            match tgt_driver
                .execute_with_params(tgt_handle, &sql, &parameters)
                .await
            {
                Ok(affected) => {
                    table_rows += affected;
                    successful_data_batches = true;
                }
                Err(error) => {
                    table_error = Some(error.to_string());
                    break 'batches;
                }
            }
        }
        if table_error.is_none() && cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
            was_cancelled = true;
            table_error = Some("transfer cancelled; current data transaction rolled back".into());
        }
        let mut identity_insert_cleanup_failed = false;
        if identity_insert_may_be_enabled {
            if let Err(error) = tgt_driver
                .set_identity_insert(
                    tgt_handle,
                    &target.database,
                    target.schema.as_deref(),
                    &table.target_table,
                    false,
                )
                .await
            {
                identity_insert_cleanup_failed = true;
                let prior_error = table_error
                    .take()
                    .map(|message| format!("{message}; "))
                    .unwrap_or_default();
                table_error = Some(format!(
                    "{prior_error}cannot disable explicit identity insertion: {error}; target connection will be discarded after rollback"
                ));
            }
        }
        // `affected` can be zero for a successful INSERT ... ON CONFLICT
        // DO NOTHING / ignore batch that still carried explicit identity
        // values. Reseed based on successful batch execution, not row-count
        // reporting, so the target sequence remains beyond imported IDs.
        if table_error.is_none() && successful_data_batches {
            if !explicit_identity_columns.is_empty() {
                if let Err(error) = tgt_driver
                    .advance_transfer_identity_sequences(
                        tgt_handle,
                        target.schema.as_deref(),
                        &table.target_table,
                        &explicit_identity_columns,
                    )
                    .await
                {
                    table_error = Some(format!(
                        "cannot safely synchronize generated identity values: {error}"
                    ));
                }
            }
        }
        if table_error.is_none() && cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
            was_cancelled = true;
            table_error = Some("transfer cancelled; current data transaction rolled back".into());
        }
        let outcome = if table_error.is_some() {
            table_rows = 0;
            partial = true;
            let rollback_outcome = match tgt_driver.rollback(tx).await {
                Ok(()) => {
                    if known_preamble_applied {
                        TableExecutionOutcome::PartiallyApplied
                    } else {
                        TableExecutionOutcome::RolledBack
                    }
                }
                Err(error) => {
                    table_error = Some(format!(
                        "{}; rollback failed, outcome UNKNOWN: {error}",
                        table_error.as_deref().unwrap_or("write failed")
                    ));
                    TableExecutionOutcome::Unknown
                }
            };
            if identity_insert_cleanup_failed {
                if let Err(error) = tgt_driver.discard_connection(tgt_handle).await {
                    table_error = Some(format!(
                        "{}; failed to discard target connection after IDENTITY_INSERT OFF failed: {error}; outcome UNKNOWN",
                        table_error.as_deref().unwrap_or("identity cleanup failed")
                    ));
                    TableExecutionOutcome::Unknown
                } else {
                    rollback_outcome
                }
            } else {
                rollback_outcome
            }
        } else if let Err(error) = tgt_driver.commit(tx).await {
            table_error = Some(format!("commit failed, outcome UNKNOWN: {error}"));
            table_rows = 0;
            partial = true;
            TableExecutionOutcome::Unknown
        } else {
            #[cfg(any(test, all(debug_assertions, feature = "webdriver")))]
            {
                if consume_test_commit_ack_loss(&table.target_table) {
                    table_error = Some(
                        "debug test seam: target commit succeeded but its acknowledgement was dropped"
                            .into(),
                    );
                    table_rows = 0;
                    partial = true;
                    TableExecutionOutcome::Unknown
                } else {
                    TableExecutionOutcome::Committed
                }
            }
            #[cfg(not(any(test, all(debug_assertions, feature = "webdriver"))))]
            {
                TableExecutionOutcome::Committed
            }
        };
        total_rows += table_rows;

        tables_out.push(TableExecutionResult::database(
            &table.source_table,
            &table.target_table,
            (outcome != TableExecutionOutcome::Unknown).then_some(table_rows),
            outcome,
            table_error,
        ));

        if outcome == TableExecutionOutcome::Unknown {
            append_not_started_after_unknown(
                &mut tables_out,
                &transfer_tables[table_index + 1..],
                completed_tables,
            );
            return Ok(TransferExecutionResult {
                tables: tables_out,
                rows_inserted: total_rows,
                cancelled: false,
                partial: true,
                resume_token: None,
            });
        }

        if was_cancelled {
            return Ok(TransferExecutionResult {
                tables: tables_out,
                rows_inserted: total_rows,
                cancelled: true,
                partial: true,
                resume_token: None,
            });
        }
        if partial && job.options.stop_on_error {
            break;
        }
    }

    Ok(TransferExecutionResult {
        tables: tables_out,
        rows_inserted: total_rows,
        cancelled: false,
        partial,
        resume_token: None,
    })
}

fn append_not_started_after_unknown(
    results: &mut Vec<TableExecutionResult>,
    remaining: &[&TableInspectResult],
    completed_tables: Option<&HashSet<String>>,
) {
    for table in remaining {
        if completed_tables.is_some_and(|completed| completed.contains(&table.source_table)) {
            continue;
        }
        results.push(TableExecutionResult::database(
            &table.source_table,
            &table.target_table,
            Some(0),
            TableExecutionOutcome::NotStarted,
            Some("not started because an earlier table has an unknown outcome".into()),
        ));
    }
}

fn append_not_started_after_stop(
    results: &mut Vec<TableExecutionResult>,
    remaining: &[&TableInspectResult],
    completed_tables: Option<&HashSet<String>>,
    reason: &str,
) {
    for table in remaining {
        if completed_tables.is_some_and(|completed| completed.contains(&table.source_table)) {
            continue;
        }
        results.push(TableExecutionResult::database(
            &table.source_table,
            &table.target_table,
            Some(0),
            TableExecutionOutcome::NotStarted,
            Some(reason.to_string()),
        ));
    }
}

fn outcome_before_data_transaction(preamble_applied: bool) -> TableExecutionOutcome {
    if preamble_applied {
        TableExecutionOutcome::PartiallyApplied
    } else {
        TableExecutionOutcome::NotStarted
    }
}
