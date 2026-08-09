//! Dialect-specific DDL fragments for schema diff plans.

pub mod mysql;
pub mod postgres;
pub mod sqlite;

use super::types::{normalize_dialect, ColumnSnapshot, PlanStatement, StatementRisk};

pub(crate) fn quote_ident(dialect: &str, name: &str) -> String {
    match normalize_dialect(dialect).as_str() {
        "mysql" | "mariadb" => format!("`{}`", name.replace('`', "``")),
        _ => {
            // Handle schema.table
            if let Some((schema, table)) = name.split_once('.') {
                format!(
                    "\"{}\".\"{}\"",
                    schema.replace('"', "\"\""),
                    table.replace('"', "\"\"")
                )
            } else {
                format!("\"{}\"", name.replace('"', "\"\""))
            }
        }
    }
}

pub(crate) fn quote_column(dialect: &str, name: &str) -> String {
    match normalize_dialect(dialect).as_str() {
        "mysql" | "mariadb" => format!("`{}`", name.replace('`', "``")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

pub(crate) fn nullability_sql(nullable: bool) -> &'static str {
    if nullable {
        ""
    } else {
        " NOT NULL"
    }
}

pub(crate) fn add_column_stmt(
    dialect: &str,
    table: &str,
    col: &ColumnSnapshot,
    type_sql: &str,
) -> PlanStatement {
    let q_table = quote_ident(dialect, table);
    let q_col = quote_column(dialect, &col.name);
    let nulls = nullability_sql(col.nullable);
    let sql = format!("ALTER TABLE {q_table} ADD COLUMN {q_col} {type_sql}{nulls}");
    let rollback = format!("ALTER TABLE {q_table} DROP COLUMN {q_col}");
    PlanStatement {
        sql,
        risk: StatementRisk::Additive,
        rollback_sql: Some(rollback),
        summary: format!("ADD COLUMN {}.{}", table, col.name),
    }
}

pub(crate) fn drop_column_stmt(dialect: &str, table: &str, col: &ColumnSnapshot) -> PlanStatement {
    let q_table = quote_ident(dialect, table);
    let q_col = quote_column(dialect, &col.name);
    let sql = format!("ALTER TABLE {q_table} DROP COLUMN {q_col}");
    // Best-effort rollback: re-add with known type (may lose defaults/constraints).
    let nulls = nullability_sql(col.nullable);
    let rollback = format!(
        "ALTER TABLE {q_table} ADD COLUMN {q_col} {}{nulls}",
        col.data_type
    );
    PlanStatement {
        sql,
        risk: StatementRisk::Destructive,
        rollback_sql: Some(rollback),
        summary: format!("DROP COLUMN {}.{}", table, col.name),
    }
}
