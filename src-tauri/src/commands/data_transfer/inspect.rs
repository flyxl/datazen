//! Inspect Data Transfer table mappings (no execute).

use std::collections::HashMap;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::types::{is_self_database, resolve_db_name};
use crate::commands::sync::compare::count_rows;
use crate::data_transfer::{
    enforce_transfer_pairing, inspect_tables, structure::enrich_create_new_target_types,
    TableInspectResult, TableMapping, TransferMode,
};
use crate::services::metadata_schema;
use datazen_driver_api::TableType;

pub(crate) async fn inspect_data_transfer_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    mode: TransferMode,
    mappings: &[TableMapping],
) -> Result<Vec<TableInspectResult>, CommandError> {
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;

    enforce_transfer_pairing(&src_config.database_type, &tgt_config.database_type)
        .map_err(CommandError::from)?;

    let src_db = resolve_db_name(source_database.as_deref(), src_config.database.as_deref());
    let tgt_db = resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;

    let source = crate::data_transfer::model::Endpoint {
        db_session_id: source_db_session_id.clone(),
        database: src_db.clone(),
        schema: metadata_schema(
            src_driver.as_ref(),
            source_schema,
            None,
            src_config.schema.as_deref(),
        ),
    };
    let target = crate::data_transfer::model::Endpoint {
        db_session_id: target_db_session_id.clone(),
        database: tgt_db.clone(),
        schema: metadata_schema(
            tgt_driver.as_ref(),
            target_schema,
            None,
            tgt_config.schema.as_deref(),
        ),
    };
    crate::data_transfer::metadata::metadata_relation_ref(&source, "")?;
    crate::data_transfer::metadata::metadata_relation_ref(&target, "")?;

    if is_self_database(
        &source_db_session_id,
        &target_db_session_id,
        &src_db,
        &tgt_db,
        source.normalized_schema(),
        target.normalized_schema(),
    ) {
        return Err(CommandError::Validation(
            "source and target are the same database; pick different databases or connections"
                .into(),
        ));
    }

    let src_tables = src_driver
        .get_tables(&src_handle, &src_db, source.normalized_schema())
        .await
        .cmd_err("inspect_data_transfer")?;
    let tgt_tables = tgt_driver
        .get_tables(&tgt_handle, &tgt_db, target.normalized_schema())
        .await
        .cmd_err("inspect_data_transfer")?;

    let src_tables: Vec<_> = src_tables
        .into_iter()
        .filter(|table| crate::data_transfer::metadata::table_in_endpoint_schema(&source, table))
        .collect();
    let tgt_tables: Vec<_> = tgt_tables
        .into_iter()
        .filter(|table| crate::data_transfer::metadata::table_in_endpoint_schema(&target, table))
        .collect();

    let mut source_schemas = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        let schema = crate::data_transfer::metadata::load_table_schema(
            src_driver.as_ref(),
            &src_handle,
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
        source_schemas.insert(table.name.clone(), schema);
    }
    let mut target_schemas = HashMap::new();
    for table in tgt_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        let schema = crate::data_transfer::metadata::load_table_schema(
            tgt_driver.as_ref(),
            &tgt_handle,
            &target,
            &table.name,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "failed to inspect target table '{}': {error}",
                table.name
            ))
        })?;
        target_schemas.insert(table.name.clone(), schema);
    }

    let mut source_row_counts = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if let Ok(n) = count_rows(
            src_driver.as_ref(),
            &src_handle,
            &src_config.database_type,
            Some(&src_db),
            source.normalized_schema(),
            &table.name,
        )
        .await
        {
            source_row_counts.insert(table.name.clone(), n);
        }
    }

    // Fill lengths and numeric precision before deriving editable target
    // types. Otherwise an inferred LONGTEXT/DECIMAL becomes an explicit UI
    // override and hides the exact type fetched later during prepare.
    if matches!(
        mode,
        TransferMode::Structure | TransferMode::StructureAndData
    ) {
        if let Some(adapter) = state.sync_adapters.get_source(&src_config.database_type) {
            crate::data_transfer::structure::enrich_source_types(
                adapter.as_ref(),
                src_driver.as_ref(),
                &src_handle,
                &source,
                &mut source_schemas,
            )
            .await
            .map_err(CommandError::from)?;
        }
    }

    let mut results = inspect_tables(
        &src_tables,
        &tgt_tables,
        mappings,
        &source_schemas,
        &target_schemas,
        mode,
        &source_row_counts,
    );

    if state
        .sync_adapters
        .ensure_pair(&src_config.database_type, &tgt_config.database_type)
        .is_ok()
    {
        if let (Some(src), Some(tgt)) = (
            state.sync_adapters.get_source(&src_config.database_type),
            state.sync_adapters.get_target(&tgt_config.database_type),
        ) {
            enrich_create_new_target_types(
                &mut results,
                &source_schemas,
                src.as_ref(),
                tgt.as_ref(),
            );
            for result in results
                .iter_mut()
                .filter(|result| result.enabled && !result.create_new)
            {
                let metadata = crate::data_transfer::metadata::load_target_character_metadata(
                    tgt.as_ref(),
                    tgt_driver.as_ref(),
                    &tgt_handle,
                    &target,
                    &result.target_table,
                )
                .await
                .map_err(|error| {
                    CommandError::Validation(format!(
                        "failed to inspect target character metadata for '{}': {error}",
                        result.target_table
                    ))
                })?;
                for (name, (character_set, collation)) in metadata {
                    if let Some(column) = result.target_column_types.get_mut(&name) {
                        column.character_set = character_set;
                        column.collation = collation;
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Inspect a SQL-file transfer using only the source session.
///
/// A file destination has no live target catalog to compare against.  The
/// source schema is still inspected through the same mapping engine so the
/// mapping UI can let the user select tables, rename targets, skip columns,
/// and review the target-native type chosen for an explicit output dialect.
pub(crate) async fn inspect_sql_file_transfer_impl(
    state: &AppState,
    source_db_session_id: String,
    source_database: Option<String>,
    source_schema: Option<String>,
    mode: TransferMode,
    target_database_type: Option<String>,
    mappings: &[TableMapping],
) -> Result<Vec<TableInspectResult>, CommandError> {
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("inspect_sql_file_transfer")?;
    let src_db = resolve_db_name(source_database.as_deref(), src_config.database.as_deref());
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("inspect_sql_file_transfer")?;
    let source = crate::data_transfer::model::Endpoint {
        db_session_id: source_db_session_id.clone(),
        database: src_db.clone(),
        schema: metadata_schema(
            src_driver.as_ref(),
            source_schema.as_deref(),
            None,
            src_config.schema.as_deref(),
        ),
    };
    crate::data_transfer::metadata::metadata_relation_ref(&source, "")?;
    let source_tables = src_driver
        .get_tables(&src_handle, &src_db, source.normalized_schema())
        .await
        .cmd_err("inspect_sql_file_transfer")?;
    let source_tables: Vec<_> = source_tables
        .into_iter()
        .filter(|table| crate::data_transfer::metadata::table_in_endpoint_schema(&source, table))
        .collect();

    let mut source_schemas = HashMap::new();
    for table in source_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        let schema = crate::data_transfer::metadata::load_table_schema(
            src_driver.as_ref(),
            &src_handle,
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
        source_schemas.insert(table.name.clone(), schema);
    }

    let mut source_row_counts = HashMap::new();
    for table in source_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if let Ok(n) = count_rows(
            src_driver.as_ref(),
            &src_handle,
            &src_config.database_type,
            Some(&src_db),
            source.normalized_schema(),
            &table.name,
        )
        .await
        {
            source_row_counts.insert(table.name.clone(), n);
        }
    }

    if let Some(target_type) = target_database_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let target = crate::data_transfer::SqlFileTarget {
            file_token: "inspect-only".into(),
            database_type: Some(target_type.to_string()),
            database: None,
            schema: None,
            encoding: None,
            compression: None,
        };
        let _driver =
            crate::data_transfer::sql_file::resolve_target_driver(src_driver.clone(), &target)
                .map_err(CommandError::from)?;
        state
            .sync_adapters
            .ensure_pair(&src_config.database_type, &target_type.to_string())
            .map_err(CommandError::Validation)?;
    }

    let adapters = if let Some(target_type) = target_database_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let source_adapter = state
            .sync_adapters
            .get_source(&src_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
        let target_adapter = state
            .sync_adapters
            .get_target(&target_type.to_string())
            .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;
        crate::data_transfer::structure::enrich_source_types(
            source_adapter.as_ref(),
            src_driver.as_ref(),
            &src_handle,
            &source,
            &mut source_schemas,
        )
        .await
        .map_err(CommandError::from)?;
        Some((source_adapter, target_adapter))
    } else {
        None
    };

    let effective_mappings = if mappings.is_empty() {
        source_tables
            .iter()
            .filter(|table| matches!(table.table_type, TableType::Table))
            .map(|table| {
                let mut mapping = TableMapping::auto(&table.name);
                mapping.create_new = true;
                mapping
            })
            .collect::<Vec<_>>()
    } else {
        mappings
            .iter()
            .cloned()
            .map(|mut mapping| {
                // SQL-file output always creates into the selected artifact;
                // accepting false here would turn a renamed mapping into an
                // implicit target lookup at preview time.
                mapping.create_new = true;
                mapping
            })
            .collect::<Vec<_>>()
    };
    let mut results = inspect_tables(
        &source_tables,
        &[],
        &effective_mappings,
        &source_schemas,
        &HashMap::new(),
        mode,
        &source_row_counts,
    );
    if let Some((source_adapter, target_adapter)) = adapters {
        enrich_create_new_target_types(
            &mut results,
            &source_schemas,
            source_adapter.as_ref(),
            target_adapter.as_ref(),
        );
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_transfer::model::TableMappingStatus;
    use crate::db::TableInfo;
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_driver::MockDriverOptions;
    use datazen_driver_api::ColumnSchema;

    fn table(name: &str) -> TableInfo {
        TableInfo {
            name: name.into(),
            schema: None,
            table_type: TableType::Table,
            row_count: Some(2),
        }
    }

    fn columns(names: &[&str]) -> Vec<ColumnSchema> {
        names
            .iter()
            .map(|name| ColumnSchema {
                name: (*name).into(),
                data_type: "text".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            })
            .collect()
    }

    /// Two sessions over databases whose catalogs deliberately differ: `app`
    /// is the source and holds only `users`, `app2` is the target and holds
    /// `users` plus `orders`. Expectations below therefore cannot be met by a
    /// source-only table list or by an empty target list.
    async fn source_and_target_state() -> (TestAppState, String, String) {
        let test = TestAppState::with_options(MockDriverOptions {
            databases: vec!["app".into(), "app2".into()],
            tables_by_database: HashMap::from([
                ("app".into(), vec![table("users")]),
                ("app2".into(), vec![table("users"), table("orders")]),
            ]),
            columns_by_database: HashMap::from([
                (
                    "app".into(),
                    HashMap::from([("users".into(), columns(&["id", "name"]))]),
                ),
                (
                    "app2".into(),
                    HashMap::from([
                        ("users".into(), columns(&["id", "name"])),
                        ("orders".into(), columns(&["id", "total"])),
                    ]),
                ),
            ]),
            count_total: 2,
            ..Default::default()
        })
        .await;
        let (_source_config, source) = test.save_and_connect("tt-source").await;
        let (_target_config, target) = test.save_and_connect("tt-target").await;
        (test, source, target)
    }

    async fn inspect_database_to_database(
        test: &TestAppState,
        source: String,
        target: String,
        mappings: &[TableMapping],
    ) -> Vec<TableInspectResult> {
        inspect_data_transfer_impl(
            &test.state,
            source,
            target,
            Some("app".into()),
            Some("app2".into()),
            None,
            None,
            TransferMode::Data,
            mappings,
        )
        .await
        .expect("inspection should succeed")
    }

    fn named_mapping(target_table: &str) -> Vec<TableMapping> {
        vec![TableMapping {
            source_table: "users".into(),
            target_table: target_table.into(),
            create_new: false,
            enabled: true,
            ..TableMapping::auto("users")
        }]
    }

    #[tokio::test]
    async fn the_target_database_catalog_reaches_the_inspection() {
        let (test, source, target) = source_and_target_state().await;

        let rows = inspect_database_to_database(&test, source, target, &[]).await;

        let users = rows
            .iter()
            .find(|row| row.source_table == "users")
            .expect("the source table must appear in the result");
        assert_eq!(users.status, TableMappingStatus::Matched);
        assert!(users.enabled);
        assert_eq!(users.target_table, "users");
        assert_eq!(
            users.target_columns,
            vec!["id".to_string(), "name".to_string()]
        );

        // `orders` exists only in the target database. A row for it can only be
        // produced from the target's own catalog, so its presence is direct
        // evidence that this path reads the live target instead of an empty
        // stand-in.
        let orders = rows
            .iter()
            .find(|row| row.target_table == "orders")
            .expect("the target-only table must appear in the result");
        assert_eq!(orders.status, TableMappingStatus::UnmappedTarget);
    }

    #[tokio::test]
    async fn a_target_name_the_target_database_lacks_is_reported_not_substituted() {
        let (test, source, target) = source_and_target_state().await;

        let matched = inspect_database_to_database(
            &test,
            source.clone(),
            target.clone(),
            &named_mapping("users"),
        )
        .await;
        let absent =
            inspect_database_to_database(&test, source, target, &named_mapping("archive_users"))
                .await;

        // The same mapping shape resolves against a name the target catalog
        // holds and is refused against one it does not, so the verdict follows
        // the target catalog rather than any name-based fallback.
        assert_eq!(matched[0].status, TableMappingStatus::Matched);
        assert_eq!(absent[0].status, TableMappingStatus::Incompatible);
        assert_eq!(absent[0].target_table, "archive_users");
        assert_ne!(absent[0].target_table, absent[0].source_table);
        assert!(absent[0]
            .incompatible_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("archive_users")));
    }

    #[tokio::test]
    async fn a_renamed_sql_file_mapping_is_reported_as_a_creation() {
        let (test, source, _target) = source_and_target_state().await;

        let rows = inspect_sql_file_transfer_impl(
            &test.state,
            source,
            Some("app".into()),
            None,
            TransferMode::Data,
            None,
            &named_mapping("archive_users"),
        )
        .await
        .expect("source-only SQL-file inspection should succeed");

        // A file destination has no catalog to look the name up in, so the
        // mapping engine must be told this row creates its target. Otherwise
        // the same renamed mapping that reads fine against a database would be
        // rejected here as an unknown target.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, TableMappingStatus::CreateNew);
        assert!(rows[0].create_new);
        assert_eq!(rows[0].target_table, "archive_users");
    }
}
