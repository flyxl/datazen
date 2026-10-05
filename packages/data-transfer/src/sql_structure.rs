//! SQL-file structure object planning and dialect rendering.

use std::collections::HashMap;

use datazen_driver_api::{DatabaseDriver, IRForeignKey, IRIndex, IRTableObjects, TableSchema};

use super::error::TransferError;
use super::model::{
    DdlPreviewItem, DdlPreviewKind, TableInspectResult, TableMappingStatus, TransferJob,
};
use super::sql_file::{create_table_sql, create_table_sql_with_target, target_table_ref};
use super::structure::table_mapping_for;
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum NamedStructureObject {
    Index,
    ForeignKey,
}

type PlannedObjectNames = HashMap<(NamedStructureObject, String, String), (String, String)>;

fn ensure_unique_object_name(
    seen: &mut PlannedObjectNames,
    target_adapter: &dyn SyncTargetAdapter,
    kind: NamedStructureObject,
    name: &str,
    target_table: &str,
    source_table: &str,
) -> Result<(), TransferError> {
    let table_scoped = match kind {
        NamedStructureObject::Index => target_adapter.index_names_are_table_scoped(),
        NamedStructureObject::ForeignKey => target_adapter.foreign_key_names_are_table_scoped(),
    };
    let case_sensitive = target_adapter.object_names_are_case_sensitive();
    let namespace = if table_scoped {
        if case_sensitive {
            target_table.to_string()
        } else {
            target_table.to_ascii_lowercase()
        }
    } else {
        "<target-schema>".into()
    };
    let normalized_name = if case_sensitive {
        name.to_string()
    } else {
        name.to_ascii_lowercase()
    };
    let key = (kind, namespace, normalized_name);
    if let Some((previous_source, previous_target)) = seen.get(&key) {
        let object_kind = match kind {
            NamedStructureObject::Index => "index",
            NamedStructureObject::ForeignKey => "foreign key",
        };
        return Err(TransferError::validation(format!(
            "target {object_kind} name '{}' collides between '{}.{}' and '{}.{}'; rename one source object before transfer",
            name, previous_source, previous_target, source_table, target_table
        )));
    }
    seen.insert(key, (source_table.to_string(), target_table.to_string()));
    Ok(())
}

fn object_column_mapping<'a>(
    mapping: Option<&'a super::model::TableMapping>,
    source_column: &str,
) -> Result<String, TransferError> {
    let Some(mapping) = mapping else {
        return Ok(source_column.to_string());
    };
    if mapping.column_mappings.is_empty() {
        return Ok(source_column.to_string());
    }
    let Some(column) = mapping
        .column_mappings
        .iter()
        .find(|column| column.source_column == source_column)
    else {
        return Err(TransferError::validation(format!(
            "source constraint column '{}' is not mapped",
            source_column
        )));
    };
    if column.skip || column.target_column.trim().is_empty() {
        return Err(TransferError::validation(format!(
            "source constraint column '{}' is skipped or has no target name",
            source_column
        )));
    }
    Ok(column.target_column.clone())
}

fn map_index_columns(
    index: &IRIndex,
    mapping: Option<&super::model::TableMapping>,
) -> Result<IRIndex, TransferError> {
    Ok(IRIndex {
        name: index.name.clone(),
        columns: index
            .columns
            .iter()
            .map(|column| object_column_mapping(mapping, column))
            .collect::<Result<Vec<_>, _>>()?,
        is_unique: index.is_unique,
        is_primary: index.is_primary,
        index_type: index.index_type.clone(),
    })
}

fn map_foreign_key_columns(
    foreign_key: &IRForeignKey,
    source_mapping: Option<&super::model::TableMapping>,
    referenced_mapping: Option<&super::model::TableMapping>,
) -> Result<IRForeignKey, TransferError> {
    if foreign_key.columns.len() != foreign_key.referenced_columns.len() {
        return Err(TransferError::validation(format!(
            "foreign key '{}' has mismatched column lists",
            foreign_key.name
        )));
    }
    Ok(IRForeignKey {
        name: foreign_key.name.clone(),
        columns: foreign_key
            .columns
            .iter()
            .map(|column| object_column_mapping(source_mapping, column))
            .collect::<Result<Vec<_>, _>>()?,
        referenced_table: foreign_key.referenced_table.clone(),
        referenced_columns: foreign_key
            .referenced_columns
            .iter()
            .map(|column| object_column_mapping(referenced_mapping, column))
            .collect::<Result<Vec<_>, _>>()?,
        on_update: foreign_key.on_update.clone(),
        on_delete: foreign_key.on_delete.clone(),
    })
}

