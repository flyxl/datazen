//! SQLite-backed P5 Job repository over the shared `datazen.sqlite` AppDb.

mod access;
mod admission;
mod codec;
mod results;
mod runtime_ops;
mod writes;

use std::sync::Arc;

use crate::store::app_db::AppDb;
use datazen_platform_api::error::PortError;
use datazen_runtime::job::JobClock;
use rusqlite::{Connection, Transaction, TransactionBehavior};

/// Local policy: job references remain queryable for 30 days; referenced attachment bytes
/// stay outside SQLite under their owning export destination and are never copied here.
pub const JOB_ARTIFACT_REFERENCE_TTL_SECS: i64 = 30 * 24 * 60 * 60;
pub const MAX_DURABLE_JOB_PLAN_BYTES: usize = 1024 * 1024;

pub struct SqliteJobRepository {
    pub(super) db: Arc<AppDb>,
    pub(super) clock: Arc<dyn JobClock>,
    pub(super) claim_ttl_secs: i64,
}

impl SqliteJobRepository {
    pub fn new(db: Arc<AppDb>, clock: Arc<dyn JobClock>, claim_ttl_secs: i64) -> Self {
        Self {
            db,
            clock,
            claim_ttl_secs: claim_ttl_secs.max(1),
        }
    }

    pub(super) fn with_tx<T>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> Result<T, PortError>,
    ) -> Result<T, PortError> {
        let mut conn = self.db.conn.lock().map_err(|_| {
            PortError::BackendUnavailable("local job database lock unavailable".into())
        })?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| {
                PortError::BackendUnavailable("local job database is unavailable".into())
            })?;
        let value = f(&tx)?;
        tx.commit().map_err(|_| {
            PortError::BackendUnavailable("local job database commit failed".into())
        })?;
        Ok(value)
    }

    pub(super) fn with_read<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, PortError>,
    ) -> Result<T, PortError> {
        let conn = self.db.conn.lock().map_err(|_| {
            PortError::BackendUnavailable("local job database lock unavailable".into())
        })?;
        f(&conn)
    }
}

pub(super) fn now_from_clock(clock: &dyn JobClock) -> datazen_platform_api::id::Timestamp {
    clock.now()
}
