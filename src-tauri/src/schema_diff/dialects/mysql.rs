//! MySQL / MariaDB ALTER TABLE helpers.

use super::{add_column_stmt, drop_column_stmt, nullability_sql, quote_column, quote_ident};
use crate::schema_diff::types::{ChangedColumnDiff, ColumnSnapshot, PlanStatement, StatementRisk};

pub fn add_column_with_default(table: &str, col: &ColumnSnapshot, type_sql: &str, default_value: &str) -> PlanStatement {
    let q_table = quote_ident("mysql", table);
    let q_col = quote_column("mysql", &col.name);
    let sql = format!("ALTER TABLE {q_table} ADD COLUMN {q_col} {type_sql}{} DEFAULT {default_value}", nullability_sql(col.nullable));
    PlanStatement {
        sql,
        risk: StatementRisk::Additive,
        rollback_sql: Some(format!("ALTER TABLE {q_table} DROP COLUMN {q_col}")),
        summary: format!("ADD COLUMN {}.{}", table, col.name),
    }
}

pub fn add_column(table: &str, col: &ColumnSnapshot, type_sql: &str) -> PlanStatement {
    add_column_stmt("mysql", table, col, type_sql)
}

pub fn drop_column(table: &str, col: &ColumnSnapshot) -> PlanStatement {
    drop_column_stmt("mysql", table, col)
}

pub fn modify_column(table: &str, change: &ChangedColumnDiff, type_sql: &str) -> PlanStatement {
    let q_table = quote_ident("mysql", table);
    let q_col = quote_column("mysql", &change.name);
    let nulls = nullability_sql(change.source.nullable);
    let sql = format!("ALTER TABLE {q_table} MODIFY COLUMN {q_col} {type_sql}{nulls}");
    let rb_nulls = nullability_sql(change.target.nullable);
    let rollback = format!(
        "ALTER TABLE {q_table} MODIFY COLUMN {q_col} {}{rb_nulls}",
        change.target.data_type
    );
    let risk = if change.changes.contains(&crate::schema_diff::types::ColumnChange::DataType) {
        StatementRisk::Rewrite
    } else if !change.source.nullable && change.target.nullable {
        StatementRisk::Rewrite
    } else {
        StatementRisk::Additive
    };
    PlanStatement {
        sql,
        risk,
        rollback_sql: Some(rollback),
        summary: format!("MODIFY COLUMN {}.{}", table, change.name),
    }
}

pub fn create_index(table: &str, name: &str, columns: &[String], unique: bool) -> PlanStatement {
    let q_table = quote_ident("mysql", table);
    let q_name = quote_ident("mysql", name);
    let cols: Vec<String> = columns.iter().map(|c| quote_column("mysql", c)).collect();
    let uniq = if unique { "UNIQUE " } else { "" };
    let sql = format!(
        "CREATE {uniq}INDEX {q_name} ON {q_table} ({})",
        cols.join(", ")
    );
    let rollback = format!("DROP INDEX {q_name} ON {q_table}");
    PlanStatement {
        sql,
        risk: StatementRisk::Additive,
        rollback_sql: Some(rollback),
        summary: format!("CREATE INDEX {name} ON {table}"),
    }
}

pub fn drop_index(table: &str, name: &str) -> PlanStatement {
    let q_table = quote_ident("mysql", table);
    let q_name = quote_ident("mysql", name);
    let sql = format!("DROP INDEX {q_name} ON {q_table}");
    PlanStatement {
        sql,
        risk: StatementRisk::Destructive,
        rollback_sql: None,
        summary: format!("DROP INDEX {name} ON {table}"),
    }
}

pub fn drop_primary_key(table: &str) -> PlanStatement {
    let q_table = quote_ident("mysql", table);
    PlanStatement {
        sql: format!("ALTER TABLE {q_table} DROP PRIMARY KEY"),
        risk: StatementRisk::Destructive,
        rollback_sql: None,
        summary: format!("DROP PRIMARY KEY {table}"),
    }
}

pub fn add_primary_key(table: &str, columns: &[String]) -> PlanStatement {
    let q_table = quote_ident("mysql", table);
    let cols: Vec<String> = columns.iter().map(|c| quote_column("mysql", c)).collect();
    PlanStatement {
        sql: format!("ALTER TABLE {q_table} ADD PRIMARY KEY ({})", cols.join(", ")),
        risk: StatementRisk::Additive,
        rollback_sql: Some(format!("ALTER TABLE {q_table} DROP PRIMARY KEY")),
        summary: format!("ADD PRIMARY KEY {table}"),
    }
}

