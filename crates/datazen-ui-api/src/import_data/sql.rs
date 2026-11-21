use serde_json::Value;

use crate::database_types::{escape_ident, get_escape_sql_value, DatabaseRegistry};

use super::types::ParsedData;

pub fn generate_insert_sql(
    table_name: &str,
    data: &ParsedData,
    database_type: Option<&str>,
    registry: &DatabaseRegistry,
) -> String {
    if data.rows.is_empty() {
        return String::new();
    }
    let escaper = get_escape_sql_value(database_type, registry);
    let col_list = data
        .columns
        .iter()
        .map(|name| escape_ident(name, database_type, registry))
        .collect::<Vec<_>>()
        .join(", ");
    data.rows
        .iter()
        .map(|row| {
            let values = data
                .columns
                .iter()
                .map(|name| escaper.escape(row.get(name).unwrap_or(&Value::Null)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "INSERT INTO {} ({}) VALUES ({});",
                escape_ident(table_name, database_type, registry),
                col_list,
                values
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
