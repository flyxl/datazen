use datazen_driver_api::{ConnectionHandle, DatabaseDriver};

// D4-b（2026-10-26）：schema 元数据端口迁入 datazen-modules::schema::application，
// 这里再导出保持 `services::ports::*` 既有路径不变。
pub use datazen_modules::schema::application::ports::{
    LogicalSchemaContextSource, PhysicalSchemaContextSource,
};
// D4-d：ArtifactChunkReader 迁入 table_editing（其消费者），这里再导出保旧路径。
pub use datazen_modules::table_editing::ports::ArtifactChunkReader;
// D6 item 4 收尾：AI 消费者端口（DatabaseToolHost / SessionConnectionLookup）
// 迁入 modules::ai::application::ports（其消费者），这里再导出保旧路径；
// Arc 前转实现随端口唯一定义在模块内（孤儿规则）。
pub use datazen_modules::ai::application::ports::{DatabaseToolHost, SessionConnectionLookup};
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::artifact::BackupOutput;
use datazen_platform_api::dto::session::SessionHandle;
use datazen_platform_api::error::ApiError;
use datazen_platform_api::target::ExecutionTarget;
use std::sync::Arc;
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
