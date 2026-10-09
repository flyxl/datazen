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
                    // Creating an absent target is an explicit user choice, so the
                    // target name is theirs to give: leave it empty rather than
                    // suggesting the source name. A pre-fill here travels on the
                    // wire as an ordinary string, indistinguishable from a name
                    // the user actually typed, so it silently satisfied the
                    // frontend's mapping gate and let an unnamed create-new row
                    // prepare as itself. Data mode below already returns an empty
                    // name for the same "target does not exist" case; this aligns
                    // the two modes.
                    target_table: String::new(),
                    create_new: true,
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
        // D-1: the target name is the user's to give, so it must arrive empty.
        // A pre-fill of the source name was indistinguishable on the wire from
        // a name the user actually typed.
        assert_eq!(maps[0].target_table, "");
        assert_ne!(maps[0].target_table, maps[0].source_table);
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
        // D-1: `inspect_tables` propagates `target_table` verbatim from the
        // effective mapping, so the empty name has to survive the whole way out
        // to the inspect result the frontend renders and gates on.
        assert_eq!(results[0].target_table, "");
        assert_ne!(results[0].target_table, results[0].source_table);
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

    fn enabled_mapping(source_table: &str, target_table: &str) -> TableMapping {
        TableMapping {
            source_table: source_table.into(),
            target_table: target_table.into(),
            create_new: false,
            enabled: true,
            ..TableMapping::auto(source_table)
        }
    }

    #[test]
    fn the_target_catalog_decides_whether_a_named_mapping_resolves() {
        let source_tables = vec![table("users")];
        let target_tables = vec![table("users")];
        let source_schemas = HashMap::from([("users".into(), schema(&[("id", "bigint")]))]);
        let target_schemas = HashMap::from([(
            "users".into(),
            schema(&[("id", "bigint"), ("name", "varchar")]),
        )]);
        let mappings = vec![enabled_mapping("users", "users")];

        let resolved = inspect_tables(
            &source_tables,
            &target_tables,
            &mappings,
            &source_schemas,
            &target_schemas,
            TransferMode::Data,
            &HashMap::new(),
        );
        let unresolved = inspect_tables(
            &source_tables,
            &[],
            &mappings,
            &source_schemas,
            &target_schemas,
            TransferMode::Data,
            &HashMap::new(),
        );

        // The catalog is the difference between these two verdicts, not a
        // decoration: the same mapping against a populated catalog matches and
        // reports the target's own columns, and against an empty catalog it
        // cannot resolve at all.
        assert_eq!(resolved[0].status, TableMappingStatus::Matched);
        assert_eq!(
            resolved[0].target_columns,
            vec!["id".to_string(), "name".to_string()]
        );
        assert_eq!(unresolved[0].status, TableMappingStatus::Incompatible);
        assert!(unresolved[0]
            .incompatible_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("users")));
    }

    #[test]
    fn an_unknown_target_name_is_reported_rather_than_replaced_by_the_source_name() {
        let source_tables = vec![table("users")];
        let target_tables = vec![table("users"), table("orders")];
        let source_schemas = HashMap::from([("users".into(), schema(&[("id", "bigint")]))]);
        let target_schemas = HashMap::from([("users".into(), schema(&[("id", "bigint")]))]);
        let mappings = vec![enabled_mapping("users", "ghost")];

        let results = inspect_tables(
            &source_tables,
            &target_tables,
            &mappings,
            &source_schemas,
            &target_schemas,
            TransferMode::Data,
            &HashMap::new(),
        );

        // A target name the catalog does not contain is reported as the user's
        // error, with the offending name intact. It is never quietly swapped
        // for the source name, because a silently substituted name would copy
        // into a table the user never asked for.
        assert_eq!(results[0].status, TableMappingStatus::Incompatible);
        assert_eq!(results[0].target_table, "ghost");
        assert_ne!(results[0].target_table, results[0].source_table);
        assert!(results[0]
            .incompatible_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("ghost")));
    }

    #[test]
    fn a_create_new_row_reports_the_same_thing_with_and_without_a_target_catalog() {
        let source_tables = vec![table("users")];
        let source_schemas = HashMap::from([("users".into(), schema(&[("id", "bigint")]))]);
        let target_schemas = HashMap::from([("users".into(), schema(&[("id", "bigint")]))]);
        // The shape a file destination produces: the target name is carried
        // through, and the row is marked as a creation.
        let mappings = vec![TableMapping {
            create_new: true,
            ..TableMapping::auto("users")
        }];

        let without_catalog = inspect_tables(
            &source_tables,
            &[],
            &mappings,
            &source_schemas,
            &target_schemas,
            TransferMode::Data,
            &HashMap::new(),
        );
        let with_catalog = inspect_tables(
            &source_tables,
            &[table("users"), table("orders")],
            &mappings,
            &source_schemas,
            &target_schemas,
            TransferMode::Data,
            &HashMap::new(),
        );

        // A row that creates its target resolves before any catalog lookup, so
        // handing this caller an empty target list changes nothing it can
        // observe about that row. Callers with no live target catalog may pass
        // an empty list without losing information. A populated catalog still
        // contributes its own additional rows; only the mapped row is pinned.
        assert_eq!(without_catalog[0].status, TableMappingStatus::CreateNew);
        let created_with_catalog = with_catalog
            .iter()
            .find(|result| result.source_table == "users")
            .expect("the mapped row must still be present");
        assert_eq!(&without_catalog[0], created_with_catalog);
    }
}
