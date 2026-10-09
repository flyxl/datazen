use super::*;
mod catalog;
mod revalidation;
use crate::schema_diff::unified_scope::MySqlViewScopeContext;
use crate::schema_diff::{
    object_identity::{SchemaObjectDependencySnapshot, SchemaObjectIdentity},
    objects::SchemaObjectSnapshot,
    reviewed::ReviewedSourceSnapshot,
    unified::build_unified_schema_diff_plan_with_source_scope,
};
pub(super) use catalog::read_target_object_catalog;
use catalog::{fetch_object_dependency_snapshot, fetch_object_snapshot};
use datazen_driver_api::{DatabaseObject, ObjectKind, TableInfo, TableType};
pub(super) use revalidation::revalidate_source_snapshot;
use std::collections::BTreeSet;

pub(super) async fn read_mysql_view_scope_context(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
) -> Option<MySqlViewScopeContext> {
    let result = driver
        .query(
            handle,
            "SELECT CURRENT_USER() AS `current_user`, @@character_set_client AS `character_set_client`, @@collation_connection AS `collation_connection`",
        )
        .await
        .ok()?;
    let read = |name: &str| {
        let index = result
            .columns
            .iter()
            .position(|column| column.name.eq_ignore_ascii_case(name))?;
        match result.rows.first()?.get(index)?.as_ref()? {
            datazen_driver_api::Value::String(value) => Some(value.clone()),
            datazen_driver_api::Value::Bytes(value) => {
                std::str::from_utf8(value).ok().map(str::to_owned)
            }
            _ => None,
        }
    };
    Some(MySqlViewScopeContext {
        current_user: read("current_user")?,
        character_set_client: read("character_set_client")?,
        collation_connection: read("collation_connection")?,
    })
}

const UNIFIED_OBJECT_KINDS: [ObjectKind; 6] = [
    ObjectKind::View,
    ObjectKind::Function,
    ObjectKind::Procedure,
    ObjectKind::Trigger,
    ObjectKind::Sequence,
    ObjectKind::Type,
];

#[tauri::command]
pub async fn prepare_schema_unified_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Option<Vec<String>>,
    target_only_table_names: Option<Vec<String>>,
    source_objects: Option<Vec<DatabaseObject>>,
    target_objects: Option<Vec<DatabaseObject>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
) -> Result<super::job::SchemaDiffJobAccepted, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    super::job::run_prepare_job(
        &state,
        crate::schema_diff::job::PrepareRequest::Unified {
            source_db_session_id,
            target_db_session_id,
            table_names,
            target_table_names,
            target_only_table_names: target_only_table_names.unwrap_or_default(),
            source_objects: source_objects.unwrap_or_default(),
            target_objects: target_objects.unwrap_or_default(),
            source_schema,
            target_schema,
            allow_destructive,
            include_indexes,
            type_overrides: type_overrides.unwrap_or_default(),
        },
    )
    .await
}

