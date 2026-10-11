//! Retained physical sessions. The transactions map owns the checkout even
//! between transactions; fixed_sessions stores the immutable server identity.
use crate::PostgresDriver;
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::session::{ObservationConfidence, SessionContext, TransactionState};
use datazen_driver_api::*;
use sqlx::Row;

impl PostgresDriver {
    pub(crate) async fn open_fixed_session(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        let mut config = config.clone();
        config.max_pool_size = 1;
        let handle = self.connect_impl(&config).await?;
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
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *connection)
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            self.transactions.lock().await.insert(
                handle.id.clone(),
                std::sync::Arc::new(tokio::sync::Mutex::new(connection)),
            );
            self.fixed_sessions
                .write()
                .await
                .insert(handle.id.clone(), pid);
            self.read_fixed_session(&handle).await?;
            Ok(handle.clone())
        }
        .await;
        if outcome.is_err() {
            let _ = self.disconnect_impl(handle).await;
        }
        outcome
    }

    pub(crate) async fn read_fixed_session(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<SessionContext, DriverError> {
        let pid = self
            .fixed_sessions
            .read()
            .await
            .get(&handle.id)
            .copied()
            .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
        // Holding the checkout serializes this observation with user SQL.
        // SQLx fetch/execute has consumed ReadyForQuery before this point.
        let pinned = self
            .transactions
            .lock()
            .await
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
        let mut connection = pinned.lock().await;
        sqlx::Connection::ping(&mut **connection)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        let control = self
            .control_pools
            .read()
            .await
            .get(&handle.pool_id)
            .cloned()
            .ok_or_else(|| {
                DriverError::ConnectionFailed("SessionLost: control connection unavailable".into())
            })?;
        let mut observer = control
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        // Explicitly invalidate any statistics snapshot retained by the observer.
        sqlx::query("SELECT pg_stat_clear_snapshot()")
            .execute(&mut *observer)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM pg_stat_activity WHERE pid = $1")
                .bind(pid)
                .fetch_optional(&mut *observer)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let transaction_state = match state.as_deref() {
            Some("idle") => TransactionState::Idle,
            Some("idle in transaction") => TransactionState::Active,
            Some("idle in transaction (aborted)") => TransactionState::Aborted,
            None => {
                return Err(DriverError::ConnectionFailed(
                    "SessionLost: backend disappeared".into(),
                ))
            }
            _ => TransactionState::Unknown,
        };
        if transaction_state == TransactionState::Aborted {
            // SQL cannot read facts in an aborted transaction. Retain the last
            // observed namespace without representing it as a fresh read.
            let mut context = self
                .fixed_contexts
                .read()
                .await
                .get(&handle.id)
                .cloned()
                .unwrap_or_else(SessionContext::unobserved);
            context.transaction_state = transaction_state;
            context.autocommit = Some(false);
            context.confidence = ObservationConfidence::Partial;
            return Ok(context);
        }
        let row = sqlx::query(
            "SELECT current_database(), current_schema(), current_user, current_schemas(false)",
        )
        .fetch_one(&mut **connection)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let context = SessionContext {
            namespace: NamespaceTarget {
                database: row.try_get(0).map_err(decode_error)?,
                schema: row.try_get(1).map_err(decode_error)?,
                catalog: None,
                path: Vec::new(),
            },
            search_path: row.try_get(3).map_err(decode_error)?,
            effective_identity: Some(row.try_get(2).map_err(decode_error)?),
            transaction_state,
            autocommit: Some(transaction_state == TransactionState::Idle),
            confidence: ObservationConfidence::Confirmed,
        };
        self.fixed_contexts
            .write()
            .await
            .insert(handle.id.clone(), context.clone());
        Ok(context)
    }
}
fn decode_error(error: sqlx::Error) -> DriverError {
    DriverError::QueryFailed(error.to_string())
}
