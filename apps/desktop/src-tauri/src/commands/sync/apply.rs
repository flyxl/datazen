//! Compare selected tables, generate ChangeSet SQL, and apply.

use datazen::jobs::domain::job::JobState;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::artifact_view;
use super::inspect::inspect_data_sync_impl;
use crate::data_sync::api::SyncComparisonRequest;
use crate::data_sync::api::{generate_write_sql_for_table, WriteSqlContext};
use crate::data_sync::comparison_store::ComparisonStorage;
use crate::data_sync::job::PrepareSpec;
use crate::data_sync::plan_store as plans;
use crate::data_sync::{
    ChangeSet, ComparisonResult, Endpoint, SyncOptions, SyncSourceFilter, TableMapping,
    TableMappingStatus, TableResult,
};
use datazen::services::schema_scope::metadata_schema;
use datazen_driver_api::LegacySqlDialect;
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
    request: SyncComparisonRequest,
) -> Result<plans::SyncComparisonPreview, CommandError> {
    let job_id = request
        .job_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("data-sync-{}", Uuid::new_v4()));
    let accepted = start_data_sync_prepare_job_impl(
        state,
        request.source_db_session_id,
        request.target_db_session_id,
        request.tables,
        job_id,
        request.source_database,
        request.target_database,
        request.source_schema,
        request.target_schema,
        request.options,
        request.source_filters,
        Some(request.mappings),
    )
    .await?;
    let finished = super::jobs::wait_for_durable_terminal(state, accepted.job_id.as_str()).await?;
    if finished.state != JobState::Succeeded {
        return Err(CommandError::Validation(finished.error.unwrap_or_else(
            || "the Data Sync comparison job did not complete; compare again".to_string(),
        )));
    }

    super::job_api::preview_for_durable_job(state, finished.job_id.as_str()).await
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
) -> Result<datazen::jobs::domain::job::JobView, CommandError> {
    let source_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &source_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let target_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let source_config = source_context.config.clone();
    let target_config = target_context.config.clone();
    let source_database_name = crate::data_sync::api::resolve_db_name(
        source_database.as_deref(),
        source_config.database.as_deref(),
    );
    let target_database_name = crate::data_sync::api::resolve_db_name(
        target_database.as_deref(),
        target_config.database.as_deref(),
    );
    crate::data_sync::require_data_sync_family(
        &source_config.database_type,
        &target_config.database_type,
    )?;
    let source_driver = source_context.driver.clone();
    let target_driver = target_context.driver.clone();
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
        db_session_id: sessions.source_id.clone(),
        database: source_database_name.clone(),
        schema: source_schema_name.clone(),
    };
    let target_endpoint = Endpoint {
        db_session_id: sessions.target_id.clone(),
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
        )?,
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
    super::host::state::mark_preview_pending(&prepared_job_id);
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
        super::host::state::mark_preview_finished(&prepared_job_id);
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
    // 一次调用只取一次作用域：比较存储与计划注册表必须落在同一个组织维度上。
    let scope = state.organization_scope()?;
    let comparison = ComparisonStorage::from_comparison(
        rebuilt,
        state.sync_stores.comparison_root.as_ref(),
        &scope,
    )
    .map_err(CommandError::Validation)?;
    let source_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        source_job_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let target_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        target_job_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let source_driver = source_context.driver.clone();
    let source_handle = source_context.handle.clone();
    let target_driver = target_context.driver.clone();
    let target_handle = target_context.handle.clone();
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
    state
        .sync_stores
        .plan_store
        .issue(
            &scope,
            source_driver.as_ref(),
            target_driver.as_ref(),
            plans::SyncPlanIssue {
                plan_id: Some(plan_id.to_string()),
                selection_revision: 1,
                source_db_session_id: source_plan_session_id.to_string(),
                target_db_session_id: target_plan_session_id.to_string(),
                source_database: source_database.to_string(),
                target_database: target_database.to_string(),
                source_schema: source_schema.map(str::to_string),
                target_schema: target_schema.map(str::to_string),
                source_schema_fingerprint: source_fingerprint,
                target_schema_fingerprint: target_fingerprint,
                comparison,
                options,
                target_read_only_at_preview: target_read_only,
            },
        )
        .map_err(CommandError::Validation)?;
    state
        .sync_stores
        .plan_store
        .attach_endpoint_identities(&scope, plan_id, source_identity, target_identity)
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

/// Generate the reviewed ChangeSet's DML for the SQL preview command.
///
/// 宿主只负责：会话/适配器解析、逐表读 schema、投影查找；装配本身在
/// `data_sync::api::generate_write_sql_for_table`（F4c-1），与复核批次共用。
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

    let target_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let tgt_config = target_context.config.clone();
    let family = crate::data_sync::require_data_sync_family(
        &tgt_config.database_type,
        &tgt_config.database_type,
    )?;
    let tgt_driver = target_context.driver.clone();
    // 占位符规则来自 SqlDialect 契约，旧驱动 API 逐条转发。
    let dialect = LegacySqlDialect::new(tgt_driver.as_ref());
    let tgt_handle = target_context.handle.clone();
    let target_database_name = crate::data_sync::api::resolve_db_name(
        target_database.as_deref(),
        tgt_config.database.as_deref(),
    );
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
        let projection = comparison
            .tables
            .iter()
            .find(|t| t.source_table == table.source_table && t.target_table == table.target_table)
            .ok_or_else(|| {
                CommandError::Validation("comparison projection missing; compare again".into())
            })?;
        let ctx = WriteSqlContext {
            family: &family,
            database: &target_database_name,
            schema: target_schema.as_deref(),
            schema_obj: &schema,
            preview_source: preview_source.as_ref(),
            preview_target: preview_target.as_ref(),
            dialect: &dialect,
            identity_insert_requires_session_toggle: tgt_driver
                .explicit_identity_insert_requires_session_toggle(),
        };
        // sqlserver 预检、投影复验、preview 字面量、限定名与 identity 盖章
        // 全部在 modules 的同一份装配里（F4c-1），与复核批次路径共用。
        let stmts = generate_write_sql_for_table(
            &ctx,
            table,
            &projection.columns,
            &projection.column_types,
            &projection.primary_keys,
            options.conflict_policy,
        )
        .map_err(CommandError::from)?;
        statements.extend(stmts);
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
