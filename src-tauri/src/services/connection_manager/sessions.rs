use super::{ActiveSession, ConnectionError, ConnectionManager};
use crate::db::{ConnectionHandle, DatabaseDriver, ServerInfo};
use std::sync::Arc;
use std::time::Instant;

impl ConnectionManager {
    pub async fn get_or_connect_session(
        &self,
        connection_id: &str,
    ) -> Result<String, ConnectionError> {
        let lock = {
            let mut locks = self
                .connect_locks
                .lock()
                .map_err(|e| ConnectionError::Internal(format!("connect lock poisoned: {e}")))?;
            locks
                .entry(connection_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        {
            let owner_map = self.session_owner_map.read().await;
            let connections = self.connections.read().await;
            for (session_id, owner_connection_id) in owner_map.iter() {
                if owner_connection_id == connection_id && connections.contains_key(session_id) {
                    let mut refs = self.ref_counts.write().await;
                    *refs.entry(session_id.clone()).or_insert(0) += 1;
                    return Ok(session_id.clone());
                }
            }
        }
        let db_session_id = self.connect(connection_id).await?;
        let mut refs = self.ref_counts.write().await;
        *refs.entry(db_session_id.clone()).or_insert(0) += 1;
        Ok(db_session_id)
    }

    pub async fn release(&self, db_session_id: &str) -> Result<bool, ConnectionError> {
        let should_disconnect = {
            let mut refs = self.ref_counts.write().await;
            if let Some(count) = refs.get_mut(db_session_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    refs.remove(db_session_id);
                    true
                } else {
                    false
                }
            } else {
                true
            }
        };
        if should_disconnect {
            self.disconnect(db_session_id).await?;
        }
        Ok(should_disconnect)
    }

    pub async fn disconnect(&self, db_session_id: &str) -> Result<(), ConnectionError> {
        self.ref_counts.write().await.remove(db_session_id);
        self.session_owner_map.write().await.remove(db_session_id);
        if let Some(active) = self.connections.write().await.remove(db_session_id) {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                // A driver that cannot confirm the teardown has left the
                // physical connection in an unknown state, so the caller is
                // told. Swallowing it here reported a clean disconnect for a
                // connection that may still be up.
                //
                // ── Why the three removals stay above the await (decided, not
                // incidental) ────────────────────────────────────────────────
                // Read the block above as a **claim**, not as cleanup that got
                // ordered before the side effect by accident. All three table
                // removals happen before the single `.await` point, and that
                // buys two properties the naive "disconnect first, remove the
                // entries on success" order destroys:
                //
                // 1. *Exactly one teardown per handle.* `connections` is one
                //    `RwLock<HashMap<..>>` and `remove` is atomic under its
                //    write lock, so two concurrent `disconnect` calls for the
                //    same `dbSessionId` cannot both obtain the same
                //    `ActiveSession` — the loser finds `None` and returns
                //    `Ok(())` without touching the driver. Deferring the
                //    removal would let both callers observe the entry and both
                //    invoke `driver.disconnect` on the same `handle`.
                //    `DatabaseDriver::disconnect`
                //    (`packages/driver-api/src/traits.rs:254`) carries no
                //    idempotency contract, so a second call on a live
                //    `pool_id` is not something the host may assume is safe.
                // 2. *No window in which a dying session can be handed out.*
                //    `get_or_connect_session` matches on
                //    `session_owner_map` ∩ `connections`, and `reconnect`
                //    resolves its owner through `session_owner_map`. Because
                //    both entries are already gone before the await, neither
                //    can observe the id while the physical connection is being
                //    torn down. If the removals were deferred, a tab opening
                //    concurrently would match the entry, bump `ref_counts` to
                //    1 and be given a `dbSessionId` whose teardown then
                //    succeeds — i.e. a session id that is dead on first use,
                //    strictly worse than "not retryable", and it would also
                //    resurrect a session that `release` had just decided was
                //    finished.
                //
                // ── The residual gap this leaves, stated honestly ──────────
                // The cost of the claim is that the failure cannot be retried:
                // the `ActiveSession` — the host's only reference to the
                // physical resource, since `ConnectionHandle`
                // (`packages/driver-api/src/types.rs:309`) is plain data with
                // no `Drop` of its own — is dropped when this function returns,
                // so a second `disconnect` with the same id takes the
                // `None` branch and reports `Ok(())` for a connection the
                // driver already said it could not tear down. That is a real
                // gap, and it is **not fixed here on purpose**: closing it
                // requires a fourth table (a quarantine holding the unconfirmed
                // `ActiveSession`, consulted by `disconnect` but deliberately
                // not by `get_or_connect_session`/`reconnect`, so property 2
                // above survives) — that is a state-shape change to the manager
                // and needs its own review, not a drive-by inside a
                // swallowed-error fix. Recording the shape here so the next
                // reader does not re-derive it.
                driver.disconnect(active.handle).await?;
            }
        }
        Ok(())
    }

    pub async fn get_session(
        &self,
        db_session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        {
            let mut connections = self.connections.write().await;
            if let Some(active) = connections.get_mut(db_session_id) {
                active.last_used = Instant::now();
                let driver = self
                    .registry
                    .get(&active.config.database_type)
                    .await
                    .ok_or_else(|| {
                        ConnectionError::DriverNotFound(active.config.database_type.clone())
                    })?;
                return Ok((driver, active.handle.clone()));
            }
        }
        self.reconnect(db_session_id).await
    }

    pub async fn resolve_session_for_connection(
        &self,
        connection_id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let db_session_id = self.get_or_connect_session(connection_id).await?;
        let (driver, handle) = self.get_session(&db_session_id).await?;
        Ok((db_session_id, driver, handle))
    }

    async fn reconnect(
        &self,
        db_session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let connection_id = self
            .session_owner_map
            .read()
            .await
            .get(db_session_id)
            .cloned()
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;
        let config = self
            .store
            .get_connection(&connection_id)
            .await
            .ok_or_else(|| ConnectionError::ConnectionConfigNotFound(connection_id.clone()))?;
        let identity_config = self.resolve_tunnel_ref(config).await?;
        let (effective_config, tunnel) = self.start_tunnel(identity_config.clone()).await?;
        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or_else(|| {
                ConnectionError::DriverNotFound(effective_config.database_type.clone())
            })?;
        let mut handle = driver.connect(&effective_config).await?;
        handle.id = db_session_id.to_string();
        self.connections.write().await.insert(
            db_session_id.to_string(),
            ActiveSession {
                handle: handle.clone(),
                config: effective_config,
                identity_config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                tunnel,
            },
        );
        Ok((driver, handle))
    }

    /// Physical routing snapshot captured before tunnel rewrites; current scope stays authoritative.
    pub(crate) async fn migration_identity_config(
        &self,
        db_session_id: &str,
    ) -> Result<crate::db::ConnectionConfig, ConnectionError> {
        let sessions = self.connections.read().await;
        let active = sessions
            .get(db_session_id)
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;
        let mut config = active.identity_config.clone();
        config.database = active.config.database.clone();
        config.schema = active.config.schema.clone();
        Ok(config)
    }

    pub async fn get_session_config(
        &self,
        db_session_id: &str,
    ) -> Result<crate::db::ConnectionConfig, ConnectionError> {
        self.connections
            .read()
            .await
            .get(db_session_id)
            .map(|active| active.config.clone())
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    pub async fn get_server_info(
        &self,
        db_session_id: &str,
    ) -> Result<ServerInfo, ConnectionError> {
        self.server_info(db_session_id).await
    }
}
