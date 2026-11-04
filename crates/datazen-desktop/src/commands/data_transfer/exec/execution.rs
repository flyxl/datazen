//! Claimed-plan Data Transfer execution and checkpoint finalization.

use super::*;
use datazen_driver_api::{LegacySqlDialect, SqlDialect};

fn validate_structure_selection(
    plan: &StoredTransferPlan,
    selection: &TransferRunSelection,
) -> Result<(), crate::data_transfer::TransferError> {
    let Some(structure) = plan.database_structure.as_ref() else {
        return Ok(());
    };
    let selected: HashSet<String> = selected_source_tables(&plan.job, selection)
        .into_iter()
        .collect();
    for item in structure.iter().filter(|item| {
        item.kind == crate::data_transfer::model::DdlPreviewKind::ForeignKey
            && selected.contains(&item.source_table)
    }) {
        if let Some(missing) = item
            .depends_on
            .iter()
            .find(|dependency| !selected.contains(*dependency))
        {
            return Err(crate::data_transfer::TransferError::validation(format!(
                "selected table '{}' has a foreign key that requires selected parent table '{}'",
                item.source_table, missing
            )));
        }
    }
    Ok(())
}

/// 无写入观察者的执行入口。
///
/// 生产路径一律走 [`execute_data_transfer_impl_with_write_observer`]（它需要
/// `write_started` 观察者把「写入是否已开始」记进迁移运行历史）。这个薄封装
/// 只剩测试夹具在用——workflow 的迁移 step 移除后，不再有生产调用方。
#[cfg(test)]
pub(crate) async fn execute_data_transfer_impl(
    state: &AppState,
    request: TransferRunRequest,
) -> Result<TransferExecutionResult, CommandError> {
    execute_data_transfer_impl_with_write_observer(state, request, None).await
}

