//! Execute Data Transfer (structure + data, same-family and IR).

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::inspect::inspect_data_transfer_impl;
use super::jobs;
use super::plans::{self, StoredTransferPlan, TransferCheckpointSession};
use crate::data_transfer::model::TableExecutionOutcome;
use crate::data_transfer::model::TransferRunSelection;
use crate::data_transfer::resume::TransferResumeCheckpoint;
use crate::data_transfer::{
    column_ir_types_by_source, create_target_tables_with_write_observer, enforce_transfer_pairing,
    execute_transfer_data_with_resume_checkpoint, is_same_family, source_schema_to_target_ir,
    validate_no_self_table_overwrite, DropCreateContext, TransferExecutionResult, TransferJob,
    TransferMode, TransferRunRequest, ValueFormatter,
};
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use datazen_driver_api::TableType;

pub(super) async fn load_sql_source_snapshot(
    state: &AppState,
    job: &TransferJob,
) -> Result<
    (
        Arc<dyn crate::db::DatabaseDriver>,
        crate::db::ConnectionHandle,
        Vec<crate::data_transfer::TableInspectResult>,
        HashMap<String, datazen_driver_api::TableSchema>,
    ),
    CommandError,
> {
    let (driver, handle) = state
        .connection_manager
        .get_session(&job.source.db_session_id)
        .await
        .cmd_err("data_transfer_sql_file")?;
    let config = state
        .connection_manager
        .get_session_config(&job.source.db_session_id)
        .await
        .cmd_err("data_transfer_sql_file")?;
    let mut source = job.source.clone();
    source.schema = crate::services::metadata_schema(
        driver.as_ref(),
        source.normalized_schema(),
        None,
        config.schema.as_deref(),
    );
    let tables = driver
        .get_tables(&handle, &source.database, source.normalized_schema())
        .await
        .cmd_err("data_transfer_sql_file")?;
    let tables: Vec<_> = tables
        .into_iter()
        .filter(|table| crate::data_transfer::metadata::table_in_endpoint_schema(&source, table))
        .collect();
    let mut schemas = HashMap::new();
    for table in tables
        .iter()
        .filter(|table| matches!(table.table_type, TableType::Table))
    {
        let schema = crate::data_transfer::metadata::load_table_schema(
            driver.as_ref(),
            &handle,
            &source,
            &table.name,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "failed to inspect source table '{}': {error}",
                table.name
            ))
        })?;
        schemas.insert(table.name.clone(), schema);
    }
    let inspected = crate::data_transfer::inspect_tables(
        &tables,
        &[],
        &job.tables,
        &schemas,
        &HashMap::new(),
        job.mode,
        &HashMap::new(),
    );
    Ok((driver, handle, inspected, schemas))
}

