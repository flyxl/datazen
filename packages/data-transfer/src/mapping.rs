//! Table/column auto-mapping for Data Transfer.

use std::collections::{HashMap, HashSet};

use datazen_driver_api::{TableInfo, TableSchema, TableType};

use super::model::{
    ColumnMapping, TableInspectResult, TableMapping, TableMappingStatus, TransferMode,
    TransferTargetColumnType,
};

fn schema_column_names(schema: &TableSchema) -> Vec<String> {
    schema.columns.iter().map(|c| c.name.clone()).collect()
}

fn source_column_names(
    source_schemas: &HashMap<String, TableSchema>,
    source_table: &str,
) -> Vec<String> {
    source_schemas
        .get(source_table)
        .map(schema_column_names)
        .unwrap_or_default()
}

fn target_column_names(
    target_schemas: &HashMap<String, TableSchema>,
    target_table: &str,
) -> Vec<String> {
    if target_table.is_empty() {
        return Vec::new();
    }
    target_schemas
        .get(target_table)
        .map(schema_column_names)
        .unwrap_or_default()
}

pub fn auto_map_columns(source: &TableSchema, target: &TableSchema) -> Vec<ColumnMapping> {
    let target_cols: HashSet<&str> = target.columns.iter().map(|c| c.name.as_str()).collect();
    source
        .columns
        .iter()
        .filter_map(|col| {
            if target_cols.contains(col.name.as_str()) {
                Some(ColumnMapping {
                    source_column: col.name.clone(),
                    target_column: col.name.clone(),
                    skip: false,
                    target_native_type: None,
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn effective_table_mappings(
    source_tables: &[TableInfo],
    target_tables: &[TableInfo],
    mappings: &[TableMapping],
    mode: TransferMode,
) -> Vec<TableMapping> {
    if !mappings.is_empty() {
        return mappings.to_vec();
    }

    let target_names: HashSet<&str> = target_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
        .map(|t| t.name.as_str())
        .collect();

    source_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
        .map(|t| {
            if target_names.contains(t.name.as_str()) {
                TableMapping::auto(&t.name)
            } else if matches!(
                mode,
                TransferMode::Structure | TransferMode::StructureAndData
            ) {
                TableMapping {
                    source_table: t.name.clone(),
                    target_table: t.name.clone(),
                    create_new: true,
                    // Creating an absent target is an explicit user choice.
                    // Keep the row in the inspect result so the mapping UI can
                    // show every source column and its target type, but do not
                    // make a structure preview create every source table by
                    // default.
                    enabled: false,
                    column_mappings: Vec::new(),
                    ddl_override: None,
                    source_filter: None,
                    recordset: None,
                }
            } else {
                TableMapping {
                    source_table: t.name.clone(),
                    target_table: String::new(),
                    create_new: false,
                    enabled: false,
                    column_mappings: Vec::new(),
                    ddl_override: None,
                    source_filter: None,
                    recordset: None,
                }
            }
        })
        .collect()
}

pub fn inspect_tables(
    source_tables: &[TableInfo],
    target_tables: &[TableInfo],
    mappings: &[TableMapping],
    source_schemas: &HashMap<String, TableSchema>,
    target_schemas: &HashMap<String, TableSchema>,
    mode: TransferMode,
    source_row_counts: &HashMap<String, u64>,
) -> Vec<TableInspectResult> {
    let source_by_name: HashMap<&str, &TableInfo> =
        source_tables.iter().map(|t| (t.name.as_str(), t)).collect();
    let target_by_name: HashMap<&str, &TableInfo> =
        target_tables.iter().map(|t| (t.name.as_str(), t)).collect();

    let effective = effective_table_mappings(source_tables, target_tables, mappings, mode);
    let mut results = Vec::new();
    let mut mapped_sources = HashSet::new();
    let mut mapped_targets = HashSet::new();

    for mapping in &effective {
        mapped_sources.insert(mapping.source_table.clone());
        if !mapping.target_table.is_empty() {
            mapped_targets.insert(mapping.target_table.clone());
        }

        if !mapping.enabled {
            let source_columns = source_column_names(source_schemas, &mapping.source_table);
            results.push(TableInspectResult {
                source_table: mapping.source_table.clone(),
                target_table: mapping.target_table.clone(),
                status: TableMappingStatus::Disabled,
                create_new: mapping.create_new,
                enabled: false,
                column_mappings: if mapping.create_new && mapping.column_mappings.is_empty() {
                    source_columns
                        .iter()
                        .map(|name| ColumnMapping {
                            source_column: name.clone(),
                            target_column: name.clone(),
                            skip: false,
                            target_native_type: None,
                        })
                        .collect()
                } else {
                    mapping.column_mappings.clone()
                },
                source_primary_keys: source_schemas
                    .get(&mapping.source_table)
                    .map(TableSchema::effective_primary_keys)
                    .unwrap_or_default(),
                source_columns,
                target_columns: target_column_names(target_schemas, &mapping.target_table),
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: None,
                source_row_count: source_row_counts.get(&mapping.source_table).copied(),
                recordset: mapping.recordset.clone(),
            });
            continue;
        }

        let Some(src_info) = source_by_name.get(mapping.source_table.as_str()) else {
            results.push(TableInspectResult {
                source_table: mapping.source_table.clone(),
                target_table: mapping.target_table.clone(),
                status: TableMappingStatus::Incompatible,
                create_new: mapping.create_new,
                enabled: true,
                column_mappings: mapping.column_mappings.clone(),
                source_primary_keys: source_schemas
                    .get(&mapping.source_table)
                    .map(TableSchema::effective_primary_keys)
                    .unwrap_or_default(),
                source_columns: source_column_names(source_schemas, &mapping.source_table),
                target_columns: target_column_names(target_schemas, &mapping.target_table),
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: Some(format!(
                    "source table '{}' not found",
                    mapping.source_table
                )),
                source_row_count: None,
                recordset: mapping.recordset.clone(),
            });
            continue;
        };

        if !matches!(src_info.table_type, TableType::Table) {
            results.push(TableInspectResult {
                source_table: mapping.source_table.clone(),
                target_table: mapping.target_table.clone(),
                status: TableMappingStatus::Incompatible,
                create_new: mapping.create_new,
                enabled: true,
                column_mappings: mapping.column_mappings.clone(),
                source_primary_keys: source_schemas
                    .get(&mapping.source_table)
                    .map(TableSchema::effective_primary_keys)
                    .unwrap_or_default(),
                source_columns: source_column_names(source_schemas, &mapping.source_table),
                target_columns: target_column_names(target_schemas, &mapping.target_table),
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: Some(format!(
                    "source '{}' is not a base table",
                    mapping.source_table
                )),
                source_row_count: source_row_counts.get(&mapping.source_table).copied(),
                recordset: mapping.recordset.clone(),
            });
            continue;
        }

        if mapping.create_new {
            let source_columns = source_column_names(source_schemas, &mapping.source_table);
            results.push(TableInspectResult {
                source_table: mapping.source_table.clone(),
                target_table: mapping.target_table.clone(),
                status: TableMappingStatus::CreateNew,
                create_new: true,
                enabled: true,
                column_mappings: if mapping.column_mappings.is_empty() {
                    source_columns
                        .iter()
                        .map(|name| ColumnMapping {
                            source_column: name.clone(),
                            target_column: name.clone(),
                            skip: false,
                            target_native_type: None,
                        })
                        .collect()
                } else {
                    mapping.column_mappings.clone()
                },
                source_primary_keys: source_schemas
                    .get(&mapping.source_table)
                    .map(TableSchema::effective_primary_keys)
                    .unwrap_or_default(),
                source_columns,
                target_columns: Vec::new(),
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: None,
                source_row_count: source_row_counts.get(&mapping.source_table).copied(),
                recordset: mapping.recordset.clone(),
            });
            continue;
        }

        let Some(tgt_info) = target_by_name.get(mapping.target_table.as_str()) else {
            let reason = if matches!(mode, TransferMode::Data) {
                format!(
                    "target table '{}' not found (data-only mode)",
                    mapping.target_table
                )
            } else {
                format!("target table '{}' not found", mapping.target_table)
            };
            results.push(TableInspectResult {
                source_table: mapping.source_table.clone(),
                target_table: mapping.target_table.clone(),
                status: TableMappingStatus::Incompatible,
                create_new: false,
                enabled: true,
                column_mappings: mapping.column_mappings.clone(),
                source_primary_keys: source_schemas
                    .get(&mapping.source_table)
                    .map(TableSchema::effective_primary_keys)
                    .unwrap_or_default(),
                source_columns: source_column_names(source_schemas, &mapping.source_table),
                target_columns: target_column_names(target_schemas, &mapping.target_table),
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: Some(reason),
                source_row_count: source_row_counts.get(&mapping.source_table).copied(),
                recordset: mapping.recordset.clone(),
            });
            continue;
        };

        if !matches!(tgt_info.table_type, TableType::Table) {
            results.push(TableInspectResult {
                source_table: mapping.source_table.clone(),
                target_table: mapping.target_table.clone(),
                status: TableMappingStatus::Incompatible,
                create_new: false,
                enabled: true,
                column_mappings: mapping.column_mappings.clone(),
                source_primary_keys: source_schemas
                    .get(&mapping.source_table)
                    .map(TableSchema::effective_primary_keys)
                    .unwrap_or_default(),
                source_columns: source_column_names(source_schemas, &mapping.source_table),
                target_columns: target_column_names(target_schemas, &mapping.target_table),
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: Some(format!(
                    "target '{}' is not a base table",
                    mapping.target_table
                )),
                source_row_count: source_row_counts.get(&mapping.source_table).copied(),
                recordset: mapping.recordset.clone(),
            });
            continue;
        }

        let column_mappings = if mapping.column_mappings.is_empty() {
            match (
                source_schemas.get(&mapping.source_table),
                target_schemas.get(&mapping.target_table),
            ) {
                (Some(src), Some(tgt)) => auto_map_columns(src, tgt),
                _ => Vec::new(),
            }
        } else {
            mapping.column_mappings.clone()
        };

        results.push(TableInspectResult {
            source_table: mapping.source_table.clone(),
            target_table: mapping.target_table.clone(),
            status: TableMappingStatus::Matched,
            create_new: false,
            enabled: true,
            column_mappings,
            source_primary_keys: source_schemas
                .get(&mapping.source_table)
                .map(TableSchema::effective_primary_keys)
                .unwrap_or_default(),
            source_columns: source_column_names(source_schemas, &mapping.source_table),
            target_columns: target_column_names(target_schemas, &mapping.target_table),
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: source_row_counts.get(&mapping.source_table).copied(),
            recordset: mapping.recordset.clone(),
        });
    }

    for table in source_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if mapped_sources.contains(&table.name) {
            continue;
        }
        results.push(TableInspectResult {
            source_table: table.name.clone(),
            target_table: String::new(),
            status: TableMappingStatus::UnmappedSource,
            create_new: false,
            enabled: false,
            column_mappings: Vec::new(),
            source_primary_keys: source_schemas
                .get(&table.name)
                .map(TableSchema::effective_primary_keys)
                .unwrap_or_default(),
            source_columns: source_column_names(source_schemas, &table.name),
            target_columns: Vec::new(),
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: source_row_counts.get(&table.name).copied(),
            recordset: None,
        });
    }

    for table in target_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if mapped_targets.contains(&table.name) {
            continue;
        }
        results.push(TableInspectResult {
            source_table: String::new(),
            target_table: table.name.clone(),
            status: TableMappingStatus::UnmappedTarget,
            create_new: false,
            enabled: false,
            column_mappings: Vec::new(),
            source_primary_keys: Vec::new(),
            source_columns: Vec::new(),
            target_columns: target_column_names(target_schemas, &table.name),
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        });
    }

    for result in &mut results {
        result.target_column_types = target_schemas
            .get(result.target_table.as_str())
            .map(|schema| {
                schema
                    .columns
                    .iter()
                    .map(|column| {
                        (
                            column.name.clone(),
                            TransferTargetColumnType {
                                native_type: column.data_type.clone(),
                                character_set: None,
                                collation: None,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        if result.source_table.is_empty() {
            continue;
        }
        result.source_column_types = source_schemas
            .get(result.source_table.as_str())
            .map(|schema| {
                schema
                    .columns
                    .iter()
                    .map(|c| (c.name.clone(), c.data_type.clone()))
                    .collect()
            })
            .unwrap_or_default();
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ColumnSchema;

    fn table(name: &str) -> TableInfo {
        TableInfo {
            name: name.into(),
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        }
    }

    fn schema(cols: &[(&str, &str)]) -> TableSchema {
        TableSchema {
            table_name: "t".into(),
            columns: cols
                .iter()
                .map(|(n, ty)| ColumnSchema {
                    name: n.to_string(),
                    data_type: ty.to_string(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                })
                .collect(),
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        }
    }

    #[test]
    fn auto_map_columns_by_name() {
        let src = schema(&[("id", "int"), ("name", "text"), ("extra", "text")]);
        let tgt = schema(&[("id", "int"), ("name", "varchar")]);
        let maps = auto_map_columns(&src, &tgt);
        assert_eq!(maps.len(), 2);
        assert!(maps.iter().any(|m| m.source_column == "id"));
        assert!(maps.iter().any(|m| m.source_column == "name"));
        assert!(!maps.iter().any(|m| m.source_column == "extra"));
    }

    #[test]
    fn effective_mappings_match_by_name() {
        let src = vec![table("users"), table("orders")];
        let tgt = vec![table("users")];
        let maps = effective_table_mappings(&src, &tgt, &[], TransferMode::Data);
        assert_eq!(maps.len(), 2);
        assert!(maps.iter().any(|m| m.source_table == "users" && m.enabled));
        assert!(maps
            .iter()
            .any(|m| m.source_table == "orders" && !m.enabled));
    }

    #[test]
    fn structure_mode_marks_missing_target_as_create_new() {
        let src = vec![table("new_table")];
        let tgt: Vec<TableInfo> = vec![];
        let maps = effective_table_mappings(&src, &tgt, &[], TransferMode::StructureAndData);
        assert_eq!(maps.len(), 1);
        assert!(maps[0].create_new);
        assert!(!maps[0].enabled);
    }

    #[test]
    fn disabled_create_new_rows_keep_all_source_columns_for_explicit_selection() {
        let src = vec![table("new_table")];
        let source_schema = schema(&[("id", "bigint"), ("active", "tinyint(1)")]);
        let mut source_schemas = HashMap::new();
        source_schemas.insert("new_table".into(), source_schema);

        let results = inspect_tables(
            &src,
            &[],
            &[],
            &source_schemas,
            &HashMap::new(),
            TransferMode::Structure,
            &HashMap::new(),
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TableMappingStatus::Disabled);
        assert!(results[0].create_new);
        assert!(!results[0].enabled);
        assert_eq!(
            results[0]
                .column_mappings
                .iter()
                .map(|mapping| mapping.source_column.as_str())
                .collect::<Vec<_>>(),
            vec!["id", "active"]
        );
    }

    #[test]
    fn inspect_results_retain_native_types_for_existing_target_columns() {
        let source_tables = vec![table("payments")];
        let target_tables = vec![table("payments")];
        let source_schemas =
            HashMap::from([("payments".into(), schema(&[("amount", "numeric(18,4)")]))]);
        let target_schemas =
            HashMap::from([("payments".into(), schema(&[("amount", "decimal(12,2)")]))]);

        let results = inspect_tables(
            &source_tables,
            &target_tables,
            &[],
            &source_schemas,
            &target_schemas,
            TransferMode::Data,
            &HashMap::new(),
        );

        assert_eq!(
            results[0]
                .target_column_types
                .get("amount")
                .map(|column| column.native_type.as_str()),
            Some("decimal(12,2)")
        );
    }
}
