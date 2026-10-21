//! Table-level Schema Diff plan preparation (source = desired -> target).

use super::job;
use super::{
    ensure_distinct_schema_scope, fetch_target_table_dependency_catalog, fetch_target_table_schema,
    is_source_unbounded_text, normalize_dialect, resolve_profile_table, schema_catalog_database,
    schema_catalog_scope, uses_schema_scope, AppState, CmdExt, ColumnTypeOverride, CommandError,
    PlanOptions, SchemaDiffPlan, State, SyncSourceAdapter, SyncTargetAdapter,
};
use crate::db::{CatalogReader, LegacyCatalogReader};
use std::collections::HashSet;
use std::sync::Arc;

/// Prepare a DDL deploy plan (source = desired → target).
#[tauri::command]
pub async fn prepare_schema_diff_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Option<Vec<String>>,
    target_only_table_names: Option<Vec<String>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
) -> Result<job::SchemaDiffJobAccepted, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    job::run_prepare_job(
        &state,
        crate::schema_diff::job::PrepareRequest::Table {
            source_db_session_id,
            target_db_session_id,
            table_names,
            target_table_names,
            target_only_table_names: target_only_table_names.unwrap_or_default(),
            source_schema,
            target_schema,
            allow_destructive,
            include_indexes,
            type_overrides: type_overrides.unwrap_or_default(),
        },
    )
    .await
}

/// Prepare a plan using schema overrides stored in a migration profile.
///
/// Interactive callers inherit the live connection schema. Persisted profiles
/// carry both source and target schema, so their runner must keep those scopes
/// explicit when it creates the reviewed immutable plan.
pub(crate) async fn prepare_schema_diff_profile_plan_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_only_table_names: Vec<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<SchemaDiffPlan, CommandError> {
    prepare_schema_diff_plan_with_schemas_impl(
        state,
        source_db_session_id,
        target_db_session_id,
        table_names.clone(),
        table_names,
        target_only_table_names,
        allow_destructive,
        include_indexes,
        type_overrides,
        source_schema,
        target_schema,
    )
    .await
}

