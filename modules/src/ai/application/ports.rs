//! AI 消费者端口（D6 item 4 收尾，2026-10-26）。
//!
//! `DatabaseToolHost` 与 `SessionConnectionLookup` 自 modules
//! `services::ports` 归位（唯一定义在此，旧路径改再导出垫片）：
//! 前者是 AI 数据库工具与 MCP 工具共用的宿主能力面（会话配置、驱动
//! 借出、目标执行与执行设置），后者是 schema 上下文实现所提的问题
//! （「哪个连接拥有此运行时会话」——由持有 Runtime 门面的形态回答）。
//! 端口定义在使用它的模块（§4.1 第 5 条），宿主提供实现。

use crate::settings::ExecutionSettings;
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use std::sync::Arc;

/// Host capability surface for AI database tools and MCP server tools.
///
/// The shared layer asks for session configs, drivers, target execution and
/// execution settings through this port; the host that owns the runtime
/// facade answers. Target execution is the single authorized entry point:
/// every database command an AI or MCP tool runs goes through it.
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
/// ("which connection owns this runtime session?"), and the host that owns
/// the Runtime facade answers it. Defining the trait here — instead of naming
/// a runtime application facade — keeps the shared feature layer free of the
/// runtime application surface.
///
/// It is deliberately synchronous and session-id based: metadata reads resolve
/// identity before any await point and never hold a handle across one.
pub trait SessionConnectionLookup: Send + Sync {
    /// The connection that owns `session_id`, or `None` when the session is
    /// unknown, closed, or no longer attached to its directory entry.
    fn connection_id_for_session(&self, session_id: &str) -> Option<String>;
}
