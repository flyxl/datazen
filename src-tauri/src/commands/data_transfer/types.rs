//! Shared helpers for Data Transfer IPC.

use crate::data_transfer::Endpoint;

pub(crate) fn resolve_db_name(selected: Option<&str>, config_default: Option<&str>) -> String {
    selected
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(config_default.map(str::trim).filter(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string()
}

pub(crate) fn is_self_database(
    source_connection_id: &str,
    target_connection_id: &str,
    source_database: &str,
    target_database: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
) -> bool {
    if source_connection_id != target_connection_id {
        return false;
    }
    if source_database != target_database {
        return false;
    }
    normalize_schema(source_schema) == normalize_schema(target_schema)
}

fn normalize_schema(schema: Option<&str>) -> Option<&str> {
    schema.map(str::trim).filter(|s| !s.is_empty())
}

pub(crate) fn endpoint_from(
    connection_id: &str,
    database: &str,
    schema: Option<String>,
) -> Endpoint {
    Endpoint {
        connection_id: connection_id.to_string(),
        database: database.to_string(),
        schema,
    }
}
