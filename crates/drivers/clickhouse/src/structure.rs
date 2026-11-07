//! ClickHouse table structure capabilities and DDL planning.
//!
//! ClickHouse (MergeTree-family) supports a constrained set of ALTER
//! operations. There is no `ENGINE` field in the structure editor model and
//! every ClickHouse table needs an engine clause, so CREATE TABLE is left to
//! the SQL editor. ALTER operations below are emitted as one ClickHouse
//! `ALTER TABLE ...` statement per operation (the most broadly compatible
//! form across server versions):
//! - `ADD COLUMN`, `DROP COLUMN`, `RENAME COLUMN`
//! - `MODIFY COLUMN` (type / nullability / default / comment)
//! - `ADD INDEX`, `DROP INDEX`

use datazen_driver_api::*;
use std::collections::HashMap;

/// ClickHouse structure capabilities exposed to the structure editor UI.
pub fn clickhouse_capabilities(dialect_id: &str) -> StructureCapabilities {
    StructureCapabilities {
        create_table: false,
        add_column: true,
        drop_column: true,
        rename_column: true,
        alter_type: true,
        alter_nullability: true,
        alter_default: true,
        alter_primary_key: false,
        reorder_column: false,
        comment: true,
        create_index: true,
        drop_index: true,
        rebuild_index: false,
        index_type: true,
        index_include: false,
        index_filter: false,
        index_comment: false,
        alter_strategy: AlterStrategy::Direct,
        dialect_id: dialect_id.to_string(),
        index_methods: vec![
            "minmax".into(),
            "set".into(),
            "bloom_filter".into(),
            "ngrambf_v1".into(),
            "tokenbf_v1".into(),
        ],
    }
}

/// Plan ClickHouse DDL for a structure change request.
pub fn plan_structure_changes(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    match request.mode {
        StructureChangeMode::Create => Err(unsupported(
            "CREATE TABLE is not supported in the structure editor; use the SQL editor",
        )),
        StructureChangeMode::Alter => plan_alter_table(caps, request),
    }
}

