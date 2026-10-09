//! Compare selected tables, generate ChangeSet SQL, and apply.

use datazen_platform_api::dto::job::JobState;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::artifact_view;
use super::comparison_store::ComparisonStore;
use super::inspect::inspect_data_sync_impl;
use super::plans;
use crate::data_sync::job::PrepareSpec;
use crate::data_sync::{
    generate_table_sql_with_qualified_table_and_policy, ChangeOperation, ChangeSet,
    ComparisonResult, ConflictPolicy, DataSyncError, Endpoint, SyncOptions, SyncSourceFilter,
    TableChangeSet, TableMapping, TableMappingStatus, TableResult,
};
use crate::services::metadata_schema;
use futures_util::FutureExt;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use uuid::Uuid;

/// Compare: statement-only pre-flight, then a `dataSyncPrepare` Job freezes the
/// ChangeSet that the returned planId points at.
///
/// The read snapshot belongs to that Job — `prepare_collect` opens one per side
/// and rolls both back — and deliberately **not** to this command: a second
/// snapshot on the same two db sessions is refused by the driver, and even where
/// it is allowed the frozen rows would be read outside the snapshot the
/// ChangeSet is attributed to.
pub(crate) async fn compare_data_sync_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: SyncOptions,
    mappings: &[TableMapping],
    source_filters: &HashMap<String, SyncSourceFilter>,
) -> Result<plans::SyncComparisonPreview, CommandError> {
    let job_id = job_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("data-sync-{}", Uuid::new_v4()));
    let accepted = start_data_sync_prepare_job_impl(
        state,
        source_db_session_id,
        target_db_session_id,
        tables,
        job_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        options,
        source_filters.clone(),
        Some(mappings.to_vec()),
    )
    .await?;
    let finished = super::jobs::wait_for_durable_terminal(state, accepted.job_id.as_str()).await?;
    if finished.state != JobState::Succeeded {
        return Err(CommandError::Validation(finished.error.unwrap_or_else(
            || "the Data Sync comparison job did not complete; compare again".to_string(),
        )));
    }

    let mut last_error = None;
    for _ in 0..40 {
        match super::job_api::preview_for_durable_job(state, finished.job_id.as_str()).await {
            Ok(preview) => return Ok(preview),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        CommandError::Validation("the comparison completed without a review plan".into())
    }))
}

