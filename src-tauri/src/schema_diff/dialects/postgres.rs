//! PostgreSQL ALTER TABLE helpers.

use super::{add_column_stmt, drop_column_stmt, quote_column, quote_ident};
use crate::schema_diff::types::{ChangedColumnDiff, ColumnSnapshot, PlanStatement, StatementRisk};

pub fn add_column_with_default(table: &str, col: &ColumnSnapshot, type_sql: &str, default_value: &str) -> PlanStatement {
    let q_table = quote_ident("postgresql", table);
    let q_col = quote_column("postgresql", &col.name);
    let sql = format!("ALTER TABLE {q_table} ADD COLUMN {q_col} {type_sql}{} DEFAULT {default_value}", super::nullability_sql(col.nullable));
    PlanStatement {
        sql,
        risk: StatementRisk::Additive,
        rollback_sql: Some(format!("ALTER TABLE {q_table} DROP COLUMN {q_col}")),
        summary: format!("ADD COLUMN {}.{}", table, col.name),
    }
}

pub fn add_column(table: &str, col: &ColumnSnapshot, type_sql: &str) -> PlanStatement {
    add_column_stmt("postgresql", table, col, type_sql)
}

pub fn drop_column(table: &str, col: &ColumnSnapshot) -> PlanStatement {
    drop_column_stmt("postgresql", table, col)
}

pub fn alter_type(table: &str, change: &ChangedColumnDiff, type_sql: &str) -> PlanStatement {
    let q_table = quote_ident("postgresql", table);
    let q_col = quote_column("postgresql", &change.name);
    let sql = format!("ALTER TABLE {q_table} ALTER COLUMN {q_col} TYPE {type_sql}");
    let rollback = format!(
        "ALTER TABLE {q_table} ALTER COLUMN {q_col} TYPE {}",
        change.target.data_type
    );
    PlanStatement {
        sql,
        risk: StatementRisk::Rewrite,
        rollback_sql: Some(rollback),
        summary: format!("ALTER TYPE {}.{}", table, change.name),
    }
}

pub fn set_nullability(table: &str, change: &ChangedColumnDiff) -> PlanStatement {
    let q_table = quote_ident("postgresql", table);
    let q_col = quote_column("postgresql", &change.name);
    let (sql, rollback) = if change.source.nullable {
        (
            format!("ALTER TABLE {q_table} ALTER COLUMN {q_col} DROP NOT NULL"),
            format!("ALTER TABLE {q_table} ALTER COLUMN {q_col} SET NOT NULL"),
        )
    } else {
        (
            format!("ALTER TABLE {q_table} ALTER COLUMN {q_col} SET NOT NULL"),
            format!("ALTER TABLE {q_table} ALTER COLUMN {q_col} DROP NOT NULL"),
        )
    };
    let risk = if change.source.nullable {
        StatementRisk::Additive
    } else {
        StatementRisk::Rewrite
    };
    PlanStatement {
        sql,
        risk,
        rollback_sql: Some(rollback),
        summary: format!("SET NULLABILITY {}.{}", table, change.name),
    }
}

pub fn create_index(table: &str, name: &str, columns: &[String], unique: bool) -> PlanStatement {
    let q_table = quote_ident("postgresql", table);
    let q_name = quote_ident("postgresql", name);
    let cols: Vec<String> = columns
        .iter()
        .map(|c| quote_column("postgresql", c))
        .collect();
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
    let q_name = quote_ident("postgresql", name);
    let sql = format!("DROP INDEX {q_name}");
    PlanStatement {
        sql,
        risk: StatementRisk::Destructive,
        rollback_sql: None,
        summary: format!("DROP INDEX {name}"),
    }
}

pub fn drop_primary_key(table: &str) -> PlanStatement {
    let q_table = quote_ident("postgresql", table);
    PlanStatement {
        sql: format!("ALTER TABLE {q_table} DROP CONSTRAINT IF EXISTS \"{}_pkey\"", table),
        risk: StatementRisk::Destructive,
        rollback_sql: None,
        summary: format!("DROP PRIMARY KEY {table}"),
    }
}

pub fn add_primary_key(table: &str, columns: &[String]) -> PlanStatement {
    let q_table = quote_ident("postgresql", table);
    let cols: Vec<String> = columns.iter().map(|c| quote_column("postgresql", c)).collect();
    PlanStatement {
        sql: format!("ALTER TABLE {q_table} ADD PRIMARY KEY ({})", cols.join(", ")),
        risk: StatementRisk::Additive,
        rollback_sql: Some(format!("ALTER TABLE {q_table} DROP CONSTRAINT IF EXISTS \"{}_pkey\"", table)),
        summary: format!("ADD PRIMARY KEY {table}"),
    }
}

