//! 会话 → Schema Diff 端点解析（W8 刀3b-2）。
//!
//! 桌面对照 `datazen-desktop/src/services/migration_endpoint.rs`：
//! `runtime_session_context` 的服务端对应物——活句柄借出（刀2 的
//! `borrow_live_for_session` 同一条路径：目录可见性闸 → 单次 resolve →
//! `borrow_live_connection_by_id`）+ 连接目录的 `ConnectionConfig` 回读 +
//! 观察上下文覆盖（文件型驱动不覆盖 database；schema 恒覆盖）。
//!
//! 端点身份（`EndpointRef.service_key`）：共享层
//! `datazen::services::migration_endpoint::checked_identify`——物理位置
//! 摘要（host/port 驱动默认补齐 + database/schema + 位置性 options），
//! 无位置可指认时 fail-closed。

use std::sync::Arc;

use datazen::connection::domain::catalog::ConnectionRepository as _;
use datazen::execution::application::{LiveConnectionError, LivePurpose};
use datazen::shared::context::RequestContext;

use crate::execution::lease::RuntimeLeaseSource;
use crate::storage::connections::ServerConnections;

/// 会话解析失败（服务层错误；路由层按桶映射 wire 码）。
#[derive(Debug, thiserror::Error)]
pub enum EndpointError {
    /// 会话不可见/不存在（防枚举：统一文案，不区分两态）。
    #[error("The addressed resource does not exist or is not visible to you.")]
    NotVisible,
    /// 会话目录/租约/活连接端口故障。
    #[error("session endpoint unavailable: {0}")]
    Unavailable(String),
    /// 连接档案缺失（会话的归属连接已不在目录里）。
    #[error("Connection profile unavailable")]
    ProfileUnavailable,
}

/// 已解析的会话端点：`SchemaDiffEndpoint` 的原料 + 会话视图（部署闸要用
/// `connection_id` / 观察上下文）。
pub struct ResolvedSession {
    pub view: datazen::execution::SessionView,
    pub driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    pub handle: datazen_driver_api::ConnectionHandle,
    pub config: datazen_driver_api::ConnectionConfig,
    _lease: datazen::execution::application::LiveConnectionLease,
}

impl ResolvedSession {
    /// 折叠成共享层 api 的端点词汇。
    pub fn endpoint(&self) -> datazen::migration::schema_diff::api::SchemaDiffEndpoint {
        datazen::migration::schema_diff::api::SchemaDiffEndpoint {
            driver: Arc::clone(&self.driver),
            handle: self.handle.clone(),
            config: self.config.clone(),
        }
    }

    /// 会话归属连接 id（目录键）。
    pub fn connection_id(&self) -> &str {
        self.view.connection_id.as_str()
    }
}

/// 端点解析依赖（composition root 注入；与 tableData 域共享同一批实例）。
#[derive(Clone)]
pub struct SessionEndpoints {
    /// 会话目录（可见性闸；与 sessions 域/租约源同一实例）。
    pub directory: Arc<dyn datazen::execution::SessionDirectory>,
    /// runtime 租约源（单次 resolve 是对的构造——CM-63 cell 语义）。
    pub lease: Arc<dyn RuntimeLeaseSource>,
    /// 连接目录（`ConnectionConfig` 事实源；与执行后端同一实例）。
    pub connections: Arc<ServerConnections>,
}

impl SessionEndpoints {
    /// 解析一个会话为端点（桌面 `runtime_session_context` 同语义）。
    pub async fn resolve(
        &self,
        ctx: &RequestContext,
        db_session_id: &str,
        purpose: LivePurpose,
    ) -> Result<ResolvedSession, EndpointError> {
        // ① 可见性闸：越组织/未知 → 404 固定文案（sessions 域裁决）。
        let owner = self
            .directory
            .lookup_owner(datazen::shared::id::DbSessionId::new(
                db_session_id.to_owned(),
            ))
            .await
            .map_err(|source| EndpointError::Unavailable(source.to_string()))?;
        match owner {
            Some(owner) if owner.organization_id == ctx.organization_id => {}
            _ => return Err(EndpointError::NotVisible),
        }
        // ② 单次 resolve → ③ 借出。
        let runtime = self
            .lease
            .resolve()
            .await
            .map_err(EndpointError::Unavailable)?;
        let (view, lease) = runtime
            .borrow_live_connection_by_id(ctx, db_session_id, purpose)
            .await
            .map_err(|error| EndpointError::Unavailable(error.to_string()))?;
        let connection = lease.connection().map_err(live_error)?;
        let driver = Arc::clone(connection.driver());
        let handle = connection.handle().clone();
        // ④ 档案 config 回读 + 观察上下文覆盖（桌面同款：文件型驱动不
        // 覆盖 database——合成命名空间防塌缩；schema 恒覆盖）。
        let mut config = self
            .connections
            .get(
                &datazen::connection::domain::ConnectionScope::new(ctx.organization_id.as_str()),
                view.connection_id.as_str(),
            )
            .await
            .map_err(|error| EndpointError::Unavailable(error.to_string()))?
            .ok_or(EndpointError::ProfileUnavailable)?;
        if !driver.database_names_location() {
            config.database = view
                .observed_context
                .namespace
                .database
                .clone()
                .or(config.database);
        }
        config.schema = view
            .observed_context
            .namespace
            .schema
            .clone()
            .or(config.schema);
        Ok(ResolvedSession {
            view,
            driver,
            handle,
            config,
            _lease: lease,
        })
    }

    /// 端点身份（EndpointRef 的 connection_id + service_key；物理位置
    /// fail-closed——无位置键时拒绝，桌面 `session_identity` 同语义）。
    pub async fn identity_of(
        &self,
        ctx: &RequestContext,
        db_session_id: &str,
    ) -> Result<datazen::services::migration_endpoint::EndpointIdentity, EndpointError> {
        let session = self
            .resolve(ctx, db_session_id, LivePurpose::Inspection)
            .await?;
        let mut config = session.config.clone();
        config.id = session.connection_id().to_owned();
        datazen::services::migration_endpoint::checked_identify(&config, session.driver.as_ref())
            .map_err(|error| EndpointError::Unavailable(error.to_string()))
    }
}

fn live_error(error: LiveConnectionError) -> EndpointError {
    EndpointError::Unavailable(format!("live connection port operation failed: {error}"))
}

/// 能力快照哈希（桌面 `current_capability_hash` 同语义）：目标连接当前
/// 配置的可核验身份——databaseType/database/schema/readOnly/owner 的
/// 规范 JSON 摘要。部署闸用它检测「计划准备后连接被改」。
pub fn capability_hash(
    config: &datazen_driver_api::ConnectionConfig,
    owner_connection_id: &str,
) -> String {
    fnv_hex_of_json(&serde_json::json!({
        "databaseType": config.database_type,
        "database": config.database,
        "schema": config.schema,
        "readOnly": config.read_only,
        "owner": owner_connection_id,
    }))
}

fn fnv_hex_of_json(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    datazen::migration::schema_diff::job::fnv1a64_hex(&bytes)
}
