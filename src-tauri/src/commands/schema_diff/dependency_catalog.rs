//! Target-side dependency catalog reads and review-snapshot validation.
//!
//! The catalog is a complete read of the target's foreign-key graph; it is
//! the evidence that lets a reviewed plan prove that dropping a parent table
//! cannot orphan an inbound dependent.

use super::{
    normalize_dialect, resolve_table_for_dialect, uses_schema_scope, CommandError, SchemaDiffPlan,
};
use crate::db::{CatalogReader, LegacyCatalogReader};
use std::collections::{BTreeMap, HashSet};

pub(super) fn is_table_missing_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("1146")
        || lower.contains("doesn't exist")
        || lower.contains("does not exist")
        || lower.contains("not found")
}

/// Convert a connection's configured database value to the catalog identifier
/// expected by metadata methods. SQLite stores a file path in the connection
/// config while its single default catalog is named `main`.
pub(super) fn schema_catalog_database<'a>(
    database_type: &str,
    configured_database: Option<&'a str>,
) -> &'a str {
    if normalize_dialect(database_type) == "sqlite" {
        "main"
    } else {
        configured_database.unwrap_or_default()
    }
}

pub(super) fn schema_catalog_scope(
    database_type: &str,
    configured_database: Option<&str>,
) -> Option<String> {
    if normalize_dialect(database_type) == "sqlite" {
        Some("main".into())
    } else {
        configured_database.map(str::to_owned)
    }
}

