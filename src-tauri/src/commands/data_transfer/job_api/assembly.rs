//! Turn a stored, reviewed plan into a frozen Job body plus resolved endpoints.
//!
//! The apply Job never carries a plan body: this module re-reads the immutable plan
//! the server issued, re-applies the reviewed selection, re-checks the endpoint
//! contract (driver type/protocol, read-only target, filter and schema fingerprints)
//! and derives the freeze evidence (mapping fingerprint, stable keys, snapshot
//! proof) that the handler re-verifies through `validate_plan`.

use std::collections::HashMap;

use crate::db::{ConnectionHandle, DatabaseDriver};
use datazen_driver_api::{TableSchema, TableType};
use sha2::{Digest, Sha256};

use super::super::exec;
use super::super::inspect::inspect_data_transfer_impl;
use super::super::plans::{self, StoredTransferPlan};
use super::super::AppState;
use super::endpoint_identity::EndpointIdentity;
use crate::commands::error::{CmdExt, CommandError};
use crate::data_transfer::job::{TransferEndpoints, TransferFreezeBody};
use crate::data_transfer::model::{Endpoint, TableInspectResult, TransferRunSelection};
use crate::data_transfer::{
    enforce_transfer_pairing, validate_no_self_table_overwrite, TransferJob, TransferMode,
    WriteMode,
};

/// Everything a `dataTransferPrepare` / `dataTransferApply` Job needs, resolved
/// once on the host side: the Job never resolves endpoints or schemas itself.
pub(crate) struct FreezeAssembly {
    pub(crate) freeze: TransferFreezeBody,
    pub(crate) endpoints: TransferEndpoints,
    /// Inspected source columns, handed to the handler as frozen evidence.
    pub(crate) inspected: Vec<TableInspectResult>,
    pub(crate) source_schemas: HashMap<String, TableSchema>,
    /// Empty for a SQL-file destination and for prepare (no target is written).
    pub(crate) target_schemas: HashMap<String, TableSchema>,
    /// Source relations participating in this run, for the runtime endpoint refs.
    pub(crate) source_objects: Vec<String>,
    /// Target relations participating in this run.
    pub(crate) target_objects: Vec<String>,
    /// Identity of the endpoint this run reads from — the real connection config
    /// its session was opened on, never a placeholder.
    pub(crate) source_identity: EndpointIdentity,
    /// Identity of the endpoint this run writes to, or `None` when the run has no
    /// writable database endpoint (a SQL-file destination). The single condition
    /// deciding this also decides whether a writer endpoint is reserved at all.
    pub(crate) target_identity: Option<EndpointIdentity>,
}

/// Resolve the plan into a frozen body and live endpoints.
pub(crate) async fn assemble(
    state: &AppState,
    plan: &StoredTransferPlan,
    selection: &TransferRunSelection,
    for_apply: bool,
) -> Result<FreezeAssembly, CommandError> {
    let mut job = plan.job.clone();
    exec::validate_selection(&job, selection)?;
    exec::apply_selection(&mut job, selection);
    let (endpoints, inspected, source_schemas, target_schemas, source_identity, target_identity) =
        if job.sql_file_target.is_some() {
            resolve_sql_file(state, plan, &job).await?
        } else {
            resolve_database(state, plan, &job, for_apply).await?
        };
    hydrate_column_mappings(&mut job, &inspected);
    let enabled: Vec<_> = job.tables.iter().filter(|table| table.enabled).collect();
    let source_objects = enabled
        .iter()
        .map(|table| table.source_table.clone())
        .collect();
    let target_objects = enabled
        .iter()
        .map(|table| table.target_table.clone())
        .collect();
    let freeze = derive_freeze(&job, &inspected, &source_schemas, plan);
    Ok(FreezeAssembly {
        freeze,
        endpoints,
        inspected,
        source_schemas,
        target_schemas,
        source_objects,
        target_objects,
        source_identity,
        target_identity,
    })
}

/// Freeze the *resolved* column mapping, not the request shape.
///
/// A review may hand back a table that only names its columns — the frontend
/// builds one with `TableMapping::auto` and lets inspection pair the columns.
/// The inspected rows carry the mapping preview already validated, so an empty
/// mapping is filled from there instead of reaching the handler as "no active
/// column mappings". An explicit mapping is never overwritten: what the user
/// confirmed is what gets frozen.
fn hydrate_column_mappings(job: &mut TransferJob, inspected: &[TableInspectResult]) {
    for table in job.tables.iter_mut().filter(|table| table.enabled) {
        if !table.column_mappings.is_empty() {
            continue;
        }
        if let Some(row) = inspected
            .iter()
            .find(|row| row.source_table == table.source_table)
        {
            table.column_mappings = row.column_mappings.clone();
        }
    }
}