pub(crate) async fn prepare_schema_unified_plan_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Option<Vec<String>>,
    target_only_table_names: Option<Vec<String>>,
    source_objects: Option<Vec<DatabaseObject>>,
    target_objects: Option<Vec<DatabaseObject>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
) -> Result<SchemaDiffPlan, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    let target_only_table_names = target_only_table_names.unwrap_or_default();
    let source_objects = source_objects.unwrap_or_default();
    let target_objects = target_objects.unwrap_or_default();
    if table_names.is_empty()
        && target_only_table_names.is_empty()
        && source_objects.is_empty()
        && target_objects.is_empty()
    {
        return Err(CommandError::Validation(
            "Select at least one table or schema object before preparing a unified plan".into(),
        ));
    }
    if table_names.len() != target_table_names.len() {
        return Err(CommandError::Validation(
            "table_names and target_table_names must have the same length".into(),
        ));
    }
    if normalize_dialect(
        &state
            .connection_manager
            .get_session_config(&source_db_session_id)
            .await
            .cmd_err("prepare_schema_unified_plan")?
            .database_type,
    ) != normalize_dialect(
        &state
            .connection_manager
            .get_session_config(&target_db_session_id)
            .await
            .cmd_err("prepare_schema_unified_plan")?
            .database_type,
    ) && source_objects.is_empty()
        && target_objects.is_empty()
    {
        return super::prepare_schema_diff_plan_with_schemas_impl(
            &state,
            source_db_session_id,
            target_db_session_id,
            table_names,
            target_table_names,
            target_only_table_names,
            allow_destructive,
            include_indexes,
            type_overrides,
            source_schema,
            target_schema,
        )
        .await;
    }

    let source_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_unified_plan")?;
    let target_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_unified_plan")?;
    if target_config.read_only {
        return Err(CommandError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    let source_schema_scope = source_schema.as_deref().or(source_config.schema.as_deref());
    let target_schema_scope = target_schema.as_deref().or(target_config.schema.as_deref());
    if crate::schema_diff::reviewed::same_endpoint(&source_config, &target_config)
        && source_schema_scope.map(str::trim) == target_schema_scope.map(str::trim)
    {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }

    let (source_driver, source_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_unified_plan")?;
    let (target_driver, target_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_unified_plan")?;
    let target_dependency_schema_scope = if uses_schema_scope(&target_config.database_type) {
        target_schema_scope.or(target_driver.default_schema())
    } else {
        None
    };
    ensure_distinct_schema_scope(
        source_driver.as_ref(),
        &source_handle,
        &source_config,
        target_driver.as_ref(),
        &target_handle,
        &target_config,
        source_schema_scope,
        target_schema_scope,
    )
    .await?;

    let source_selectors = unique_object_selectors(&source_objects, "source")?;
    let target_selectors = unique_object_selectors(&target_objects, "target")?;
    let source_catalog =
        read_selected_object_catalog(source_driver.as_ref(), &source_handle, &source_selectors)
            .await?;
    let mut target_catalog_complete = true;
    let (target_object_list, target_object_catalog) = read_target_object_catalog(
        target_driver.as_ref(),
        &target_handle,
        super::schema_catalog_database(
            &target_config.database_type,
            target_config.database.as_deref(),
        ),
        &target_config.database_type,
        target_dependency_schema_scope,
        &mut target_catalog_complete,
    )
    .await;
    let selected_source = resolve_object_selectors(&source_selectors, &source_catalog, "source")?;
    let selected_target =
        resolve_object_selectors(&target_selectors, &target_object_list, "target")?;
    let target_mysql_view_scope_context = if normalize_dialect(&source_config.database_type)
        == "mysql"
        && normalize_dialect(&target_config.database_type) == "mysql"
        && (selected_source
            .iter()
            .any(|object| ObjectKind::parse(&object.kind) == Some(ObjectKind::View))
            || selected_target
                .iter()
                .any(|object| ObjectKind::parse(&object.kind) == Some(ObjectKind::View)))
    {
        read_mysql_view_scope_context(target_driver.as_ref(), &target_handle).await
    } else {
        None
    };

    let mut source_snapshots = Vec::with_capacity(selected_source.len());
    for object in &selected_source {
        source_snapshots
            .push(fetch_object_snapshot(source_driver.as_ref(), &source_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(selected_target.len());
    for object in &selected_target {
        target_snapshots
            .push(fetch_object_snapshot(target_driver.as_ref(), &target_handle, object).await?);
    }

    let mut table_pairs = Vec::new();
    let mut source_table_snapshots = Vec::new();
    let mut source_table_dependency_catalog = Vec::new();
    let mut planner_source_table_dependencies = Vec::new();
    let mut source_to_target_table = std::collections::HashMap::new();
    for (source_table, target_table) in table_names.iter().zip(&target_table_names) {
        let source_table = resolve_profile_table(
            &source_config.database_type,
            source_table,
            source_schema.as_deref(),
        );
        let target_table = resolve_profile_table(
            &target_config.database_type,
            target_table,
            target_schema.as_deref(),
        );
        let source_schema_snapshot = source_driver
            .get_table_schema(
                &source_handle,
                &source_table,
                super::schema_catalog_database(
                    &source_config.database_type,
                    source_config.database.as_deref(),
                ),
                source_schema_scope,
            )
            .await
            .cmd_err("prepare_schema_unified_plan")?;
        source_table_snapshots.push((source_table.clone(), source_schema_snapshot.clone()));
        let source_table_identity = table_identity_for_target(
            &source_table,
            &source_config.database_type,
            source_schema_scope,
            Some(super::schema_catalog_database(
                &source_config.database_type,
                source_config.database.as_deref(),
            )),
        );
        let target_table_identity = table_identity_for_target(
            &target_table,
            &target_config.database_type,
            target_dependency_schema_scope,
            Some(super::schema_catalog_database(
                &target_config.database_type,
                target_config.database.as_deref(),
            )),
        );
        source_to_target_table.insert(source_table_identity.clone(), target_table_identity.clone());
        let source_table_object = DatabaseObject {
            kind: ObjectKind::Table.as_str().into(),
            schema: source_table_identity.schema.clone(),
            name: source_table_identity.name.clone(),
            signature: None,
            target_schema: None,
            target_name: None,
        };
        let source_dependency_snapshot = fetch_object_dependency_snapshot(
            source_driver.as_ref(),
            &source_handle,
            &source_table_object,
        )
        .await;
        let source_dependencies = source_dependency_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.dependencies.clone());
        let sequence_dependency_usages = source_dependency_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.sequence_dependency_usages.clone());
        let type_dependency_usages =
            source_dependency_snapshot.and_then(|snapshot| snapshot.type_dependency_usages);
        source_table_dependency_catalog.push(SchemaObjectDependencySnapshot {
            identity: source_table_identity.clone(),
            dependencies: source_dependencies.clone(),
            type_dependency_usages: type_dependency_usages.clone(),
            sequence_dependency_usages: sequence_dependency_usages.clone(),
        });
        planner_source_table_dependencies.push(SchemaObjectDependencySnapshot {
            identity: target_table_identity,
            dependencies: source_dependencies,
            type_dependency_usages,
            sequence_dependency_usages,
        });
        let target_schema_snapshot = fetch_target_table_schema(
            target_driver.as_ref(),
            &target_handle,
            &target_table,
            target_config.database.as_deref().unwrap_or_default(),
            target_schema_scope,
        )
        .await
        .cmd_err("prepare_schema_unified_plan")?;
        table_pairs.push((target_table, source_schema_snapshot, target_schema_snapshot));
    }

    for snapshot in &mut planner_source_table_dependencies {
        if let Some(dependencies) = snapshot.dependencies.as_mut() {
            for dependency in dependencies {
                if let Some(target_identity) = source_to_target_table.get(dependency) {
                    *dependency = target_identity.clone();
                }
            }
        }
        if let Some(usages) = snapshot.sequence_dependency_usages.as_mut() {
            for usage in usages {
                if let Some(target_identity) = source_to_target_table.get(&usage.owner_table) {
                    usage.owner_table = target_identity.clone();
                }
            }
        }
    }

    let mut planner_source_snapshots = source_snapshots.clone();
    for snapshot in &mut planner_source_snapshots {
        let target_object_scope = if uses_schema_scope(&target_config.database_type) {
            target_dependency_schema_scope
        } else {
            Some(super::schema_catalog_database(
                &target_config.database_type,
                target_config.database.as_deref(),
            ))
        };
        if snapshot.kind == ObjectKind::View && snapshot.schema.as_deref() != target_object_scope {
            // Cross-scope view dependencies must retain their source identities
            // until the target driver's scope mapper returns exact mapped pairs.
            continue;
        }
        if let Some(dependencies) = snapshot.dependencies.as_mut() {
            for dependency in dependencies {
                if let Some(target_identity) = source_to_target_table.get(dependency) {
                    *dependency = target_identity.clone();
                }
            }
        }
        if let Some(usages) = snapshot.sequence_dependency_usages.as_mut() {
            for usage in usages {
                if let Some(target_identity) = source_to_target_table.get(&usage.owner_table) {
                    usage.owner_table = target_identity.clone();
                }
            }
        }
    }

    let mut target_only_tables = Vec::new();
    let mut selected_target_table_snapshots = table_pairs
        .iter()
        .map(|(table, _, schema)| (table.clone(), schema.clone()))
        .collect::<Vec<_>>();
    for table in &target_only_table_names {
        let table = resolve_profile_table(
            &target_config.database_type,
            table,
            target_schema.as_deref(),
        );
        let snapshot = target_driver
            .get_table_schema(
                &target_handle,
                &table,
                super::schema_catalog_database(
                    &target_config.database_type,
                    target_config.database.as_deref(),
                ),
                target_schema_scope,
            )
            .await
            .cmd_err("prepare_schema_unified_plan")?;
        if table_pairs.iter().any(|(paired, _, _)| paired == &table) {
            return Err(CommandError::Validation(format!(
                "Table `{table}` cannot be selected both as a source table and target-only table"
            )));
        }
        target_only_tables.push(table.clone());
        selected_target_table_snapshots.push((table.clone(), snapshot.clone()));
    }

    let has_selected_type_drop = selected_target.iter().any(|target| {
        ObjectKind::parse(&target.kind) == Some(ObjectKind::Type)
            && !selected_source
                .iter()
                .any(|source| object_identity(source) == object_identity(target))
    });
    let needs_full_table_catalog = !target_only_tables.is_empty() || has_selected_type_drop;
    let mut target_dependency_tables = selected_target_table_snapshots.clone();
    let mut target_table_identity_catalog = Vec::new();
    let mut has_complete_target_dependency_catalog = false;
    if needs_full_table_catalog {
        match fetch_target_table_dependency_catalog(
            target_driver.as_ref(),
            &target_handle,
            target_config.database.as_deref().unwrap_or_default(),
            &target_config.database_type,
            target_dependency_schema_scope,
            &selected_target_table_snapshots,
        )
        .await
        {
            Ok(catalog) => {
                target_table_identity_catalog = catalog
                    .iter()
                    .map(|(table, _)| {
                        table_identity_for_target(
                            table,
                            &target_config.database_type,
                            target_dependency_schema_scope,
                            Some(super::schema_catalog_database(
                                &target_config.database_type,
                                target_config.database.as_deref(),
                            )),
                        )
                    })
                    .collect();
                target_dependency_tables = catalog;
                has_complete_target_dependency_catalog = true;
            }
            Err(error) => {
                target_catalog_complete = false;
                tracing::warn!(%error, "Unified plan could not read complete target table catalog");
            }
        }
    }
    if target_table_identity_catalog.is_empty() {
        match read_target_table_identities(
            target_driver.as_ref(),
            &target_handle,
            super::schema_catalog_database(
                &target_config.database_type,
                target_config.database.as_deref(),
            ),
            &target_config.database_type,
            target_dependency_schema_scope,
        )
        .await
        {
            Ok(identities) => target_table_identity_catalog = identities,
            Err(error) => {
                target_catalog_complete = false;
                tracing::warn!(%error, "Unified plan could not read target table identities");
            }
        }
    }

    let mut target_object_snapshots = Vec::with_capacity(target_object_catalog.len());
    for dependency_snapshot in &target_object_catalog {
        target_object_snapshots.push(catalog_entry_snapshot(dependency_snapshot));
    }
    for (object, snapshot) in selected_target.iter().zip(target_snapshots.iter()) {
        let identity = object_identity(object);
        if let Some(entry) = target_object_snapshots
            .iter_mut()
            .find(|entry| entry.identity() == identity)
        {
            entry.definition = snapshot.definition.clone();
            entry.dependencies = snapshot.dependencies.clone();
            entry.sequence_dependency_usages = snapshot.sequence_dependency_usages.clone();
        }
    }

    let mut target_identity_set = target_table_identity_catalog
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    target_identity_set.extend(
        target_object_catalog
            .iter()
            .map(|snapshot| snapshot.identity.clone()),
    );

    let Some(renderer) = target_driver.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            target_config.database_type
        )));
    };
    let Some(capabilities) = target_driver.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            target_config.database_type
        )));
    };
    let source_object_scope = if uses_schema_scope(&source_config.database_type) {
        source_schema_scope.or(source_driver.default_schema())
    } else {
        Some(super::schema_catalog_database(
            &source_config.database_type,
            source_config.database.as_deref(),
        ))
    };
    let mut plan = build_unified_schema_diff_plan_with_source_scope(
        &table_pairs,
        &target_only_tables,
        &planner_source_snapshots,
        &target_snapshots,
        &target_object_snapshots,
        &planner_source_table_dependencies,
        &target_identity_set.into_iter().collect::<Vec<_>>(),
        target_catalog_complete,
        &target_dependency_tables,
        &source_config.database_type,
        &target_config.database_type,
        target_dependency_schema_scope,
        Some(super::schema_catalog_database(
            &target_config.database_type,
            target_config.database.as_deref(),
        )),
        allow_destructive,
        include_indexes.unwrap_or(true),
        type_overrides.as_deref().unwrap_or_default(),
        renderer.as_ref(),
        capabilities.as_ref(),
        source_object_scope,
        target_mysql_view_scope_context.as_ref(),
    );
    crate::schema_diff::reviewed::freeze_with_unified_catalog(
        &mut plan,
        target_db_session_id,
        &target_handle,
        &target_config,
        target_dependency_tables,
        target_snapshots,
        target_object_catalog,
        target_table_identity_catalog,
        has_complete_target_dependency_catalog,
        target_catalog_complete,
        super::schema_catalog_scope(
            &target_config.database_type,
            target_config.database.as_deref(),
        ),
        target_schema_scope.map(str::to_owned),
        target_dependency_schema_scope.map(str::to_owned),
        ReviewedSourceSnapshot {
            session: source_db_session_id,
            pool: source_handle.pool_id.clone(),
            identity: crate::schema_diff::reviewed::identity(&source_config),
            database_scope: source_config.database.clone(),
            schema_scope: source_schema_scope.map(str::to_owned),
            table_snapshots: source_table_snapshots,
            object_snapshots: source_snapshots,
            table_dependency_catalog: source_table_dependency_catalog,
        },
        target_mysql_view_scope_context,
    )
    .await;
    Ok(plan)
}

