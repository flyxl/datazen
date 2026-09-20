//! Connection sessions, connection establishment, and tunnel lifecycle.

mod connections;
mod sessions;
mod tunnels;

use crate::db::registry::DriverRegistry;
use crate::db::{ConnectionConfig, ConnectionHandle, DatabaseType, DriverError};
use crate::store::Store;
use crate::tunnel::Tunnel;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::Duration;

pub(super) struct ActiveSession {
    pub(super) handle: ConnectionHandle,
    pub(super) config: ConnectionConfig,
    #[allow(dead_code)]
    pub(super) created_at: Instant,
    pub(super) last_used: Instant,
    pub(super) tunnel: Option<Tunnel>,
}

pub struct ConnectionManager {
    pub(super) registry: Arc<DriverRegistry>,
    pub(super) connections: Arc<RwLock<HashMap<String, ActiveSession>>>,
    pub(super) session_owner_map: Arc<RwLock<HashMap<String, String>>>,
    pub(super) ref_counts: Arc<RwLock<HashMap<String, usize>>>,
    pub(super) store: Arc<Store>,
    pub(super) idle_timeout: Duration,
    pub(super) connect_locks: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ConnectionManager {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>) -> Self {
        Self {
            registry,
            connections: Arc::new(RwLock::new(HashMap::new())),
            session_owner_map: Arc::new(RwLock::new(HashMap::new())),
            ref_counts: Arc::new(RwLock::new(HashMap::new())),
            store,
            idle_timeout: Duration::from_secs(1800),
            connect_locks: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error(
        "Connection config '{0}' not found (connectionId refers to a persisted \
		 connection configuration; no such configuration is stored)"
    )]
    ConnectionConfigNotFound(String),

    #[error(
        "DB session '{0}' not found (a dbSessionId is a runtime session id; \
		 maybe you passed a connectionId where a dbSessionId was expected)"
    )]
    DbSessionNotFound(String),

    #[error("Driver not found for type: {0}")]
    DriverNotFound(DatabaseType),

    #[error("Driver error: {0}")]
    DriverError(#[from] DriverError),

    #[error("Internal error: {0}")]
    Internal(String),
}
