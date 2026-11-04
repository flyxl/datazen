/// Human-readable table description for MCP `describe_table` tool output.
pub fn format_table_description(table: &str, schema: &datazen_driver_api::TableSchema) -> String {
    let mut desc = format!("Table: {table}\n\nColumns:\n");
    for col in &schema.columns {
        desc.push_str(&format!(
            "  - {} {} {}{}\n",
            col.name,
            col.data_type,
            if col.is_primary_key { "PK " } else { "" },
            if col.nullable { "" } else { "NOT NULL " },
        ));
    }

    if !schema.primary_keys.is_empty() {
        desc.push_str(&format!(
            "\nPrimary Key: ({})\n",
            schema.primary_keys.join(", ")
        ));
    }

    if !schema.indexes.is_empty() {
        desc.push_str("\nIndexes:\n");
        for idx in &schema.indexes {
            desc.push_str(&format!(
                "  - {} ({}) {}\n",
                idx.name,
                idx.columns.join(", "),
                if idx.is_unique { "UNIQUE" } else { "" }
            ));
        }
    }

    if !schema.foreign_keys.is_empty() {
        desc.push_str("\nForeign Keys:\n");
        for fk in &schema.foreign_keys {
            desc.push_str(&format!(
                "  - {} ({}) → {}.{}\n",
                fk.name,
                fk.columns.join(", "),
                fk.referenced_table,
                fk.referenced_columns.join(", ")
            ));
        }
    }

    desc
}
