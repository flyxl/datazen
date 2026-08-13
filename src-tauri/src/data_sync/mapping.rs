//! Table mapping + per-table gate → TableResult (no row compare yet).

use std::collections::{HashMap, HashSet};

use datazen_driver_api::{TableInfo, TableSchema, TableType};

use super::gate::{check_base_table, check_table_gate, GateVerdict};
use super::model::{TableMapping, TableMappingStatus, TableResult};

pub fn classify_tables(
    family: &str,
    source_tables: &[TableInfo],
    target_tables: &[TableInfo],
    mappings: &[TableMapping],
    source_schemas: &HashMap<String, TableSchema>,
    target_schemas: &HashMap<String, TableSchema>,
) -> Vec<TableResult> {
    let source_by_name: HashMap<&str, &TableInfo> =
        source_tables.iter().map(|t| (t.name.as_str(), t)).collect();
    let target_by_name: HashMap<&str, &TableInfo> =
        target_tables.iter().map(|t| (t.name.as_str(), t)).collect();

    let mut results = Vec::new();
    let mut mapped_sources = HashSet::new();
    let mut mapped_targets = HashSet::new();

    let effective: Vec<TableMapping> = if mappings.is_empty() {
        source_tables
            .iter()
            .filter(|t| matches!(t.table_type, TableType::Table))
            .filter(|t| target_by_name.contains_key(t.name.as_str()))
            .map(|t| TableMapping::auto(&t.name))
            .collect()
    } else {
        mappings.to_vec()
    };

    for mapping in &effective {
        mapped_sources.insert(mapping.source_table.clone());
        if !mapping.target_table.is_empty() {
            mapped_targets.insert(mapping.target_table.clone());
        }

        if !mapping.enabled {
            results.push(TableResult::disabled(
                &mapping.source_table,
                &mapping.target_table,
            ));
            continue;
        }

        let src_info = source_by_name.get(mapping.source_table.as_str());
        let tgt_info = target_by_name.get(mapping.target_table.as_str());

        if src_info.is_none() {
            let mut r = TableResult::incompatible(
                &mapping.source_table,
                &mapping.target_table,
                format!("source table '{}' not found", mapping.source_table),
            );
            r.status = TableMappingStatus::Incompatible;
            results.push(r);
            continue;
        }
        if tgt_info.is_none() {
            results.push(TableResult::incompatible(
                &mapping.source_table,
                &mapping.target_table,
                format!("target table '{}' not found", mapping.target_table),
            ));
            continue;
        }

        let mut issues = Vec::new();
        if let Some(issue) = check_base_table(
            &src_info.unwrap().table_type,
            "source",
            &mapping.source_table,
        ) {
            issues.push(issue.message);
        }
        if let Some(issue) = check_base_table(
            &tgt_info.unwrap().table_type,
            "target",
            &mapping.target_table,
        ) {
            issues.push(issue.message);
        }
        if !issues.is_empty() {
            results.push(TableResult::incompatible(
                &mapping.source_table,
                &mapping.target_table,
                issues.join("; "),
            ));
            continue;
        }

        let Some(src_schema) = source_schemas.get(&mapping.source_table) else {
            results.push(TableResult::incompatible(
                &mapping.source_table,
                &mapping.target_table,
                format!(
                    "source schema for '{}' was not loaded",
                    mapping.source_table
                ),
            ));
            continue;
        };
        let Some(tgt_schema) = target_schemas.get(&mapping.target_table) else {
            results.push(TableResult::incompatible(
                &mapping.source_table,
                &mapping.target_table,
                format!(
                    "target schema for '{}' was not loaded",
                    mapping.target_table
                ),
            ));
            continue;
        };

        match check_table_gate(family, src_schema, tgt_schema) {
            GateVerdict::Compatible { .. } => {
                results.push(TableResult::matched(
                    &mapping.source_table,
                    &mapping.target_table,
                    vec![],
                ));
            }
            GateVerdict::Incompatible { issues } => {
                results.push(TableResult::incompatible(
                    &mapping.source_table,
                    &mapping.target_table,
                    issues
                        .into_iter()
                        .map(|i| i.message)
                        .collect::<Vec<_>>()
                        .join("; "),
                ));
            }
        }
    }

    if mappings.is_empty() {
        for src in source_tables {
            if matches!(src.table_type, TableType::Table) && !mapped_sources.contains(&src.name) {
                results.push(TableResult::unmapped_source(&src.name));
            }
        }
        for tgt in target_tables {
            if matches!(tgt.table_type, TableType::Table) && !mapped_targets.contains(&tgt.name) {
                results.push(TableResult::unmapped_target(&tgt.name));
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use datazen_driver_api::{ColumnSchema, TableSchema};

    use super::*;

    fn table_info(name: &str, table_type: TableType) -> TableInfo {
        TableInfo {
            name: name.into(),
            schema: None,
            table_type,
            row_count: None,
        }
    }

    fn col(name: &str, ty: &str, pk: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: ty.into(),
            nullable: !pk,
            default_value: None,
            comment: None,
            is_primary_key: pk,
            is_auto_increment: pk,
        }
    }

    fn schema(name: &str, cols: Vec<ColumnSchema>) -> TableSchema {
        let pks: Vec<String> = cols
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        TableSchema {
            table_name: name.into(),
            columns: cols,
            primary_keys: pks,
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    fn users() -> TableSchema {
        schema(
            "users",
            vec![col("id", "INT", true), col("n", "INT", false)],
        )
    }

    #[test]
    fn auto_maps_same_name_and_marks_unmapped() {
        let source = vec![
            table_info("users", TableType::Table),
            table_info("only_src", TableType::Table),
            table_info("v", TableType::View),
        ];
        let target = vec![
            table_info("users", TableType::Table),
            table_info("only_tgt", TableType::Table),
        ];
        let mut schemas_src = HashMap::new();
        schemas_src.insert("users".into(), users());
        let mut schemas_tgt = HashMap::new();
        schemas_tgt.insert("users".into(), users());
        let results = classify_tables("mysql", &source, &target, &[], &schemas_src, &schemas_tgt);
        assert!(results
            .iter()
            .any(|r| { r.status == TableMappingStatus::Matched && r.source_table == "users" }));
        assert!(results.iter().any(
            |r| r.status == TableMappingStatus::UnmappedSource && r.source_table == "only_src"
        ));
        assert!(results.iter().any(
            |r| r.status == TableMappingStatus::UnmappedTarget && r.target_table == "only_tgt"
        ));
        assert!(!results.iter().any(|r| r.source_table == "v"));
    }

    #[test]
    fn disabled_mapping_skips_gate() {
        let mut mapping = TableMapping::auto("users");
        mapping.enabled = false;
        let source = vec![table_info("users", TableType::Table)];
        let target = vec![table_info("users", TableType::Table)];
        let results = classify_tables(
            "mysql",
            &source,
            &target,
            &[mapping],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TableMappingStatus::Disabled);
    }

    #[test]
    fn renamed_mapping_still_requires_identical_structure() {
        let mapping = TableMapping::mapped("customers", "clients");
        let source = vec![table_info("customers", TableType::Table)];
        let target = vec![table_info("clients", TableType::Table)];
        let mut src_s = HashMap::new();
        src_s.insert(
            "customers".into(),
            schema(
                "customers",
                vec![col("id", "INT", true), col("n", "INT", false)],
            ),
        );
        let mut tgt_s = HashMap::new();
        tgt_s.insert(
            "clients".into(),
            schema(
                "clients",
                vec![col("id", "INT", true), col("n", "INT", false)],
            ),
        );
        let results = classify_tables("mysql", &source, &target, &[mapping], &src_s, &tgt_s);
        assert_eq!(results[0].status, TableMappingStatus::Matched);
        assert_eq!(results[0].target_table, "clients");
    }

    #[test]
    fn missing_table_or_schema_or_view_is_incompatible() {
        let source = vec![table_info("users", TableType::Table)];
        let target = vec![table_info("users", TableType::View)];
        let results = classify_tables(
            "mysql",
            &source,
            &target,
            &[TableMapping::auto("users")],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(results[0].status, TableMappingStatus::Incompatible);

        let results = classify_tables(
            "mysql",
            &source,
            &[],
            &[TableMapping::auto("users")],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(results[0]
            .incompatible_reason
            .as_deref()
            .unwrap()
            .contains("target table"));

        let results = classify_tables(
            "mysql",
            &[],
            &target,
            &[TableMapping::auto("ghost")],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(results[0]
            .incompatible_reason
            .as_deref()
            .unwrap()
            .contains("source table"));
    }

    #[test]
    fn structure_mismatch_after_rename_is_incompatible() {
        let mapping = TableMapping::mapped("a", "b");
        let source = vec![table_info("a", TableType::Table)];
        let target = vec![table_info("b", TableType::Table)];
        let mut src_s = HashMap::new();
        src_s.insert("a".into(), schema("a", vec![col("id", "INT", true)]));
        let mut tgt_s = HashMap::new();
        tgt_s.insert(
            "b".into(),
            schema(
                "b",
                vec![col("id", "INT", true), col("extra", "INT", false)],
            ),
        );
        let results = classify_tables("mysql", &source, &target, &[mapping], &src_s, &tgt_s);
        assert_eq!(results[0].status, TableMappingStatus::Incompatible);
        assert!(results[0]
            .incompatible_reason
            .as_deref()
            .unwrap()
            .contains("extra"));
    }

    #[test]
    fn missing_schema_payload_is_incompatible() {
        let source = vec![table_info("users", TableType::Table)];
        let target = vec![table_info("users", TableType::Table)];
        let mut src_s = HashMap::new();
        src_s.insert("users".into(), users());
        let results = classify_tables(
            "mysql",
            &source,
            &target,
            &[TableMapping::auto("users")],
            &src_s,
            &HashMap::new(),
        );
        assert!(results[0]
            .incompatible_reason
            .as_deref()
            .unwrap()
            .contains("target schema"));
        let results = classify_tables(
            "mysql",
            &source,
            &target,
            &[TableMapping::auto("users")],
            &HashMap::new(),
            &src_s,
        );
        assert!(results[0]
            .incompatible_reason
            .as_deref()
            .unwrap()
            .contains("source schema"));
    }
}
