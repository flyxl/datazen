//! Driver metadata names are logical identifiers, not SQL-quoted expressions.
use super::{error::TransferError, model::Endpoint};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, Value};
use crate::transfer::adapter::SyncTargetAdapter;
use datazen_driver_api::TableSchema;
use std::collections::HashMap;

pub fn metadata_relation_ref(endpoint: &Endpoint, table: &str) -> Result<String, TransferError> {
    match endpoint.normalized_schema() {
        Some(schema) => {
            // The current string contract splits once on a dot. A dot inside
            // the schema cannot be represented without changing that contract.
            if schema.contains('.') {
                return Err(TransferError::validation(
                    "metadata schema names containing '.' require structured relation support",
                ));
            }
            Ok(format!("{schema}.{table}"))
        }
        None => Ok(table.to_string()), // database/catalog is not a schema
    }
}

pub fn table_in_endpoint_schema(
    endpoint: &Endpoint,
    table: &datazen_driver_api::TableInfo,
) -> bool {
    match (endpoint.normalized_schema(), table.schema.as_deref()) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => true,
    }
}

pub async fn load_table_schema(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    endpoint: &Endpoint,
    table: &str,
) -> Result<TableSchema, TransferError> {
    metadata_relation_ref(endpoint, table)?;
    driver
        .get_table_schema(
            handle,
            table,
            &endpoint.database,
            endpoint.normalized_schema(),
        )
        .await
        .map_err(|error| TransferError::validation(error.to_string()))
}

pub async fn load_target_character_metadata(
    adapter: &dyn SyncTargetAdapter,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    endpoint: &Endpoint,
    table: &str,
) -> Result<HashMap<String, (Option<String>, Option<String>)>, TransferError> {
    let Some(query) = adapter.transfer_target_character_metadata_query(
        &endpoint.database,
        endpoint.normalized_schema(),
        table,
    ) else {
        return Ok(HashMap::new());
    };
    let result = driver
        .query(handle, &query)
        .await
        .map_err(|error| TransferError::validation(error.to_string()))?;
    let mut metadata = HashMap::new();
    for row in &result.rows {
        let column = query_cell_text(row.first())?.ok_or_else(|| {
            TransferError::validation(format!(
                "target character metadata for '{table}' has an empty column name"
            ))
        })?;
        let character_set = query_cell_text(row.get(1))?;
        let collation = query_cell_text(row.get(2))?;
        metadata.insert(column, (character_set, collation));
    }
    Ok(metadata)
}

fn query_cell_text(cell: Option<&Option<Value>>) -> Result<Option<String>, TransferError> {
    match cell {
        None => Err(TransferError::validation(
            "target character metadata query returned an incomplete row",
        )),
        Some(None) | Some(Some(Value::Null)) => Ok(None),
        Some(Some(Value::String(value))) | Some(Some(Value::Timestamp(value))) => {
            Ok(Some(value.clone()))
        }
        Some(Some(Value::Bytes(value))) => {
            String::from_utf8(value.clone()).map(Some).map_err(|_| {
                TransferError::validation("target character metadata query returned non-UTF-8 text")
            })
        }
        Some(Some(_)) => Err(TransferError::validation(
            "target character metadata query returned a non-text value",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_is_explicit_and_catalog_and_literal_names_remain_unchanged() {
        let mut endpoint = Endpoint {
            db_session_id: "session".into(),
            database: "catalog".into(),
            schema: None,
        };
        assert_eq!(
            metadata_relation_ref(&endpoint, "literal.table").unwrap(),
            "literal.table"
        );
        endpoint.schema = Some(" \t".into());
        assert_eq!(metadata_relation_ref(&endpoint, "table").unwrap(), "table");
        endpoint.schema = Some(" Selected ".into());
        assert_eq!(
            metadata_relation_ref(&endpoint, "literal.table").unwrap(),
            "Selected.literal.table"
        );
        assert_eq!(
            metadata_relation_ref(&endpoint, "quoted\"name").unwrap(),
            "Selected.quoted\"name"
        );
        endpoint.schema = Some("ambiguous.schema".into());
        assert!(metadata_relation_ref(&endpoint, "table").is_err());
    }
    #[test]
    fn table_listing_scope_does_not_treat_catalog_as_schema() {
        let mut endpoint = Endpoint {
            db_session_id: "s".into(),
            database: "mysql_catalog".into(),
            schema: None,
        };
        let table = datazen_driver_api::TableInfo {
            name: "t".into(),
            schema: Some("mysql_catalog".into()),
            table_type: datazen_driver_api::TableType::Table,
            row_count: None,
        };
        assert!(table_in_endpoint_schema(&endpoint, &table));
        endpoint.schema = Some("selected".into());
        assert!(!table_in_endpoint_schema(&endpoint, &table));
        let selected = datazen_driver_api::TableInfo {
            schema: Some("selected".into()),
            ..table
        };
        assert!(table_in_endpoint_schema(&endpoint, &selected));
    }
}
