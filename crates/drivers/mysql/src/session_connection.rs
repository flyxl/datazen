//! Which physical connection a statement runs on.
//!
//! A retained (`fixed`) session checks out its pool's **only** connection for
//! its whole lifetime and keeps it in `MysqlDriver::transactions`. Catalog
//! reads used to borrow from that same pool, so they waited for a checkout that
//! is never returned before disconnect and failed after the acquire timeout.
//! The rule is structural: a statement for a session with a pinned connection
//! runs on that pinned connection.
//!
//! Both shapes deref to `&mut MySqlConnection`, so callers write the query once.

use crate::MysqlDriver;
use datazen_driver_api::{ConnectionHandle, DriverError};
use sqlx::mysql::MySqlConnection;
use sqlx::pool::PoolConnection;
use sqlx::MySql;
use std::ops::{Deref, DerefMut};

pub(crate) enum SessionConnection {
    Pinned(tokio::sync::OwnedMutexGuard<PoolConnection<MySql>>),
    Pooled(PoolConnection<MySql>),
}

impl SessionConnection {
    /// True when this is the retained session's own connection.
    pub(crate) fn is_pinned(&self) -> bool {
        matches!(self, Self::Pinned(_))
    }
}

impl Deref for SessionConnection {
    type Target = MySqlConnection;

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
/// wait for the checkout that the retained session itself is holding. MySQL
/// catalog queries bind `TABLE_SCHEMA`, so unlike PostgreSQL a pinned session
/// can safely read another database without a `USE`.
pub(crate) async fn for_session(
    driver: &MysqlDriver,
    handle: &ConnectionHandle,
) -> Result<SessionConnection, DriverError> {
    let pinned = driver.transactions.lock().await.get(&handle.id).cloned();
    if let Some(pinned) = pinned {
        return Ok(SessionConnection::Pinned(pinned.lock_owned().await));
    }

    let pool = {
        let pools = driver.pools.read().await;
        MysqlDriver::get_pool(&pools, handle)?.clone()
    };
    let connection = pool
        .acquire()
        .await
        .map_err(|error| DriverError::ConnectionFailed(error.to_string()))?;
    Ok(SessionConnection::Pooled(connection))
}
