//! Shared source/target table-schema loading for the Data Transfer command flow.

use std::collections::HashMap;

use datazen_driver_api::{ConnectionHandle, DatabaseDriver, TableSchema};

use super::super::error::CommandError;
use crate::data_transfer::model::Endpoint;

/// Load table schemas and preserve the common, side-specific validation error.
pub(crate) async fn load_named_table_schemas(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    endpoint: &Endpoint,
    table_names: &[String],
    side: &str,
) -> Result<HashMap<String, TableSchema>, CommandError> {
    let mut schemas = HashMap::with_capacity(table_names.len());
    for table_name in table_names {
        let schema = crate::data_transfer::metadata::load_table_schema(
            driver, handle, endpoint, table_name,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "failed to inspect {side} table '{table_name}': {error}"
            ))
        })?;
        schemas.insert(table_name.clone(), schema);
    }
    Ok(schemas)
}