/// Accept a durable prepare job, then return its id while a detached dispatcher
/// performs the row comparison and builds the process-local review projection.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_data_sync_prepare_job_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    job_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: SyncOptions,
    source_filters: HashMap<String, SyncSourceFilter>,
    mapping_overrides: Option<Vec<TableMapping>>,
) -> Result<datazen_platform_api::dto::job::JobView, CommandError> {
    let source_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("start_data_sync_prepare_job")?;
    let target_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("start_data_sync_prepare_job")?;
    let source_database_name = super::types::resolve_db_name(
        source_database.as_deref(),
        source_config.database.as_deref(),
    );
    let target_database_name = super::types::resolve_db_name(
        target_database.as_deref(),
        target_config.database.as_deref(),
    );
    crate::data_sync::require_data_sync_family(
        &source_config.database_type,
        &target_config.database_type,
    )?;
    let (source_driver, _) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("start_data_sync_prepare_job")?;
    let (target_driver, _) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("start_data_sync_prepare_job")?;
    let source_schema_name = metadata_schema(
        source_driver.as_ref(),
        source_schema.as_deref(),
        None,
        source_config.schema.as_deref(),
    );
    let target_schema_name = metadata_schema(
        target_driver.as_ref(),
        target_schema.as_deref(),
        None,
        target_config.schema.as_deref(),
    );

    let sessions = super::job_api::own_sessions(
        state,
        &source_db_session_id,
        &target_db_session_id,
        &source_database_name,
        &target_database_name,
        source_schema_name.as_deref(),
        target_schema_name.as_deref(),
    )
    .await?;
    let mapping_overrides = mapping_overrides.unwrap_or_default();
    let preflight = async {
        let (source_driver, _) = state
            .connection_manager
            .get_session(&sessions.source_id)
            .await
            .cmd_err("start_data_sync_prepare_job")?;
        let (target_driver, _) = state
            .connection_manager
            .get_session(&sessions.target_id)
            .await
            .cmd_err("start_data_sync_prepare_job")?;
        let inspected = inspect_data_sync_impl(
            state,
            sessions.source_id.clone(),
            sessions.target_id.clone(),
            Some(source_database_name.clone()),
            Some(target_database_name.clone()),
            source_schema_name.clone(),
            target_schema_name.clone(),
            &mapping_overrides,
        )
        .await?;
        state
            .sync_adapters
            .ensure_pair(&source_config.database_type, &target_config.database_type)
            .map_err(CommandError::Validation)?;
        if state
            .sync_adapters
            .get_source(&source_config.database_type)
            .is_none()
            || state
                .sync_adapters
                .get_source(&target_config.database_type)
                .is_none()
        {
            return Err(CommandError::Validation(
                "a Data Sync endpoint has no compatible key contract".into(),
            ));
        }
        options.validate().map_err(CommandError::from)?;
        let wanted = tables.into_iter().collect::<std::collections::HashSet<_>>();
        let order = inspected
            .iter()
            .map(|table| (table.source_table.clone(), table.target_table.clone()))
            .collect::<Vec<_>>();
        let selected = inspected
            .iter()
            .filter(|table| {
                table.status == TableMappingStatus::Matched
                    && (wanted.is_empty() || wanted.contains(&table.source_table))
            })
            .map(|table| (table.source_table.clone(), table.target_table.clone()))
            .collect::<Vec<_>>();
        let skipped = inspected
            .into_iter()
            .filter(|table| {
                table.status != TableMappingStatus::Matched
                    || (!wanted.is_empty() && !wanted.contains(&table.source_table))
            })
            .collect::<Vec<_>>();
        let mappings = selected
            .iter()
            .map(|pair| {
                let mut mapping = mapping_overrides
                    .iter()
                    .find(|mapping| {
                        mapping.enabled
                            && mapping.source_table == pair.0
                            && mapping.target_table == pair.1
                    })
                    .cloned()
                    .unwrap_or_else(|| mapping_of(pair, &source_filters));
                if let Some(filter) = source_filters.get(&pair.0) {
                    mapping.source_filter = Some(filter.clone());
                }
                mapping
            })
            .collect::<Vec<_>>();
        Ok::<_, CommandError>((
            order,
            selected,
            skipped,
            mappings,
            source_driver,
            target_driver,
        ))
    }
    .await;
    let (order, _selected, skipped, mappings, _source_driver, _target_driver) = match preflight {
        Ok(value) => value,
        Err(error) => {
            super::job_api::release_sessions(state, &sessions.ids()).await;
            return Err(error);
        }
    };

    let plan_id = Uuid::new_v4().to_string();
    let source_endpoint = Endpoint {
        connection_id: sessions.source_id.clone(),
        database: source_database_name.clone(),
        schema: source_schema_name.clone(),
    };
    let target_endpoint = Endpoint {
        connection_id: sessions.target_id.clone(),
        database: target_database_name.clone(),
        schema: target_schema_name.clone(),
    };
    let spec = PrepareSpec {
        source: source_endpoint.clone(),
        target: target_endpoint.clone(),
        mappings,
        options: options.clone(),
        filters: source_filters,
        plan_id: Some(plan_id.clone()),
    };
    let handler = Arc::new(crate::data_sync::job::DataSyncHandler::for_prepare(
        spec,
        super::jobs::recording_host(
            state,
            source_endpoint,
            target_endpoint,
            HashMap::new(),
            &job_id,
        ),
    ));
    let endpoints = sessions.endpoints();
    let payload = serde_json::json!({
        "planVersion": crate::data_sync::job::body::PLAN_VERSION,
        "handlerVersion": crate::data_sync::job::body::HANDLER_VERSION,
        "checkpointVersion": crate::data_sync::job::body::CHECKPOINT_VERSION,
        "planId": plan_id.clone(),
        "recoveryTargets": sessions.recovery_targets(),
        "recoveryPolicy": "verify-only",
    });
    let admission = match super::jobs::admit_durable(
        state,
        &job_id,
        super::jobs::PREPARE_KIND,
        payload,
        &format!("data-sync-prepare:{job_id}"),
        &endpoints,
    )
    .await
    {
        Ok(admission) => admission,
        Err(error) => {
            super::job_api::release_sessions(state, &sessions.ids()).await;
            return Err(error);
        }
    };
    if !admission.dispatch {
        super::job_api::release_sessions(state, &sessions.ids()).await;
        return Ok(admission.view);
    }

    let background_state = state.clone();
    let owned_ids = sessions.ids();
    let owned_source_id = sessions.source_id.clone();
    let owned_target_id = sessions.target_id.clone();
    let source_identity = crate::services::migration_endpoint::EndpointIdentity {
        connection_id: sessions.source_endpoint.connection_id.clone(),
        service_key: sessions.source_endpoint.service_key.clone(),
    };
    let target_identity = crate::services::migration_endpoint::EndpointIdentity {
        connection_id: sessions.target_endpoint.connection_id.clone(),
        service_key: sessions.target_endpoint.service_key.clone(),
    };
    let prepared_job_id = admission.job_id.as_str().to_string();
    let dispatch_endpoints = endpoints.clone();
    let accepted_view = admission.view.clone();
    tauri::async_runtime::spawn(async move {
        let work = AssertUnwindSafe(async {
            super::jobs::dispatch_durable(
                &background_state,
                &admission,
                &dispatch_endpoints,
                handler,
            )
            .await;
            match super::jobs::read_durable_job(&background_state, &prepared_job_id).await {
                Ok(view) if view.state == JobState::Succeeded => {
                    if finish_async_prepare_preview(
                        &background_state,
                        &prepared_job_id,
                        &plan_id,
                        &source_db_session_id,
                        &target_db_session_id,
                        &owned_source_id,
                        &owned_target_id,
                        &source_database_name,
                        &target_database_name,
                        source_schema_name.as_deref(),
                        target_schema_name.as_deref(),
                        options,
                        skipped,
                        order,
                        target_config.read_only,
                        source_identity,
                        target_identity,
                    )
                    .await
                    .is_err()
                    {
                        super::host::state::record_failure(
                            &prepared_job_id,
                            "preparePreviewUnavailable",
                        );
                    }
                }
                _ => {}
            }
        })
        .catch_unwind()
        .await;
        if work.is_err() {
            super::host::state::record_failure(&prepared_job_id, "prepareWorkerPanicked");
        }
        super::job_api::release_sessions(&background_state, &owned_ids).await;
    });
    Ok(accepted_view)
}