fn structure_table_order(
    inspected: &[TableInspectResult],
    schemas: &HashMap<String, TableSchema>,
    job: &TransferJob,
) -> Result<Vec<String>, TransferError> {
    use std::collections::{BTreeSet, HashMap as StdHashMap};

    let selected: BTreeSet<String> = inspected
        .iter()
        .filter(|table| table.enabled && schemas.contains_key(&table.source_table))
        .map(|table| table.source_table.clone())
        .collect();
    let mut target_names = StdHashMap::new();
    for source in &selected {
        let mapping = table_mapping_for(job, source).ok_or_else(|| {
            TransferError::validation(format!("missing table mapping for '{source}'"))
        })?;
        target_names.insert(source.clone(), mapping.target_table.clone());
    }

    // Edges point from a referenced parent to its child. Foreign keys are
    // emitted after all data, but this ordering still makes the artifact
    // deterministic and gives consumers a safe table creation sequence.
    let mut outgoing: StdHashMap<String, BTreeSet<String>> = StdHashMap::new();
    let mut indegree: StdHashMap<String, usize> =
        selected.iter().map(|table| (table.clone(), 0)).collect();
    for source in &selected {
        let schema = schemas.get(source).ok_or_else(|| {
            TransferError::validation(format!("source schema is unavailable for '{source}'"))
        })?;
        for foreign_key in &schema.foreign_keys {
            let referenced_source =
                canonical_source_table_name(&foreign_key.referenced_table, job, &selected)?;
            if referenced_source == *source {
                continue;
            }
            let children = outgoing.entry(referenced_source).or_default();
            if children.insert(source.clone()) {
                *indegree.entry(source.clone()).or_default() += 1;
            }
        }
    }

    let mut ready: BTreeSet<(String, String)> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(source, _)| {
            (
                target_names.get(source).cloned().unwrap_or_default(),
                source.clone(),
            )
        })
        .collect();
    let mut order = Vec::with_capacity(selected.len());
    while let Some((_, source)) = ready.pop_first() {
        order.push(source.clone());
        if let Some(children) = outgoing.get(&source) {
            for child in children {
                let degree = indegree
                    .get_mut(child)
                    .ok_or_else(|| TransferError::validation("invalid FK dependency graph"))?;
                *degree -= 1;
                if *degree == 0 {
                    ready.insert((
                        target_names.get(child).cloned().unwrap_or_default(),
                        child.clone(),
                    ));
                }
            }
        }
    }
    if order.len() != selected.len() {
        // Cycles are valid when constraints are deferred until after data.
        // Keep the cyclic remainder stable rather than emitting source order.
        let emitted: BTreeSet<_> = order.iter().cloned().collect();
        order.extend(
            selected
                .iter()
                .filter(|source| !emitted.contains(*source))
                .cloned(),
        );
    }
    Ok(order)
}

fn structure_has_cycle(
    tables: &[TableInspectResult],
    schemas: &HashMap<String, TableSchema>,
    job: &TransferJob,
) -> Result<bool, TransferError> {
    use std::collections::{BTreeSet, HashMap as StdHashMap};

    let selected: BTreeSet<String> = tables
        .iter()
        .filter(|table| table.enabled && schemas.contains_key(&table.source_table))
        .map(|table| table.source_table.clone())
        .collect();
    let mut outgoing: StdHashMap<String, BTreeSet<String>> = StdHashMap::new();
    let mut indegree: StdHashMap<String, usize> =
        selected.iter().map(|table| (table.clone(), 0)).collect();
    for source in &selected {
        let schema = schemas.get(source).ok_or_else(|| {
            TransferError::validation(format!("source schema is unavailable for '{source}'"))
        })?;
        for foreign_key in &schema.foreign_keys {
            let parent =
                canonical_source_table_name(&foreign_key.referenced_table, job, &selected)?;
            if parent != *source && outgoing.entry(parent).or_default().insert(source.clone()) {
                *indegree.entry(source.clone()).or_default() += 1;
            }
        }
    }
    let mut ready: BTreeSet<String> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(table, _)| table.clone())
        .collect();
    let mut visited = 0;
    while let Some(source) = ready.pop_first() {
        visited += 1;
        if let Some(children) = outgoing.get(&source) {
            for child in children {
                let degree = indegree
                    .get_mut(child)
                    .ok_or_else(|| TransferError::validation("invalid FK dependency graph"))?;
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(child.clone());
                }
            }
        }
    }
    Ok(visited != selected.len())
}

/// Resolve driver-reported FK names to the source mapping key. PostgreSQL
/// reports `schema.table`; the transfer plan keys tables by their selected
/// bare name, so strip only the explicitly selected schema. An FK into any
/// other schema remains a dependency error instead of being guessed by its
/// final name segment.
fn canonical_source_table_name(
    referenced_table: &str,
    job: &TransferJob,
    selected: &std::collections::BTreeSet<String>,
) -> Result<String, TransferError> {
    if selected.contains(referenced_table) {
        return Ok(referenced_table.to_string());
    }
    if let Some((schema, table)) = referenced_table.split_once('.') {
        if job.source.normalized_schema() == Some(schema) && selected.contains(table) {
            return Ok(table.to_string());
        }
    }
    Err(TransferError::validation(format!(
        "foreign key references unselected or out-of-scope table '{referenced_table}'"
    )))
}