async fn execute_sql_file_target(
    state: &AppState,
    plan: &StoredTransferPlan,
    request: &TransferRunRequest,
) -> Result<TransferExecutionResult, CommandError> {
    let token = plan
        .job
        .sql_file_target
        .as_ref()
        .ok_or_else(|| CommandError::Validation("SQL file target is missing".into()))?
        .file_token
        .clone();
    let destination =
        crate::data_transfer::sql_file::resolve_path(&token).map_err(CommandError::from)?;
    if plans::filter_fingerprint(&plan.job).map_err(CommandError::from)? != plan.filter {
        return Err(CommandError::Validation(
            "source filter or recordset changed since preview; return to comparison".into(),
        ));
    }
    if plans::target_scope_fingerprint(&plan.job).map_err(CommandError::from)?
        != plan.target_scope_fingerprint
    {
        return Err(CommandError::Validation(
            "SQL-file target scope changed since preview; return to comparison".into(),
        ));
    }
    let mut validation_job = plan.job.clone();
    apply_selection(&mut validation_job, &request.selection);
    let (driver, handle, mut inspected, mut schemas) =
        load_sql_source_snapshot(state, &validation_job).await?;
    if driver.driver_type() != plan.source_driver_type
        || plans::driver_protocol_version(driver.as_ref()) != plan.source_driver_protocol
    {
        return Err(CommandError::Validation(
            "source driver contract changed since preview; return to preview".into(),
        ));
    }
    let target_driver = crate::data_transfer::sql_file::resolve_target_driver(
        driver.clone(),
        plan.job
            .sql_file_target
            .as_ref()
            .ok_or_else(|| CommandError::Validation("SQL file target is missing".into()))?,
    )
    .map_err(CommandError::from)?;
    let target = plan
        .job
        .sql_file_target
        .as_ref()
        .ok_or_else(|| CommandError::Validation("SQL file target is missing".into()))?;
    crate::data_transfer::sql_file::validate_target_scope_for_driver(
        target_driver.as_ref(),
        target,
    )
    .map_err(CommandError::from)?;
    if target_driver.driver_type() != plan.target_driver_type
        || plans::driver_protocol_version(target_driver.as_ref()) != plan.target_driver_protocol
    {
        return Err(CommandError::Validation(
            "SQL file target dialect contract changed since preview; return to preview".into(),
        ));
    }
    let src_config = state
        .connection_manager
        .get_session_config(&plan.job.source.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;
    let adapters = if let Some(target_type) = plan
        .job
        .sql_file_target
        .as_ref()
        .and_then(|target| target.normalized_database_type())
    {
        state
            .sync_adapters
            .ensure_pair(&src_config.database_type, &target_type.to_string())
            .map_err(CommandError::Validation)?;
        let src_adapter = state
            .sync_adapters
            .get_source(&src_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
        let tgt_adapter = state
            .sync_adapters
            .get_target(&target_type.to_string())
            .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;
        Some((src_adapter, tgt_adapter))
    } else {
        None
    };
    let source_structure_adapter = if let Some((source, _)) = adapters.as_ref() {
        Some(source.clone())
    } else if state
        .sync_adapters
        .ensure_type(&src_config.database_type)
        .is_ok()
    {
        state.sync_adapters.get_source(&src_config.database_type)
    } else {
        None
    };
    crate::data_transfer::sql_file::validate_target_dialect_job(&plan.job)
        .map_err(CommandError::from)?;
    // The preview plan fingerprints the source schemas after the optional
    // full-type enrichment used by cross-dialect rendering. Re-enrich this
    // pre-claim snapshot before validating the IR and calculating the
    // fingerprint, otherwise a driver-reported placeholder such as
    // USER-DEFINED would be rejected (or hash differently) even though the
    // same source type was resolved successfully during preview.
    if let Some(src_adapter) = source_structure_adapter.as_ref() {
        crate::data_transfer::structure::enrich_source_types(
            src_adapter.as_ref(),
            driver.as_ref(),
            &handle,
            &plan.job.source,
            &mut schemas,
        )
        .await
        .map_err(CommandError::from)?;
        crate::data_transfer::structure::validate_transfer_source_columns(
            &validation_job,
            &inspected,
            &schemas,
            src_adapter.as_ref(),
        )
        .map_err(CommandError::from)?;
    }
    if let Some((src_adapter, target_adapter)) = &adapters {
        crate::data_transfer::structure::enrich_create_new_target_types(
            &mut inspected,
            &schemas,
            src_adapter.as_ref(),
            target_adapter.as_ref(),
        );
        crate::data_transfer::structure::validate_transfer_column_types(
            &validation_job,
            &inspected,
            &schemas,
            src_adapter.as_ref(),
            target_adapter.as_ref(),
        )
        .map_err(CommandError::from)?;
        crate::data_transfer::sql_file::validate_target_ir(
            src_adapter.as_ref(),
            driver.as_ref(),
            target_driver.as_ref(),
            &schemas,
            &inspected,
        )
        .map_err(CommandError::from)?;
    }
    if matches!(
        plan.job.mode,
        crate::data_transfer::TransferMode::Structure
            | crate::data_transfer::TransferMode::StructureAndData
    ) {
        if let Some(source_adapter) = source_structure_adapter.as_ref() {
            crate::data_transfer::structure::validate_source_structure_metadata(
                source_adapter.as_ref(),
                driver.as_ref(),
                &handle,
                &plan.job.source,
                &schemas,
                &inspected,
            )
            .await
            .map_err(CommandError::from)?;
        }
    }
    let fingerprint =
        plans::fingerprint_schemas(plans::participating_tables(&plan.job).map(|table| {
            (
                table.source_table.clone(),
                schemas.get(&table.source_table).cloned(),
            )
        }))
        .map_err(CommandError::from)?;
    if fingerprint != plan.source_schema_fingerprint {
        return Err(CommandError::Validation(
            "source schema changed since preview; return to comparison".into(),
        ));
    }
    let claimed = plans::claim_plan(&request.plan_id).map_err(CommandError::from)?;
    let immutable_structure = claimed.sql_file_structure.clone();
    let mut job = claimed.job;
    apply_selection(&mut job, &request.selection);
    let (driver, handle, mut inspected, mut schemas) =
        load_sql_source_snapshot(state, &job).await?;
    if let Some(src_adapter) = source_structure_adapter.as_ref() {
        crate::data_transfer::structure::enrich_source_types(
            src_adapter.as_ref(),
            driver.as_ref(),
            &handle,
            &job.source,
            &mut schemas,
        )
        .await
        .map_err(CommandError::from)?;
        crate::data_transfer::structure::validate_transfer_source_columns(
            &job,
            &inspected,
            &schemas,
            src_adapter.as_ref(),
        )
        .map_err(CommandError::from)?;
    }
    if let Some((src_adapter, target_adapter)) = &adapters {
        crate::data_transfer::structure::enrich_create_new_target_types(
            &mut inspected,
            &schemas,
            src_adapter.as_ref(),
            target_adapter.as_ref(),
        );
        crate::data_transfer::structure::validate_transfer_column_types(
            &job,
            &inspected,
            &schemas,
            src_adapter.as_ref(),
            target_adapter.as_ref(),
        )
        .map_err(CommandError::from)?;
        crate::data_transfer::sql_file::validate_target_ir(
            src_adapter.as_ref(),
            driver.as_ref(),
            target_driver.as_ref(),
            &schemas,
            &inspected,
        )
        .map_err(CommandError::from)?;
    }
    if matches!(
        job.mode,
        crate::data_transfer::TransferMode::Structure
            | crate::data_transfer::TransferMode::StructureAndData
    ) {
        if let Some(source_adapter) = source_structure_adapter.as_ref() {
            crate::data_transfer::structure::validate_source_structure_metadata(
                source_adapter.as_ref(),
                driver.as_ref(),
                &handle,
                &job.source,
                &schemas,
                &inspected,
            )
            .await
            .map_err(CommandError::from)?;
        }
    }
    let cancelled = match request.job_id.as_deref() {
        Some(id) => Some(jobs::ensure_job(id).await),
        None => None,
    };
    let result = crate::data_transfer::sql_file::execute_with_target(
        driver.as_ref(),
        target_driver.as_ref(),
        adapters.as_ref().map(|(src, _)| src.as_ref()),
        adapters.as_ref().map(|(_, tgt)| tgt.as_ref()),
        &handle,
        &job,
        &inspected,
        &schemas,
        destination,
        cancelled,
        immutable_structure.as_deref(),
    )
    .await
    .map_err(CommandError::from);
    if let Some(id) = request.job_id.as_deref() {
        jobs::remove_job(id).await;
    }
    result
}

pub(super) struct TransferAdapters {
    pub(super) src_source: Arc<dyn SyncSourceAdapter>,
    pub(super) tgt_target: Arc<dyn SyncTargetAdapter>,
}

pub(super) async fn resolve_transfer_adapters(
    state: &AppState,
    src_db_type: &str,
    tgt_db_type: &str,
) -> Result<TransferAdapters, CommandError> {
    state
        .sync_adapters
        .ensure_pair(&src_db_type.to_string(), &tgt_db_type.to_string())
        .map_err(CommandError::Validation)?;
    let src_source = state
        .sync_adapters
        .get_source(&src_db_type.to_string())
        .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
    let tgt_target = state
        .sync_adapters
        .get_target(&tgt_db_type.to_string())
        .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;
    Ok(TransferAdapters {
        src_source,
        tgt_target,
    })
}

pub(super) struct ValidatedTransferContext {
    pub(super) src_config: crate::db::ConnectionConfig,
    pub(super) tgt_config: crate::db::ConnectionConfig,
    pub(super) src_driver: Arc<dyn crate::db::DatabaseDriver>,
    pub(super) src_handle: crate::db::ConnectionHandle,
    pub(super) tgt_driver: Arc<dyn crate::db::DatabaseDriver>,
    pub(super) tgt_handle: crate::db::ConnectionHandle,
}

async fn schema_fingerprint_for_side(
    endpoint: &crate::data_transfer::model::Endpoint,
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    job: &TransferJob,
    source: bool,
) -> Result<String, CommandError> {
    let mut entries = Vec::with_capacity(plans::participating_tables(job).size_hint().0);
    for table in plans::participating_tables(job) {
        let relation = if source {
            &table.source_table
        } else {
            &table.target_table
        };
        // CREATE NEW mappings intentionally have no target schema at preview
        // time. Preserve that `None` sentinel during revalidation instead of
        // turning a driver's empty-schema response into `Some(empty)`, which
        // would make an unchanged immutable plan stale.
        let schema = if relation.trim().is_empty() || (!source && table.create_new) {
            None
        } else {
            match crate::data_transfer::metadata::load_table_schema(
                driver, handle, endpoint, relation,
            )
            .await
            {
                Ok(schema) => Some(schema),
                // The preview snapshot records missing relations as `None`.
                // Reusing the same representation makes a newly created table,
                // a dropped table, or a permission failure change the hash and
                // fail closed before any target write.
                Err(_) => None,
            }
        };
        entries.push((relation.clone(), schema));
    }
    plans::fingerprint_schemas(entries).map_err(CommandError::from)
}

pub(super) async fn validate_plan_context(
    state: &AppState,
    plan: &StoredTransferPlan,
) -> Result<ValidatedTransferContext, CommandError> {
    let target = plan.job.database_target().map_err(CommandError::from)?;
    if plan.target_read_only_at_preview {
        return Err(CommandError::Validation(
            "target connection was read-only during preview; return to preview".into(),
        ));
    }
    if plans::filter_fingerprint(&plan.job).map_err(CommandError::from)? != plan.filter {
        return Err(CommandError::Validation(
            "source filter or recordset changed since preview; return to comparison".into(),
        ));
    }
    let src_config = state
        .connection_manager
        .get_session_config(&plan.job.source.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;

    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "target connection is now read-only; return to preview".into(),
        ));
    }

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&plan.job.source.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;

    if src_driver.driver_type() != plan.source_driver_type
        || tgt_driver.driver_type() != plan.target_driver_type
        || plans::driver_protocol_version(src_driver.as_ref()) != plan.source_driver_protocol
        || plans::driver_protocol_version(tgt_driver.as_ref()) != plan.target_driver_protocol
    {
        return Err(CommandError::Validation(
            "driver contract changed since preview; return to preview".into(),
        ));
    }

    let src_fingerprint = schema_fingerprint_for_side(
        &plan.job.source,
        src_driver.as_ref(),
        &src_handle,
        &plan.job,
        true,
    )
    .await?;
    let tgt_fingerprint =
        schema_fingerprint_for_side(target, tgt_driver.as_ref(), &tgt_handle, &plan.job, false)
            .await?;
    let source_schema_changed = src_fingerprint != plan.source_schema_fingerprint;
    let target_schema_changed = tgt_fingerprint != plan.target_schema_fingerprint;
    if source_schema_changed || target_schema_changed {
        return Err(CommandError::Validation(
            "source or target schema changed since preview; return to comparison".into(),
        ));
    }

    if plan.job.mode == crate::data_transfer::TransferMode::StructureAndData
        && plan.job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert
    {
        if let Some(structure) = plan.database_structure.as_deref() {
            crate::data_transfer::structure::validate_drop_create_target_dependencies(
                tgt_driver.as_ref(),
                &tgt_handle,
                target,
                structure,
            )
            .await
            .map_err(CommandError::from)?;
        }
    }

    Ok(ValidatedTransferContext {
        src_config,
        tgt_config,
        src_driver,
        src_handle,
        tgt_driver,
        tgt_handle,
    })
}