#[allow(clippy::too_many_arguments)]
async fn finish_async_prepare_preview(
    state: &AppState,
    job_id: &str,
    plan_id: &str,
    source_plan_session_id: &str,
    target_plan_session_id: &str,
    source_job_session_id: &str,
    target_job_session_id: &str,
    source_database: &str,
    target_database: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    options: SyncOptions,
    skipped: Vec<TableResult>,
    order: Vec<(String, String)>,
    target_read_only: bool,
    source_identity: crate::services::migration_endpoint::EndpointIdentity,
    target_identity: crate::services::migration_endpoint::EndpointIdentity,
) -> Result<(), CommandError> {
    let artifact = super::host::state::load_artifact(plan_id).ok_or_else(|| {
        CommandError::Validation("the prepare job produced no frozen ChangeSet".into())
    })?;
    let mut rebuilt = artifact_view::comparison_from_artifact(&artifact);
    rebuilt.tables.extend(skipped);
    rebuilt.tables.sort_by_key(|table| {
        order
            .iter()
            .position(|(source, target)| {
                *source == table.source_table && *target == table.target_table
            })
            .unwrap_or(usize::MAX)
    });
    let comparison = ComparisonStore::from_comparison(rebuilt).map_err(CommandError::Validation)?;
    let (source_driver, source_handle) = state
        .connection_manager
        .get_session(source_job_session_id)
        .await
        .cmd_err("finish_data_sync_prepare_job")?;
    let (target_driver, target_handle) = state
        .connection_manager
        .get_session(target_job_session_id)
        .await
        .cmd_err("finish_data_sync_prepare_job")?;
    let mut source_entries = Vec::new();
    let mut target_entries = Vec::new();
    for table in comparison
        .summaries()
        .map_err(CommandError::Validation)?
        .into_iter()
        .filter(|table| table.table.status == TableMappingStatus::Matched)
    {
        let source_schema_snapshot = source_driver
            .get_table_schema(
                &source_handle,
                &table.table.source_table,
                source_database,
                source_schema,
            )
            .await
            .cmd_err("finish_data_sync_prepare_job")?;
        let target_schema_snapshot = target_driver
            .get_table_schema(
                &target_handle,
                &table.table.target_table,
                target_database,
                target_schema,
            )
            .await
            .cmd_err("finish_data_sync_prepare_job")?;
        source_entries.push((
            table.table.source_table.clone(),
            Some(source_schema_snapshot),
            table.table.source_filter.clone(),
        ));
        target_entries.push((
            table.table.target_table.clone(),
            Some(target_schema_snapshot),
            table.table.source_filter.clone(),
        ));
    }
    let source_fingerprint =
        plans::fingerprint_relations_with_filters(source_database, source_schema, source_entries)
            .map_err(CommandError::Validation)?;
    let target_fingerprint =
        plans::fingerprint_relations_with_filters(target_database, target_schema, target_entries)
            .map_err(CommandError::Validation)?;
    plans::issue_plan_with_store_and_id(
        plan_id.to_string(),
        source_plan_session_id.to_string(),
        target_plan_session_id.to_string(),
        source_database.to_string(),
        target_database.to_string(),
        source_schema.map(str::to_string),
        target_schema.map(str::to_string),
        source_driver.as_ref(),
        target_driver.as_ref(),
        source_fingerprint,
        target_fingerprint,
        comparison,
        options,
        target_read_only,
    )
    .map_err(CommandError::Validation)?;
    plans::attach_endpoint_identities(plan_id, source_identity, target_identity)
        .map_err(CommandError::Validation)?;
    tracing::debug!(job_id, "Data Sync review plan is ready");
    Ok(())
}

