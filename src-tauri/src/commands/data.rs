use super::error::{CmdExt, CommandError};
use super::AppState;
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

pub(crate) async fn commit_row_updates_impl(
    state: &AppState,
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

    driver
        .execute(&handle, "BEGIN")
        .await
        .cmd_err("commit_row_updates")?;

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
            driver
                .execute(&handle, "COMMIT")
                .await
                .cmd_err("commit_row_updates")?;
            tracing::info!(%connection_id, %table, batch_count = updates.len(), "commit_row_updates OK");
            Ok(())
        }
        Err(e) => {
            if let Err(rb_err) = driver.execute(&handle, "ROLLBACK").await {
                tracing::warn!("rollback failed: {rb_err}");
            }
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn commit_row_updates(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    updates: Vec<RowUpdateBatch>,
) -> Result<(), CommandError> {
    commit_row_updates_impl(&state, connection_id, table, updates).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::TestAppState;

    #[tokio::test]
    async fn commit_row_updates_success() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("data-cfg").await;
        commit_row_updates_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowUpdateBatch {
                set_columns: vec![CellUpdate {
                    column: "name".into(),
                    value: Some(Value::String("Alice".into())),
                }],
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn commit_row_updates_not_connected_errors() {
        let test = TestAppState::new().await;
        assert!(commit_row_updates_impl(
            &test.state,
            "missing".into(),
            "users".into(),
            vec![],
        )
        .await
        .is_err());
    }
}
