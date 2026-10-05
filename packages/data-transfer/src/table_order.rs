//! Foreign-key-aware ordering for database-target Data Transfer writes.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use datazen_driver_api::{TableInfo, TableSchema, TableType};

use super::error::TransferError;
use super::model::{
    Endpoint, TableInspectResult, TransferMode, TransferPreview, WriteMode, WritePlanItem,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetTableDependency {
    ParentBeforeChild {
        parent_source_table: String,
        child_source_table: String,
    },
    Unresolved {
        child_source_table: String,
        reference: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Namespace {
    Known(String),
    Unknown,
    Ambiguous,
}

/// Capture only dependencies among tables that are part of the reviewed plan.
/// An unselected parent is deliberately not made a requirement: its rows may
/// already exist at the target and partial selection must remain usable.
pub fn capture_target_fk_dependencies(
    write_plans: &[WritePlanItem],
    target_schemas: &HashMap<String, TableSchema>,
    target_relations: &[TableInfo],
    target: &Endpoint,
) -> Vec<TargetTableDependency> {
    let mut dependencies = Vec::new();

    for child in write_plans {
        let Some(schema) = target_schemas.get(&child.target_table) else {
            continue;
        };

        for foreign_key in &schema.foreign_keys {
            let reference = foreign_key.referenced_table.trim();
            if reference.is_empty() {
                dependencies.push(TargetTableDependency::Unresolved {
                    child_source_table: child.source_table.clone(),
                    reference: "<empty foreign-key target>".into(),
                });
                continue;
            }

            let mut matches = Vec::new();
            let mut unresolved = false;
            for parent in write_plans {
                let Some(prefix) = reference_prefix(reference, &parent.target_table) else {
                    continue;
                };
                let namespace = target_namespace(&parent.target_table, target_relations, target);
                let resolves = match prefix {
                    None => matches!(namespace, Namespace::Known(_)),
                    Some(prefix) => match namespace {
                        Namespace::Known(namespace) => prefix == namespace,
                        Namespace::Unknown | Namespace::Ambiguous => {
                            unresolved = true;
                            false
                        }
                    },
                };

                if resolves {
                    // A dotted table name is indistinguishable from a
                    // schema-qualified relation in the current string-only
                    // ForeignKeyInfo contract. Do not guess between them.
                    if prefix.is_none() && parent.target_table.contains('.') {
                        unresolved = true;
                        continue;
                    }
                    matches.push(parent.source_table.clone());
                }
            }

            matches.sort();
            matches.dedup();
            if unresolved || matches.len() > 1 {
                dependencies.push(TargetTableDependency::Unresolved {
                    child_source_table: child.source_table.clone(),
                    reference: reference.to_string(),
                });
            } else if let Some(parent_source_table) = matches.pop() {
                dependencies.push(TargetTableDependency::ParentBeforeChild {
                    parent_source_table,
                    child_source_table: child.source_table.clone(),
                });
            }
        }
    }

    dependencies.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
    dependencies.dedup();
    dependencies
}

fn reference_prefix<'a>(reference: &'a str, target_table: &str) -> Option<Option<&'a str>> {
    if reference == target_table {
        return Some(None);
    }
    let suffix = format!(".{target_table}");
    reference
        .strip_suffix(&suffix)
        .map(|prefix| Some(prefix.trim()))
}

fn target_namespace(
    target_table: &str,
    target_relations: &[TableInfo],
    target: &Endpoint,
) -> Namespace {
    let mut schemas: Vec<String> = target_relations
        .iter()
        .filter(|relation| {
            relation.name == target_table
                && matches!(relation.table_type, TableType::Table)
                && super::metadata::table_in_endpoint_schema(target, relation)
        })
        .filter_map(|relation| {
            relation
                .schema
                .as_deref()
                .map(str::trim)
                .filter(|schema| !schema.is_empty())
                .map(str::to_string)
        })
        .collect();
    schemas.sort();
    schemas.dedup();

    match schemas.as_slice() {
        [schema] => return Namespace::Known(schema.clone()),
        [_, _, ..] => return Namespace::Ambiguous,
        _ => {}
    }

    if let Some(schema) = target.normalized_schema() {
        return Namespace::Known(schema.to_string());
    }

    let relation_exists = target_relations.iter().any(|relation| {
        relation.name == target_table
            && matches!(relation.table_type, TableType::Table)
            && super::metadata::table_in_endpoint_schema(target, relation)
    });
    if relation_exists {
        // Drivers whose namespace is the database/catalog (for example,
        // MySQL) may return no separate schema in TableInfo.
        return Namespace::Known(target.database.clone());
    }

    Namespace::Unknown
}

/// Order one run's selected mappings. Edges are ignored when their parent is
/// not selected, preserving the existing subset semantics.
pub fn order_selected_tables(
    selected_source_tables: &[String],
    dependencies: &[TargetTableDependency],
    write_mode: WriteMode,
) -> Result<Vec<String>, TransferError> {
    let mut selected = Vec::new();
    let mut selected_set = HashSet::new();
    for table in selected_source_tables {
        if selected_set.insert(table.clone()) {
            selected.push(table.clone());
        }
    }
    let selected_positions: HashMap<&str, usize> = selected
        .iter()
        .enumerate()
        .map(|(index, table)| (table.as_str(), index))
        .collect();

    let mut edges = HashSet::new();
    for dependency in dependencies {
        match dependency {
            TargetTableDependency::ParentBeforeChild {
                parent_source_table,
                child_source_table,
            } if selected_set.contains(parent_source_table)
                && selected_set.contains(child_source_table) =>
            {
                edges.insert((parent_source_table.clone(), child_source_table.clone()));
            }
            TargetTableDependency::Unresolved {
                child_source_table,
                reference,
            } if selected_set.contains(child_source_table) => {
                return Err(TransferError::validation(format!(
                    "cannot safely order selected target table '{child_source_table}': its foreign-key reference '{reference}' cannot be matched unambiguously to a selected target table; choose an explicit target schema or correct the table mapping before writing"
                )));
            }
            _ => {}
        }
    }

    if !edges.is_empty() && write_mode != WriteMode::Insert {
        let mode = match write_mode {
            WriteMode::Insert => "Insert",
            WriteMode::TruncateInsert => "Truncate + Insert",
            WriteMode::DropCreateInsert => "Drop + Create + Insert",
        };
        return Err(TransferError::validation(format!(
            "selected target tables have foreign-key dependencies; {mode} cannot safely interleave its destructive step with parent-before-child writes. Use Insert or select a non-dependent subset"
        )));
    }

    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut indegree: HashMap<String, usize> =
        selected.iter().cloned().map(|table| (table, 0)).collect();
    for (parent, child) in edges {
        children.entry(parent).or_default().push(child.clone());
        if let Some(degree) = indegree.get_mut(&child) {
            *degree += 1;
        }
    }
    for child_list in children.values_mut() {
        child_list.sort_by_key(|table| selected_positions.get(table.as_str()).copied());
    }

    let mut ready = BinaryHeap::new();
    for table in &selected {
        if indegree.get(table).copied() == Some(0) {
            if let Some(position) = selected_positions.get(table.as_str()) {
                ready.push(Reverse((*position, table.clone())));
            }
        }
    }

    let mut ordered = Vec::with_capacity(selected.len());
    while let Some(Reverse((_position, parent))) = ready.pop() {
        ordered.push(parent.clone());
        if let Some(dependents) = children.get(&parent) {
            for child in dependents {
                let Some(degree) = indegree.get_mut(child) else {
                    continue;
                };
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    if let Some(position) = selected_positions.get(child.as_str()) {
                        ready.push(Reverse((*position, child.clone())));
                    }
                }
            }
        }
    }

    if ordered.len() != selected.len() {
        let cycle_tables: Vec<_> = selected
            .iter()
            .filter(|table| indegree.get(*table).copied().unwrap_or_default() > 0)
            .cloned()
            .collect();
        return Err(TransferError::validation(format!(
            "selected target tables contain a foreign-key dependency cycle: {}. Data Transfer commits each table separately and cannot defer these constraints across tables; select an acyclic subset or change the target constraints before writing",
            cycle_tables.join(", ")
        )));
    }

    Ok(ordered)
}