fn plan_alter_table(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    let original_cols = index_columns_by_id(&request.original_columns);
    let current_cols = index_columns_by_id(&request.current_columns);
    let original_idx = index_indexes_by_id(&request.original_indexes);
    let current_idx = index_indexes_by_id(&request.current_indexes);

    let table = quote_table(request.schema.as_deref(), &request.table);
    let mut statements: Vec<PlanStatement> = Vec::new();

    // ── dropped indexes ──
    for (id, idx) in &original_idx {
        if current_idx.contains_key(id) {
            continue;
        }
        if !caps.drop_index {
            return Err(unsupported("drop_index is not supported"));
        }
        if idx.is_primary {
            return Err(unsupported("dropping the primary key is not supported"));
        }
        statements.push(PlanStatement {
            sql: format!(
                "ALTER TABLE {} DROP INDEX {}",
                table,
                quote_ident(&idx.name)
            ),
            summary: format!("Drop index {}", idx.name),
            risk: StatementRisk::Destructive,
        });
    }

    // ── dropped columns ──
    for (id, col) in &original_cols {
        if current_cols.contains_key(id) {
            continue;
        }
        if !caps.drop_column {
            return Err(unsupported("drop_column is not supported"));
        }
        statements.push(PlanStatement {
            sql: format!(
                "ALTER TABLE {} DROP COLUMN {}",
                table,
                quote_ident(&col.name)
            ),
            summary: format!("Drop column {}", col.name),
            risk: StatementRisk::Destructive,
        });
    }

    // ── column adds, renames, and definition changes ──
    for col in &request.current_columns {
        match original_cols.get(col.id.as_str()) {
            None => {
                if !caps.add_column {
                    return Err(unsupported("add_column is not supported"));
                }
                statements.push(PlanStatement {
                    sql: format!(
                        "ALTER TABLE {} ADD COLUMN {}",
                        table,
                        format_column_definition(col, true)
                    ),
                    summary: format!("Add column {}", col.name),
                    risk: StatementRisk::Additive,
                });
            }
            Some(orig) => {
                let renamed = orig.name != col.name;
                if renamed {
                    if !caps.rename_column {
                        return Err(unsupported("rename_column is not supported"));
                    }
                    statements.push(PlanStatement {
                        sql: format!(
                            "ALTER TABLE {} RENAME COLUMN {} TO {}",
                            table,
                            quote_ident(&orig.name),
                            quote_ident(&col.name)
                        ),
                        summary: format!("Rename column {} to {}", orig.name, col.name),
                        risk: StatementRisk::Additive,
                    });
                }

                let type_changed = orig.data_type != col.data_type;
                let null_changed = orig.nullable != col.nullable;
                let def_changed = orig.default_value != col.default_value;
                let comment_changed = orig.comment != col.comment;

                if type_changed || null_changed || def_changed || comment_changed {
                    if type_changed && !caps.alter_type {
                        return Err(unsupported("alter_type is not supported"));
                    }
                    if null_changed && !caps.alter_nullability {
                        return Err(unsupported("alter_nullability is not supported"));
                    }
                    if def_changed && !caps.alter_default {
                        return Err(unsupported("alter_default is not supported"));
                    }
                    if comment_changed && !caps.comment {
                        return Err(unsupported("comment is not supported"));
                    }
                    statements.push(PlanStatement {
                        sql: format!(
                            "ALTER TABLE {} MODIFY COLUMN {}",
                            table,
                            format_modify_clause(col)
                        ),
                        summary: format!("Modify column {}", col.name),
                        risk: StatementRisk::Rewrite,
                    });
                }

                if orig.is_primary_key != col.is_primary_key {
                    return Err(unsupported(
                        "changing the primary/sorting key is not supported",
                    ));
                }
                if orig.is_auto_increment != col.is_auto_increment {
                    return Err(unsupported("changing auto-increment is not supported"));
                }
            }
        }
    }

    // ── new indexes ──
    for (id, idx) in &current_idx {
        if original_idx.contains_key(id) {
            continue;
        }
        if !caps.create_index {
            return Err(unsupported("create_index is not supported"));
        }
        if idx.is_primary {
            return Err(unsupported("adding a primary key index is not supported"));
        }
        validate_index_draft(idx)?;
        let cols = idx
            .columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let skp_type = if idx.index_type.is_empty() {
            "bloom_filter".to_string()
        } else {
            idx.index_type.clone()
        };
        statements.push(PlanStatement {
            sql: format!(
                "ALTER TABLE {} ADD INDEX {} ({}) TYPE {} GRANULARITY 1",
                table,
                quote_ident(&idx.name),
                cols,
                skp_type
            ),
            summary: format!("Create index {}", idx.name),
            risk: StatementRisk::Additive,
        });
    }

    // ── modified indexes (unsupported; drop + recreate) ──
    for (id, cur) in &current_idx {
        if let Some(orig) = original_idx.get(id) {
            if index_definition_changed(orig, cur) {
                return Err(unsupported(
                    "modifying an existing index is not supported; drop and recreate it",
                ));
            }
        }
    }

    if statements.is_empty() {
        return Err(unsupported("no structure changes detected"));
    }

    Ok(StructureChangePlan {
        statements,
        warnings: Vec::new(),
    })
}

// ░░ helpers ░░

fn format_column_definition(col: &StructureColumnDraft, include_name: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if include_name {
        parts.push(quote_ident(&col.name));
    }
    parts.push(col.data_type.clone());

    if !col.nullable {
        parts.push("NOT NULL".into());
    }
    if let Some(v) = &col.default_value {
        if !v.is_empty() && !v.eq_ignore_ascii_case("NULL") {
            parts.push(format!("DEFAULT {v}"));
        }
    }
    if let Some(c) = &col.comment {
        if !c.is_empty() {
            parts.push(format!("COMMENT '{}'", escape_single(c)));
        }
    }

    parts.join(" ")
}

/// MODIFY COLUMN clause — a single `MODIFY COLUMN` statement.
fn format_modify_clause(col: &StructureColumnDraft) -> String {
    format!(
        "{} {}",
        quote_ident(&col.name),
        format_column_definition(col, false)
    )
}

fn validate_index_draft(idx: &StructureIndexDraft) -> Result<(), DriverError> {
    if idx.columns.is_empty() {
        return Err(unsupported("index requires at least one column"));
    }
    Ok(())
}

fn index_definition_changed(a: &StructureIndexDraft, b: &StructureIndexDraft) -> bool {
    a.is_unique != b.is_unique
        || a.columns != b.columns
        || a.is_primary != b.is_primary
        || a.index_type != b.index_type
        || a.include_columns != b.include_columns
        || a.filter != b.filter
        || a.comment != b.comment
}

