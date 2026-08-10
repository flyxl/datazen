//! Column (and index) comparison with source = desired.

use super::types::{ChangedColumnDiff, ColumnSnapshot, TableColumnDiff};
use crate::db::{ColumnSchema, IndexInfo, TableSchema};
use std::collections::HashMap;

pub fn column_snapshot(col: &ColumnSchema) -> ColumnSnapshot {
    ColumnSnapshot {
        name: col.name.clone(),
        data_type: col.data_type.clone(),
        nullable: col.nullable,
        is_primary_key: col.is_primary_key,
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
                changes.push("dataType".into());
            }
            if col.nullable != tgt_col.nullable {
                changes.push("nullable".into());
            }
            if col.is_primary_key != tgt_col.is_primary_key {
                changes.push("isPrimaryKey".into());
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

pub fn diff_indexes(src: &TableSchema, tgt: &TableSchema) -> IndexDiff {
    let key = |idx: &IndexInfo| {
        format!(
            "{}|{}|{}|{}",
            idx.name,
            idx.columns.join(","),
            idx.is_unique,
            idx.is_primary
        )
    };
    let src_keys: HashMap<String, &IndexInfo> =
        src.indexes.iter().map(|i| (key(i), i)).collect();
    let tgt_keys: HashMap<String, &IndexInfo> =
        tgt.indexes.iter().map(|i| (key(i), i)).collect();

    let mut missing_on_target = Vec::new();
    let mut extra_on_target = Vec::new();

    for (k, idx) in &src_keys {
        if !tgt_keys.contains_key(k) && !idx.is_primary {
            missing_on_target.push((*idx).clone());
        }
    }
    for (k, idx) in &tgt_keys {
        if !src_keys.contains_key(k) && !idx.is_primary {
            extra_on_target.push((*idx).clone());
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
    fn target_only_column_is_drop() {
        let src = schema(vec![col("id", "int")]);
        let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
        let diff = diff_table_schemas("users", &src, &tgt);
        assert_eq!(diff.extra_on_target.len(), 1);
        assert_eq!(diff.extra_on_target[0].name, "legacy");
        assert_eq!(diff.removed.len(), 1);
    }
}
