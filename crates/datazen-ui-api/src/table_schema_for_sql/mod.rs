#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::{
    capabilities_for_db_session, format_table_identifier, generate_table_sql, DatabaseRegistry,
    GeneratedSqlType, SessionCapabilitiesEntry, TableSchema,
};
use crate::types::ColumnSchema;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationRef {
    pub database: String,
    pub schema: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadColumnsValue {
    pub relation: RelationRef,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadColumnsRow {
    pub status: String,
    pub value: Option<ReadColumnsValue>,
}

pub type ReadCachedSchema<'a> =
    dyn Fn(&str, &str, &str, Option<&str>, &str) -> Result<TableSchema, String> + 'a;
pub type ReadRelationColumns<'a> =
    dyn Fn(&str, &RelationRef) -> Result<Vec<ReadColumnsRow>, String> + 'a;

pub struct FetchTableSchemaInput<'a> {
    pub db_session_id: &'a str,
    pub connection_id: &'a str,
    pub table_name: &'a str,
    pub schema: Option<&'a str>,
    pub database: &'a str,
    pub capability_entries: &'a [SessionCapabilitiesEntry],
    pub column_map: Option<&'a BTreeMap<String, Vec<String>>>,
    pub read_cached_schema: &'a ReadCachedSchema<'a>,
    pub read_relation_columns: &'a ReadRelationColumns<'a>,
}

pub struct GenerateFallbackOptions<'a> {
    pub schema_prefix: Option<&'a str>,
    pub table_name: &'a str,
    pub table_ref_label: Option<&'a str>,
    pub registry: &'a DatabaseRegistry,
}

pub fn build_pseudo_table_schema(table_name: &str, column_names: &[String]) -> TableSchema {
    TableSchema {
        table_name: table_name.to_owned(),
        columns: column_names
            .iter()
            .map(|name| ColumnSchema {
                name: name.clone(),
                data_type: String::new(),
                nullable: true,
                default_value: None,
                is_primary_key: None,
                is_auto_increment: None,
                comment: None,
            })
            .collect(),
        primary_keys: Vec::new(),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
    }
}

pub fn fetch_table_schema_for_sql_generation(input: &FetchTableSchemaInput<'_>) -> Option<TableSchema> {
    let capabilities =
        capabilities_for_db_session(Some(input.db_session_id), input.capability_entries);
    let relation_schema = crate::relation_schema_for(capabilities.as_ref(), input.schema);
    let relation = RelationRef {
        database: input.database.to_owned(),
        schema: relation_schema.clone(),
        name: input.table_name.to_owned(),
    };

    if let Ok(mut cached) = (input.read_cached_schema)(
        input.db_session_id,
        input.table_name,
        input.database,
        relation_schema.as_deref(),
        input.connection_id,
    ) {
        if !cached.columns.is_empty() {
            cached.table_name = input.table_name.to_owned();
            return Some(cached);
        }
    }

    if let Ok(rows) = (input.read_relation_columns)(input.connection_id, &relation) {
        let matched = rows.into_iter().find(|row| {
            if row.status != "ok" {
                return false;
            }
            match row.value.as_ref() {
                Some(value) => {
                    value.relation.database == input.database
                        && value.relation.schema == relation_schema
                        && value.relation.name == input.table_name
                }
                None => false,
            }
        });
        if let Some(value) = matched.and_then(|row| row.value) {
            if !value.columns.is_empty() {
                let mut pseudo = build_pseudo_table_schema(input.table_name, &[]);
                pseudo.columns = value.columns;
                pseudo.primary_keys = value.primary_keys;
                return Some(pseudo);
            }
        }
    }

    if let Some(columns) = input
        .column_map
        .and_then(|map| map.get(input.table_name))
        .filter(|columns| !columns.is_empty())
    {
        return Some(build_pseudo_table_schema(input.table_name, columns));
    }

    None
}

fn fallback_quote<'a>(database_type: &str, registry: &'a DatabaseRegistry) -> &'a str {
    registry
        .get(database_type)
        .map(|meta| meta.quote_char.as_str())
        .unwrap_or("\"")
}

pub fn generate_table_sql_with_fallbacks(
    table_schema: Option<&TableSchema>,
    sql_type: GeneratedSqlType,
    database_type: &str,
    opts: &GenerateFallbackOptions<'_>,
) -> String {
    let quote = fallback_quote(database_type, opts.registry);
    if let Some(schema) = table_schema {
        if !schema.columns.is_empty() {
            return generate_table_sql(schema, sql_type, opts.schema_prefix, None, quote);
        }
    }
    let table_ref = format_table_identifier(opts.table_name, opts.schema_prefix, None, quote);
    if sql_type == GeneratedSqlType::Select {
        return format!("SELECT *\nFROM {table_ref};");
    }
    format!(
        "/* No column metadata available for {} */",
        opts.table_ref_label.unwrap_or(opts.table_name)
    )
}