pub(super) fn validate_selection(
    job: &TransferJob,
    selection: &TransferRunSelection,
) -> Result<(), CommandError> {
    let Some(source_tables) = &selection.source_tables else {
        return Ok(());
    };
    // SQL-file previews may discover their default source table set on the
    // server because the UI has no target snapshot to inspect. Treat an
    // empty client selection as “use that immutable server selection” rather
    // than silently disabling every table. Database targets retain their
    // existing explicit-selection semantics.
    if job.sql_file_target.is_some() && source_tables.is_empty() {
        return Ok(());
    }
    let mut seen = std::collections::HashSet::new();
    for name in source_tables {
        if name.trim().is_empty() || !seen.insert(name) {
            return Err(CommandError::Validation(
                "transfer selection contains an empty or duplicate source table".into(),
            ));
        }
        let Some(table) = job.tables.iter().find(|table| table.source_table == *name) else {
            return Err(CommandError::Validation(
                "transfer selection contains a table that was not in the preview plan".into(),
            ));
        };
        if !table.enabled {
            return Err(CommandError::Validation(
                "transfer selection cannot enable a table disabled in the preview plan".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn apply_selection(job: &mut TransferJob, selection: &TransferRunSelection) {
    let Some(source_tables) = &selection.source_tables else {
        return;
    };
    if job.sql_file_target.is_some() && source_tables.is_empty() {
        return;
    }
    for table in &mut job.tables {
        table.enabled = source_tables
            .iter()
            .any(|source_table| source_table == &table.source_table);
    }
}

pub(super) fn selected_source_tables(
    job: &TransferJob,
    selection: &TransferRunSelection,
) -> Vec<String> {
    job.tables
        .iter()
        .filter(|table| table.enabled)
        .filter(|table| {
            selection.source_tables.as_ref().map_or(true, |selected| {
                selected.iter().any(|name| name == &table.source_table)
            })
        })
        .map(|table| table.source_table.clone())
        .collect()
}

/// The first resumability slice is intentionally narrow. Every completed unit
/// is an existing target table committed in its own transaction, so replaying
/// a resume token can only start an uncommitted table from its source boundary.
fn supports_bounded_resume(job: &TransferJob) -> bool {
    job.sql_file_target.is_none()
        && job.mode == TransferMode::Data
        && job.write_mode == crate::data_transfer::WriteMode::Insert
        && plans::participating_tables(job).all(|table| !table.create_new)
}

fn has_unknown_outcome(result: &TransferExecutionResult) -> bool {
    result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::Unknown))
}

fn has_unknown_table_outcome(
    results: &[crate::data_transfer::model::TableExecutionResult],
) -> bool {
    results
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::Unknown))
}

fn should_stop_after_structure_phase(
    results: &[crate::data_transfer::model::TableExecutionResult],
    cancelled: bool,
    partial: bool,
    stop_on_error: bool,
) -> bool {
    has_unknown_table_outcome(results) || cancelled || (partial && stop_on_error)
}

fn completed_tables_from_result(prior: &[String], result: &TransferExecutionResult) -> Vec<String> {
    let mut completed: HashSet<String> = prior.iter().cloned().collect();
    completed.extend(
        result
            .tables
            .iter()
            .filter(|table| table.success)
            .map(|table| table.source_table.clone()),
    );
    let mut completed: Vec<_> = completed.into_iter().collect();
    completed.sort();
    completed
}

mod execution;
mod resume_preflight;
pub(crate) use execution::{
    execute_data_transfer_impl, execute_data_transfer_impl_with_write_observer,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_transfer::model::{
        Endpoint, TableExecutionOutcome, TableExecutionResult, TableMapping, TransferOptions,
        WriteMode,
    };

    #[test]
    fn unknown_structure_outcome_always_stops_before_data() {
        let results = [TableExecutionResult::database(
            "first",
            "first",
            None,
            TableExecutionOutcome::Unknown,
            Some("lost DDL acknowledgement".into()),
        )];
        assert!(should_stop_after_structure_phase(
            &results, false, true, false
        ));
    }

    #[test]
    fn confirmed_structure_failure_follows_continue_preference() {
        let results = [TableExecutionResult::database(
            "first",
            "first",
            Some(0),
            TableExecutionOutcome::NotStarted,
            Some("preflight rejected table".into()),
        )];
        assert!(!should_stop_after_structure_phase(
            &results, false, true, false
        ));
        assert!(should_stop_after_structure_phase(
            &results, false, true, true
        ));
        assert!(should_stop_after_structure_phase(&[], true, false, false));
    }

    #[test]
    fn partially_applied_preambles_cannot_create_or_replay_resume_tokens() {
        let mut job = TransferJob {
            source: Endpoint {
                db_session_id: "source".into(),
                database: "source_db".into(),
                schema: None,
            },
            target: Some(Endpoint {
                db_session_id: "target".into(),
                database: "target_db".into(),
                schema: None,
            }),
            sql_file_target: None,
            mode: TransferMode::Data,
            write_mode: WriteMode::Insert,
            tables: vec![TableMapping::auto("users")],
            options: TransferOptions::default(),
        };

        assert!(supports_bounded_resume(&job));
        job.write_mode = WriteMode::TruncateInsert;
        assert!(!supports_bounded_resume(&job));
        job.write_mode = WriteMode::DropCreateInsert;
        assert!(!supports_bounded_resume(&job));
        job.write_mode = WriteMode::Insert;
        job.mode = TransferMode::StructureAndData;
        assert!(!supports_bounded_resume(&job));
    }
}