/// Freeze evidence: mapping digest, per-table stable keys, snapshot proof.
fn derive_freeze(
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    plan: &StoredTransferPlan,
) -> TransferFreezeBody {
    let enabled: Vec<_> = job.tables.iter().filter(|table| table.enabled).collect();
    let mapping_fingerprint = mapping_digest(job);
    let stable_key_columns: Vec<Vec<String>> = enabled
        .iter()
        .map(|table| stable_keys(table, inspected, source_schemas))
        .collect();
    let snapshot_proven = enabled.iter().all(|table| {
        source_schemas
            .get(&table.source_table)
            .map(|schema| schema.table_options.supports_consistent_snapshot == Some(true))
            .unwrap_or(false)
    });
    let resumable = snapshot_proven
        && stable_key_columns.iter().all(|keys| !keys.is_empty())
        && job.source.db_session_id
            != job
                .target
                .as_ref()
                .map(|target| target.db_session_id.clone())
                .unwrap_or_default();
    TransferFreezeBody {
        job: job.clone(),
        mapping_fingerprint,
        recovery_policy: if resumable {
            "resumeAfterVerify".to_string()
        } else {
            "forbidAutoResume".to_string()
        },
        has_structure_ir: plan.database_structure.is_some(),
        stable_key_columns,
        snapshot_proven,
    }
}

/// Stable complete key columns per participating table; empty means the table
/// cannot be resumed automatically: a resumed table has to be distinguishable
/// from a fresh copy of the same table, and without its complete key there is
/// nothing to resume *by*.
fn stable_keys(
    table: &crate::data_transfer::model::TableMapping,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
) -> Vec<String> {
    inspected
        .iter()
        .find(|row| row.source_table == table.source_table)
        .filter(|row| !row.source_primary_keys.is_empty())
        .map(|row| row.source_primary_keys.clone())
        .or_else(|| {
            source_schemas
                .get(&table.source_table)
                .map(TableSchema::effective_primary_keys)
        })
        .unwrap_or_default()
}

fn mapping_digest(job: &TransferJob) -> String {
    match serde_json::to_vec(&job.tables) {
        Ok(bytes) => format!("{:x}", Sha256::digest(bytes)),
        // Serializing owned mappings cannot fail in practice; a stable sentinel
        // keeps the freeze verifiable instead of panicking in a production path.
        Err(_) => "sha256:mapping-unavailable".to_string(),
    }
}

/// Database target: reuse the execution-path context validation, then resolve
/// inspection, schemas and adapters for the Job.
async fn resolve_database(
    state: &AppState,
    plan: &StoredTransferPlan,
    job: &TransferJob,
    for_apply: bool,
) -> Result<
    (
        TransferEndpoints,
        Vec<TableInspectResult>,
        HashMap<String, TableSchema>,
        HashMap<String, TableSchema>,
        EndpointIdentity,
        Option<EndpointIdentity>,
    ),
    CommandError,
