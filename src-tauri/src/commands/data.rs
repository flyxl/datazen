use super::{AppState, log_err};
use crate::db::Value;
use tauri::State;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellUpdate {
    pub column: String,
    pub value: Option<Value>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowUpdateBatch {
    pub set_columns: Vec<CellUpdate>,
    pub pk_columns: Vec<CellUpdate>,
}

#[tauri::command]
pub async fn commit_row_updates(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    updates: Vec<RowUpdateBatch>,
) -> Result<(), String> {
    tracing::info!(%connection_id, %table, batch_count = updates.len(), "commit_row_updates");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("commit_row_updates", &e))?;

    for batch in &updates {
        let set_columns: Vec<(&str, Option<Value>)> = batch
            .set_columns
            .iter()
            .map(|c| (c.column.as_str(), c.value.clone()))
            .collect();
        let pk_columns: Vec<(&str, Option<Value>)> = batch
            .pk_columns
            .iter()
            .map(|c| (c.column.as_str(), c.value.clone()))
            .collect();
        let sql = driver.build_update_sql(&table, &set_columns, &pk_columns);
        driver
            .execute(&handle, &sql)
            .await
            .map_err(|e| log_err("commit_row_updates", &e))?;
    }

    tracing::info!(%connection_id, %table, batch_count = updates.len(), "commit_row_updates OK");
    Ok(())
}