pub(crate) async fn prepare_schema_diff_plan_with_schemas_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Vec<String>,
    target_only_table_names: Vec<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
    source_schema_override: Option<String>,
    target_schema_override: Option<String>,
) -> Result<SchemaDiffPlan, CommandError> {
    tracing::info!(
        %source_db_session_id,
        %target_db_session_id,
        tables = table_names.len(),
        allow_destructive,
        "prepare_schema_diff_plan"
    );

    if table_names.is_empty() && target_only_table_names.is_empty() {
        return Err(CommandError::Validation(
            "table_names and target_only_table_names must not both be empty".into(),
        ));
    }
    if table_names.len() != target_table_names.len() {
        return Err(CommandError::Validation(
            "table_names and target_table_names must have the same length".into(),
        ));
    }

    let src_context = crate::services::migration_endpoint::runtime_session_context(
        &state,
        &source_db_session_id,
        datazen_runtime::application::LivePurpose::Inspection,
    )
    .await?;
    let tgt_context = crate::services::migration_endpoint::runtime_session_context(
        &state,
        &target_db_session_id,
        datazen_runtime::application::LivePurpose::Inspection,
    )
    .await?;
    let src_config = src_context.config.clone();
    let tgt_config = tgt_context.config.clone();

    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    let source_schema_scope = source_schema_override
        .as_deref()
        .or(src_config.schema.as_deref());
    let target_schema_scope = target_schema_override
        .as_deref()
        .or(tgt_config.schema.as_deref());
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config)
        && source_schema_scope.map(str::trim) == target_schema_scope.map(str::trim)
    {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }

    let src_driver = src_context.driver.clone();
    let src_handle = src_context.handle.clone();
    let tgt_driver = tgt_context.driver.clone();
    let tgt_handle = tgt_context.handle.clone();
    // 表结构读取走窄契约，而不是整块驱动。
    let src_catalog = LegacyCatalogReader::new(src_driver.as_ref());
    let tgt_catalog = LegacyCatalogReader::new(tgt_driver.as_ref());
    let target_dependency_schema_scope = if uses_schema_scope(&tgt_config.database_type) {
        target_schema_scope.or(tgt_driver.default_schema())
    } else {
        None
    };
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        source_schema_scope,
        target_schema_scope,
    )
    .await?;

    let mut pairs = Vec::new();
    for (table, target_table) in table_names.iter().zip(&target_table_names) {
        let src_table = resolve_profile_table(
            &src_config.database_type,
            table,
            source_schema_override.as_deref(),
        );
        let tgt_table = resolve_profile_table(
            &tgt_config.database_type,
            target_table,
            target_schema_override.as_deref(),
        );
        let src_schema = src_catalog
            .get_table_schema(
                &src_handle,
                &src_table,
                schema_catalog_database(&src_config.database_type, src_config.database.as_deref()),
                source_schema_override
                    .as_deref()
                    .or(src_config.schema.as_deref()),
            )
            .await
            .cmd_err("prepare_schema_diff_plan")?;
        let tgt_schema = fetch_target_table_schema(
            tgt_driver.as_ref(),
            &tgt_handle,
            &tgt_table,
            tgt_config.database.as_deref().unwrap_or_default(),
            target_schema_override
                .as_deref()
                .or(tgt_config.schema.as_deref()),
        )
        .await
        .cmd_err("prepare_schema_diff_plan")?;
        // DDL in the plan targets the target dialect, so the pair's table identifier must be
        // target-resolved. Using `table` here leaks the source's schema qualification into the
        // target DDL (e.g. `public.table` on MySQL) and breaks deploy.
        pairs.push((tgt_table, src_schema, tgt_schema));
    }

    let mut target_only_snapshots = Vec::new();
    let mut target_only_tables = Vec::new();
    let source_target_tables = pairs
        .iter()
        .map(|(table, _, _)| table.as_str())
        .collect::<HashSet<_>>();
    let mut seen_target_only_tables = HashSet::new();
    for table in &target_only_table_names {
        let tgt_table = resolve_profile_table(
            &tgt_config.database_type,
            table,
            target_schema_override.as_deref(),
        );
        // Target-only selection is an explicit destructive request. Read the
        // real target schema and fail closed if the selected table disappeared;
        // do not turn it into an empty source schema sentinel.
        let tgt_schema = tgt_catalog
            .get_table_schema(
                &tgt_handle,
                &tgt_table,
                schema_catalog_database(&tgt_config.database_type, tgt_config.database.as_deref()),
                target_schema_override
                    .as_deref()
                    .or(tgt_config.schema.as_deref()),
            )
            .await
            .cmd_err("prepare_schema_diff_plan")?;
        if source_target_tables.contains(tgt_table.as_str()) {
            return Err(CommandError::Validation(format!(
                "Table `{tgt_table}` cannot be selected as both a source table and target-only table"
            )));
        }
        if !seen_target_only_tables.insert(tgt_table.clone()) {
            return Err(CommandError::Validation(format!(
                "Target-only table `{tgt_table}` was selected more than once"
            )));
        }
        target_only_tables.push(tgt_table.clone());
        target_only_snapshots.push((tgt_table, tgt_schema));
    }

    let mut selected_target_snapshots = pairs
        .iter()
        .map(|(table, _, target)| (table.clone(), target.clone()))
        .collect::<Vec<_>>();
    selected_target_snapshots.extend(target_only_snapshots.iter().cloned());

    let (target_dependency_catalog, target_dependency_catalog_error) =
        if allow_destructive && !target_only_tables.is_empty() {
            match fetch_target_table_dependency_catalog(
                tgt_driver.as_ref(),
                &tgt_handle,
                tgt_config.database.as_deref().unwrap_or_default(),
                &tgt_config.database_type,
                target_dependency_schema_scope,
                &selected_target_snapshots,
            )
            .await
            {
                Ok(catalog) => (Some(catalog), None),
                Err(error) => (None, Some(error.to_string())),
            }
        } else {
            (None, None)
        };

    let src_d = normalize_dialect(&src_config.database_type);
    let tgt_d = normalize_dialect(&tgt_config.database_type);
    let include_indexes = include_indexes.unwrap_or(true);

    let mut plan = if src_d != tgt_d {
        state
            .sync_adapters
            .ensure_pair(&src_config.database_type, &tgt_config.database_type)
            .map_err(CommandError::Validation)?;
        let src_adapter: Arc<dyn SyncSourceAdapter> = state
            .sync_adapters
            .get_source(&src_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
        let tgt_adapter: Arc<dyn SyncTargetAdapter> = state
            .sync_adapters
            .get_target(&tgt_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;

        let tgt_is_mysql = tgt_d == "mysql";
        let mapper = |table: &str, source_type: &str, col_name: &str| -> Result<String, String> {
            for (tgt_tbl, src, _) in pairs.iter().filter(|(t, _, _)| t == table) {
                let matching_col = src
                    .columns
                    .iter()
                    .find(|c| c.name == col_name && c.data_type == source_type)
                    .or_else(|| src.columns.iter().find(|c| c.name == col_name));
                if let Some(col) = matching_col {
                    // 1. User explicit column type overrides take highest priority
                    if let Some(ref overrides) = type_overrides {
                        if let Some(ov) = overrides.iter().find(|o| {
                            resolve_profile_table(
                                &tgt_config.database_type,
                                &o.table,
                                target_schema_override.as_deref(),
                            ) == *tgt_tbl
                                && o.column == col_name
                        }) {
                            return Ok(ov.target_type.clone());
                        }
                    }

                    // 2. MySQL target: unbounded text columns default to pre-filled suggested VARCHAR(255)
                    if tgt_is_mysql && is_source_unbounded_text(&col.data_type) {
                        return Ok("VARCHAR(255)".into());
                    }

                    let ir = src_adapter.column_to_ir(col, Some(source_type));
                    if col.default_value.is_some()
                        && !tgt_adapter.allows_column_default(&ir.ir_type)
                    {
                        if let Some(fallback) = tgt_adapter.default_capable_type_for(&ir.ir_type) {
                            return Ok(tgt_adapter.ir_type_to_native(&fallback));
                        }
                    }
                    return Ok(tgt_adapter.ir_type_to_native(&ir.ir_type));
                }
            }
            Err(format!(
                "cannot map type `{source_type}` for column `{col_name}`"
            ))
        };

        crate::schema_diff::plan::build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
            &pairs,
            &target_only_tables,
            target_dependency_catalog.as_deref(),
            &src_d,
            &tgt_d,
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            )),
            target_dependency_schema_scope,
            PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: Some(&mapper),
                cross_dialect: true,
            },
        )
    } else {
        crate::schema_diff::plan::build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
            &pairs,
            &target_only_tables,
            target_dependency_catalog.as_deref(),
            &src_d,
            &tgt_d,
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            )),
            target_dependency_schema_scope,
            PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: None,
                cross_dialect: false,
            },
        )
    };

    if let Some(error) = target_dependency_catalog_error.as_deref() {
        for requirement in &mut plan.requirements {
            if let crate::schema_diff::types::PlanRequirement::Unsupported { operation, reason } =
                requirement
            {
                if operation == "target-only-table-drop-order" {
                    *reason = format!(
                        "Could not inspect the target foreign-key dependency catalog ({error}). Confirm catalog access, select dependent tables, or remove their foreign keys before dropping a parent."
                    );
                }
            }
        }
        plan.statements.clear();
    }

    let has_complete_target_dependency_catalog =
        allow_destructive && !target_only_tables.is_empty() && target_dependency_catalog.is_some();
    let frozen_target_snapshots = target_dependency_catalog
        .clone()
        .unwrap_or_else(|| selected_target_snapshots.clone());

    if has_complete_target_dependency_catalog {
        crate::schema_diff::reviewed::freeze_with_dependency_catalog(
            &mut plan,
            target_db_session_id,
            &tgt_handle,
            &tgt_config,
            frozen_target_snapshots,
            schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
            target_schema_scope.map(str::to_owned),
            target_dependency_schema_scope.map(str::to_owned),
        )
        .await;
    } else {
        crate::schema_diff::reviewed::freeze(
            &mut plan,
            target_db_session_id,
            &tgt_handle,
            &tgt_config,
            frozen_target_snapshots,
            schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
            target_schema_scope.map(str::to_owned),
        )
        .await;
    }

    tracing::info!(
        statements = plan.statements.len(),
        warnings = plan.warnings.len(),
        "prepare_schema_diff_plan OK"
    );
    Ok(plan)
}
