//! Column (and index) comparison with source = desired.

use super::types::{ChangedColumnDiff, ColumnSnapshot, TableColumnDiff};
use crate::db::{ColumnSchema, IndexInfo, TableSchema};
use std::collections::HashMap;

pub fn column_snapshot(col: &ColumnSchema) -> ColumnSnapshot {
    ColumnSnapshot {
        name: col.name.clone(),
        data_type: col.data_type.clone(),
        nullable: col.nullable,
        default_value: col.default_value.clone(),
        comment: col.comment.clone(),
        is_primary_key: col.is_primary_key,
        is_auto_increment: col.is_auto_increment,
    }
}

/// Diff table schemas. **Source is the desired state.**
pub fn diff_table_schemas(table: &str, src: &TableSchema, tgt: &TableSchema) -> TableColumnDiff {
    let src_map: HashMap<&str, &ColumnSchema> =
        src.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    let tgt_map: HashMap<&str, &ColumnSchema> =
        tgt.columns.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut missing_on_target = Vec::new();
    let mut extra_on_target = Vec::new();
    let mut changed = Vec::new();

    for col in &src.columns {
        if !tgt_map.contains_key(col.name.as_str()) {
            missing_on_target.push(column_snapshot(col));
        }
    }

    for col in &tgt.columns {
        if !src_map.contains_key(col.name.as_str()) {
            extra_on_target.push(column_snapshot(col));
        }
    }

    for col in &src.columns {
        if let Some(tgt_col) = tgt_map.get(col.name.as_str()) {
            let mut changes = Vec::new();
            if col.data_type != tgt_col.data_type {
                changes.push(super::types::ColumnChange::DataType);
            }
            if col.nullable != tgt_col.nullable {
                changes.push(super::types::ColumnChange::Nullable);
            }
            if col.is_primary_key != tgt_col.is_primary_key {
                changes.push(super::types::ColumnChange::PrimaryKey);
            }
            if col.default_value != tgt_col.default_value {
                changes.push(super::types::ColumnChange::Default);
            }
            if col.comment != tgt_col.comment {
                changes.push(super::types::ColumnChange::Comment);
            }
            if col.is_auto_increment != tgt_col.is_auto_increment {
                changes.push(super::types::ColumnChange::AutoIncrement);
            }
            if !changes.is_empty() {
                changed.push(ChangedColumnDiff {
                    name: col.name.clone(),
                    source: column_snapshot(col),
                    target: column_snapshot(tgt_col),
                    changes,
                });
            }
        }
    }

    TableColumnDiff {
        table: table.to_string(),
        added: missing_on_target.clone(),
        removed: extra_on_target.clone(),
        missing_on_target,
        extra_on_target,
        changed,
    }
}

#[derive(Debug, Clone)]
pub struct IndexDiff {
    pub missing_on_target: Vec<IndexInfo>,
    pub extra_on_target: Vec<IndexInfo>,
}

fn index_definition_equal(a: &IndexInfo, b: &IndexInfo) -> bool {
    a.columns == b.columns && a.is_unique == b.is_unique
}

pub fn diff_indexes(src: &TableSchema, tgt: &TableSchema) -> IndexDiff {
    let src_by_name: HashMap<&str, &IndexInfo> =
        src.indexes.iter().map(|i| (i.name.as_str(), i)).collect();
    let tgt_by_name: HashMap<&str, &IndexInfo> =
        tgt.indexes.iter().map(|i| (i.name.as_str(), i)).collect();

    let mut missing_on_target = Vec::new();
    let mut extra_on_target = Vec::new();

    for (name, src_idx) in &src_by_name {
        if src_idx.is_primary {
            continue;
        }
        match tgt_by_name.get(name) {
            None => missing_on_target.push((*src_idx).clone()),
            Some(tgt_idx) if tgt_idx.is_primary => {}
            Some(tgt_idx) if !index_definition_equal(src_idx, tgt_idx) => {
                extra_on_target.push((*tgt_idx).clone());
                missing_on_target.push((*src_idx).clone());
            }
            Some(_) => {}
        }
    }

    for (name, tgt_idx) in &tgt_by_name {
        if tgt_idx.is_primary {
            continue;
        }
        if !src_by_name.contains_key(name) {
            extra_on_target.push((*tgt_idx).clone());
        }
    }

    IndexDiff {
        missing_on_target,
        extra_on_target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnSchema;

    fn col(name: &str, ty: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: ty.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    fn schema(cols: Vec<ColumnSchema>) -> TableSchema {
        TableSchema {
            table_name: "users".into(),
            columns: cols,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    #[test]
    fn source_column_missing_on_target_is_add() {
        let src = schema(vec![col("id", "int"), col("email", "varchar")]);
        let tgt = schema(vec![col("id", "int")]);
        let diff = diff_table_schemas("users", &src, &tgt);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert_eq!(diff.missing_on_target[0].name, "email");
        assert_eq!(diff.added.len(), 1);
        assert!(diff.extra_on_target.is_empty());
    }

    #[test]
    fn column_default_and_auto_increment_changes_are_detected() {
        let mut src_col = col("id", "int");
        src_col.default_value = Some("0".into());
        src_col.is_auto_increment = true;
        let mut tgt_col = col("id", "int");
        tgt_col.default_value = Some("1".into());
        let src = schema(vec![src_col]);
        let tgt = schema(vec![tgt_col]);
        let diff = diff_table_schemas("users", &src, &tgt);
        assert_eq!(diff.changed.len(), 1);
        assert!(diff.changed[0]
            .changes
            .contains(&super::super::types::ColumnChange::Default));
        assert!(diff.changed[0]
            .changes
            .contains(&super::super::types::ColumnChange::AutoIncrement));
    }

    #[test]
    fn target_only_column_is_drop() {
        let src = schema(vec![col("id", "int")]);
        let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
        let diff = diff_table_schemas("users", &src, &tgt);
        assert_eq!(diff.extra_on_target.len(), 1);
        assert_eq!(diff.extra_on_target[0].name, "legacy");
        assert_eq!(diff.removed.len(), 1);
    }

    fn index(name: &str, columns: &[&str], unique: bool) -> IndexInfo {
        IndexInfo {
            name: name.into(),
            columns: columns.iter().map(|c| (*c).into()).collect(),
            is_unique: unique,
            is_primary: false,
            index_type: "btree".into(),
        }
    }

    #[test]
    fn index_column_change_is_drop_and_create() {
        let mut src = schema(vec![col("id", "int"), col("email", "varchar")]);
        src.indexes.push(index("idx_email", &["email"], false));
        let mut tgt = schema(vec![col("id", "int"), col("email", "varchar")]);
        tgt.indexes.push(index("idx_email", &["id", "email"], false));
        let diff = diff_indexes(&src, &tgt);
        assert_eq!(diff.extra_on_target.len(), 1);
        assert_eq!(diff.extra_on_target[0].columns, vec!["id", "email"]);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert_eq!(diff.missing_on_target[0].columns, vec!["email"]);
    }

    #[test]
    fn index_unique_change_is_drop_and_create() {
        let mut src = schema(vec![col("id", "int")]);
        src.indexes.push(index("idx_id", &["id"], true));
        let mut tgt = schema(vec![col("id", "int")]);
        tgt.indexes.push(index("idx_id", &["id"], false));
        let diff = diff_indexes(&src, &tgt);
        assert_eq!(diff.extra_on_target.len(), 1);
        assert!(!diff.extra_on_target[0].is_unique);
        assert_eq!(diff.missing_on_target.len(), 1);
        assert!(diff.missing_on_target[0].is_unique);
    }
}