pub(super) async fn fetch_target_table_schema(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<crate::db::TableSchema, CommandError> {
    // 表结构读取走窄契约，`driver_type()` 仍是驱动专属规则。
    let catalog = LegacyCatalogReader::new(driver);
    let database = schema_catalog_database(&driver.driver_type(), Some(database));
    match catalog
        .get_table_schema(handle, table, database, schema)
        .await
    {
        Ok(schema) => Ok(schema),
        Err(e) => {
            let msg = e.to_string();
            if is_table_missing_error(&msg) {
                tracing::info!(%table, "target table does not exist, treating as empty schema");
                Ok(crate::db::TableSchema {
                    table_name: table.to_string(),
                    columns: Vec::new(),
                    primary_keys: Vec::new(),
                    indexes: Vec::new(),
                    foreign_keys: Vec::new(),
                    check_constraints: Vec::new(),
                    table_options: datazen_driver_api::TableOptions::default(),
                })
            } else {
                Err(CommandError::Driver(e))
            }
        }
    }
}

pub(super) async fn fetch_target_table_dependency_catalog(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    database: &str,
    dialect: &str,
    schema_scope: Option<&str>,
    selected_snapshots: &[(String, crate::db::TableSchema)],
) -> Result<Vec<(String, crate::db::TableSchema)>, CommandError> {
    const CATALOG_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    let started_at = std::time::Instant::now();
    let metadata_database = schema_catalog_database(dialect, Some(database));
    // 目录读取走窄契约；外键可见性检查仍是驱动专属规则。
    let catalog = LegacyCatalogReader::new(driver);
    let (snapshots, table_count) = tokio::time::timeout(CATALOG_READ_TIMEOUT, async {
        let databases = if normalize_dialect(dialect) == "mysql" {
            if !driver
                .has_complete_foreign_key_catalog_visibility(handle)
                .await
                .map_err(CommandError::Driver)?
            {
                return Err(CommandError::Validation(
                    "MySQL target-only table drops require direct global SELECT or ALL PRIVILEGES on *.* with no partial REVOKE so every database dependency can be inspected; use a suitably privileged connection and retry.".into(),
                ));
            }
            let databases = catalog
                .get_databases(handle)
                .await
                .map_err(CommandError::Driver)?;
            if !databases.iter().any(|candidate| candidate == database) {
                return Err(CommandError::Validation(
                    "MySQL reported an incomplete database list despite global metadata visibility; target-only table drops are blocked.".into(),
                ));
            }
            databases
        } else {
            vec![metadata_database.to_string()]
        };
        // This must be a catalog read, not a union with individually fetched
        // selected tables: adding selected snapshots here could make an
        // incomplete catalog look complete and hide an inbound FK dependent.
        let mut snapshots = Vec::new();
        let mut seen = HashSet::new();
        let mut table_count = 0usize;

        for catalog_database in databases {
            let tables = catalog
                .get_tables(handle, &catalog_database, None)
                .await
                .map_err(CommandError::Driver)?;
            table_count += tables
                .iter()
                .filter(|table| matches!(&table.table_type, datazen_driver_api::TableType::Table))
                .count();
            for table in tables {
                if !matches!(&table.table_type, datazen_driver_api::TableType::Table) {
                    continue;
                }
                let relation = table
                    .schema
                    .as_deref()
                    .filter(|schema| !schema.is_empty())
                    .map(|schema| format!("{schema}.{}", table.name))
                    .unwrap_or_else(|| table.name.clone());
                let table_identity = if normalize_dialect(dialect) == "mysql"
                    && catalog_database != database
                {
                    format!("{catalog_database}.{relation}")
                } else {
                    relation
                };
                let table_identity =
                    dependency_relation_identity(dialect, &table_identity, schema_scope);
                if !seen.insert(table_identity.clone()) {
                    continue;
                }
                let mut schema = fetch_target_table_schema(
                    driver,
                    handle,
                    &table.name,
                    &catalog_database,
                    table.schema.as_deref(),
                )
                .await?;
                if normalize_dialect(dialect) == "mysql" {
                    for foreign_key in &mut schema.foreign_keys {
                        if let Some((referenced_database, referenced_table)) =
                            foreign_key.referenced_table.split_once('.')
                        {
                            if referenced_database == database {
                                foreign_key.referenced_table = referenced_table.to_string();
                            }
                        }
                    }
                }
                snapshots.push((table_identity, schema));
            }
        }
        for (selected_table, _) in selected_snapshots {
            let mut selected_identity =
                dependency_relation_identity(dialect, selected_table, schema_scope);
            if normalize_dialect(dialect) == "mysql" {
                if let Some((selected_database, relation)) = selected_identity.split_once('.') {
                    if selected_database == database {
                        selected_identity = relation.to_string();
                    }
                }
            }
            if snapshots
                .iter()
                .any(|(identity, _)| identity == &selected_identity)
            {
                continue;
            }

            if uses_schema_scope(dialect) && !selected_table.contains('.') {
                let candidates = snapshots
                    .iter()
                    .filter(|(identity, _)| {
                        identity.rsplit('.').next() == Some(selected_table.as_str())
                            && schema_scope
                                .map(str::trim)
                                .filter(|schema| !schema.is_empty())
                                .map(|schema| identity.starts_with(&format!("{schema}.")))
                                .unwrap_or(true)
                    })
                    .collect::<Vec<_>>();
                match candidates.as_slice() {
                    [_] => continue,
                    [] => {
                        return Err(CommandError::Validation(format!(
                            "Selected target table `{selected_table}` is absent from the complete target dependency catalog"
                        )));
                    }
                    _ => {
                        return Err(CommandError::Validation(format!(
                            "Selected target table `{selected_table}` has an ambiguous schema identity in the complete target dependency catalog"
                        )));
                    }
                }
            }

            return Err(CommandError::Validation(format!(
                "Selected target table `{selected_table}` is absent from the complete target dependency catalog"
            )));
        }

        Ok::<_, CommandError>((snapshots, table_count))
    })
    .await
    .map_err(|_| {
        CommandError::Internal(
            "Reading the target foreign-key dependency catalog exceeded 30 seconds; scope the target selection or retry after reducing catalog load.".into(),
        )
    })??;

    tracing::info!(
        table_count,
        elapsed_ms = started_at.elapsed().as_millis(),
        "Schema Diff target dependency catalog read"
    );
    Ok(snapshots)
}

pub(super) fn resolve_profile_table(dialect: &str, table: &str, schema: Option<&str>) -> String {
    let Some(schema) = schema.map(str::trim).filter(|value| !value.is_empty()) else {
        return resolve_table_for_dialect(dialect, table);
    };
    let relation = table
        .trim()
        .rsplit_once('.')
        .map(|(_, name)| name)
        .unwrap_or_else(|| table.trim());
    format!("{schema}.{relation}")
}

fn dependency_relation_identity(dialect: &str, table: &str, schema_scope: Option<&str>) -> String {
    let table = table.trim();
    if uses_schema_scope(dialect) && !table.contains('.') {
        if let Some(schema) = schema_scope.filter(|value| !value.trim().is_empty()) {
            return format!("{}.{}", schema.trim(), table);
        }
    }
    table.to_string()
}

pub(super) fn resolve_reviewed_table_snapshot(
    dialect: &str,
    table: &str,
    database_scope: &str,
    schema_scope: Option<&str>,
) -> (String, String, Option<String>) {
    if uses_schema_scope(dialect) {
        if let Some((schema, relation)) = table.rsplit_once('.') {
            return (
                relation.to_string(),
                database_scope.to_string(),
                Some(schema.to_string()),
            );
        }
    }
    if normalize_dialect(dialect) == "mysql" {
        if let Some((database, relation)) = table.rsplit_once('.') {
            return (relation.to_string(), database.to_string(), None);
        }
    }
    (
        table.to_string(),
        database_scope.to_string(),
        schema_scope.map(str::to_owned),
    )
}

pub(super) fn has_table_drop_statement(plan: &SchemaDiffPlan) -> bool {
    plan.statements.iter().any(|statement| {
        statement
            .summary
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("DROP TABLE")
    })
}

pub(super) fn validate_target_dependency_catalog(
    reviewed: &[(String, crate::db::TableSchema)],
    current: &[(String, crate::db::TableSchema)],
) -> Result<(), String> {
    fn catalog_map<'a>(
        snapshots: &'a [(String, crate::db::TableSchema)],
    ) -> Result<BTreeMap<&'a str, &'a crate::db::TableSchema>, String> {
        let mut catalog = BTreeMap::new();
        for (identity, snapshot) in snapshots {
            if catalog.insert(identity.as_str(), snapshot).is_some() {
                return Err(format!(
                    "Target dependency catalog contains duplicate relation identity `{identity}`"
                ));
            }
        }
        Ok(catalog)
    }

    let reviewed = catalog_map(reviewed)?;
    let current = catalog_map(current)?;
    if reviewed.len() != current.len()
        || reviewed
            .keys()
            .any(|identity| !current.contains_key(identity))
    {
        let added = current
            .keys()
            .filter(|identity| !reviewed.contains_key(**identity))
            .copied()
            .collect::<Vec<_>>();
        let removed = reviewed
            .keys()
            .filter(|identity| !current.contains_key(**identity))
            .copied()
            .collect::<Vec<_>>();
        return Err(format!(
            "Target dependency catalog changed after review (new relations: [{}]; removed relations: [{}]); compare again before dropping tables",
            added.join(", "),
            removed.join(", ")
        ));
    }

    for (identity, reviewed_snapshot) in reviewed {
        let current_snapshot = current
            .get(identity)
            .ok_or_else(|| format!("Target relation `{identity}` disappeared after review"))?;
        crate::schema_diff::reviewed::validate_snapshot(
            identity,
            reviewed_snapshot,
            current_snapshot,
        )?;
    }
    Ok(())
}