/// Build the reviewed database-target structure plan. Preview stores this
/// sequence on the server-side plan and execution consumes it verbatim, so
/// table/object ordering and all name mappings are fixed before the first
/// target write.
pub fn build_database_structure_plan(
    source_adapter: &dyn SyncSourceAdapter,
    target_adapter: &dyn SyncTargetAdapter,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
) -> Result<Vec<DdlPreviewItem>, TransferError> {
    let create_tables: Vec<_> = inspected
        .iter()
        .filter(|table| {
            table.enabled
                && (table.status == TableMappingStatus::CreateNew
                    || (job.mode == super::model::TransferMode::StructureAndData
                        && job.write_mode == super::model::WriteMode::DropCreateInsert
                        && table.status == TableMappingStatus::Matched))
                && source_schemas.contains_key(&table.source_table)
        })
        .cloned()
        .collect();
    let is_drop_create = job.mode == super::model::TransferMode::StructureAndData
        && job.write_mode == super::model::WriteMode::DropCreateInsert;
    if is_drop_create && structure_has_cycle(&create_tables, source_schemas, job)? {
        return Err(TransferError::unsupported(
            "Drop + Create cannot safely replace mutually referencing tables; use Insert or remove the cycle before retrying",
        ));
    }
    let order = structure_table_order(&create_tables, source_schemas, job)?;
    let mut statements = Vec::new();
    let mut object_sets = Vec::new();

    if is_drop_create {
        for source_table in order.iter().rev() {
            let table = create_tables
                .iter()
                .find(|table| table.source_table == *source_table)
                .ok_or_else(|| TransferError::validation("structure table disappeared"))?;
            let ddl = format!(
                "DROP TABLE IF EXISTS {}",
                super::structure::target_relation_ref(job, &table.target_table, target_adapter)
            );
            statements.push(DdlPreviewItem {
                source_table: source_table.clone(),
                target_table: table.target_table.clone(),
                ddl,
                kind: DdlPreviewKind::DropTable,
                depends_on: Vec::new(),
            });
        }
    }

    for source_table in &order {
        let table = create_tables
            .iter()
            .find(|table| table.source_table == *source_table)
            .ok_or_else(|| TransferError::validation("structure table disappeared"))?;
        let schema = source_schemas.get(source_table).ok_or_else(|| {
            TransferError::validation(format!("source schema is unavailable for '{source_table}'"))
        })?;
        let ddl = super::structure::mapped_create_ddl(
            source_adapter,
            target_adapter,
            schema,
            table,
            job,
        )?;
        statements.push(DdlPreviewItem {
            source_table: source_table.clone(),
            target_table: table.target_table.clone(),
            ddl,
            kind: DdlPreviewKind::Table,
            depends_on: Vec::new(),
        });

        object_sets.push((
            source_table.clone(),
            table.target_table.clone(),
            source_adapter.table_objects_to_ir(schema),
        ));
    }

    let selected: std::collections::BTreeSet<String> = create_tables
        .iter()
        .map(|table| table.source_table.clone())
        .collect();
    let mut indexes = Vec::new();
    let mut foreign_keys = Vec::new();
    let mut seen_names = PlannedObjectNames::new();
    for (source_table, target_table, objects) in object_sets {
        let mapping = table_mapping_for(job, &source_table);
        for index in objects.indexes.iter().filter(|index| !index.is_primary) {
            let mapped = map_index_columns(index, mapping)?;
            let table = create_tables
                .iter()
                .find(|table| table.source_table == source_table)
                .ok_or_else(|| TransferError::validation("structure table disappeared"))?;
            let schema = source_schemas.get(&source_table).ok_or_else(|| {
                TransferError::validation(format!(
                    "source schema is unavailable for '{source_table}'"
                ))
            })?;
            let target_ir = super::structure::mapped_target_table_ir(
                source_adapter,
                target_adapter,
                schema,
                table,
                job,
            )?;
            let mut index_columns = Vec::with_capacity(mapped.columns.len());
            for column_name in &mapped.columns {
                let column = target_ir
                    .columns
                    .iter()
                    .find(|column| column.name == *column_name)
                    .ok_or_else(|| {
                        TransferError::validation(format!(
                            "mapped target index column '{column_name}' was not created"
                        ))
                    })?;
                index_columns.push(column.clone());
            }
            target_adapter
                .validate_index_columns(&index_columns)
                .map_err(|error| {
                    TransferError::unsupported(format!(
                        "cannot preserve index '{}' on '{}': {error}",
                        index.name, source_table
                    ))
                })?;
            let table_ref =
                super::structure::target_relation_ref(job, &target_table, target_adapter);
            let ddl = target_adapter
                .render_index_ddl(&table_ref, &mapped)
                .map_err(|error| {
                    TransferError::unsupported(format!(
                        "cannot preserve index '{}' on '{}': {error}",
                        index.name, source_table
                    ))
                })?;
            if let Some(ddl) = ddl {
                ensure_unique_object_name(
                    &mut seen_names,
                    target_adapter,
                    NamedStructureObject::Index,
                    &mapped.name,
                    &target_table,
                    &source_table,
                )?;
                indexes.push(DdlPreviewItem {
                    source_table: source_table.clone(),
                    target_table: target_table.clone(),
                    ddl,
                    kind: DdlPreviewKind::Index,
                    depends_on: vec![source_table.clone()],
                });
            }
        }
        for foreign_key in objects.foreign_keys {
            let referenced_source =
                canonical_source_table_name(&foreign_key.referenced_table, job, &selected)?;
            let referenced_mapping =
                table_mapping_for(job, &referenced_source).ok_or_else(|| {
                    TransferError::validation(format!(
                        "foreign key '{}' references unmapped table '{}'",
                        foreign_key.name, foreign_key.referenced_table
                    ))
                })?;
            let mapped = map_foreign_key_columns(&foreign_key, mapping, Some(referenced_mapping))?;
            let child_ref =
                super::structure::target_relation_ref(job, &target_table, target_adapter);
            let parent_ref = super::structure::target_relation_ref(
                job,
                &referenced_mapping.target_table,
                target_adapter,
            );
            let ddl = target_adapter
                .render_foreign_key_ddl(&child_ref, &mapped, &parent_ref)
                .map_err(|error| {
                    TransferError::unsupported(format!(
                        "cannot preserve foreign key '{}' on '{}': {error}",
                        foreign_key.name, source_table
                    ))
                })?;
            ensure_unique_object_name(
                &mut seen_names,
                target_adapter,
                NamedStructureObject::ForeignKey,
                &mapped.name,
                &target_table,
                &source_table,
            )?;
            foreign_keys.push(DdlPreviewItem {
                source_table: source_table.clone(),
                target_table: target_table.clone(),
                ddl,
                kind: DdlPreviewKind::ForeignKey,
                depends_on: vec![referenced_source],
            });
        }
    }
    indexes.sort_by(|left, right| {
        left.target_table
            .cmp(&right.target_table)
            .then(left.ddl.cmp(&right.ddl))
    });
    foreign_keys.sort_by(|left, right| {
        left.target_table
            .cmp(&right.target_table)
            .then(left.ddl.cmp(&right.ddl))
    });
    statements.extend(indexes);
    statements.extend(foreign_keys);
    Ok(statements)
}

