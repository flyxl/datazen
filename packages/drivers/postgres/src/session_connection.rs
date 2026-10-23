//! Which physical connection a statement runs on.
//!
//! A retained (`fixed`) session checks out its pool's **only** connection for
//! its whole lifetime and keeps it in `PostgresDriver::transactions`. Catalog
//! and admin reads used to borrow from that same pool, so they waited for the
//! checkout to be returned — which never happens before disconnect — and only
//! failed after the pool's acquire timeout. The rule is structural: a statement
//! for a session with a pinned connection runs on that pinned connection.
//!
//! The two shapes are unified here so callers write the query once:
//! `SessionConnection` derefs to `&mut PgConnection` whether it owns a pooled
//! checkout or an owned guard over the pinned connection.

use crate::postgres::PostgresDriver;
use datazen_driver_api::{ConnectionHandle, DriverError};
use sqlx::{pool::PoolConnection, PgConnection, Postgres};
use std::ops::{Deref, DerefMut};

pub(crate) enum SessionConnection {
    Pinned(tokio::sync::OwnedMutexGuard<PoolConnection<Postgres>>),
    Pooled(PoolConnection<Postgres>),
}

impl Deref for SessionConnection {
    type Target = PgConnection;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Pinned(guard) => guard,
            Self::Pooled(connection) => connection,
        }
    }
}

impl DerefMut for SessionConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Pinned(guard) => guard,
            Self::Pooled(connection) => connection,
        }
    }
}

/// Borrow the connection a statement for `handle` must run on.
///
/// Pinned wins over the pool: borrowing from a retained session's pool would
/// wait for the checkout that the retained session itself is holding. A
/// retained session is also bound to its own database, so a foreign target is
/// rejected here instead of quietly reading the wrong catalog.
pub(crate) async fn for_session(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    database: &str,
) -> Result<SessionConnection, DriverError> {
    let pinned = driver.transactions.lock().await.get(&handle.id).cloned();
    if let Some(pinned) = pinned {
        let database = database.trim();
        if !database.is_empty() && !driver.is_active_database(handle, database).await {
            return Err(DriverError::TransactionError(format!(
                "cannot read database '{database}' through a session bound to another database"
            )));
        }
        return Ok(SessionConnection::Pinned(pinned.lock_owned().await));
    }

    let pool = driver.pool_for_target(handle, database).await?;
    let connection = pool
        .acquire()
        .await
        .map_err(|error| DriverError::ConnectionFailed(error.to_string()))?;
    Ok(SessionConnection::Pooled(connection))
}
