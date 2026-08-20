//! DuckDB table structure capabilities and DDL planning.
//!
//! DuckDB is a transactional embedded database whose ALTER TABLE grammar
//! closely follows PostgreSQL's. Operations map to direct DuckDB statements:
//! - `ADD COLUMN`, `DROP COLUMN`, `RENAME COLUMN`
//! - `ALTER COLUMN ... TYPE / SET/DROP DEFAULT / SET/DROP NOT NULL`
//! - `CREATE INDEX`, `DROP INDEX`

use datazen_driver_api::*;
use std::collections::HashMap;

/// DuckDB structure capabilities exposed to the structure editor UI.
pub fn duckdb_capabilities(dialect_id: &str) -> StructureCapabilities {
    StructureCapabilities {
        create_table: true,
        add_column: true,
        drop_column: true,
        rename_column: true,
        alter_type: true,
        alter_nullability: true,
        alter_default: true,
        alter_primary_key: false,
        reorder_column: false,
        comment: false,
        create_index: true,
        drop_index: true,
        rebuild_index: false,
        index_type: false,
        index_include: false,
        index_filter: false,
        index_comment: false,
        alter_strategy: AlterStrategy::Direct,
        dialect_id: dialect_id.to_string(),
        index_methods: Vec::new(),
    }
}

/// Plan DuckDB DDL for a structure change request.
pub fn plan_structure_changes(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    match request.mode {
        StructureChangeMode::Create => plan_create_table(caps, request),
        StructureChangeMode::Alter => plan_alter_table(caps, request),
    }
}

fn plan_create_table(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    if !caps.create_table {
        return Err(unsupported("create_table is not supported"));
    }
    if request.current_columns.is_empty() {
        return Err(unsupported("create table requires at least one column"));
    }
    validate_column_drafts(&request.current_columns)?;

    let table = quote_table(request.schema.as_deref(), &request.table);
    let mut defs: Vec<String> = request
        .current_columns
        .iter()
        .map(|c| format_column_definition(c, true))
        .collect();

    if let Some(pk) = primary_key_clause(&request.current_columns) {
        defs.push(pk);
    }

    let sql = format!("CREATE TABLE {} (\n  {}\n)", table, defs.join(",\n  "));

    Ok(StructureChangePlan {
        statements: vec![PlanStatement {
            sql,
            summary: format!("Create table {}", request.table),
            risk: StatementRisk::Additive,
        }],
        warnings: Vec::new(),
    })
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
            sql: format!("DROP INDEX {}", quote_ident(&idx.name)),
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

                if orig.data_type != col.data_type {
                    if !caps.alter_type {
                        return Err(unsupported("alter_type is not supported"));
                    }
                    statements.push(PlanStatement {
                        sql: format!(
                            "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                            table,
                            quote_ident(&col.name),
                            col.data_type
                        ),
                        summary: format!("Change type of {} to {}", col.name, col.data_type),
                        risk: StatementRisk::Rewrite,
                    });
                }

                if orig.nullable != col.nullable {
                    if !caps.alter_nullability {
                        return Err(unsupported("alter_nullability is not supported"));
                    }
                    let kw = if col.nullable {
                        "DROP NOT NULL"
                    } else {
                        "SET NOT NULL"
                    };
                    statements.push(PlanStatement {
                        sql: format!(
                            "ALTER TABLE {} ALTER COLUMN {} {}",
                            table,
                            quote_ident(&col.name),
                            kw
                        ),
                        summary: format!("Set nullability of {} to {}", col.name, col.nullable),
                        risk: StatementRisk::Rewrite,
                    });
                }

                if orig.default_value != col.default_value {
                    if !caps.alter_default {
                        return Err(unsupported("alter_default is not supported"));
                    }
                    let clause = match &col.default_value {
                        Some(v) if !v.is_empty() => format!("SET DEFAULT {v}"),
                        _ => "DROP DEFAULT".to_string(),
                    };
                    statements.push(PlanStatement {
                        sql: format!(
                            "ALTER TABLE {} ALTER COLUMN {} {}",
                            table,
                            quote_ident(&col.name),
                            clause
                        ),
                        summary: format!("Update default of {}", col.name),
                        risk: StatementRisk::Rewrite,
                    });
                }

                if orig.is_primary_key != col.is_primary_key {
                    return Err(unsupported("alter_primary_key is not supported"));
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
        let unique = if idx.is_unique { "UNIQUE " } else { "" };
        let cols = idx
            .columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        statements.push(PlanStatement {
            sql: format!(
                "CREATE {unique}INDEX {} ON {} ({})",
                quote_ident(&idx.name),
                table,
                cols
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

    parts.join(" ")
}

fn primary_key_clause(columns: &[StructureColumnDraft]) -> Option<String> {
    let pks: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| quote_ident(&c.name))
        .collect();
    if pks.is_empty() {
        None
    } else {
        Some(format!("PRIMARY KEY ({})", pks.join(", ")))
    }
}

fn validate_column_drafts(columns: &[StructureColumnDraft]) -> Result<(), DriverError> {
    for col in columns {
        if col.is_auto_increment {
            return Err(unsupported(
                "serial/identity columns are set in the SQL editor",
            ));
        }
    }
    Ok(())
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

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quote_table(schema: Option<&str>, table: &str) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
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

    #[test]
    fn capabilities_reflect_duckdb_support() {
        let caps = duckdb_capabilities("duckdb");
        assert!(caps.create_table);
        assert!(caps.add_column);
        assert!(caps.drop_column);
        assert!(caps.rename_column);
        assert!(caps.alter_type);
        assert!(caps.alter_nullability);
        assert!(caps.alter_default);
        assert!(caps.create_index);
        assert!(caps.drop_index);
        assert_eq!(caps.alter_strategy, AlterStrategy::Direct);
    }

    #[test]
    fn add_column_generates_duckdb_ddl() {
        let caps = duckdb_capabilities("duckdb");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![col("1", "age", "INTEGER", true, Some("0"))],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0]
            .sql
            .contains("ALTER TABLE \"users\" ADD COLUMN \"age\" INTEGER"));
    }

    #[test]
    fn type_and_nullability_changes_emit_alter_column() {
        let caps = duckdb_capabilities("duckdb");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: vec![col("1", "c", "INTEGER", false, None)],
            current_columns: vec![col("1", "c", "BIGINT", true, None)],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        let sqls: Vec<&str> = plan.statements.iter().map(|s| s.sql.as_str()).collect();
        assert!(sqls
            .iter()
            .any(|s| s.contains("ALTER COLUMN \"c\" TYPE BIGINT")));
        assert!(sqls.iter().any(|s| s.contains("DROP NOT NULL")));
    }

    #[test]
    fn create_table_generates_sql() {
        let caps = duckdb_capabilities("duckdb");
        let mut id_col = col("1", "id", "INTEGER", false, None);
        id_col.is_primary_key = true;
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: None,
            table: "t".into(),
            original_columns: vec![],
            current_columns: vec![id_col, col("2", "name", "VARCHAR", true, None)],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0].sql.contains("CREATE TABLE \"t\""));
        assert!(plan.statements[0].sql.contains("PRIMARY KEY (\"id\")"));
    }
}