fn unique_object_selectors(
    objects: &[DatabaseObject],
    side: &str,
) -> Result<Vec<DatabaseObject>, CommandError> {
    let mut seen = BTreeSet::new();
    let mut selectors = Vec::with_capacity(objects.len());
    for object in objects {
        let kind = ObjectKind::parse(&object.kind).ok_or_else(|| {
            CommandError::Validation(format!("Unsupported schema object kind `{}`", object.kind))
        })?;
        if object.name.trim().is_empty() {
            return Err(CommandError::Validation(
                "Schema object name must not be empty".into(),
            ));
        }
        let identity = object_identity(object);
        if kind == ObjectKind::Table {
            return Err(CommandError::Validation(format!(
                "Table `{}` must be selected through the table list",
                identity.display_key()
            )));
        }
        if !seen.insert(identity) {
            return Err(CommandError::Validation(format!(
                "{side} schema object selection contains a duplicate identity"
            )));
        }
        selectors.push(object.clone());
    }
    Ok(selectors)
}

fn object_identity(object: &DatabaseObject) -> SchemaObjectIdentity {
    SchemaObjectIdentity {
        kind: ObjectKind::parse(&object.kind).unwrap_or(ObjectKind::Table),
        schema: object.schema.clone(),
        name: object.name.clone(),
        signature: object.signature.clone(),
        target_schema: object.target_schema.clone(),
        target_name: object.target_name.clone(),
    }
}

