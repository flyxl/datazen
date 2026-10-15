//! Shared server-wide control channel. Business databases do not define identity.
//! The registry retains both pool and permit until physical close is confirmed.
use crate::{connection::build_pg_options, PostgresDriver};
use datazen_driver_api::{
    resource::{BudgetPermit, BudgetPort},
    ConnectionConfig, ConnectionHandle, DriverError,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{collections::HashMap, sync::Arc, time::Duration};

pub(crate) struct ControlPoolEntry {
    pool: Option<PgPool>,
    owners: HashMap<String, ConnectionConfig>,
    anchor: ConnectionConfig,
    closing: bool,
    budget: Option<(Arc<dyn BudgetPort>, BudgetPermit)>,
}

impl PostgresDriver {
    pub(crate) fn control_pool_key(config: &ConnectionConfig) -> String {
        // Escaped quoted fields prevent ambiguous profile/host/user separators.
        format!(
            "pgctl|{:?}|{:?}|{}|{:?}|{:?}",
            config.id,
            config
                .host
                .as_deref()
                .unwrap_or(crate::connection::DEFAULT_HOST),
            config.port.unwrap_or(crate::connection::DEFAULT_PORT),
            config.username,
            config.ssl_mode
        )
    }

    pub(crate) async fn acquire_control_pool(
        &self,
        owner: &str,
        config: &ConnectionConfig,
        anchor: &ConnectionConfig,
        budget: Option<Arc<dyn BudgetPort>>,
    ) -> Result<(), DriverError> {
        let key = Self::control_pool_key(config);
        // Serialize creation, replacement and last-close, including their awaits.
        let mut pools = self.control_pools.write().await;
        if let Some(entry) = pools.get_mut(&key) {
            if entry.closing {
                return Err(DriverError::ConnectionFailed(
                    "shared control closure is pending".into(),
                ));
            }
            // Credentials can rotate without entering a map key. New profile
            // material also serves recovery for the registered business targets.
            entry.anchor = anchor.clone();
            if entry.budget.is_none() {
                if let Some(budget) = budget {
                    let permit = budget
                        .acquire_control_connection()
                        .await
                        .map_err(budget_error)?;
                    entry.budget = Some((budget, permit));
                }
            }
            entry.owners.insert(owner.into(), config.clone());
            return Ok(());
        }
        let charge = match budget {
            Some(budget) => {
                let permit = budget
                    .acquire_control_connection()
                    .await
                    .map_err(budget_error)?;
                Some((budget, permit))
            }
            None => None, // Legacy DatabaseDriver has no budget port.
        };
        // Register before connecting: cancelled connects cannot drop the permit
        // while a pool is still closing. A subsequent close can retry cleanup.
        pools.insert(
            key.clone(),
            ControlPoolEntry {
                pool: None,
                owners: HashMap::from([(owner.into(), config.clone())]),
                anchor: anchor.clone(),
                closing: false,
                budget: charge,
            },
        );
        let entry = pools.get_mut(&key).ok_or(DriverError::SessionLost)?;
        if let Err(error) = entry.ensure_pool().await {
            entry.close().await?;
            pools.remove(&key);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn control_pool_for(&self, handle: &ConnectionHandle) -> Option<PgPool> {
        let key = self
            .connect_configs
            .read()
            .await
            .get(&handle.pool_id)
            .map(Self::control_pool_key)?;
        let mut pools = self.control_pools.write().await;
        let entry = pools.get_mut(&key)?;
        entry.ensure_pool().await.ok()?;
        entry.pool.clone()
    }

    pub(crate) async fn release_control_pool(&self, owner: &str) -> Result<(), DriverError> {
        let key = self
            .connect_configs
            .read()
            .await
            .get(owner)
            .map(Self::control_pool_key);
        let Some(key) = key else {
            return Ok(());
        };
        let mut pools = self.control_pools.write().await;
        let Some(entry) = pools.get_mut(&key) else {
            return Ok(());
        };
        if !entry.owners.contains_key(owner) {
            return Ok(());
        }
        if entry.owners.len() > 1 {
            entry.owners.remove(owner);
        } else {
            // Keep the owner registered across cancellation/failure, so retries
            // cannot decrement twice or release before close completes.
            entry.close().await?;
            pools.remove(&key);
        }
        Ok(())
    }
}

fn budget_error(_: datazen_driver_api::resource::ResourceError) -> DriverError {
    DriverError::ConnectionFailed("control connection budget refused".into())
}

impl ControlPoolEntry {
    async fn ensure_pool(&mut self) -> Result<(), DriverError> {
        if self.closing {
            return Err(DriverError::ConnectionFailed(
                "shared control closure is pending".into(),
            ));
        }
        // Bound the complete recovery journey, including failed candidates.
        let seconds = u64::from(self.anchor.connection_timeout)
            .saturating_mul(3)
            .clamp(1, 10);
        tokio::time::timeout(Duration::from_secs(seconds), self.ensure_pool_inner())
            .await
            .map_err(|_| {
                DriverError::ConnectionFailed("shared control recovery timed out".into())
            })?
    }

    async fn ensure_pool_inner(&mut self) -> Result<(), DriverError> {
        if let Some(pool) = &self.pool {
            if sqlx::query("SELECT 1").execute(pool).await.is_ok() {
                return Ok(());
            }
            // Never overlap old and replacement sockets, even if callers still
            // retain pool clones. close() waits for checked-out operations.
            pool.close().await;
            self.pool = None;
        }
        let mut candidates = vec![self.anchor.clone()];
        let mut maintenance = self.anchor.clone();
        maintenance.database = Some("postgres".into());
        candidates.push(maintenance);
        candidates.extend(self.owners.values().map(|owner| {
            let mut candidate = self.anchor.clone();
            candidate.database = owner.database.clone();
            candidate
        }));
        let mut tried = std::collections::HashSet::new();
        for candidate in candidates {
            let database = PostgresDriver::resolve_connect_database(&candidate).to_owned();
            if !tried.insert(database) {
                continue;
            }
            let opts = build_pg_options(&candidate)?;
            let pool = PgPoolOptions::new()
                .max_connections(crate::resource::capabilities::CONTROL_POOL_CONNECTIONS)
                .min_connections(0)
                .idle_timeout(None)
                .max_lifetime(None)
                .acquire_timeout(Duration::from_secs(candidate.connection_timeout as u64))
                .connect_lazy_with(opts);
            // Save lazy pool before await, making cancelled connects retryable.
            self.pool = Some(pool.clone());
            if sqlx::query("SELECT 1").execute(&pool).await.is_ok() {
                return Ok(());
            }
            pool.close().await;
            self.pool = None;
        }
        Err(DriverError::ConnectionFailed(
            "no available database for shared control connection".into(),
        ))
    }

    async fn close(&mut self) -> Result<(), DriverError> {
        self.closing = true;
        if let Some(pool) = &self.pool {
            pool.close().await;
        }
        if let Some((budget, permit)) = &self.budget {
            budget
                .release_control_connection(permit)
                .await
                .map_err(budget_error)?;
        }
        self.pool = None;
        self.budget = None;
        Ok(())
    }
}

/// Owns admission until the provider registers it. An abandoned acquisition
/// completes cleanup in an owned task instead of freeing a live socket's claim.
pub(crate) struct OpenedResource {
    driver: Arc<PostgresDriver>,
    budget: Arc<dyn BudgetPort>,
    state: Option<(ConnectionHandle, BudgetPermit)>,
}
impl OpenedResource {
    pub(crate) fn new(
        driver: Arc<PostgresDriver>,
        connection: ConnectionHandle,
        budget: Arc<dyn BudgetPort>,
        permit: BudgetPermit,
    ) -> Self {
        Self {
            driver,
            budget,
            state: Some((connection, permit)),
        }
    }
    pub(crate) fn deliver(mut self) -> Result<(ConnectionHandle, BudgetPermit), DriverError> {
        self.state.take().ok_or(DriverError::SessionLost)
    }
}
impl Drop for OpenedResource {
    fn drop(&mut self) {
        let Some((connection, permit)) = self.state.take() else {
            return;
        };
        let driver = self.driver.clone();
        let budget = self.budget.clone();
        tokio::spawn(async move {
            if driver.disconnect_impl(connection).await.is_ok() {
                let _ = budget.release_physical_connections(&permit).await;
            } else {
                // Unconfirmed closure must retain the ledger owner too.
                std::mem::forget(budget);
            }
        });
    }
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
