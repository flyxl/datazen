//! Desktop infrastructure adapter for the module-owned connection repository.
//!
//! D2 (2026-10-26): the connection catalog's domain boundary lives in
//! `datazen-modules::connection::domain`; this adapter is the desktop's
//! implementation. Storage, cache, write lock, and AES-256-GCM keying stay in
//! [`DesktopRepository`] — one storage fact, one cache, one lock — exactly as
//! before; only the port implementation moves here.
//!
//! Desktop is **single-tenant**: it recognizes exactly its local organization
//! and treats every other scope as nonexistent (empty list / `None` /
//! `NotFound`), never as a permission error (CM-05).

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::ConnectionConfig;
use datazen_modules::connection::domain::{
    ConnectionRepository, ConnectionRepositoryError, ConnectionScope,
};

use crate::store::DesktopRepository;

/// The desktop's connection catalog repository (D2: `DesktopConnectionStore`).
#[derive(Clone)]
pub struct DesktopConnectionStore {
    store: Arc<DesktopRepository>,
}

impl DesktopConnectionStore {
    pub fn new(store: Arc<DesktopRepository>) -> Self {
        Self { store }
    }
}

/// The modules-side scope of the desktop's single local organization.
///
/// Used by desktop-internal paths that have no request context (monitor,
/// MCP, schedulers) — they operate on local data under the local org.
pub fn local_connection_scope() -> ConnectionScope {
    ConnectionScope::new(super::identity::LOCAL_ORGANIZATION_ID)
}

fn is_local(scope: &ConnectionScope) -> bool {
    scope.organization_id() == super::identity::LOCAL_ORGANIZATION_ID
}

fn backend_error(error: crate::store::DesktopRepositoryError) -> ConnectionRepositoryError {
    ConnectionRepositoryError::BackendUnavailable(error.to_string())
}

#[async_trait]
impl ConnectionRepository for DesktopConnectionStore {
    async fn list(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        if !is_local(scope) {
            return Ok(Vec::new());
        }
        Ok(self.store.get_connections().await)
    }

    async fn get(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        if !is_local(scope) {
            return Ok(None);
        }
        Ok(self.store.get_connection(connection_id).await)
    }

    async fn save(
        &self,
        scope: &ConnectionScope,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        if !is_local(scope) {
            return Err(ConnectionRepositoryError::NotFound("connection".into()));
        }
        self.store.save_connection(config).await.map_err(backend_error)
    }

    async fn delete(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        if !is_local(scope) {
            return Err(ConnectionRepositoryError::NotFound("connection".into()));
        }
        self.store
            .delete_connection(connection_id)
            .await
            .map_err(backend_error)
    }

    async fn reorder(
        &self,
        scope: &ConnectionScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        if !is_local(scope) {
            return Err(ConnectionRepositoryError::NotFound("connection".into()));
        }
        self.store
            .reorder_connections(ordered_ids)
            .await
            .map_err(backend_error)
    }

    async fn touch_last_connected_at(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        if !is_local(scope) {
            return Err(ConnectionRepositoryError::NotFound("connection".into()));
        }
        self.store
            .touch_last_connected_at(connection_id)
            .await
            .map_err(backend_error)
    }

    async fn groups(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<String>, ConnectionRepositoryError> {
        if !is_local(scope) {
            return Ok(Vec::new());
        }
        Ok(self.store.get_groups().await)
    }

    async fn save_groups(
        &self,
        scope: &ConnectionScope,
        groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        if !is_local(scope) {
            return Err(ConnectionRepositoryError::NotFound("connection group".into()));
        }
        self.store.save_groups(groups).await.map_err(backend_error)
    }
}