async fn read_selected_object_catalog(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    selectors: &[DatabaseObject],
) -> Result<Vec<DatabaseObject>, CommandError> {
    let mut catalog = Vec::new();
    let kinds = selectors
        .iter()
        .filter_map(|selector| ObjectKind::parse(&selector.kind))
        .collect::<BTreeSet<_>>();
    for kind in kinds {
        catalog.extend(list_schema_objects(driver, handle, kind).await?);
    }
    Ok(catalog)
}

fn resolve_object_selectors(
    selectors: &[DatabaseObject],
    catalog: &[DatabaseObject],
    side: &str,
) -> Result<Vec<DatabaseObject>, CommandError> {
    let mut selected = Vec::with_capacity(selectors.len());
    for selector in selectors {
        let wanted = object_identity(selector);
        let matches = catalog
            .iter()
            .filter(|candidate| object_identity(candidate) == wanted)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(CommandError::Validation(format!(
                "The {side} object `{}` is missing or ambiguous in the live catalog; refresh the selection and compare again",
                wanted.display_key()
            )));
        }
        selected.push(matches[0].clone());
    }
    Ok(selected)
}

fn catalog_entry_snapshot(entry: &SchemaObjectDependencySnapshot) -> SchemaObjectSnapshot {
    SchemaObjectSnapshot {
        kind: entry.identity.kind,
        schema: entry.identity.schema.clone(),
        name: entry.identity.name.clone(),
        signature: entry.identity.signature.clone(),
        target_schema: entry.identity.target_schema.clone(),
        target_name: entry.identity.target_name.clone(),
        definition: String::new(),
        dependencies: entry.dependencies.clone(),
        sequence_dependency_usages: entry.sequence_dependency_usages.clone(),
        mysql_view_metadata: None,
    }
}