> {
    let context = exec::validate_plan_context(state, plan).await?;
    // Endpoint identity comes from the two configs this run actually connects
    // over — the same ones `validate_plan_context` just proved connectable.
    // Each endpoint is identified through *its own* driver, because that is what
    // resolves a left-out host/port into the address the run really dials.
    let source_identity = crate::services::migration_endpoint::session_identity(
        &state.connection_manager,
        &job.source.db_session_id,
        Some((&job.source.database, job.source.normalized_schema())),
    )
    .await?;
    let target = job.database_target().map_err(CommandError::from)?;
    let target_identity = crate::services::migration_endpoint::session_identity(
        &state.connection_manager,
        &target.db_session_id,
        Some((&target.database, target.normalized_schema())),
    )
    .await?;
    let pairing = enforce_transfer_pairing(
        &context.src_config.database_type,
        &context.tgt_config.database_type,
    )
    .map_err(CommandError::from)?;
    let inspected = inspect_data_transfer_impl(
        state,
        job.source.db_session_id.clone(),
        target.db_session_id.clone(),
        Some(job.source.database.clone()),
        Some(target.database.clone()),
        job.source.normalized_schema(),
        target.normalized_schema(),
        job.mode,
        &job.tables,
    )
    .await?;
    if matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    ) {
        validate_no_self_table_overwrite(job, &inspected).map_err(CommandError::from)?;
    }
    let mut source_schemas = load_schemas(
        context.src_driver.as_ref(),
        &context.src_handle,
        &job.source,
        "source",
    )
    .await?;
    let target_schemas = if for_apply {
        load_schemas(
            context.tgt_driver.as_ref(),
            &context.tgt_handle,
            target,
            "target",
        )
        .await?
    } else {
        HashMap::new()
    };
    let needs_adapters = !crate::data_transfer::is_same_family(&pairing)
        || matches!(
            job.mode,
            TransferMode::Structure | TransferMode::StructureAndData
        )
        || job.write_mode == WriteMode::DropCreateInsert;
    if needs_adapters {
        let adapters = exec::resolve_transfer_adapters(
            state,
            &context.src_config.database_type,
            &context.tgt_config.database_type,
        )
        .await?;
        crate::data_transfer::structure::enrich_source_types(
            adapters.src_source.as_ref(),
            context.src_driver.as_ref(),
            &context.src_handle,
            &job.source,
            &mut source_schemas,
        )
        .await
        .map_err(CommandError::from)?;
        crate::data_transfer::structure::validate_transfer_column_types(
            job,
            &inspected,
            &source_schemas,
            adapters.src_source.as_ref(),
            adapters.tgt_target.as_ref(),
        )
        .map_err(CommandError::from)?;
    }
    if for_apply
        && matches!(
            job.mode,
            TransferMode::Data | TransferMode::StructureAndData
        )
    {
        // Check the target data transaction path before the pipeline can mutate
        // anything; an unwritable target must fail during admission, not mid-run.
        let probe = context
            .tgt_driver
            .begin_transaction(&context.tgt_handle)
            .await
            .cmd_err("apply_data_transfer_job")?;
        context
            .tgt_driver
            .rollback(probe)
            .await
            .cmd_err("apply_data_transfer_job")?;
    }
    Ok((
        TransferEndpoints::Database {
            source_driver: context.src_driver.clone(),
            source_handle: context.src_handle.clone(),
            target_driver: context.tgt_driver.clone(),
            target_handle: context.tgt_handle.clone(),
            source_type: context.src_config.database_type.clone(),
            target_type: context.tgt_config.database_type.clone(),
        },
        inspected,
        source_schemas,
        target_schemas,
        source_identity,
        Some(target_identity),
    ))
}

/// SQL-file target: no target connection, so only the source snapshot is live.
#[allow(clippy::too_many_lines)]
async fn resolve_sql_file(
    state: &AppState,
    plan: &StoredTransferPlan,
    job: &TransferJob,
) -> Result<
    (
        TransferEndpoints,
        Vec<TableInspectResult>,
        HashMap<String, TableSchema>,
        HashMap<String, TableSchema>,
        EndpointIdentity,
        Option<EndpointIdentity>,
    ),
    CommandError,
