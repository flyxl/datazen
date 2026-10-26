use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver};

// D4-b（2026-10-26）：schema 元数据端口迁入 datazen-modules::schema::application，
// 这里再导出保持 `services::ports::*` 既有路径不变。
pub use datazen_modules::schema::application::ports::{
    LogicalSchemaContextSource, PhysicalSchemaContextSource,
};
// D4-d：ArtifactChunkReader 迁入 table_editing（其消费者），这里再导出保旧路径。
use datazen_modules::settings::ExecutionSettings;
pub use datazen_modules::table_editing::ports::ArtifactChunkReader;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::artifact::BackupOutput;
use datazen_platform_api::dto::session::SessionHandle;
use datazen_platform_api::error::ApiError;
use datazen_platform_api::target::ExecutionTarget;
use std::sync::Arc;
#[async_trait::async_trait]
pub trait DatabaseToolHost: Send + Sync {
    async fn get_session_config(&self, session_id: &str) -> Result<ConnectionConfig, String>;
    async fn profile_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        self.get_session_config(id).await
    }
    async fn driver_for_connection(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String>;
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        let _ = (id, command, input);
        Err("DatabaseToolHost does not support target execution".into())
    }
    async fn execution_settings(&self) -> ExecutionSettings;
}
/// Why a live session could not be bound for a foreign-key navigation probe.
///
/// This is not a navigation rule failure: the `grid.fk.*` codes cover the
/// rules, while a missing session is an ordinary host condition.
#[derive(Debug, Clone)]
pub enum FkSessionError {
    /// The session (or the connection profile behind it) is gone.
    NotFound(String),
    /// The host could not lend a driver/handle for the session.
    Unavailable(String),
}

impl std::fmt::Display for FkSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message) | Self::Unavailable(message) => f.write_str(message),
        }
    }
}

/// One live database session bound for a foreign-key navigation probe.
///
/// The host lends the driver and the handle for the duration of the probe and
/// keeps its own session lease alive through `guard`; the shared layer treats
/// the guard as an opaque token and never inspects it. Dropping the binding
/// releases the lease, so the binding must outlive every driver call made
/// with it.
pub struct FkSessionBinding {
    /// The driver of the bound session.
    pub driver: Arc<dyn DatabaseDriver>,
    /// The connection handle of the bound session.
    pub handle: ConnectionHandle,
    /// Database the connection profile is bound to; the fallback when the
    /// request names none.
    pub database: Option<String>,
    /// Schema the connection profile is bound to; the fallback when the
    /// request names none.
    pub schema: Option<String>,
    /// Opaque host guard that keeps the session lease alive.
    pub guard: Box<dyn std::any::Any + Send + Sync>,
}

/// Host capability for resolving a foreign-key navigation target.
///
/// Foreign-key navigation needs one live session for three reads — the source
/// table's foreign keys, the target table's metadata, and one existence probe
/// against the target table — so the port hands out a single binding instead of
/// separate driver lookups that could straddle two sessions.
#[async_trait::async_trait]
pub trait ForeignKeyNavigationSource: Send + Sync {
    /// Bind the live session behind `db_session_id`.
    async fn bind_session(&self, db_session_id: &str) -> Result<FkSessionBinding, FkSessionError>;
}

#[async_trait::async_trait]
impl<T: DatabaseToolHost + ?Sized> DatabaseToolHost for Arc<T> {
    async fn get_session_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        (**self).get_session_config(id).await
    }
    async fn profile_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        (**self).profile_config(id).await
    }
    async fn driver_for_connection(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        (**self).driver_for_connection(id).await
    }
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        (**self).execute_target(id, command, input).await
    }
    async fn execution_settings(&self) -> ExecutionSettings {
        (**self).execution_settings().await
    }
}
/// Session-to-connection identity offered by the form that owns the sessions.
///
/// This is a *consumer-defined* port: the shared layer only knows the question
/// ("which connection owns this runtime session?"), and the host that owns the
/// Runtime facade answers it. Defining the trait here — instead of naming
/// `datazen_runtime::application::RuntimeConnectionUseCases` — keeps the shared
/// feature layer free of the runtime application facade, whose use-case surface
/// is already available through [`ConnectionUseCases`](datazen_platform_api::use_cases::ConnectionUseCases).
///
/// It is deliberately synchronous and session-id based: metadata reads resolve
/// identity before any await point and never hold a handle across one.
pub trait SessionConnectionLookup: Send + Sync {
    /// The connection that owns `session_id`, or `None` when the session is
    /// unknown, closed, or no longer attached to its directory entry.
    fn connection_id_for_session(&self, session_id: &str) -> Option<String>;
}

/// 备份输出绑定（消费者定义端口）。
///
/// 与 [`ArtifactChunkReader`] 同理：绑定的实现需要 session 归属判定、执行条目
/// 可见性与 sink 生命周期，这些都在 runtime 内部。返回的守卫是**能力对象**
/// ——drop 即撤销，实现方负责把 in-flight 的写等完并拒收迟到输出（共享层不认识
/// 实现的 Drop 逻辑，只依赖这个语义）。
#[async_trait::async_trait]
pub trait BackupOutputBinder: Send + Sync + 'static {
    async fn bind(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
        target: &ExecutionTarget,
        callback: BackupOutput,
    ) -> Result<Box<dyn BackupOutputGuard>, ApiError>;
}

/// 备份输出守卫：存活期间输出可用，drop 即撤销。
pub trait BackupOutputGuard: Send + Sync {}
