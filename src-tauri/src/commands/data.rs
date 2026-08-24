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

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDeleteBatch {
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
        .get_session(&connection_id)
        .await
        .cmd_err("commit_row_updates")?;
    let read_only = state
        .connection_manager
        .get_session_config(&connection_id)
        .await
        .map(|c| c.read_only)
        .unwrap_or(false);
    if read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; row edits are not allowed".into(),
        ));
    }

    let session_open = state
        .session_transactions
        .lock()
        .await
        .contains_key(&connection_id);

    // Prefer DatabaseDriver transaction API; fall back to BEGIN/COMMIT strings
    // for drivers that still stub the trait. Skip wrapping when a session
    // transaction is already open so row edits join that transaction.
    let trait_tx = if session_open {
        None
    } else {
        match driver.begin_transaction(&handle).await {
            Ok(tx) => Some(tx),
            Err(DriverError::TransactionError(_)) => None,
            Err(e) => return Err(CommandError::Driver(e)),
        }
    };

    if !session_open && trait_tx.is_none() {
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
            if session_open {
                tracing::info!(%connection_id, %table, batch_count = updates.len(), "commit_row_updates OK (session tx)");
                return Ok(());
            }
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
            if session_open {
                return Err(e);
            }
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

#[tauri::command]
pub async fn commit_row_updates(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    updates: Vec<RowUpdateBatch>,
) -> Result<(), CommandError> {
    commit_row_updates_impl(&state, connection_id, table, updates).await
}

pub(crate) async fn commit_row_deletes_impl(
    state: &AppState,
    connection_id: String,
    table: String,
    deletes: Vec<RowDeleteBatch>,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, %table, batch_count = deletes.len(), "commit_row_deletes");
    let (driver, handle) = state
        .connection_manager
        .get_session(&connection_id)
        .await
        .cmd_err("commit_row_deletes")?;
    let read_only = state
        .connection_manager
        .get_session_config(&connection_id)
        .await
        .map(|c| c.read_only)
        .unwrap_or(false);
    if read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; row deletes are not allowed".into(),
        ));
    }
    if deletes.is_empty() {
        return Ok(());
    }
    for batch in &deletes {
        if batch.pk_columns.is_empty() {
            return Err(CommandError::Validation(
                "Row delete requires primary-key columns".into(),
            ));
        }
    }

    let session_open = state
        .session_transactions
        .lock()
        .await
        .contains_key(&connection_id);

    let trait_tx = if session_open {
        None
    } else {
        match driver.begin_transaction(&handle).await {
            Ok(tx) => Some(tx),
            Err(DriverError::TransactionError(_)) => None,
            Err(e) => return Err(CommandError::Driver(e)),
        }
    };

    if !session_open && trait_tx.is_none() {
        driver
            .execute(&handle, "BEGIN")
            .await
            .cmd_err("commit_row_deletes")?;
    }

    let result: Result<(), CommandError> = async {
        for batch in &deletes {
            let pk_columns: Vec<(&str, Option<Value>)> = batch
                .pk_columns
                .iter()
                .map(|c| (c.column.as_str(), c.value.clone()))
                .collect();
            let sql = driver.build_delete_sql(&table, &pk_columns);
            driver
                .execute(&handle, &sql)
                .await
                .cmd_err("commit_row_deletes")?;
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            if session_open {
                tracing::info!(%connection_id, %table, batch_count = deletes.len(), "commit_row_deletes OK (session tx)");
                return Ok(());
            }
            if let Some(tx) = trait_tx {
                driver.commit(tx).await.cmd_err("commit_row_deletes")?;
            } else {
                driver
                    .execute(&handle, "COMMIT")
                    .await
                    .cmd_err("commit_row_deletes")?;
            }
            tracing::info!(%connection_id, %table, batch_count = deletes.len(), "commit_row_deletes OK");
            Ok(())
        }
        Err(e) => {
            if session_open {
                return Err(e);
            }
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

#[tauri::command]
pub async fn commit_row_deletes(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    deletes: Vec<RowDeleteBatch>,
) -> Result<(), CommandError> {
    commit_row_deletes_impl(&state, connection_id, table, deletes).await
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
        assert!(
            commit_row_updates_impl(&test.state, "missing".into(), "users".into(), vec![],)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn commit_row_updates_rejects_read_only_connection() {
        let test = TestAppState::with_tables().await;
        let mut config = crate::testing::app_state::sample_postgres_config("ro-edit");
        config.read_only = true;
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("ro-edit").await;
        let err = commit_row_updates_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowUpdateBatch {
                set_columns: vec![CellUpdate {
                    column: "name".into(),
                    value: Some(Value::String("x".into())),
                }],
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn commit_row_updates_joins_open_session_transaction() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("tx-edit").await;
        crate::commands::begin_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        commit_row_updates_impl(
            &test.state,
            conn_id.clone(),
            "users".into(),
            vec![RowUpdateBatch {
                set_columns: vec![CellUpdate {
                    column: "name".into(),
                    value: Some(Value::String("Bob".into())),
                }],
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap();
        assert!(
            crate::commands::session_transaction_status_impl(&test.state, conn_id.clone())
                .await
                .unwrap()
        );
        crate::commands::commit_session_transaction_impl(&test.state, conn_id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn commit_row_deletes_success() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("data-del").await;
        commit_row_deletes_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowDeleteBatch {
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
    async fn commit_row_deletes_rejects_read_only_connection() {
        let test = TestAppState::with_tables().await;
        let mut config = crate::testing::app_state::sample_postgres_config("ro-del");
        config.read_only = true;
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("ro-del").await;
        let err = commit_row_deletes_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowDeleteBatch {
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn commit_row_deletes_rejects_empty_pk() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("data-del-empty-pk").await;
        let err = commit_row_deletes_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowDeleteBatch { pk_columns: vec![] }],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("primary-key"));
    }
}