fn index_columns_by_id(columns: &[StructureColumnDraft]) -> HashMap<&str, &StructureColumnDraft> {
    columns.iter().map(|c| (c.id.as_str(), c)).collect()
}

fn index_indexes_by_id(indexes: &[StructureIndexDraft]) -> HashMap<&str, &StructureIndexDraft> {
    indexes.iter().map(|i| (i.id.as_str(), i)).collect()
}

/// ClickHouse quotes identifiers with double quotes (backticks also accepted).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quote_table(schema: Option<&str>, table: &str) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
}

fn escape_single(value: &str) -> String {
    value.replace('\'', "''")
}

fn unsupported(message: impl Into<String>) -> DriverError {
    DriverError::Unsupported(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(
        id: &str,
        name: &str,
        data_type: &str,
        nullable: bool,
        default: Option<&str>,
    ) -> StructureColumnDraft {
        StructureColumnDraft {
            id: id.into(),
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            default_value: default.map(Into::into),
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
            is_unique: false,
        }
    }

    fn idx(id: &str, name: &str, columns: &[&str], index_type: &str) -> StructureIndexDraft {
        StructureIndexDraft {
            id: id.into(),
            name: name.into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            is_unique: false,
            is_primary: false,
            index_type: index_type.into(),
            include_columns: Vec::new(),
            filter: None,
            comment: None,
        }
    }

    #[test]
    fn capabilities_reflect_clickhouse_support() {
        let caps = clickhouse_capabilities("clickhouse");
        assert!(!caps.create_table);
        assert!(caps.add_column);
        assert!(caps.drop_column);
        assert!(caps.rename_column);
        assert!(caps.alter_type);
        assert!(caps.alter_nullability);
        assert!(caps.alter_default);
        assert!(caps.comment);
        assert!(caps.create_index);
        assert!(caps.drop_index);
        assert_eq!(caps.alter_strategy, AlterStrategy::Direct);
        assert!(caps.index_methods.contains(&"bloom_filter".to_string()));
    }

    #[test]
    fn add_column_generates_alter_table_add() {
        let caps = clickhouse_capabilities("clickhouse");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "events".into(),
            original_columns: vec![],
            current_columns: vec![col("c1", "user_id", "UInt64", false, None)],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert!(plan.statements[0]
            .sql
            .contains("ALTER TABLE \"events\" ADD COLUMN \"user_id\" UInt64 NOT NULL"));
    }

    #[test]
    fn rename_column_generates_rename() {
        let caps = clickhouse_capabilities("clickhouse");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: vec![col("1", "old_name", "String", true, None)],
            current_columns: vec![col("1", "new_name", "String", true, None)],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0]
            .sql
            .contains("RENAME COLUMN \"old_name\" TO \"new_name\""));
    }

    #[test]
    fn modify_column_generates_alter_table_modify() {
        let caps = clickhouse_capabilities("clickhouse");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: vec![col("1", "c", "UInt32", false, None)],
            current_columns: vec![col("1", "c", "UInt64", true, Some("0"))],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0]
            .sql
            .contains("MODIFY COLUMN \"c\" UInt64 DEFAULT 0"));
    }

    #[test]
    fn drop_column_generates_alter_table_drop() {
        let caps = clickhouse_capabilities("clickhouse");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: vec![col("1", "gone", "UInt8", true, None)],
            current_columns: vec![],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0].sql.contains("DROP COLUMN \"gone\""));
    }

    #[test]
    fn create_index_adds_skip_index() {
        let caps = clickhouse_capabilities("clickhouse");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: vec![col("1", "c", "String", true, None)],
            current_columns: vec![col("1", "c", "String", true, None)],
            original_indexes: vec![],
            current_indexes: vec![idx("1", "ix_c", &["c"], "bloom_filter")],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0]
            .sql
            .contains("ADD INDEX \"ix_c\" (\"c\") TYPE bloom_filter"));
    }

    #[test]
    fn create_table_mode_is_unsupported() {
        let caps = clickhouse_capabilities("clickhouse");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: None,
            table: "t".into(),
            original_columns: vec![],
            current_columns: vec![col("1", "c", "UInt64", true, None)],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(err.to_string().contains("CREATE TABLE"));
    }
}
