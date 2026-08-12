//! SQLite ALTER helpers (limited).

use super::{add_column_stmt, quote_ident};
use crate::schema_diff::types::{ColumnSnapshot, PlanStatement, StatementRisk};

pub fn add_column(table: &str, col: &ColumnSnapshot, type_sql: &str) -> PlanStatement {
    // SQLite ADD COLUMN cannot include NOT NULL without DEFAULT in many cases;
    // emit nullable ADD when source is NOT NULL and warn at plan level.
    let mut col = col.clone();
    if !col.nullable {
        col.nullable = true;
    }
    add_column_stmt("sqlite", table, &col, type_sql)
}

pub fn unsupported_drop(table: &str, col: &str) -> String {
    format!("SQLite cannot DROP COLUMN `{col}` on `{table}` via simple ALTER in this planner")
}

pub fn unsupported_modify(table: &str, col: &str) -> String {
    format!("SQLite cannot MODIFY COLUMN `{col}` on `{table}` via simple ALTER in this planner")
}

pub fn create_index(table: &str, name: &str, columns: &[String], unique: bool) -> PlanStatement {
    let q_table = quote_ident("sqlite", table);
    let q_name = quote_ident("sqlite", name);
    let cols: Vec<String> = columns.iter().map(|c| quote_ident("sqlite", c)).collect();
    let uniq = if unique { "UNIQUE " } else { "" };
    let sql = format!(
        "CREATE {uniq}INDEX {q_name} ON {q_table} ({})",
        cols.join(", ")
    );
    let rollback = format!("DROP INDEX {q_name}");
    PlanStatement {
        sql,
        risk: StatementRisk::Additive,
        rollback_sql: Some(rollback),
        summary: format!("CREATE INDEX {name} ON {table}"),
    }
}

pub fn drop_index(name: &str) -> PlanStatement {
    let q_name = quote_ident("sqlite", name);
    let sql = format!("DROP INDEX {q_name}");
    PlanStatement {
        sql,
        risk: StatementRisk::Destructive,
        rollback_sql: None,
        summary: format!("DROP INDEX {name}"),
    }
}
