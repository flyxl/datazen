//! Source snapshot checks performed again immediately before reviewed-plan writes.

use super::*;
use crate::schema_diff::object_identity::SchemaObjectDependencySnapshot;
use datazen_driver_api::DatabaseObject;

pub(crate) async fn revalidate_source_snapshot(
    state: &AppState,
    reviewed: &crate::schema_diff::reviewed::ReviewedPlan,
) -> Result<(), CommandError> {
    let Some(source) = reviewed.source_snapshot.as_ref() else {
        return Ok(());
    };
    let context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &source.session,
        datazen_modules::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let driver = context.driver;
    let handle = context.handle;
    let config = context.config;
    crate::schema_diff::reviewed::validate_source_connection_snapshot(
        source,
        &source.session,
        &handle.pool_id,
        &config,
    )
    .map_err(CommandError::Validation)?;

    for (table, snapshot) in &source.table_snapshots {
        let (relation, database, schema) = super::resolve_reviewed_table_snapshot(
            &config.database_type,
            table,
            config.database.as_deref().unwrap_or_default(),
            source.schema_scope.as_deref(),
        );
        let current = super::fetch_target_table_schema(
            driver.as_ref(),
            &handle,
            &relation,
            &database,
            schema.as_deref(),
        )
        .await?;
        crate::schema_diff::reviewed::validate_snapshot(table, snapshot, &current)
            .map_err(CommandError::Validation)?;
    }

    for snapshot in &source.object_snapshots {
        let object = DatabaseObject {
            kind: snapshot.kind.as_str().into(),
            schema: snapshot.schema.clone(),
            name: snapshot.name.clone(),
            signature: snapshot.signature.clone(),
            target_schema: snapshot.target_schema.clone(),
            target_name: snapshot.target_name.clone(),
        };
        let current = fetch_object_snapshot(driver.as_ref(), &handle, &object).await?;
        crate::schema_diff::reviewed::validate_object_snapshot(snapshot, &current)
            .map_err(CommandError::Validation)?;
        crate::schema_diff::reviewed::validate_object_dependency_catalog(
            &[SchemaObjectDependencySnapshot {
                identity: snapshot.identity(),
                dependencies: snapshot.dependencies.clone(),
                type_dependency_usages: None,
                sequence_dependency_usages: snapshot.sequence_dependency_usages.clone(),
            }],
            &[SchemaObjectDependencySnapshot {
                identity: current.identity(),
                dependencies: current.dependencies,
                type_dependency_usages: None,
                sequence_dependency_usages: current.sequence_dependency_usages,
            }],
        )
        .map_err(CommandError::Validation)?;
    }

    let mut current_table_dependencies = Vec::with_capacity(source.table_dependency_catalog.len());
    for snapshot in &source.table_dependency_catalog {
        let object = DatabaseObject {
            kind: snapshot.identity.kind.as_str().into(),
            schema: snapshot.identity.schema.clone(),
            name: snapshot.identity.name.clone(),
            signature: snapshot.identity.signature.clone(),
            target_schema: snapshot.identity.target_schema.clone(),
            target_name: snapshot.identity.target_name.clone(),
        };
        let mut current = fetch_object_dependency_snapshot(driver.as_ref(), &handle, &object).await;
        if snapshot.dependencies.is_some() && current.is_none() {
            return Err(CommandError::Validation(format!(
                "Source dependency visibility for `{}` is no longer complete; compare again",
                snapshot.identity.display_key()
            )));
        }
        if let Some(current) = current.as_mut() {
            current.identity = snapshot.identity.clone();
        }
        current_table_dependencies.push(current.unwrap_or_else(|| {
            SchemaObjectDependencySnapshot {
                identity: snapshot.identity.clone(),
                dependencies: None,
                type_dependency_usages: None,
                sequence_dependency_usages: None,
            }
        }));
    }
    crate::schema_diff::reviewed::validate_object_dependency_catalog(
        &source.table_dependency_catalog,
        &current_table_dependencies,
    )
    .map_err(CommandError::Validation)?;
    Ok(())
}
