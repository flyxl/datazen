//! Full column type queries shared by Transfer, Schema Diff, and Sync.

use std::collections::HashMap;

use crate::db::{ConnectionHandle, DatabaseDriver, Value};
use crate::transfer::SyncSourceAdapter;

/// Run adapter-provided full-type SQL (if any) and map `(name, full_type)` rows.
pub async fn fetch_full_column_types(
    adapter: &dyn SyncSourceAdapter,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    table: &str,
) -> Result<HashMap<String, String>, String> {
    let Some(sql) = adapter.full_column_types_query(table) else {
        return Ok(HashMap::new());
    };
    let result = driver
        .query(handle, &sql)
        .await
        .map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for row in &result.rows {
        if let (
            Some(Some(Value::String(name))),
            Some(Some(Value::String(ft))),
        ) = (row.get(0), row.get(1))
        {
            map.insert(name.clone(), ft.clone());
        }
    }
    Ok(map)
}