> {
    let target = job
        .sql_file_target
        .as_ref()
        .ok_or_else(|| CommandError::Validation("SQL file target is missing".into()))?;
    let destination = crate::data_transfer::sql_file::resolve_path(&target.file_token)
        .map_err(CommandError::from)?;
    if plans::filter_fingerprint(job).map_err(CommandError::from)? != plan.filter {
        return Err(CommandError::Validation(
            "source filter or recordset changed since preview; return to comparison".into(),
        ));
    }
    if plans::target_scope_fingerprint(job).map_err(CommandError::from)?
        != plan.target_scope_fingerprint
    {
        return Err(CommandError::Validation(
            "SQL-file target scope changed since preview; return to comparison".into(),
        ));
    }
    let (driver, handle, mut inspected, mut schemas) =
        exec::load_sql_source_snapshot(state, job).await?;
    if driver.driver_type() != plan.source_driver_type
        || plans::driver_protocol_version(driver.as_ref()) != plan.source_driver_protocol
    {
        return Err(CommandError::Validation(
            "source driver contract changed since preview; return to preview".into(),
        ));
    }
    let target_driver =
        crate::data_transfer::sql_file::resolve_target_driver(driver.clone(), target)
            .map_err(CommandError::from)?;
    crate::data_transfer::sql_file::validate_target_scope_for_driver(
        target_driver.as_ref(),
        target,
    )
    .map_err(CommandError::from)?;
    if target_driver.driver_type() != plan.target_driver_type
        || plans::driver_protocol_version(target_driver.as_ref()) != plan.target_driver_protocol
    {
        return Err(CommandError::Validation(
            "SQL-file target dialect contract changed since preview; return to preview".into(),
        ));
    }
    crate::data_transfer::sql_file::validate_target_dialect_job(job).map_err(CommandError::from)?;
    let src_config = state
        .connection_manager
        .get_session_config(&job.source.db_session_id)
        .await
        .cmd_err("apply_data_transfer_job")?;
    // A SQL-file destination has no connection config of its own, so there is no
    // target identity to reserve against — and therefore no writer endpoint.
    let source_identity = crate::services::migration_endpoint::session_identity(
        &state.connection_manager,
        &job.source.db_session_id,
        Some((&job.source.database, job.source.normalized_schema())),
    )
    .await?;
    let source_type = src_config.database_type.clone();
    let target_type = target_driver.driver_type().to_string();
    let mut adapters = None;
    if let Some(declared) = target.normalized_database_type() {
        state
            .sync_adapters
            .ensure_pair(&source_type, &declared.to_string())
            .map_err(CommandError::Validation)?;
        let src_adapter = state
            .sync_adapters
            .get_source(&source_type)
            .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
        let tgt_adapter = state
            .sync_adapters
            .get_target(&declared.to_string())
            .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;
        adapters = Some((src_adapter, tgt_adapter));
    } else if state.sync_adapters.ensure_type(&source_type).is_ok() {
        // A source-dialect export declares no second dialect to look up, but the
        // renderer still needs the target adapter of the *source* dialect —
        // the same fallback the legacy SQL-file path uses (`preview.rs`).
        // Without it the apply Job refused every source-dialect export with
        // "SQL file target requires both source and target IR adapters".
        adapters = match (
            state.sync_adapters.get_source(&source_type),
            state.sync_adapters.get_target(&source_type),
        ) {
            (Some(source), Some(target)) => Some((source, target)),
            _ => None,
        };
    }
    let source_adapter = adapters.as_ref().map(|(src, _)| src.clone());
    if let Some(adapter) = source_adapter.as_ref() {
        crate::data_transfer::structure::enrich_source_types(
            adapter.as_ref(),
            driver.as_ref(),
            &handle,
            &job.source,
            &mut schemas,
        )
        .await
        .map_err(CommandError::from)?;
        crate::data_transfer::structure::validate_transfer_source_columns(
            job,
            &inspected,
            &schemas,
            adapter.as_ref(),
        )
        .map_err(CommandError::from)?;
    }
    if let Some((src_adapter, tgt_adapter)) = &adapters {
        crate::data_transfer::structure::enrich_create_new_target_types(
            &mut inspected,
            &schemas,
            src_adapter.as_ref(),
            tgt_adapter.as_ref(),
        );
        crate::data_transfer::structure::validate_transfer_column_types(
            job,
            &inspected,
            &schemas,
            src_adapter.as_ref(),
            tgt_adapter.as_ref(),
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
    let endpoints = TransferEndpoints::SqlFile {
        source_driver: driver,
        source_handle: handle,
        source_type: source_type.clone(),
        target_type: target_type.clone(),
        destination,
        structure: plan.sql_file_structure.clone(),
        source_adapter,
        target_adapter: adapters.map(|(_, target)| target),
    };
    Ok((
        endpoints,
        inspected,
        schemas,
        HashMap::new(),
        source_identity,
        None,
    ))
}

/// Load the live schema of every base table in one endpoint's scope.
async fn load_schemas(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    endpoint: &Endpoint,
    side: &str,
) -> Result<HashMap<String, TableSchema>, CommandError> {
    let tables = driver
        .get_tables(handle, &endpoint.database, endpoint.normalized_schema())
        .await
        .cmd_err("apply_data_transfer_job")?;
    let mut schemas = HashMap::new();
    for table in tables
        .into_iter()
        .filter(|table| crate::data_transfer::metadata::table_in_endpoint_schema(endpoint, table))
        .filter(|table| matches!(table.table_type, TableType::Table))
    {
        let schema = crate::data_transfer::metadata::load_table_schema(
            driver,
            handle,
            endpoint,
            &table.name,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "failed to inspect {side} table '{}': {error}",
                table.name
            ))
        })?;
        schemas.insert(table.name.clone(), schema);
    }
    Ok(schemas)
}

/// Drop the helper argument when the platform-only source check is unused.
const _: fn(&AppState) = |state| {
    let _ = state;
};