pub fn reorder_inspected_tables(
    inspected: &mut [TableInspectResult],
    ordered_source_tables: &[String],
) {
    let positions: HashMap<&str, usize> = ordered_source_tables
        .iter()
        .enumerate()
        .map(|(index, table)| (table.as_str(), index))
        .collect();
    inspected.sort_by_key(|table| {
        positions
            .get(table.source_table.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
}

pub fn reorder_preview_write_plans(
    preview: &mut TransferPreview,
    dependencies: &[TargetTableDependency],
) {
    if preview.mode != TransferMode::Data {
        return;
    }
    let selected: Vec<_> = preview
        .write_plans
        .iter()
        .map(|table| table.source_table.clone())
        .collect();
    match order_selected_tables(&selected, dependencies, preview.write_mode) {
        Ok(order) => {
            let positions: HashMap<&str, usize> = order
                .iter()
                .enumerate()
                .map(|(index, table)| (table.as_str(), index))
                .collect();
            preview.write_plans.sort_by_key(|table| {
                positions
                    .get(table.source_table.as_str())
                    .copied()
                    .unwrap_or(usize::MAX)
            });
        }
        Err(error) => preview.warnings.push(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo};

    fn endpoint(schema: Option<&str>) -> Endpoint {
        Endpoint {
            db_session_id: "target-session".into(),
            database: "target-db".into(),
            schema: schema.map(str::to_string),
        }
    }

    fn plan(source: &str, target: &str) -> WritePlanItem {
        WritePlanItem {
            source_table: source.into(),
            target_table: target.into(),
            write_mode: WriteMode::Insert,
            mapped_columns: Vec::new(),
            estimated_rows: None,
            preamble: Vec::new(),
            source_filter_preview: None,
            recordset_preview: None,
        }
    }

    fn schema(table: &str, foreign_keys: Vec<ForeignKeyInfo>) -> TableSchema {
        TableSchema {
            table_name: table.into(),
            columns: Vec::new(),
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys,
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    fn fk(reference: &str) -> ForeignKeyInfo {
        ForeignKeyInfo {
            name: "fk_test".into(),
            columns: vec!["parent_id".into()],
            referenced_table: reference.into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        }
    }

    fn relation(name: &str, schema: Option<&str>) -> TableInfo {
        TableInfo {
            name: name.into(),
            schema: schema.map(str::to_string),
            table_type: TableType::Table,
            row_count: None,
        }
    }

    #[test]
    fn selected_tables_order_parent_before_child_and_allow_partial_selection() {
        let write_plans = vec![plan("child", "child"), plan("parent", "parent")];
        let target_schemas = HashMap::from([
            ("child".into(), schema("child", vec![fk("public.parent")])),
            ("parent".into(), schema("parent", vec![])),
        ]);
        let dependencies = capture_target_fk_dependencies(
            &write_plans,
            &target_schemas,
            &[
                relation("child", Some("public")),
                relation("parent", Some("public")),
            ],
            &endpoint(None),
        );
        let ordered = order_selected_tables(
            &["child".into(), "parent".into()],
            &dependencies,
            WriteMode::Insert,
        )
        .unwrap();

        assert_eq!(ordered, ["parent", "child"]);
        assert_eq!(
            order_selected_tables(&["child".into()], &dependencies, WriteMode::Insert).unwrap(),
            ["child"]
        );
    }

    #[test]
    fn selected_foreign_key_cycles_fail_before_execution_even_if_deferrable() {
        let dependencies = vec![
            TargetTableDependency::ParentBeforeChild {
                parent_source_table: "a".into(),
                child_source_table: "b".into(),
            },
            TargetTableDependency::ParentBeforeChild {
                parent_source_table: "b".into(),
                child_source_table: "a".into(),
            },
        ];

        let error =
            order_selected_tables(&["a".into(), "b".into()], &dependencies, WriteMode::Insert)
                .unwrap_err();

        assert!(error.to_string().contains("foreign-key dependency cycle"));
        assert!(error.to_string().contains("commits each table separately"));
    }

    #[test]
    fn ambiguous_foreign_key_namespace_is_rejected_only_if_child_is_selected() {
        let write_plans = vec![plan("child", "child"), plan("parent", "parent")];
        let target_schemas = HashMap::from([
            ("child".into(), schema("child", vec![fk("public.parent")])),
            ("parent".into(), schema("parent", vec![])),
        ]);
        let dependencies = capture_target_fk_dependencies(
            &write_plans,
            &target_schemas,
            &[relation("child", Some("public"))],
            &endpoint(None),
        );

        assert!(
            order_selected_tables(&["parent".into()], &dependencies, WriteMode::Insert).is_ok()
        );
        let error = order_selected_tables(
            &["child".into(), "parent".into()],
            &dependencies,
            WriteMode::Insert,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("cannot be matched unambiguously"));
    }

    #[test]
    fn destructive_write_modes_reject_selected_fk_dependencies_before_writes() {
        let dependencies = vec![TargetTableDependency::ParentBeforeChild {
            parent_source_table: "parent".into(),
            child_source_table: "child".into(),
        }];

        let error = order_selected_tables(
            &["parent".into(), "child".into()],
            &dependencies,
            WriteMode::TruncateInsert,
        )
        .unwrap_err();

        assert!(error.to_string().contains("cannot safely interleave"));
    }
}
