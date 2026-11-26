//! Transaction lifecycle, including the consistent-snapshot read path.

use super::*;

impl MysqlDriver {
    pub(crate) async fn begin_transaction_inner(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let txs = self.transactions.lock().await;
        if self.fixed_sessions.read().await.contains_key(&handle.id) {
            let pinned = txs
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            drop(txs);
            let mut conn = pinned.lock().await;
            Self::reject_nested_fixed_transaction(&mut conn).await?;
            Self::execute_text_on_conn(&mut *conn, "BEGIN")
                .await
                .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                handle.id.clone(),
                datazen_driver_api::session::TransactionState::Active,
            );
            return Ok(TransactionHandle {
                id: format!("fixed_tx_{}", uuid::Uuid::new_v4()),
                connection_id: handle.id.clone(),
            });
        }
        if txs.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }

        drop(txs);
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        drop(pools);

        self.apply_active_database(handle, &mut conn).await?;

        Self::execute_text_on_conn(&mut conn, "BEGIN")
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;

        self.transactions.lock().await.insert(
            handle.id.clone(),
            std::sync::Arc::new(tokio::sync::Mutex::new(conn)),
        );
        Ok(TransactionHandle {
            id: format!("mysql_tx_{}", uuid::Uuid::new_v4()),
            connection_id: handle.id.clone(),
        })
    }

    // BEGIN would implicitly commit an existing MySQL transaction. A savepoint
    // is ignored outside a transaction, so only a successful RELEASE proves
    // that one was active. Unlike SET TRANSACTION this probe leaves the user's
    // one-shot isolation/access settings alone. It also works for raw SQL
    // transactions when Performance Schema observation is unavailable.
    async fn reject_nested_fixed_transaction(
        conn: &mut sqlx::MySqlConnection,
    ) -> Result<(), DriverError> {
        let name = format!("datazen_begin_{}", uuid::Uuid::new_v4().simple());
        Self::execute_text_on_conn(conn, &format!("SAVEPOINT {name}"))
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        match Self::execute_text_on_conn(conn, &format!("RELEASE SAVEPOINT {name}")).await {
            Ok(_) => Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            )),
            Err(sqlx::Error::Database(error))
                if error
                    .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                    .is_some_and(|error| error.number() == 1305) =>
            {
                Ok(())
            }
            Err(error) => Err(DriverError::TransactionError(error.to_string())),
        }
    }

    pub(crate) async fn begin_read_snapshot_inner(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let txs = self.transactions.lock().await;
        if self.fixed_sessions.read().await.contains_key(&handle.id) {
            let pinned = txs
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            drop(txs);
            let mut conn = pinned.lock().await;
            Self::execute_text_on_conn(
                &mut *conn,
                "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
            )
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            Self::execute_text_on_conn(
                &mut *conn,
                "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY",
            )
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                handle.id.clone(),
                datazen_driver_api::session::TransactionState::Active,
            );
            return Ok(TransactionHandle {
                id: format!("fixed_tx_{}", uuid::Uuid::new_v4()),
                connection_id: handle.id.clone(),
            });
        }
        if txs.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }

        drop(txs);
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        drop(pools);

        self.apply_active_database(handle, &mut conn).await?;

        // WITH CONSISTENT SNAPSHOT is only effective with REPEATABLE READ.
        // Scope the isolation override to the next transaction so the pool's
        // session defaults are not changed for later checkouts.
        if let Err(e) =
            Self::execute_text_on_conn(&mut conn, "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
                .await
        {
            let _ = conn.close().await;
            return Err(DriverError::TransactionError(e.to_string()));
        }

        // MySQL and MariaDB support these transaction characteristics. If a
        // server rejects one, close the connection so a pending one-shot
        // isolation setting cannot leak into a later pooled transaction.
        if let Err(e) = Self::execute_text_on_conn(
            &mut conn,
            "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY",
        )
        .await
        {
            let _ = conn.close().await;
            return Err(DriverError::TransactionError(e.to_string()));
        }

        self.transactions.lock().await.insert(
            handle.id.clone(),
            std::sync::Arc::new(tokio::sync::Mutex::new(conn)),
        );
        Ok(TransactionHandle {
            id: format!("mysql_snapshot_{}", uuid::Uuid::new_v4()),
            connection_id: handle.id.clone(),
        })
    }

    pub(crate) async fn commit_inner(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        if self
            .fixed_sessions
            .read()
            .await
            .contains_key(&tx.connection_id)
        {
            let pinned = self
                .transactions
                .lock()
                .await
                .get(&tx.connection_id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut conn = pinned.lock().await;
            Self::execute_text_on_conn(&mut *conn, "COMMIT")
                .await
                .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                tx.connection_id.clone(),
                datazen_driver_api::session::TransactionState::Idle,
            );
            return Ok(());
        }
        let pinned = self
            .transactions
            .lock()
            .await
            .remove(&tx.connection_id)
            .ok_or_else(|| {
                DriverError::TransactionError("Transaction not found or already ended".into())
            })?;
        let mut conn = pinned.lock().await;

        Self::execute_text_on_conn(&mut conn, "COMMIT")
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        Ok(())
    }

    pub(crate) async fn rollback_inner(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        if self
            .fixed_sessions
            .read()
            .await
            .contains_key(&tx.connection_id)
        {
            let pinned = self
                .transactions
                .lock()
                .await
                .get(&tx.connection_id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut conn = pinned.lock().await;
            Self::execute_text_on_conn(&mut *conn, "ROLLBACK")
                .await
                .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                tx.connection_id.clone(),
                datazen_driver_api::session::TransactionState::Idle,
            );
            return Ok(());
        }
        let pinned = self
            .transactions
            .lock()
            .await
            .remove(&tx.connection_id)
            .ok_or_else(|| {
                DriverError::TransactionError("Transaction not found or already ended".into())
            })?;
        let mut conn = pinned.lock().await;

        Self::execute_text_on_conn(&mut conn, "ROLLBACK")
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        Ok(())
    }
}
