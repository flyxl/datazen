use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{DriverError, Value};
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
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, %table, batch_count = updates.len(), "commit_row_updates");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("commit_row_updates")?;

    // Prefer DatabaseDriver transaction API; fall back to BEGIN/COMMIT strings
    // for drivers that still stub the trait (most SQL engines today).
    let trait_tx = match driver.begin_transaction(&handle).await {
        Ok(tx) => Some(tx),
        Err(DriverError::TransactionError(_)) => None,
        Err(e) => return Err(CommandError::Driver(e)),
    };

    if trait_tx.is_none() {
        driver
            .execute(&handle, "BEGIN")
            .await
            .cmd_err("commit_row_updates")?;
    }

    let result: Result<(), CommandError> = async {
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
                .cmd_err("commit_row_updates")?;
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            if let Some(tx) = trait_tx {
                driver.commit(tx).await.cmd_err("commit_row_updates")?;
            } else {
                driver
                    .execute(&handle, "COMMIT")
                    .await
                    .cmd_err("commit_row_updates")?;
            }
            tracing::info!(%connection_id, %table, batch_count = updates.len(), "commit_row_updates OK");
            Ok(())
        }
        Err(e) => {
            if let Some(tx) = trait_tx {
                if let Err(rb_err) = driver.rollback(tx).await {
                    tracing::warn!("rollback failed: {rb_err}");
                }
            } else if let Err(rb_err) = driver.execute(&handle, "ROLLBACK").await {
                tracing::warn!("rollback failed: {rb_err}");
            }
            Err(e)
        }
    }
}