pub(crate) async fn execute_data_transfer_impl_with_write_observer(
    state: &AppState,
    request: TransferRunRequest,
    write_started: Option<&AtomicBool>,
) -> Result<TransferExecutionResult, CommandError> {
    if request.plan_id.trim().is_empty() {
        return Err(CommandError::Validation(
            "execute_data_transfer requires a preview planId".into(),
        ));
    }
    let scope = state.organization_scope()?;
    let (plan, checkpoint) = match request.resume_token.as_deref() {
        Some(token) => state
            .transfer_plans
            .peek_checkpoint(&scope, token, &request.plan_id)
            .map(|(plan, checkpoint)| (plan, Some(checkpoint)))
            .map_err(CommandError::from)?,
        None => (
            state
                .transfer_plans
                .peek(&scope, &request.plan_id)
                .map_err(CommandError::from)?,
            None,
        ),
    };
    if !plan.can_execute {
        return Err(CommandError::Validation(
            "transfer preview is blocked; return to comparison".into(),
        ));
    }
    validate_selection(&plan.job, &request.selection)?;
    if checkpoint.is_some() && !supports_bounded_resume(&plan.job) {
        return Err(CommandError::Validation(
            "this transfer plan is outside the bounded resume contract; return to comparison"
                .into(),
        ));
    }
    if let Some(saved) = &checkpoint {
        if plans::source_boundary_fingerprint(&plan).map_err(CommandError::from)?
            != saved.source_boundary
            || plans::target_boundary_fingerprint(&plan).map_err(CommandError::from)?
                != saved.target_boundary
        {
            return Err(CommandError::Validation(
                "resume token boundary no longer matches the immutable transfer plan".into(),
            ));
        }
    }
    let effective_selection = if let Some(saved) = &checkpoint {
        if request.selection.source_tables.is_some()
            && selected_source_tables(&plan.job, &request.selection) != saved.selected_tables
        {
            return Err(CommandError::Validation(
                "resume token is bound to a different table selection".into(),
            ));
        }
        TransferRunSelection {
            source_tables: Some(saved.selected_tables.clone()),
        }
    } else {
        request.selection.clone()
    };
    let data_table_order = if plan.job.mode == TransferMode::Data {
        Some(
            crate::data_transfer::order_selected_tables(
                &selected_source_tables(&plan.job, &effective_selection),
                &plan.target_table_dependencies,
                plan.job.write_mode,
            )
            .map_err(CommandError::from)?,
        )
    } else {
        None
    };
    validate_structure_selection(&plan, &effective_selection).map_err(CommandError::from)?;
    if plan.job.write_mode.is_destructive()
        && !plan.job.options.confirmed_destructive
        && !request.options.confirmed_destructive
    {
        return Err(CommandError::Validation(
            "destructive write mode requires final confirmation".into(),
        ));
    }

    if plan.job.sql_file_target.is_some() {
        if request.resume_token.is_some() {
            return Err(CommandError::Validation(
                "SQL-file transfers cannot be resumed; the atomic output must be regenerated"
                    .into(),
            ));
        }
        return execute_sql_file_target(state, &plan, &request).await;
    }

    // Context validation happens before the atomic claim. A stale schema or
    // changed read-only policy sends the user back to comparison without
    // burning a still-valid plan; once claimed, retries are always refused.
    let context = match validate_plan_context(state, &plan).await {
        Ok(context) => context,
        Err(error) => {
            if let Some(token) = request.resume_token.as_deref() {
                state.transfer_plans.invalidate_checkpoint(&scope, token);
            }
            return Err(error);
        }
    };
    let job_id = request.job_id.clone();
    let cancelled = match job_id.as_deref() {
        Some(id) => Some(jobs::ensure_job(id).await),
        None => None,
    };
    if cancelled
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
    {
        return Err(CommandError::Validation(
            "transfer cancelled before start".into(),
        ));
    }
    let (claimed, claimed_checkpoint) = match request.resume_token.as_deref() {
        Some(token) => state
            .transfer_plans
            .claim_checkpoint(&scope, token, &request.plan_id)
            .map(|(plan, checkpoint)| (plan, Some(checkpoint)))
            .map_err(CommandError::from)?,
        None => (
            state
                .transfer_plans
                .claim(&scope, &request.plan_id)
                .map_err(CommandError::from)?,
            None,
        ),
    };
    let immutable_structure = claimed.database_structure.clone();
    let mut job = claimed.job;
    apply_selection(&mut job, &effective_selection);
    let src_config = context.src_config;
    let tgt_config = context.tgt_config;
    let src_driver = context.src_driver;
    let src_handle = context.src_handle;
    let tgt_driver = context.tgt_driver;
    let tgt_handle = context.tgt_handle;
    let resume_completed = claimed_checkpoint.as_ref().map(|checkpoint| {
        checkpoint
            .completed_tables
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
    });
    let prior_completed = claimed_checkpoint
        .as_ref()
        .map(|checkpoint| checkpoint.completed_tables.clone())
        .unwrap_or_default();
    let mut checkpoint_session = TransferCheckpointSession::new(
        state.transfer_plans.clone(),
        scope.clone(),
        request.plan_id.clone(),
        selected_source_tables(&job, &effective_selection),
        prior_completed.clone(),
        request.resume_token.clone(),
    );
    let pairing = enforce_transfer_pairing(&src_config.database_type, &tgt_config.database_type)
        .map_err(CommandError::from)?;

    let mut inspected = inspect_data_transfer_impl(
        state,
        job.source.db_session_id.clone(),
        job.database_target()
            .map_err(CommandError::from)?
            .db_session_id
            .clone(),
        Some(job.source.database.clone()),
        Some(
            job.database_target()
                .map_err(CommandError::from)?
                .database
                .clone(),
        ),
        job.source.normalized_schema(),
        job.database_target()
            .map_err(CommandError::from)?
            .normalized_schema(),
        job.mode,
        &job.tables,
    )
    .await?;

    if let Some(order) = data_table_order.as_deref() {
        crate::data_transfer::reorder_inspected_tables(&mut inspected, order);
    }

    if matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    ) {
        validate_no_self_table_overwrite(&job, &inspected).map_err(CommandError::from)?;
    }

    if matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    ) {
        // 占位符规则来自 SqlDialect 契约，旧驱动 API 逐条转发。
        let tgt_dialect = LegacySqlDialect::new(tgt_driver.as_ref());
        tgt_dialect
            .parameter_placeholder(1, None)
            .map_err(|error| CommandError::Validation(error.to_string()))?;
        // Check the target data transaction path before any structure mutation.
        let probe = tgt_driver
            .begin_transaction(&tgt_handle)
            .await
            .cmd_err("execute_data_transfer")?;
        tgt_driver
            .rollback(probe)
            .await
            .cmd_err("execute_data_transfer")?;
    }

    let src_tables = src_driver
        .get_tables(
            &src_handle,
            &job.source.database,
            job.source.normalized_schema(),
        )
        .await
        .cmd_err("execute_data_transfer")?;

    let src_tables: Vec<_> = src_tables
        .into_iter()
        .filter(|table| {
            crate::data_transfer::metadata::table_in_endpoint_schema(&job.source, table)
        })
        .collect();

    let source_table_names = src_tables
        .iter()
        .filter(|table| matches!(table.table_type, TableType::Table))
        .map(|table| table.name.as_str())
        .collect::<Vec<_>>();
    let mut source_schemas = super::super::schema_loading::load_named_table_schemas(
        src_driver.as_ref(),
        &src_handle,
        &job.source,
        &source_table_names,
        "source",
    )
    .await?;

    let resume_preflight = if supports_bounded_resume(&job) {
        match super::resume_preflight::inspect_resume_contract(
            &job,
            &inspected,
            &source_schemas,
            src_driver.as_ref(),
            &src_handle,
            tgt_driver.as_ref(),
            &tgt_handle,
        )
        .await
        {
            Ok(preflight) => Some(preflight),
            Err(reason) => {
                checkpoint_session.invalidate();
                checkpoint_session.abort();
                if let Some(id) = job_id.as_deref() {
                    jobs::remove_job(id).await;
                }
                return Err(CommandError::Validation(format!(
                    "target metadata could not establish safe transfer checkpoint boundaries before writing: {reason}"
                )));
            }
        }
    } else {
        None
    };
    if claimed_checkpoint.is_some()
        && resume_preflight
            .as_ref()
            .is_some_and(|preflight| !preflight.table_boundary_safe())
    {
        let reason = resume_preflight
            .as_ref()
            .and_then(|preflight| preflight.table_boundary_reason.as_deref())
            .unwrap_or("the target/session checkpoint contract is no longer proven");
        checkpoint_session.invalidate();
        checkpoint_session.abort();
        if let Some(id) = job_id.as_deref() {
            jobs::remove_job(id).await;
        }
        return Err(CommandError::Validation(format!(
            "resume token was invalidated before writing: {reason}"
        )));
    }
    let table_boundary_resume_safe = resume_preflight
        .as_ref()
        .is_some_and(|preflight| preflight.table_boundary_safe());
    let resume_unavailable_reason = resume_preflight.as_ref().and_then(|preflight| {
        if !preflight.table_boundary_safe() {
            preflight
                .table_boundary_reason
                .as_deref()
                .map(|reason| format!("resume token not issued: {reason}"))
        } else {
            preflight.chunk_reason.as_deref().map(|reason| {
                format!(
                    "row-level resume is unavailable ({reason}); any continuation is limited to atomic whole-table boundaries"
                )
            })
        }
    });

    let needs_adapters = !is_same_family(&pairing)
        || matches!(
            job.mode,
            TransferMode::Structure | TransferMode::StructureAndData
        )
        || job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert;

    let adapters = if needs_adapters {
        Some(
            resolve_transfer_adapters(state, &src_config.database_type, &tgt_config.database_type)
                .await?,
        )
    } else {
        None
    };

    if let Some(adapters) = &adapters {
        crate::data_transfer::structure::enrich_source_types(
            adapters.src_source.as_ref(),
            src_driver.as_ref(),
            &src_handle,
            &job.source,
            &mut source_schemas,
        )
        .await
        .map_err(CommandError::from)?;
        crate::data_transfer::structure::validate_transfer_column_types(
            &job,
            &inspected,
            &source_schemas,
            adapters.src_source.as_ref(),
            adapters.tgt_target.as_ref(),
        )
        .map_err(CommandError::from)?;
        if matches!(
            job.mode,
            TransferMode::Structure | TransferMode::StructureAndData
        ) || job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert
        {
            crate::data_transfer::structure::validate_source_structure_metadata(
                adapters.src_source.as_ref(),
                src_driver.as_ref(),
                &src_handle,
                &job.source,
                &source_schemas,
                &inspected,
            )
            .await
            .map_err(CommandError::from)?;
        }
    }

    let mut all_tables = Vec::new();
    let mut total_rows = 0u64;
    let mut cancelled_flag = false;
    let mut partial = false;

    if matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    ) {
        let (src_adapter, tgt_adapter) = match &adapters {
            Some(a) => (a.src_source.as_ref(), a.tgt_target.as_ref()),
            None => {
                return Err(CommandError::Validation(
                    "IR sync adapters are required for structure operations".into(),
                ));
            }
        };

        let structure_results = if let Some(structure_plan) = immutable_structure.as_ref() {
            let selected_tables: HashSet<String> = job
                .tables
                .iter()
                .filter(|table| table.enabled)
                .map(|table| table.source_table.clone())
                .collect();
            let phase = if job.mode == TransferMode::Structure {
                crate::data_transfer::structure::DatabaseStructurePhase::All
            } else {
                crate::data_transfer::structure::DatabaseStructurePhase::Prepare
            };
            crate::data_transfer::structure::execute_database_structure_plan(
                tgt_driver.as_ref(),
                &tgt_handle,
                structure_plan,
                &selected_tables,
                phase,
                cancelled.clone(),
                write_started,
            )
            .await
        } else {
            create_target_tables_with_write_observer(
                src_adapter,
                tgt_adapter,
                src_driver.as_ref(),
                &src_handle,
                tgt_driver.as_ref(),
                &tgt_handle,
                &job,
                &inspected,
                &source_schemas,
                cancelled.clone(),
                write_started,
            )
            .await
            .map_err(CommandError::from)?
        };

        for r in &structure_results {
            if !r.success {
                partial = true;
                // Never write data into a table whose structure phase failed.
                for table in &mut inspected {
                    if table.source_table == r.source_table {
                        table.enabled = false;
                    }
                }
            }
        }
        cancelled_flag = cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst));
        let stop_after_structure = should_stop_after_structure_phase(
            &structure_results,
            cancelled_flag,
            partial,
            job.options.stop_on_error,
        );
        all_tables.extend(structure_results);

        if stop_after_structure {
            if let Some(id) = job_id.as_deref() {
                jobs::remove_job(id).await;
            }
            return Ok(TransferExecutionResult {
                tables: all_tables,
                rows_inserted: 0,
                cancelled: cancelled_flag,
                partial: true,
                resume_token: None,
            });
        }
    }

    if matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    ) {
        let table_ir_types: HashMap<String, HashMap<String, crate::transfer::ir::IRType>> =
            if let Some(a) = &adapters {
                inspected
                    .iter()
                    .filter(|t| t.enabled)
                    .filter_map(|t| {
                        let schema = source_schemas.get(&t.source_table)?;
                        let ir = source_schema_to_target_ir(
                            a.src_source.as_ref(),
                            schema,
                            None,
                            &t.target_table,
                        );
                        Some((t.source_table.clone(), column_ir_types_by_source(&ir)))
                    })
                    .collect()
            } else {
                HashMap::new()
            };

        let formatter = if !is_same_family(&pairing) {
            let a = adapters.as_ref().ok_or_else(|| {
                CommandError::Validation("cross-family execute requires IR sync adapters".into())
            })?;
            ValueFormatter::Ir {
                tgt_adapter: a.tgt_target.as_ref(),
                source_column_ir_types: &table_ir_types,
            }
        } else {
            ValueFormatter::SameFamily
        };

        let drop_create = if job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert {
            let a = adapters.as_ref().ok_or_else(|| {
                CommandError::Validation("drop+create requires IR sync adapters".into())
            })?;
            Some(DropCreateContext {
                src_adapter: a.src_source.as_ref(),
                tgt_adapter: a.tgt_target.as_ref(),
                src_driver: src_driver.as_ref(),
                src_handle: &src_handle,
                tgt_driver: tgt_driver.as_ref(),
                tgt_handle: &tgt_handle,
                source_schemas: &source_schemas,
                structure_precreated: job.mode == TransferMode::StructureAndData
                    && job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert
                    && immutable_structure.is_some(),
            })
        } else {
            None
        };

        let checkpoint_hook: Option<&mut dyn TransferResumeCheckpoint> =
            if table_boundary_resume_safe {
                Some(&mut checkpoint_session)
            } else {
                None
            };
        let data_result = execute_transfer_data_with_resume_checkpoint(
            src_driver.as_ref(),
            &src_handle,
            tgt_driver.as_ref(),
            &tgt_handle,
            &job,
            &inspected,
            &source_schemas,
            &formatter,
            drop_create.as_ref(),
            tgt_config.read_only,
            cancelled.clone(),
            resume_completed.as_ref(),
            write_started,
            checkpoint_hook,
        )
        .await
        .map_err(CommandError::from)?;

        let data_phase_complete = !data_result.partial && !data_result.cancelled;
        total_rows = data_result.rows_inserted;
        cancelled_flag = data_result.cancelled;
        partial = partial || data_result.partial;
        all_tables.extend(data_result.tables);

        if job.mode == TransferMode::StructureAndData {
            if let Some(structure_plan) = immutable_structure.as_ref() {
                let selected_tables: HashSet<String> = job
                    .tables
                    .iter()
                    .filter(|table| table.enabled)
                    .map(|table| table.source_table.clone())
                    .collect();
                if data_phase_complete {
                    let constraint_results =
                        crate::data_transfer::structure::execute_database_structure_plan(
                            tgt_driver.as_ref(),
                            &tgt_handle,
                            structure_plan,
                            &selected_tables,
                            crate::data_transfer::structure::DatabaseStructurePhase::ForeignKeys,
                            cancelled.clone(),
                            write_started,
                        )
                        .await;
                    if constraint_results.iter().any(|result| !result.success) {
                        partial = true;
                    }
                    all_tables.extend(constraint_results);
                } else {
                    for item in structure_plan.iter().filter(|item| {
                        item.kind == crate::data_transfer::model::DdlPreviewKind::ForeignKey
                            && selected_tables.contains(&item.source_table)
                    }) {
                        all_tables.push(crate::data_transfer::model::TableExecutionResult::database(
                            &item.source_table,
                            &item.target_table,
                            Some(0),
                            crate::data_transfer::model::TableExecutionOutcome::NotStarted,
                            Some("foreign key was not installed because the data phase did not complete".into()),
                        ));
                    }
                    if structure_plan.iter().any(|item| {
                        item.kind == crate::data_transfer::model::DdlPreviewKind::ForeignKey
                            && selected_tables.contains(&item.source_table)
                    }) {
                        partial = true;
                    }
                }
            }
        }
    }

    let mut output = TransferExecutionResult {
        tables: all_tables,
        rows_inserted: total_rows,
        cancelled: cancelled_flag,
        partial,
        resume_token: None,
    };
    if supports_bounded_resume(&job) && table_boundary_resume_safe {
        if has_unknown_outcome(&output) {
            checkpoint_session.invalidate();
        }
        let completed = completed_tables_from_result(&prior_completed, &output);
        output.resume_token = checkpoint_session
            .finish(completed, output.partial || output.cancelled)
            .map_err(CommandError::from)?;
    } else {
        checkpoint_session.abort();
    }
    if (output.partial || output.cancelled) && resume_unavailable_reason.is_some() {
        append_resume_unavailable_reason(
            &mut output,
            resume_unavailable_reason
                .as_deref()
                .unwrap_or("checkpoint safety could not be proven"),
        );
    }
    if let Some(id) = job_id.as_deref() {
        jobs::remove_job(id).await;
    }

    Ok(output)
}

fn append_resume_unavailable_reason(output: &mut TransferExecutionResult, reason: &str) {
    if let Some(table) = output.tables.iter_mut().find(|table| !table.success) {
        let error = table.error.get_or_insert_with(String::new);
        if !error.is_empty() {
            error.push_str("; ");
        }
        error.push_str(reason);
    }
}