/// Render every SQL-file structure object in a deterministic, dependency-aware
/// sequence. The returned vector is copied into the immutable transfer plan by
/// the command layer and consumed verbatim at execution time.
pub fn build_structure_plan(
    source_adapter: Option<&dyn SyncSourceAdapter>,
    target_adapter: Option<&dyn SyncTargetAdapter>,
    target_driver: &dyn DatabaseDriver,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
) -> Result<Vec<DdlPreviewItem>, TransferError> {
    super::sql_file::validate_target_dialect_job(job)?;
    if let Some(target) = job.sql_file_target.as_ref() {
        super::sql_file::validate_target_scope_for_driver(target_driver, target)?;
    }
    let order = structure_table_order(inspected, source_schemas, job)?;
    let mut statements = Vec::new();
    let mut object_sets: Vec<(String, String, IRTableObjects)> = Vec::new();

    for source_table in &order {
        let table = inspected
            .iter()
            .find(|table| table.enabled && table.source_table == *source_table)
            .ok_or_else(|| TransferError::validation("structure table disappeared"))?;
        let schema = source_schemas.get(source_table).ok_or_else(|| {
            TransferError::validation(format!("source schema is unavailable for '{source_table}'"))
        })?;
        let mapping = table_mapping_for(job, source_table);
        let ddl_override = mapping
            .and_then(|mapping| mapping.ddl_override.as_deref())
            .map(str::trim)
            .filter(|ddl| !ddl.is_empty());
        let ddl = match ddl_override {
            Some(ddl) => ddl.to_string(),
            None => match (source_adapter, target_adapter) {
                (Some(source_adapter), Some(target_adapter)) => create_table_sql_with_target(
                    source_adapter,
                    target_adapter,
                    target_driver,
                    job,
                    table,
                    schema,
                )?,
                _ => create_table_sql(target_driver, job, table, schema)?,
            },
        };
        statements.push(DdlPreviewItem {
            source_table: source_table.clone(),
            target_table: table.target_table.clone(),
            ddl,
            kind: DdlPreviewKind::Table,
            depends_on: Vec::new(),
        });

        let (Some(source_adapter), Some(_)) = (source_adapter, target_adapter) else {
            if !schema.indexes.is_empty() || !schema.foreign_keys.is_empty() {
                return Err(TransferError::unsupported(format!(
                    "SQL-file structure objects on '{}' require a registered source IR adapter",
                    source_table
                )));
            }
            continue;
        };
        object_sets.push((
            source_table.clone(),
            table.target_table.clone(),
            source_adapter.table_objects_to_ir(schema),
        ));
    }

    let mut indexes = Vec::new();
    let mut foreign_keys = Vec::new();
    let Some(target_adapter) = target_adapter else {
        if object_sets.is_empty() {
            return Ok(statements);
        }
        return Err(TransferError::unsupported(
            "SQL-file structure objects require a target IR adapter",
        ));
    };
    let mut seen_names = PlannedObjectNames::new();
    for (source_table, target_table, objects) in object_sets {
        let mapping = table_mapping_for(job, &source_table);
        for index in objects.indexes.iter().filter(|index| !index.is_primary) {
            let mapped = map_index_columns(index, mapping)?;
            let table_ref = target_table_ref(target_driver, job, &target_table);
            let rendered = target_adapter
                .render_index_ddl(&table_ref, &mapped)
                .map_err(|error| {
                    TransferError::unsupported(format!(
                        "cannot render index '{}' on '{}': {error}",
                        index.name, source_table
                    ))
                })?;
            if let Some(ddl) = rendered {
                ensure_unique_object_name(
                    &mut seen_names,
                    target_adapter,
                    NamedStructureObject::Index,
                    &mapped.name,
                    &target_table,
                    &source_table,
                )?;
                indexes.push(DdlPreviewItem {
                    source_table: source_table.clone(),
                    target_table: target_table.clone(),
                    ddl,
                    kind: DdlPreviewKind::Index,
                    depends_on: vec![source_table.clone()],
                });
            }
        }
        for foreign_key in objects.foreign_keys {
            let selected: std::collections::BTreeSet<String> = order.iter().cloned().collect();
            let referenced_source =
                canonical_source_table_name(&foreign_key.referenced_table, job, &selected)?;
            let referenced_target =
                table_mapping_for(job, &referenced_source).ok_or_else(|| {
                    TransferError::validation(format!(
                        "foreign key '{}' references unmapped table '{}'",
                        foreign_key.name, foreign_key.referenced_table
                    ))
                })?;
            let mapped = map_foreign_key_columns(&foreign_key, mapping, Some(referenced_target))?;
            let child_ref = target_table_ref(target_driver, job, &target_table);
            let parent_ref = target_table_ref(target_driver, job, &referenced_target.target_table);
            let ddl = target_adapter
                .render_foreign_key_ddl(&child_ref, &mapped, &parent_ref)
                .map_err(|error| {
                    TransferError::unsupported(format!(
                        "cannot render foreign key '{}' on '{}': {error}",
                        foreign_key.name, source_table
                    ))
                })?;
            ensure_unique_object_name(
                &mut seen_names,
                target_adapter,
                NamedStructureObject::ForeignKey,
                &mapped.name,
                &target_table,
                &source_table,
            )?;
            foreign_keys.push(DdlPreviewItem {
                source_table: source_table.clone(),
                target_table: target_table.clone(),
                ddl,
                kind: DdlPreviewKind::ForeignKey,
                depends_on: vec![referenced_source],
            });
        }
    }
    indexes.sort_by(|left, right| {
        left.target_table
            .cmp(&right.target_table)
            .then(left.ddl.cmp(&right.ddl))
    });
    foreign_keys.sort_by(|left, right| {
        left.target_table
            .cmp(&right.target_table)
            .then(left.ddl.cmp(&right.ddl))
    });
    statements.extend(indexes);
    statements.extend(foreign_keys);
    Ok(statements)
}

#[cfg(test)]
mod database_plan_tests {
    use super::*;
    use crate::model::{TransferMode, WriteMode};
    use crate::sql_structure_fixtures::*;
    use datazen_driver_api::ForeignKeyInfo;
    use datazen_driver_api::IndexInfo;

