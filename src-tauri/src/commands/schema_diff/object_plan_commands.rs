//! Object-level Schema Diff plan preparation for views, routines, sequences
//! and user-defined types.

use datazen_driver_api::SchemaMigration as _;

use super::unified_plan;
use super::{
    apply_sqlserver_object_scope_gate, build_routine_trigger_migration_plan_with_components,
    build_sequence_migration_plan_with_components, build_type_migration_plan_with_components,
    build_view_migration_plan_with_components, ensure_distinct_schema_scope, fetch_schema_object,
    fetch_schema_view, list_schema_objects, list_schema_views, normalize_dialect,
    schema_catalog_scope, select_schema_object_pair, AppState, CommandError, SchemaDiffPlan, State,
};

/// Prepare a reviewed migration plan for selected views.
///
/// The source and target definitions are read by the backend from live object
/// metadata. The client supplies only qualified object selectors; it never
/// supplies replacement SQL. The returned plan can be deployed through the
/// existing `execute_schema_diff_deploy` command and is guarded by the same
/// one-shot identity and target-snapshot checks as table plans.
#[tauri::command]
pub async fn prepare_schema_view_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
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
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let src_driver = src_context.driver.clone();
    let src_handle = src_context.handle.clone();
    let tgt_driver = tgt_context.driver.clone();
    let tgt_handle = tgt_context.handle.clone();
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_views(src_driver.as_ref(), &src_handle).await?;
    let target_available = list_schema_views(tgt_driver.as_ref(), &tgt_handle).await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;

    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_view(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_view(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let target_mysql_view_scope_context = if src_dialect == "mysql" && tgt_dialect == "mysql" {
        unified_plan::read_mysql_view_scope_context(tgt_driver.as_ref(), &tgt_handle).await
    } else {
        None
    };
    let migration = datazen_driver_api::LegacySchemaMigration::new(tgt_driver.as_ref());
    let Some(renderer) = migration.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = migration.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_view_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    if tgt_dialect == "mysql" {
        for object in source_snapshots.iter().chain(&target_snapshots) {
            if let Err(reason) =
                crate::schema_diff::unified_scope::validate_mysql_view_snapshot_creation_semantics(
                    object,
                    target_mysql_view_scope_context.as_ref(),
                )
            {
                plan.requirements
                    .push(crate::schema_diff::types::PlanRequirement::Unsupported {
                        operation: object.identity().display_key(),
                        reason,
                    });
            }
        }
        if src_config.database.as_deref() != tgt_config.database.as_deref() {
            plan.requirements.push(crate::schema_diff::types::PlanRequirement::Unsupported {
                operation: "mysql-view-scope".into(),
                reason: "Cross-database MySQL view migration requires the unified dependency-reviewed plan; this object-only plan cannot prove source-to-target relation mapping.".into(),
            });
        }
        if !plan.requirements.is_empty() {
            plan.statements.clear();
        }
    }
    crate::schema_diff::reviewed::freeze_with_objects_and_mysql_view_context(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
        target_mysql_view_scope_context,
    )
    .await;
    Ok(plan)
}

/// Prepare a reviewed same-dialect plan for PostgreSQL/MySQL functions,
/// procedures, or triggers. The backend owns object DDL retrieval so callers
/// cannot inject replacement SQL into the migration plan.
#[tauri::command]
pub async fn prepare_schema_routine_trigger_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    kind: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    let kind = datazen_driver_api::ObjectKind::parse(&kind).ok_or_else(|| {
        CommandError::Validation("kind must be function, procedure, or trigger".into())
    })?;
    if !matches!(
        kind,
        datazen_driver_api::ObjectKind::Function
            | datazen_driver_api::ObjectKind::Procedure
            | datazen_driver_api::ObjectKind::Trigger
    ) {
        return Err(CommandError::Validation(
            "kind must be function, procedure, or trigger".into(),
        ));
    }
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
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
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let src_driver = src_context.driver.clone();
    let src_handle = src_context.handle.clone();
    let tgt_driver = tgt_context.driver.clone();
    let tgt_handle = tgt_context.handle.clone();
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_objects(src_driver.as_ref(), &src_handle, kind).await?;
    let target_available = list_schema_objects(tgt_driver.as_ref(), &tgt_handle, kind).await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;
    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_object(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_object(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let migration = datazen_driver_api::LegacySchemaMigration::new(tgt_driver.as_ref());
    let Some(renderer) = migration.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = migration.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_routine_trigger_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    crate::schema_diff::reviewed::freeze_with_objects(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
    )
    .await;
    Ok(plan)
}

/// Prepare a reviewed same-dialect plan for PostgreSQL sequences. The client
/// supplies only qualified selectors; both source and target DDL are read
/// from the live driver catalog and frozen for the one-shot deploy gate.
#[tauri::command]
pub async fn prepare_schema_sequence_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
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
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let src_driver = src_context.driver.clone();
    let src_handle = src_context.handle.clone();
    let tgt_driver = tgt_context.driver.clone();
    let tgt_handle = tgt_context.handle.clone();
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_objects(
        src_driver.as_ref(),
        &src_handle,
        datazen_driver_api::ObjectKind::Sequence,
    )
    .await?;
    let target_available = list_schema_objects(
        tgt_driver.as_ref(),
        &tgt_handle,
        datazen_driver_api::ObjectKind::Sequence,
    )
    .await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;
    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_object(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_object(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let migration = datazen_driver_api::LegacySchemaMigration::new(tgt_driver.as_ref());
    let Some(renderer) = migration.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = migration.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_sequence_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    crate::schema_diff::reviewed::freeze_with_objects(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
    )
    .await;
    Ok(plan)
}

/// Prepare a reviewed same-dialect plan for user-defined types. The client
/// supplies only qualified selectors; both source and target DDL are read
/// from the live driver catalog and frozen for the one-shot deploy gate.
#[tauri::command]
pub async fn prepare_schema_type_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
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
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let src_driver = src_context.driver.clone();
    let src_handle = src_context.handle.clone();
    let tgt_driver = tgt_context.driver.clone();
    let tgt_handle = tgt_context.handle.clone();
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_objects(
        src_driver.as_ref(),
        &src_handle,
        datazen_driver_api::ObjectKind::Type,
    )
    .await?;
    let target_available = list_schema_objects(
        tgt_driver.as_ref(),
        &tgt_handle,
        datazen_driver_api::ObjectKind::Type,
    )
    .await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;
    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_object(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_object(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let migration = datazen_driver_api::LegacySchemaMigration::new(tgt_driver.as_ref());
    let Some(renderer) = migration.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = migration.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_type_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    crate::schema_diff::reviewed::freeze_with_objects(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
    )
    .await;
    Ok(plan)
}
