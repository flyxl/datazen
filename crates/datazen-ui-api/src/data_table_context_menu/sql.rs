use serde_json::Value;

use crate::database_types::{escape_ident, get_escape_sql_value, DatabaseRegistry};

fn esc_value(value: &Value, dialect: Option<&str>, registry: &DatabaseRegistry) -> String {
    get_escape_sql_value(dialect, registry).escape(value)
}

fn quote(name: &str, dialect: Option<&str>, registry: &DatabaseRegistry) -> String {
    escape_ident(name, dialect, registry)
}

pub fn format_row_as_sql_insert(
    table_name: &str,
    column_names: &[String],
    row: &[Value],
    dialect: Option<&str>,
    registry: &DatabaseRegistry,
) -> String {
    let cols: Vec<String> = column_names
        .iter()
        .map(|name| quote(name, dialect, registry))
        .collect();
    let values: Vec<String> = column_names
        .iter()
        .enumerate()
        .map(|(index, _)| esc_value(row.get(index).unwrap_or(&Value::Null), dialect, registry))
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        quote(table_name, dialect, registry),
        cols.join(", "),
        values.join(", ")
    )
}

fn where_keys(column_names: &[String], primary_key_columns: Option<&[String]>) -> Vec<String> {
    let pk = match primary_key_columns {
        Some(keys) if !keys.is_empty() => keys
            .iter()
            .filter(|key| column_names.contains(key))
            .cloned()
            .collect::<Vec<_>>(),
        _ => column_names.first().cloned().into_iter().collect(),
    };
    if pk.is_empty() {
        column_names.first().cloned().into_iter().collect()
    } else {
        pk
    }
}

pub fn format_row_as_sql_update(
    table_name: &str,
    column_names: &[String],
    row: &[Value],
    primary_key_columns: Option<&[String]>,
    dialect: Option<&str>,
    registry: &DatabaseRegistry,
) -> String {
    let keys = where_keys(column_names, primary_key_columns);
    let where_set: Vec<&String> = keys.iter().collect();
    let mut set_cols: Vec<String> = column_names
        .iter()
        .filter(|name| !where_set.contains(name))
        .cloned()
        .collect();
    if set_cols.is_empty() {
        set_cols = column_names.to_vec();
    }
    let set_clauses: Vec<String> = set_cols
        .iter()
        .map(|col| {
            let index = column_names
                .iter()
                .position(|name| name == col)
                .unwrap_or(0);
            format!(
                "{} = {}",
                quote(col, dialect, registry),
                esc_value(row.get(index).unwrap_or(&Value::Null), dialect, registry)
            )
        })
        .collect();
    let where_clause: Vec<String> = keys
        .iter()
        .map(|col| {
            let index = column_names
                .iter()
                .position(|name| name == col)
                .unwrap_or(0);
            format!(
                "{} = {}",
                quote(col, dialect, registry),
                esc_value(row.get(index).unwrap_or(&Value::Null), dialect, registry)
            )
        })
        .collect();
    format!(
        "UPDATE {} SET {} WHERE {};",
        quote(table_name, dialect, registry),
        set_clauses.join(", "),
        where_clause.join(" AND ")
    )
}

pub fn format_row_as_sql_delete(
    table_name: &str,
    column_names: &[String],
    row: &[Value],
    primary_key_columns: Option<&[String]>,
    dialect: Option<&str>,
    registry: &DatabaseRegistry,
) -> String {
    let keys = where_keys(column_names, primary_key_columns);
    let where_clause: Vec<String> = keys
        .iter()
        .map(|col| {
            let index = column_names
                .iter()
                .position(|name| name == col)
                .unwrap_or(0);
            let value = row.get(index).unwrap_or(&Value::Null);
            if matches!(value, Value::Null) {
                format!("{} IS NULL", quote(col, dialect, registry))
            } else {
                format!(
                    "{} = {}",
                    quote(col, dialect, registry),
                    esc_value(value, dialect, registry)
                )
            }
        })
        .collect();
    format!(
        "DELETE FROM {} WHERE {};",
        quote(table_name, dialect, registry),
        where_clause.join(" AND ")
    )
}

pub fn format_rows_as_sql_insert(
    table_name: &str,
    column_names: &[String],
    rows: &[Vec<Value>],
    dialect: Option<&str>,
    registry: &DatabaseRegistry,
) -> String {
    rows.iter()
        .map(|row| format_row_as_sql_insert(table_name, column_names, row, dialect, registry))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_rows_as_sql_update(
    table_name: &str,
    column_names: &[String],
    rows: &[Vec<Value>],
    primary_key_columns: Option<&[String]>,
    dialect: Option<&str>,
    registry: &DatabaseRegistry,
) -> String {
    rows.iter()
        .map(|row| {
            format_row_as_sql_update(
                table_name,
                column_names,
                row,
                primary_key_columns,
                dialect,
                registry,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
