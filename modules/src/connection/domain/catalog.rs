//! 连接目录（表单词汇）的仓储边界：配置 + 分组。
//!
//! The connection aggregate is the persisted connection configuration plus its
//! groups. Its shape is [`ConnectionConfig`] from `datazen-driver-api` — the
//! one form every host already links and the exact IPC payload of the
//! connection form. This module deliberately does **not** re-project it: the
//! form must echo back verbatim, and a second projection type would drift
//! into user-visible data loss. The server-side profile vocabulary of the
//! same aggregate lives in [`super::profile`].

use async_trait::async_trait;
use datazen_driver_api::ConnectionConfig;

/// Organization boundary for connection storage access.
///
/// Cross-organization access is treated as **nonexistent** (empty list /
/// `None` / `NotFound` on write), never as a permission error — the latter
/// would leak "exists but forbidden" (CM-05). Desktop is single-tenant: it
/// recognizes exactly one local organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionScope {
    organization_id: String,
}

impl ConnectionScope {
    pub fn new(organization_id: impl Into<String>) -> Self {
        Self {
            organization_id: organization_id.into(),
        }
    }

    pub fn organization_id(&self) -> &str {
        &self.organization_id
    }
}

/// Storage failures of the connection repository.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionRepositoryError {
    /// The connection (or group) does not exist in this scope.
    #[error("connection not found: {0}")]
    NotFound(String),
    /// The backing store failed (IO, parse, crypto).
    #[error("connection storage unavailable: {0}")]
    BackendUnavailable(String),
}

/// Persistence port of the connection catalog (configs + groups).
///
/// Groups are whole-table read/write: a group is just a name with no identity
/// of its own, so per-item CRUD would have to invent one. `touch_last_connected_at`
/// is a separate method on purpose: a full `get → mutate → save` round-trip
/// would write the whole config back and clobber a concurrent front-end edit
/// (lost update); the narrow method lets the implementation change only the
/// timestamp under its lock.
#[async_trait]
pub trait ConnectionRepository: Send + Sync + 'static {
    /// All connection configs in user order. Out-of-scope → empty list.
    async fn list(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError>;

    /// One connection config including plaintext credentials. Out-of-scope → `None`.
    async fn get(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError>;

    /// Create or update one connection config. Out-of-scope →
    /// [`ConnectionRepositoryError::NotFound`].
    async fn save(
        &self,
        scope: &ConnectionScope,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError>;

    /// Delete one connection config (never touches live sessions).
    async fn delete(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError>;

    /// Reorder to the given id order; unmentioned ids keep relative order at
    /// the end; unknown ids are ignored.
    async fn reorder(
        &self,
        scope: &ConnectionScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError>;

    /// Record a "connected" timestamp without advancing the config revision.
    async fn touch_last_connected_at(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError>;

    /// Union of persisted custom groups and groups found on connections.
    async fn groups(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<String>, ConnectionRepositoryError>;

    /// Replace the persisted custom groups (whole table).
    async fn save_groups(
        &self,
        scope: &ConnectionScope,
        groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError>;
}
