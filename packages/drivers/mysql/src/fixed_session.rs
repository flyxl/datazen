//! Own a checked-out connection for the entire resource lifetime.
use crate::MysqlDriver;
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::session::{ObservationConfidence, SessionContext, TransactionState};
use datazen_driver_api::*;
use sqlx::Row;

impl MysqlDriver {
    pub(crate) async fn open_fixed_session(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        let mut config = config.clone();
        config.max_pool_size = 1;
        let handle = self.connect(&config).await?;
        let outcome = async {
            let pool = self
                .pools
                .read()
                .await
                .get(&handle.pool_id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut connection = pool
                .acquire()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
                .fetch_one(&mut *connection)
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            self.transactions.lock().await.insert(
                handle.id.clone(),
                std::sync::Arc::new(tokio::sync::Mutex::new(connection)),
            );
            self.fixed_states
                .write()
                .await
                .insert(handle.id.clone(), TransactionState::Idle);
            self.fixed_sessions
                .write()
                .await
                .insert(handle.id.clone(), id);
            Ok(handle.clone())
        }
        .await;
        if outcome.is_err() {
            let _ = self.disconnect(handle).await;
        }
        outcome
    }
    pub(crate) async fn read_fixed_session(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<SessionContext, DriverError> {
        let expected_id = self
            .fixed_sessions
            .read()
            .await
            .get(&handle.id)
            .copied()
            .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
        let pinned = self
            .transactions
            .lock()
            .await
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
        let mut connection = pinned.lock().await;
        let row =
            sqlx::query("SELECT DATABASE(), CURRENT_USER(), @@session.autocommit, CONNECTION_ID()")
                .fetch_one(&mut **connection)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let id: u64 = row.try_get(3).map_err(decode_error)?;
        if id != expected_id {
            return Err(DriverError::ConnectionFailed(
                "SessionLost: backend changed".into(),
            ));
        }
        let transaction_state = if self.is_mariadb {
            let active: i64 = sqlx::query_scalar("SELECT @@session.in_transaction")
                .fetch_one(&mut **connection)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            if active == 0 {
                TransactionState::Idle
            } else {
                TransactionState::Active
            }
        } else {
            // MySQL's wire IN_TRANS flag is private in SQLx. Never derive it
            // from SQL text or TransactionManager's unrelated nesting counter.
            let fallback = self
                .fixed_states
                .read()
                .await
                .get(&handle.id)
                .copied()
                .unwrap_or(TransactionState::Unknown);
            // Only trust an empty current-transaction table after proving that
            // the transaction instrumentation and its consumer are enabled for
            // this exact thread. The control query cannot observe its own
            // autocommit SELECT as the target transaction.
            let control = self
                .control_pools
                .read()
                .await
                .get(&handle.pool_id)
                .cloned();
            if let Some(control) = control {
                let evidence = sqlx::query("SELECT t.INSTRUMENTED = 'YES' AND EXISTS (SELECT 1 FROM performance_schema.setup_instruments WHERE NAME = 'transaction' AND ENABLED = 'YES') AND EXISTS (SELECT 1 FROM performance_schema.setup_consumers WHERE NAME = 'events_transactions_current' AND ENABLED = 'YES') AND EXISTS (SELECT 1 FROM performance_schema.setup_consumers WHERE NAME = 'global_instrumentation' AND ENABLED = 'YES') AND EXISTS (SELECT 1 FROM performance_schema.setup_consumers WHERE NAME = 'thread_instrumentation' AND ENABLED = 'YES') AS observable, e.STATE FROM performance_schema.threads t LEFT JOIN performance_schema.events_transactions_current e ON e.THREAD_ID = t.THREAD_ID WHERE t.PROCESSLIST_ID = ?")
                    .bind(expected_id).fetch_optional(&control).await;
                match evidence {
                    Ok(Some(row)) if row.try_get::<i64, _>(0).unwrap_or(0) != 0 => {
                        match row
                            .try_get::<Option<String>, _>(1)
                            .ok()
                            .flatten()
                            .as_deref()
                        {
                            Some("ACTIVE") => TransactionState::Active,
                            Some("COMMITTED") | Some("ROLLED BACK") | None => {
                                TransactionState::Idle
                            }
                            _ => TransactionState::Unknown,
                        }
                    }
                    _ => fallback,
                }
            } else {
                fallback
            }
        };
        Ok(SessionContext {
            namespace: NamespaceTarget {
                database: row.try_get(0).map_err(decode_error)?,
                catalog: None,
                schema: None,
                path: Vec::new(),
            },
            effective_identity: Some(row.try_get(1).map_err(decode_error)?),
            search_path: Vec::new(),
            transaction_state,
            autocommit: Some(row.try_get::<i64, _>(2).map_err(decode_error)? != 0),
            confidence: if transaction_state == TransactionState::Unknown {
                ObservationConfidence::Partial
            } else {
                ObservationConfidence::Confirmed
            },
        })
    }
}
fn decode_error(error: sqlx::Error) -> DriverError {
    DriverError::QueryFailed(error.to_string())
}