pub(super) async fn read_target_table_identities(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    database: &str,
    dialect: &str,
    schema_scope: Option<&str>,
) -> Result<Vec<SchemaObjectIdentity>, CommandError> {
    let metadata_database = super::schema_catalog_database(dialect, Some(database));
    let tables = driver
        .get_tables(handle, metadata_database, None)
        .await
        .map_err(CommandError::Driver)?;
    Ok(table_identities_from_catalog(
        &tables,
        dialect,
        schema_scope,
        Some(metadata_database),
    ))
}

fn table_identities_from_catalog(
    tables: &[TableInfo],
    dialect: &str,
    schema_scope: Option<&str>,
    database: Option<&str>,
) -> Vec<SchemaObjectIdentity> {
    let mut identities = tables
        .iter()
        .filter(|table| matches!(table.table_type, TableType::Table))
        .map(|table| {
            let scope = table.schema.as_deref().or_else(|| {
                if uses_schema_scope(dialect) {
                    schema_scope
                } else {
                    database
                }
            });
            SchemaObjectIdentity::table(scope, &table.name)
        })
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    identities
}

fn table_identity_for_target(
    table: &str,
    dialect: &str,
    schema_scope: Option<&str>,
    database: Option<&str>,
) -> SchemaObjectIdentity {
    let (schema, name) = table
        .rsplit_once('.')
        .map(|(schema, name)| (Some(schema), name))
        .unwrap_or((None, table));
    let scope = schema.or_else(|| {
        if uses_schema_scope(dialect) {
            schema_scope
        } else {
            database
        }
    });
    SchemaObjectIdentity::table(scope, name)
}