/// Project one already-inspected matched table back into the frozen
/// `TableMapping` carried by the prepare Job.
///
/// `inspect_data_sync_impl` already resolved structure/PK compatibility, so the
/// mapping only has to name the pair and repeat the structured filter the
/// caller asked for; column matching is re-derived from the live schemas inside
/// the Job's `mapped_table_schemas` stage.
fn mapping_of(
    table: &(String, String),
    source_filters: &HashMap<String, SyncSourceFilter>,
) -> TableMapping {
    TableMapping {
        source_table: table.0.clone(),
        target_table: table.1.clone(),
        enabled: true,
        matching_columns: Vec::new(),
        source_filter: source_filters.get(&table.0).cloned(),
    }
}

pub(super) fn resolve_projection_types(
    projection: &TableResult,
    schema: &datazen_driver_api::TableSchema,
    family: &str,
) -> Result<Vec<String>, CommandError> {
    let columns = &projection.columns;
    let unique: std::collections::HashSet<_> = columns.iter().collect();
    if columns.is_empty()
        || unique.len() != columns.len()
        || columns.len() != schema.columns.len()
        || projection.column_types.len() != columns.len()
        || projection.primary_keys != schema.effective_primary_keys()
    {
        return Err(CommandError::Validation(
            "comparison projection is stale or missing; compare again".into(),
        ));
    }
    columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let column = schema
                .columns
                .iter()
                .find(|c| &c.name == name)
                .ok_or_else(|| {
                    CommandError::Validation(format!("target column {name} changed; compare again"))
                })?;
            if !crate::data_sync::types_eq::types_equivalent(
                family,
                &projection.column_types[index],
                &column.data_type,
            ) {
                return Err(CommandError::Validation(format!(
                    "target type for {name} changed; compare again"
                )));
            }
            Ok(column.data_type.clone())
        })
        .collect()
}