    #[test]
    fn database_structure_plan_maps_renamed_tables_columns_indexes_and_fks() {
        let mut parent = schema("parent", &[("id", true), ("code", false)]);
        parent.indexes.push(IndexInfo {
            name: "ix_parent_code".into(),
            columns: vec!["code".into()],
            is_unique: true,
            is_primary: false,
            index_type: "btree".into(),
        });
        let mut child = schema("child", &[("id", true), ("parent_id", false)]);
        child.foreign_keys.push(ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: "public.parent".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "CASCADE".into(),
            deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
        });
        let mappings = vec![
            mapping(
                "child",
                "child_copy",
                &[("id", "child_key"), ("parent_id", "parent_key")],
            ),
            mapping(
                "parent",
                "parent_copy",
                &[("id", "parent_key"), ("code", "external_code")],
            ),
        ];
        let inspected = vec![
            inspected(
                "child",
                "child_copy",
                &[("id", "child_key"), ("parent_id", "parent_key")],
            ),
            inspected(
                "parent",
                "parent_copy",
                &[("id", "parent_key"), ("code", "external_code")],
            ),
        ];
        let schemas = HashMap::from([("child".into(), child), ("parent".into(), parent)]);

        let plan =
            build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
                .expect("map database table structure");

        assert_eq!(plan.len(), 4);
        assert_eq!(plan[0].kind, DdlPreviewKind::Table);
        assert_eq!(plan[0].source_table, "parent");
        assert!(plan[0].ddl.contains(r#""archive"."parent_copy""#));
        assert!(plan[0].ddl.contains(r#""parent_key" INTEGER"#));
        assert_eq!(plan[1].kind, DdlPreviewKind::Table);
        assert_eq!(plan[1].source_table, "child");
        assert!(plan[2].ddl.contains(r#""external_code""#));
        assert_eq!(plan[2].kind, DdlPreviewKind::Index);
        assert!(plan[3].ddl.contains(r#""child_copy""#));
        assert!(plan[3].ddl.contains(r#""parent_copy" ("parent_key")"#));
        assert_eq!(plan[3].kind, DdlPreviewKind::ForeignKey);
        assert_eq!(plan[3].depends_on, vec!["parent"]);
    }

    #[test]
    fn identity_structure_data_plan_fails_before_ddl_without_explicit_insert_support() {
        let mut identity_schema = schema("records", &[("id", true)]);
        identity_schema.columns[0].is_auto_increment = true;
        let mappings = vec![mapping("records", "records_copy", &[("id", "id")])];
        let inspected_tables = vec![inspected("records", "records_copy", &[("id", "id")])];
        let schemas = HashMap::from([("records".into(), identity_schema)]);

        let error = build_database_structure_plan(
            &Source,
            &IdentityTarget,
            &job(mappings.clone()),
            &inspected_tables,
            &schemas,
        )
        .expect_err("data phase cannot insert explicit values into target identity");
        assert!(error.to_string().contains("cannot insert explicit values"));

        let mut structure_only = job(mappings);
        structure_only.mode = TransferMode::Structure;
        let plan = build_database_structure_plan(
            &Source,
            &IdentityTarget,
            &structure_only,
            &inspected_tables,
            &schemas,
        )
        .expect("structure-only plans can preserve the identity marker");
        assert!(plan[0].ddl.contains("IDENTITY(1,1)"));
    }

    #[test]
    fn postgres_identity_plan_accepts_inferred_type_but_rejects_custom_override() {
        let mut identity_schema = schema("records", &[("id", true)]);
        identity_schema.columns[0].is_auto_increment = true;
        let schemas = HashMap::from([("records".into(), identity_schema)]);
        let inspected_tables = vec![inspected("records", "records_copy", &[("id", "id")])];

        let mut inferred_mappings = vec![mapping("records", "records_copy", &[("id", "id")])];
        inferred_mappings[0].column_mappings[0].target_native_type = Some("INTEGER".into());
        let plan = build_database_structure_plan(
            &Source,
            &datazen_driver_postgres::PgSyncAdapter,
            &job(inferred_mappings),
            &inspected_tables,
            &schemas,
        )
        .expect("the unchanged target-native suggestion preserves identity semantics");
        assert!(plan[0].ddl.contains("GENERATED BY DEFAULT AS IDENTITY"));

        let mut custom_mappings = vec![mapping("records", "records_copy", &[("id", "id")])];
        custom_mappings[0].column_mappings[0].target_native_type = Some("TEXT".into());
        let error = build_database_structure_plan(
            &Source,
            &datazen_driver_postgres::PgSyncAdapter,
            &job(custom_mappings),
            &inspected_tables,
            &schemas,
        )
        .expect_err("a real incompatible type override must fail during planning");
        let message = error.to_string();
        assert!(message.contains("custom target type 'TEXT'"), "{message}");
        assert!(message.contains("inferred type 'integer'"), "{message}");
    }

    #[test]
    fn mysql_database_structure_preserves_defaults_for_varchar_override_and_rejects_longtext() {
        let mut source_schema = schema("records", &[("value", false)]);
        source_schema.columns[0].data_type = "date".into();
        source_schema.columns[0].default_value = Some("'2024-01-01'::date".into());
        let schemas = HashMap::from([("records".into(), source_schema)]);
        let inspected_tables = vec![inspected("records", "records_copy", &[("value", "value")])];

        let make_job = |native_type: &str| {
            let mut table = mapping("records", "records_copy", &[("value", "value")]);
            table.column_mappings[0].target_native_type = Some(native_type.into());
            job(vec![table])
        };

        let error = build_database_structure_plan(
            &datazen_driver_postgres::PgSyncAdapter,
            &datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false },
            &make_job("LONGTEXT"),
            &inspected_tables,
            &schemas,
        )
        .expect_err("LONGTEXT cannot preserve a MySQL column default");
        assert!(error.to_string().contains("default on 'records.value'"));

        let plan = build_database_structure_plan(
            &datazen_driver_postgres::PgSyncAdapter,
            &datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false },
            &make_job("VARCHAR(64)"),
            &inspected_tables,
            &schemas,
        )
        .expect("VARCHAR can retain this literal default without changing the override");
        assert!(
            plan[0]
                .ddl
                .contains("`value` VARCHAR(64) NOT NULL DEFAULT '2024-01-01'"),
            "{}",
            plan[0].ddl
        );
    }

    #[test]
    fn database_plan_rejects_unmapped_mysql_collation_before_any_write() {
        let mut source_schema = schema("records", &[("id", true), ("name", false)]);
        source_schema.table_options = datazen_driver_api::TableOptions {
            engine: Some("InnoDB".into()),
            charset: Some("utf8mb4".into()),
            collation: Some("utf8mb4_0900_ai_ci".into()),
            ..Default::default()
        };
        let schemas = HashMap::from([("records".into(), source_schema)]);
        let mappings = vec![mapping(
            "records",
            "records_copy",
            &[("id", "id"), ("name", "name")],
        )];
        let inspected_tables = vec![inspected(
            "records",
            "records_copy",
            &[("id", "id"), ("name", "name")],
        )];

        let mut transfer_job = job(mappings);
        let error = build_database_structure_plan(
            &Source,
            &datazen_driver_postgres::PgSyncAdapter,
            &transfer_job,
            &inspected_tables,
            &schemas,
        )
        .expect_err("unproven collation semantics must stop plan creation before DDL");
        let message = error.to_string();
        assert!(message.contains("utf8mb4_0900_ai_ci"), "{message}");
        assert!(message.contains("no proven equivalent"), "{message}");
        assert!(message.contains("PostgreSQL UTF8"), "{message}");

        transfer_job.options.use_target_default_collation = true;
        let plan = build_database_structure_plan(
            &Source,
            &datazen_driver_postgres::PgSyncAdapter,
            &transfer_job,
            &inspected_tables,
            &schemas,
        )
        .expect("explicit target-default selection permits a visible semantics conversion");
        assert!(plan[0].ddl.contains("CREATE TABLE"), "{}", plan[0].ddl);
        assert!(!plan[0].ddl.contains("COLLATE"), "{}", plan[0].ddl);
    }

    #[test]
    fn mysql_structure_plan_rejects_unbounded_text_index_before_any_ddl() {
        let mut source_schema = schema("records", &[("id", true), ("label", false)]);
        source_schema.columns[1].data_type = "text".into();
        source_schema.indexes.push(IndexInfo {
            name: "ix_records_label".into(),
            columns: vec!["label".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let schemas = HashMap::from([("records".into(), source_schema)]);
        let mappings = vec![mapping(
            "records",
            "records_copy",
            &[("id", "id"), ("label", "label")],
        )];
        let inspected_tables = vec![inspected(
            "records",
            "records_copy",
            &[("id", "id"), ("label", "label")],
        )];

        let error = build_database_structure_plan(
            &datazen_driver_postgres::PgSyncAdapter,
            &datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false },
            &job(mappings),
            &inspected_tables,
            &schemas,
        )
        .expect_err("unbounded TEXT index must block the full DDL plan");
        let message = error.to_string();
        assert!(message.contains("ix_records_label"), "{message}");
        assert!(message.contains("target column 'label'"), "{message}");
        assert!(
            message.contains("map it to a bounded VARCHAR/BINARY type"),
            "{message}"
        );
    }

    #[test]
    fn mysql_structure_plan_keeps_bounded_varchar_indexes_portable() {
        let mut source_schema = schema("records", &[("id", true), ("label", false)]);
        source_schema.columns[1].data_type = "character varying(120)".into();
        source_schema.indexes.push(IndexInfo {
            name: "ix_records_label".into(),
            columns: vec!["label".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let schemas = HashMap::from([("records".into(), source_schema)]);
        let mappings = vec![mapping(
            "records",
            "records_copy",
            &[("id", "id"), ("label", "label")],
        )];
        let inspected_tables = vec![inspected(
            "records",
            "records_copy",
            &[("id", "id"), ("label", "label")],
        )];

        let plan = build_database_structure_plan(
            &datazen_driver_postgres::PgSyncAdapter,
            &datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false },
            &job(mappings),
            &inspected_tables,
            &schemas,
        )
        .expect("bounded VARCHAR supports a normal secondary index on MySQL");
        assert!(plan[0].ddl.contains("VARCHAR(120)"));
        assert_eq!(plan[1].kind, DdlPreviewKind::Index);
        assert!(plan[1].ddl.contains("`label`"));
    }

    #[test]
    fn mysql_source_varchar_create_preflight_fails_closed_without_column_collation_metadata() {
        let mut source_schema = schema("records", &[("id", true), ("label", false)]);
        source_schema.columns[1].data_type = "varchar(80)".into();
        let schemas = HashMap::from([("records".into(), source_schema)]);
        let mappings = vec![mapping(
            "records",
            "records_copy",
            &[("id", "id"), ("label", "label")],
        )];
        let inspected_tables = vec![inspected(
            "records",
            "records_copy",
            &[("id", "id"), ("label", "label")],
        )];
        let source = datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false };
        let target = datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false };

        let transfer_job = job(mappings);
        let error = crate::structure::validate_transfer_column_types(
            &transfer_job,
            &inspected_tables,
            &schemas,
            &source,
            &target,
        )
        .expect_err(
            "CREATE preflight must not replace unknown source column collation with the target default",
        );
        let message = error.to_string();
        assert!(
            message.contains("source type 'varchar(80)' requires collation preservation"),
            "{message}"
        );
        assert!(message.contains("not represented or proven"), "{message}");
    }

    #[test]
    fn mysql_structure_plan_rejects_composite_key_over_conservative_limit() {
        let mut source_schema = schema(
            "records",
            &[("id", true), ("first", false), ("second", false)],
        );
        source_schema.columns[1].data_type = "character varying(100)".into();
        source_schema.columns[2].data_type = "character varying(100)".into();
        source_schema.indexes.push(IndexInfo {
            name: "ix_records_both".into(),
            columns: vec!["first".into(), "second".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let schemas = HashMap::from([("records".into(), source_schema)]);
        let mappings = vec![mapping(
            "records",
            "records_copy",
            &[("id", "id"), ("first", "first"), ("second", "second")],
        )];
        let inspected_tables = vec![inspected(
            "records",
            "records_copy",
            &[("id", "id"), ("first", "first"), ("second", "second")],
        )];

        let error = build_database_structure_plan(
            &datazen_driver_postgres::PgSyncAdapter,
            &datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false },
            &job(mappings),
            &inspected_tables,
            &schemas,
        )
        .expect_err("composite index beyond conservative InnoDB key size must be refused");
        let message = error.to_string();
        assert!(message.contains("ix_records_both"), "{message}");
        assert!(message.contains("800 bytes"), "{message}");
        assert!(message.contains("767-byte InnoDB bound"), "{message}");
        assert!(message.contains("newer servers"), "{message}");
    }

    #[test]
    fn database_plan_rejects_schema_scoped_postgres_index_name_collisions() {
        let mut first = schema("first", &[("id", true), ("value", false)]);
        let mut second = schema("second", &[("id", true), ("value", false)]);
        for table_schema in [&mut first, &mut second] {
            table_schema.indexes.push(IndexInfo {
                name: "ix_shared".into(),
                columns: vec!["value".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
            });
        }
        let mappings = vec![
            mapping("first", "first_copy", &[("id", "id"), ("value", "value")]),
            mapping("second", "second_copy", &[("id", "id"), ("value", "value")]),
        ];
        let inspected_tables = vec![
            inspected("first", "first_copy", &[("id", "id"), ("value", "value")]),
            inspected("second", "second_copy", &[("id", "id"), ("value", "value")]),
        ];
        let schemas = HashMap::from([("first".into(), first), ("second".into(), second)]);

        let error = build_database_structure_plan(
            &Source,
            &datazen_driver_postgres::PgSyncAdapter,
            &job(mappings),
            &inspected_tables,
            &schemas,
        )
        .expect_err("PostgreSQL index names share a schema namespace");
        assert!(error
            .to_string()
            .contains("index name 'ix_shared' collides"));
    }

    #[test]
    fn database_plan_preserves_case_sensitive_postgres_object_names() {
        let mut first = schema("first", &[("id", true), ("value", false)]);
        let mut second = schema("second", &[("id", true), ("value", false)]);
        for (table_schema, name) in [(&mut first, "Idx"), (&mut second, "idx")] {
            table_schema.indexes.push(IndexInfo {
                name: name.into(),
                columns: vec!["value".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
            });
        }
        let mappings = vec![
            mapping("first", "first_copy", &[("id", "id"), ("value", "value")]),
            mapping("second", "second_copy", &[("id", "id"), ("value", "value")]),
        ];
        let inspected_tables = vec![
            inspected("first", "first_copy", &[("id", "id"), ("value", "value")]),
            inspected("second", "second_copy", &[("id", "id"), ("value", "value")]),
        ];
        let schemas = HashMap::from([("first".into(), first), ("second".into(), second)]);

        let plan = build_database_structure_plan(
            &Source,
            &datazen_driver_postgres::PgSyncAdapter,
            &job(mappings),
            &inspected_tables,
            &schemas,
        )
        .expect("quoted PostgreSQL names preserve case");
        assert_eq!(
            plan.iter()
                .filter(|item| item.kind == DdlPreviewKind::Index)
                .count(),
            2
        );
    }

    #[test]
    fn database_plan_rejects_schema_scoped_mysql_foreign_key_name_collisions() {
        let parent_a = schema("parent_a", &[("id", true)]);
        let parent_b = schema("parent_b", &[("id", true)]);
        let child_a = TableSchema {
            foreign_keys: vec![ForeignKeyInfo {
                name: "fk_shared".into(),
                columns: vec!["parent_id".into()],
                referenced_table: "public.parent_a".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "NO ACTION".into(),
                deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
            }],
            ..schema("child_a", &[("id", true), ("parent_id", false)])
        };
        let child_b = TableSchema {
            foreign_keys: vec![ForeignKeyInfo {
                name: "fk_shared".into(),
                columns: vec!["parent_id".into()],
                referenced_table: "public.parent_b".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "NO ACTION".into(),
                deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
            }],
            ..schema("child_b", &[("id", true), ("parent_id", false)])
        };
        let tables = ["parent_a", "parent_b", "child_a", "child_b"];
        let mappings = tables
            .iter()
            .map(|table| {
                if table.starts_with("parent") {
                    mapping(table, &format!("{table}_copy"), &[("id", "id")])
                } else {
                    mapping(
                        table,
                        &format!("{table}_copy"),
                        &[("id", "id"), ("parent_id", "parent_id")],
                    )
                }
            })
            .collect();
        let inspected_tables = tables
            .iter()
            .map(|table| {
                if table.starts_with("parent") {
                    inspected(table, &format!("{table}_copy"), &[("id", "id")])
                } else {
                    inspected(
                        table,
                        &format!("{table}_copy"),
                        &[("id", "id"), ("parent_id", "parent_id")],
                    )
                }
            })
            .collect::<Vec<_>>();
        let schemas = HashMap::from([
            ("parent_a".into(), parent_a),
            ("parent_b".into(), parent_b),
            ("child_a".into(), child_a),
            ("child_b".into(), child_b),
        ]);

        let error = build_database_structure_plan(
            &Source,
            &datazen_driver_mysql::MysqlSyncAdapter { is_mariadb: false },
            &job(mappings),
            &inspected_tables,
            &schemas,
        )
        .expect_err("MySQL foreign-key symbols share a schema namespace");
        assert!(error
            .to_string()
            .contains("foreign key name 'fk_shared' collides"));
    }

    #[test]
    fn database_structure_plan_rejects_missing_fk_dependency_before_writes() {
        let child = TableSchema {
            foreign_keys: vec![ForeignKeyInfo {
                name: "fk_child_parent".into(),
                columns: vec!["parent_id".into()],
                referenced_table: "public.parent".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "NO ACTION".into(),
                deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
            }],
            ..schema("child", &[("id", true), ("parent_id", false)])
        };
        let mappings = vec![mapping(
            "child",
            "child_copy",
            &[("id", "id"), ("parent_id", "parent_id")],
        )];
        let inspected = vec![inspected(
            "child",
            "child_copy",
            &[("id", "id"), ("parent_id", "parent_id")],
        )];
        let schemas = HashMap::from([("child".into(), child)]);

        let error =
            build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
                .expect_err("missing FK parent must fail before the executor sees a plan");

        assert!(error
            .to_string()
            .contains("unselected or out-of-scope table 'public.parent'"));
    }

    #[test]
    fn database_structure_plan_keeps_indexes_when_table_ddl_is_overridden() {
        let mut table_mapping = mapping("events", "events_copy", &[("id", "event_id")]);
        table_mapping.ddl_override =
            Some("CREATE TABLE \"archive\".\"events_copy\" (\"event_id\" INT)".into());
        let mut events = schema("events", &[("id", true)]);
        events.indexes.push(IndexInfo {
            name: "events_event_id_idx".into(),
            columns: vec!["id".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let inspected = vec![inspected("events", "events_copy", &[("id", "event_id")])];
        let schemas = HashMap::from([("events".into(), events)]);

        let plan = build_database_structure_plan(
            &Source,
            &Target,
            &job(vec![table_mapping]),
            &inspected,
            &schemas,
        )
        .expect("table DDL override must not suppress mapped secondary objects");

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].kind, DdlPreviewKind::Table);
        assert!(plan[0].ddl.starts_with("CREATE TABLE \"archive\""));
        assert_eq!(plan[1].kind, DdlPreviewKind::Index);
        assert!(plan[1].ddl.contains("events_copy"));
    }

    #[test]
    fn database_structure_plan_rejects_unrenderable_identity_before_writes() {
        let mut events = schema("events", &[("id", true)]);
        events.columns[0].is_auto_increment = true;
        let mappings = vec![mapping("events", "events_copy", &[("id", "id")])];
        let inspected = vec![inspected("events", "events_copy", &[("id", "id")])];
        let schemas = HashMap::from([("events".into(), events)]);

        let error =
            build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
                .expect_err("target without identity rendering must refuse before execution");

        assert!(error
            .to_string()
            .contains("cannot be rendered by the target adapter"));
    }

    #[test]
    fn database_structure_plan_orders_drop_create_and_defers_foreign_keys() {
        let parent = schema("parent", &[("id", true)]);
        let mut child = schema("child", &[("id", true), ("parent_id", false)]);
        child.foreign_keys.push(ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: "public.parent".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
        });
        let mappings = vec![
            mapping(
                "child",
                "child_copy",
                &[("id", "id"), ("parent_id", "parent_id")],
            ),
            mapping("parent", "parent_copy", &[("id", "id")]),
        ];
        let inspected = vec![
            inspected(
                "child",
                "child_copy",
                &[("id", "id"), ("parent_id", "parent_id")],
            ),
            inspected("parent", "parent_copy", &[("id", "id")]),
        ];
        let schemas = HashMap::from([("child".into(), child), ("parent".into(), parent)]);
        let mut transfer = job(mappings);
        transfer.write_mode = WriteMode::DropCreateInsert;

        let plan = build_database_structure_plan(&Source, &Target, &transfer, &inspected, &schemas)
            .expect("acyclic Drop + Create plan");

        assert_eq!(plan[0].kind, DdlPreviewKind::DropTable);
        assert_eq!(plan[0].source_table, "child");
        assert_eq!(plan[1].kind, DdlPreviewKind::DropTable);
        assert_eq!(plan[1].source_table, "parent");
        assert_eq!(plan[2].kind, DdlPreviewKind::Table);
        assert_eq!(plan[2].source_table, "parent");
        assert_eq!(plan[3].kind, DdlPreviewKind::Table);
        assert_eq!(plan[3].source_table, "child");
        assert_eq!(plan[4].kind, DdlPreviewKind::ForeignKey);
    }

    #[test]
    fn database_structure_plan_keeps_two_table_fk_cycles_deterministic() {
        let mut left = schema("left_table", &[("id", true), ("right_id", false)]);
        let mut right = schema("right_table", &[("id", true), ("left_id", false)]);
        left.foreign_keys.push(ForeignKeyInfo {
            name: "fk_left_right".into(),
            columns: vec!["right_id".into()],
            referenced_table: "public.right_table".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
        });
        right.foreign_keys.push(ForeignKeyInfo {
            name: "fk_right_left".into(),
            columns: vec!["left_id".into()],
            referenced_table: "public.left_table".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
        });
        let mappings = vec![
            mapping(
                "left_table",
                "left_copy",
                &[("id", "id"), ("right_id", "right_id")],
            ),
            mapping(
                "right_table",
                "right_copy",
                &[("id", "id"), ("left_id", "left_id")],
            ),
        ];
        let inspected = vec![
            inspected(
                "left_table",
                "left_copy",
                &[("id", "id"), ("right_id", "right_id")],
            ),
            inspected(
                "right_table",
                "right_copy",
                &[("id", "id"), ("left_id", "left_id")],
            ),
        ];
        let schemas = HashMap::from([("left_table".into(), left), ("right_table".into(), right)]);
        let plan =
            build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
                .expect(
                    "normal Structure+Data supports cyclic FK graphs because FK DDL is deferred",
                );

        assert_eq!(plan[0].source_table, "left_table");
        assert_eq!(plan[1].source_table, "right_table");
        assert!(plan[..2]
            .iter()
            .all(|item| item.kind == DdlPreviewKind::Table));
        assert!(plan[2..]
            .iter()
            .all(|item| item.kind == DdlPreviewKind::ForeignKey));
    }
}