pub(super) fn sqlserver_write_preflight(
    schema: &datazen_driver_api::TableSchema,
    changes: &TableChangeSet,
    conflict_policy: ConflictPolicy,
    target_supports_explicit_identity: bool,
) -> Result<(), String> {
    let has_insert = changes
        .changes
        .iter()
        .any(|change| change.operation == ChangeOperation::Insert);
    if has_insert && !target_supports_explicit_identity {
        if let Some(column) = schema
            .columns
            .iter()
            .find(|column| column.is_auto_increment)
        {
            return Err(format!(
                "SQL Server Data Sync cannot insert selected rows into identity column '{}' because scoped IDENTITY_INSERT support is unavailable",
                column.name
            ));
        }
    }

    let needs_optimistic_condition = conflict_policy != ConflictPolicy::Force
        && changes.changes.iter().any(|change| {
            matches!(
                change.operation,
                ChangeOperation::Update | ChangeOperation::Delete
            )
        });
    if needs_optimistic_condition {
        let primary_keys = schema.effective_primary_keys();
        for column in &schema.columns {
            if primary_keys.iter().any(|key| key == &column.name) {
                continue;
            }
            let base_type = column
                .data_type
                .trim()
                .split(['(', ' ', ','])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(
                base_type.as_str(),
                "char" | "nchar" | "varchar" | "nvarchar" | "text" | "ntext"
            ) {
                return Err(format!(
                    "SQL Server optimistic write conditions cannot verify column '{}' exactly because collation and trailing-space comparison semantics are unavailable; choose an explicit conflict policy that does not compare the prior row or use a table without this column type",
                    column.name
                ));
            }
            if matches!(
                base_type.as_str(),
                "xml" | "image" | "geography" | "geometry" | "hierarchyid" | "sql_variant"
            ) {
                return Err(format!(
                    "SQL Server optimistic write conditions do not support equality for column '{}' of type '{}'; refusing to generate UPDATE/DELETE SQL",
                    column.name, column.data_type
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn identity_insert_target_for_projection(
    family: &str,
    schema: &datazen_driver_api::TableSchema,
    projection_columns: &[String],
    database: &str,
    target_schema: Option<&str>,
    table: &str,
    supports_explicit_values: bool,
    requires_session_toggle: bool,
) -> Option<crate::data_sync::IdentityInsertTarget> {
    if !family.eq_ignore_ascii_case("sqlserver")
        || !supports_explicit_values
        || !requires_session_toggle
    {
        return None;
    }
    let has_projected_identity = schema.columns.iter().any(|column| {
        column.is_auto_increment && projection_columns.iter().any(|name| name == &column.name)
    });
    has_projected_identity.then(|| crate::data_sync::IdentityInsertTarget {
        database: database.to_string(),
        schema: target_schema.map(str::to_string),
        table: table.to_string(),
    })
}

pub(super) fn validate_projected_identity_values(
    schema: &datazen_driver_api::TableSchema,
    projection_columns: &[String],
    changes: &TableChangeSet,
) -> Result<(), String> {
    let identities = schema
        .columns
        .iter()
        .filter(|column| {
            column.is_auto_increment && projection_columns.iter().any(|name| name == &column.name)
        })
        .filter_map(|column| {
            projection_columns
                .iter()
                .position(|name| name == &column.name)
                .map(|index| (column.name.as_str(), index))
        })
        .collect::<Vec<_>>();
    for (column, index) in identities {
        for change in changes
            .changes
            .iter()
            .filter(|change| change.operation == ChangeOperation::Insert)
        {
            let value = change
                .source_row
                .as_ref()
                .and_then(|row| row.get(index))
                .and_then(Option::as_ref);
            if value.is_none() || matches!(value, Some(datazen_driver_api::Value::Null)) {
                return Err(format!(
                    "SQL Server Data Sync cannot safely insert selected rows because identity column '{column}' has a missing or NULL source value"
                ));
            }
        }
    }
    Ok(())
}

pub(crate) async fn generate_data_sync_sql_impl(
    state: &AppState,
    target_db_session_id: String,
    tables: Vec<TableResult>,
    options: SyncOptions,
    target_database: Option<String>,
    target_schema: Option<String>,
) -> Result<Vec<crate::data_sync::SqlStatement>, CommandError> {
    options.validate().map_err(CommandError::from)?;
    let comparison = ComparisonResult::new(tables);
    let set = ChangeSet::from_comparison("ui-preview", &comparison, &options);
    set.validate_executable().map_err(CommandError::from)?;

    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("generate_data_sync_sql")?;
    let family = crate::data_sync::require_data_sync_family(
        &tgt_config.database_type,
        &tgt_config.database_type,
    )?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("generate_data_sync_sql")?;
    let target_database_name =
        super::types::resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());
    let target_schema = metadata_schema(
        tgt_driver.as_ref(),
        target_schema.as_deref(),
        None,
        tgt_config.schema.as_deref(),
    );
    state
        .sync_adapters
        .ensure_type(&tgt_config.database_type)
        .map_err(CommandError::Validation)?;
    let preview_source = state
        .sync_adapters
        .get_source(&tgt_config.database_type)
        .ok_or_else(|| {
            CommandError::Validation("target driver has no SQL preview type adapter".into())
        })?;
    let preview_target = state
        .sync_adapters
        .get_target(&tgt_config.database_type)
        .ok_or_else(|| {
            CommandError::Validation("target driver has no SQL preview literal renderer".into())
        })?;

    let mut statements = Vec::new();
    for table in &set.tables {
        let schema = tgt_driver
            .get_table_schema(
                &tgt_handle,
                &table.target_table,
                &target_database_name,
                target_schema.as_deref(),
            )
            .await
            .cmd_err("generate_data_sync_sql")?;
        if family == "sqlserver" {
            sqlserver_write_preflight(
                &schema,
                table,
                options.conflict_policy,
                preview_target.supports_explicit_identity_values(),
            )
            .map_err(CommandError::Validation)?;
        }
        let projection = comparison
            .tables
            .iter()
            .find(|t| t.source_table == table.source_table && t.target_table == table.target_table)
            .ok_or_else(|| {
                CommandError::Validation("comparison projection missing; compare again".into())
            })?;
        let column_names = &projection.columns;
        if family == "sqlserver" && preview_target.supports_explicit_identity_values() {
            validate_projected_identity_values(&schema, column_names, table)
                .map_err(CommandError::Validation)?;
        }
        let column_types = resolve_projection_types(projection, &schema, &family)?;
        let pk = &projection.primary_keys;
        let preview_types = schema
            .columns
            .iter()
            .map(|column| {
                let ir = preview_source.column_to_ir(column, Some(&column.data_type));
                (column.name.clone(), ir.ir_type)
            })
            .collect::<std::collections::HashMap<_, _>>();
        let preview_literal = |column: &str,
                               value: &Option<datazen_driver_api::Value>,
                               _data_type: Option<&str>|
         -> Result<String, DataSyncError> {
            let ir_type = preview_types.get(column).ok_or_else(|| {
                DataSyncError::validation(format!(
                    "target preview metadata for column '{column}' is missing; compare again"
                ))
            })?;
            Ok(preview_target.format_literal(value, ir_type))
        };
        let qualified_table = preview_target.qualify_relation(
            &target_database_name,
            target_schema.as_deref(),
            &table.target_table,
        );
        let stmts = generate_table_sql_with_qualified_table_and_policy(
            table,
            &qualified_table,
            pk,
            column_names,
            &column_types,
            |name| preview_target.quote_ident(name),
            |index, data_type| {
                tgt_driver
                    .parameter_placeholder(index, data_type)
                    .map_err(|error| DataSyncError::validation(error.to_string()))
            },
            options.conflict_policy,
            preview_literal,
        )
        .map_err(CommandError::from)?;
        let has_insert = table
            .changes
            .iter()
            .any(|change| change.operation == ChangeOperation::Insert);
        let identity_insert_target = has_insert
            .then(|| {
                identity_insert_target_for_projection(
                    &family,
                    &schema,
                    column_names,
                    &target_database_name,
                    target_schema.as_deref(),
                    &table.target_table,
                    preview_target.supports_explicit_identity_values(),
                    tgt_driver.explicit_identity_insert_requires_session_toggle(),
                )
            })
            .flatten();
        statements.extend(stmts.into_iter().map(|mut statement| {
            if statement.operation == ChangeOperation::Insert {
                statement.identity_insert = identity_insert_target.clone();
            }
            statement
        }));
    }
    Ok(statements)
}

pub(crate) async fn apply_data_sync_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: SyncOptions,
) -> Result<crate::data_sync::ExecutionResult, CommandError> {
    let _ = (
        state,
        source_db_session_id,
        target_db_session_id,
        tables,
        job_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        options,
    );
    Err(CommandError::Validation(
        "legacy apply has no reviewed plan; compare and submit the reviewed selection as a durable Data Sync job".into(),
    ))
}

/// Re-run inspect gates for selected tables; returns stale table names when structure/PK drifted.
pub(crate) async fn revalidate_data_sync_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<serde_json::Value, CommandError> {
    let inspected = inspect_data_sync_impl(
        state,
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        &[],
    )
    .await?;
    let wanted: std::collections::HashSet<String> = tables.into_iter().collect();
    let mut stale = Vec::new();
    for row in inspected {
        if !wanted.is_empty()
            && !wanted.contains(&row.source_table)
            && !wanted.contains(&row.target_table)
        {
            continue;
        }
        if row.status != TableMappingStatus::Matched {
            stale.push(serde_json::json!({
                "sourceTable": row.source_table,
                "targetTable": row.target_table,
                "status": row.status,
                "reason": row.incompatible_reason,
            }));
        }
    }
    Ok(serde_json::json!({
        "ok": stale.is_empty(),
        "staleTables": stale,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        identity_insert_target_for_projection, sqlserver_write_preflight,
        validate_projected_identity_values,
    };
    use crate::commands::sync::types::{resolve_options, SyncOptionsInput};
    use crate::data_sync::{ChangeOperation, ConflictPolicy, RowChange, TableChangeSet};
    use datazen_driver_api::{ColumnSchema, TableSchema, Value};

    #[test]
    fn sqlserver_write_preflight_rejects_identity_inserts_without_session_support() {
        let schema = test_sqlserver_schema(true);
        let changes = TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange {
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(1)],
                source_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("x".into())),
                ]),
                target_row: None,
                changed_columns: Vec::new(),
                selected: true,
            }],
        };
        let error = sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Abort, false)
            .expect_err("IDENTITY_INSERT must not be silently skipped");
        assert!(error.contains("IDENTITY_INSERT"), "{error}");
    }

    #[test]
    fn identity_insert_runtime_target_requires_projected_identity_and_session_toggle() {
        let schema = test_sqlserver_schema(true);
        let projection = vec!["id".to_string(), "label".to_string()];
        let target = identity_insert_target_for_projection(
            "sqlserver",
            &schema,
            &projection,
            "target_db",
            Some("dbo"),
            "target",
            true,
            true,
        )
        .expect("projected identity insert must carry its runtime session target");
        assert_eq!(target.database, "target_db");
        assert_eq!(target.schema.as_deref(), Some("dbo"));
        assert_eq!(target.table, "target");

        assert!(identity_insert_target_for_projection(
            "sqlserver",
            &schema,
            &["label".into()],
            "target_db",
            Some("dbo"),
            "target",
            true,
            true,
        )
        .is_none());
        assert!(identity_insert_target_for_projection(
            "sqlserver",
            &schema,
            &projection,
            "target_db",
            Some("dbo"),
            "target",
            true,
            false,
        )
        .is_none());
    }

    #[test]
    fn explicit_identity_insert_rejects_missing_or_null_source_identity_values() {
        let schema = test_sqlserver_schema(true);
        let projection = vec!["id".to_string(), "label".to_string()];
        let changes = |id: Option<Value>| TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange {
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(1)],
                source_row: Some(vec![id, Some(Value::String("x".into()))]),
                target_row: None,
                changed_columns: Vec::new(),
                selected: true,
            }],
        };
        assert!(
            validate_projected_identity_values(&schema, &projection, &changes(None))
                .unwrap_err()
                .contains("missing or NULL")
        );
        assert!(validate_projected_identity_values(
            &schema,
            &projection,
            &changes(Some(Value::Null))
        )
        .unwrap_err()
        .contains("missing or NULL"));
        assert!(validate_projected_identity_values(
            &schema,
            &projection,
            &changes(Some(Value::Integer(1)))
        )
        .is_ok());
        assert!(
            validate_projected_identity_values(&schema, &["label".into()], &changes(None)).is_ok()
        );
    }

    #[test]
    fn sqlserver_write_preflight_rejects_unverifiable_string_conditions() {
        let schema = test_sqlserver_schema(false);
        let changes = TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange {
                operation: ChangeOperation::Update,
                key: vec![Value::Integer(1)],
                source_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("new".into())),
                ]),
                target_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("old".into())),
                ]),
                changed_columns: vec!["label".into()],
                selected: true,
            }],
        };
        let error = sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Abort, false)
            .expect_err("case/trailing-space changes require exact SQL equality");
        assert!(error.contains("collation and trailing-space"), "{error}");
        assert!(
            sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Force, false,).is_ok()
        );
    }

    fn test_sqlserver_schema(identity: bool) -> TableSchema {
        TableSchema {
            table_name: "target".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: identity,
                },
                ColumnSchema {
                    name: "label".into(),
                    data_type: "nvarchar(64)".into(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    #[test]
    fn sync_options_input_overrides_defaults() {
        let input = SyncOptionsInput {
            insert: Some(false),
            update: Some(true),
            delete: Some(true),
            matching_strategy: None,
            batch_size: Some(50),
            large_value_mode: None,
            conflict_policy: None,
        };
        let opts = resolve_options(Some(input));
        assert!(!opts.insert);
        assert!(opts.update);
        assert!(opts.delete);
        assert_eq!(opts.batch_size, 50);
    }
    #[test]
    fn reordered_target_columns_keep_canonical_values_and_reject_drift() {
        use crate::data_sync::{
            generate_table_sql, ChangeSet, ComparisonResult, RowChange, SyncOptions, TableResult,
        };
        use datazen_driver_api::{ColumnSchema, TableSchema, Value};
        let opts = SyncOptions::default();
        let mut result = TableResult::matched(
            "source",
            "target",
            vec![RowChange::update(
                vec![Value::Integer(1)],
                vec![
                    Some(Value::Integer(1)),
                    Some(Value::Integer(10)),
                    Some(Value::Integer(20)),
                ],
                vec![
                    Some(Value::Integer(1)),
                    Some(Value::Integer(9)),
                    Some(Value::Integer(20)),
                ],
                vec!["a".into()],
                &opts,
            )],
        );
        result.columns = vec!["id".into(), "a".into(), "b".into()];
        result.column_types = vec!["INT".into(); 3];
        result.primary_keys = vec!["id".into()];
        let mut schema = TableSchema {
            table_name: "target".into(),
            columns: ["id", "b", "a"]
                .iter()
                .map(|name| ColumnSchema {
                    name: (*name).into(),
                    data_type: "INT".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: *name == "id",
                    is_auto_increment: false,
                })
                .collect(),
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let types = super::resolve_projection_types(&result, &schema, "mysql").unwrap();
        let set =
            ChangeSet::from_comparison("t", &ComparisonResult::new(vec![result.clone()]), &opts);
        let statements = generate_table_sql(
            &set.tables[0],
            None,
            &result.primary_keys,
            &result.columns,
            &types,
            |name| format!("`{name}`"),
            |_, _| "?".into(),
        )
        .unwrap();
        assert!(matches!(statements[0].parameters[0], Value::Integer(10)));
        assert_eq!(
            statements[0].sql,
            "UPDATE `target` SET `a` = ? WHERE `id` = ? AND (`a` = ? OR (`a` IS NULL AND ? IS NULL)) AND (`b` = ? OR (`b` IS NULL AND ? IS NULL))"
        );
        schema.primary_keys = vec!["b".into()];
        assert!(super::resolve_projection_types(&result, &schema, "mysql").is_err());
        schema.primary_keys = vec!["id".into()];
        schema.columns[2].data_type = "TEXT".into();
        assert!(super::resolve_projection_types(&result, &schema, "mysql").is_err());
        result.columns.clear();
        assert!(super::resolve_projection_types(&result, &schema, "mysql").is_err());
    }
}
