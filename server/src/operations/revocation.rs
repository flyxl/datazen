//! 撤销传播（WR-D6）：授权变更 → 同事务递增 `permission_versions` →
//! EventSink 发布撤销事件 → 会话校验助手。
//!
//! ## 事务性是验收核心
//!
//! [team-server §6.3](../../../../docs/architecture/platform/team-server-and-auth.md)
//! 的可执行形式：「实际授权变更与 `permission_version` 递增**同事务**提交」。
//! 本模块的组合器 [`RevocationService::propagate`] 把这个约束做成唯一入口：
//!
//! 1. 打开**一个**管理库事务；
//! 2. 执行调用方注入的授权变更（闭包拿到 `&Transaction`，任何一步
//!    `Err` → 事务整体回滚：版本不递增、授权不变化、会话不撤销）；
//! 3. 同事务调用 `crate::policy::version::bump_tx`（UPSERT + RETURNING，
//!    首建为 2、其后递增）；
//! 4. `commit` 成功后**才**发布撤销事件与审计——事件是提交后的副作用，
//!    发布失败不回滚（版本已递增，缓存分区键 `policyIsolationKey` 已失效），
//!    只记错误日志并如实上报 `published: false`。
//!
//! 「仅增加版本不构成撤权」（§6.3）反过来也成立：版本递增只出现在本
//! 组合器里（与授权变更绑定），不存在只 bump 不变更的入口。
//!
//! ## 事件词汇的已登记缺口
//!
//! `ConnectionEvent` 的 wire 词汇（Session/Execution/ResultChunk/StreamReset）
//! 没有「权限撤销」专事件。本模块定义本域端口 [`RevocationEventSink`]，
//! 并提供 [`EventSinkBridge`] 把撤销映射为最接近的**流复位信号**
//! `StreamResetRequired { reason: "permissionRevoked/<version>" }`——客户端
//! 收到后重订阅，重订阅路径经 L2/会话校验感知新版本。专事件词汇是
//! `modules` 侧的增量（报告中登记），本目录不私改共享词汇。
//!
//! ## 会话校验助手
//!
//! 登录会话行携带建立时的 `permission_version` 快照（`login_sessions`
//! v5 列）。[`RevocationService::check_session_permission`] 把「会话行的
//! 快照」与「组织当前版本」比对，中间件（L2 后置 / SSE 心跳复核）用它判
//! 失效：快照落后即当前会话携带的授权面已过期（具体处置——撤销或强制
//! 重认证——归调用方策略）。

use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::Transaction;

use datazen::execution::application::{ConnectionEvent, EventEnvelope, EventSequence};
use datazen::shared::audit::{AuditEvent, AuditOutcome, AuditPrincipalKind, AuditSink};
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::{
    ClientInstanceId, OrganizationId, PrincipalId, RequestId, RuntimeEpoch, StreamId, Timestamp,
};

use crate::storage::login_sessions::revoke_all_for_principal_tx;
use crate::storage::{AdminStore, LoginSessions, StorageError, ValidateFailure};

/// 撤销事件发布端口（本域）。`Ok(None)` = 该实现没有活的下游（如未配置
/// SSE），事务提交仍然有效。
#[async_trait]
pub trait RevocationEventSink: Send + Sync + 'static {
    async fn publish_permission_revoked(
        &self,
        organization_id: &OrganizationId,
        principal_id: Option<&str>,
        permission_version: i64,
    ) -> Result<Option<EventSequence>, PortError>;
}

/// 把撤销事件桥接到 [`datazen::execution::application::EventSink`]：
/// `StreamResetRequired { reason: "permissionRevoked/<version>" }`（见模块
/// 文档的词汇缺口登记）。`stream_id` / `runtime_epoch` 由装配层给定
/// （bootstrap 持有 SSE 注册表与当前 epoch）。
pub struct EventSinkBridge {
    sink: Arc<dyn datazen::execution::application::EventSink>,
    stream_id: StreamId,
    runtime_epoch: RuntimeEpoch,
}

impl EventSinkBridge {
    pub fn new(
        sink: Arc<dyn datazen::execution::application::EventSink>,
        stream_id: StreamId,
        runtime_epoch: RuntimeEpoch,
    ) -> Self {
        Self {
            sink,
            stream_id,
            runtime_epoch,
        }
    }
}

#[async_trait]
impl RevocationEventSink for EventSinkBridge {
    async fn publish_permission_revoked(
        &self,
        organization_id: &OrganizationId,
        principal_id: Option<&str>,
        permission_version: i64,
    ) -> Result<Option<EventSequence>, PortError> {
        // 服务端控制面的合成上下文：principal 缺席时用 service 形态词
        // （不冒充任何真实用户）；delegation 恒为 None。
        let ctx = RequestContext::new(
            organization_id.clone(),
            PrincipalId::new(principal_id.unwrap_or("service:permission-revocation")),
            None,
            ClientInstanceId::new("server-policy-plane"),
            RequestId::new(format!("perm-revocation-{}", uuid::Uuid::new_v4())),
            None,
        );
        let envelope = EventEnvelope::new(
            self.stream_id.clone(),
            EventSequence::new(0), // 下游 sink 负责流内单调 sequence
            self.runtime_epoch.clone(),
            ConnectionEvent::StreamResetRequired {
                reason: format!("permissionRevoked/{permission_version}"),
            },
        );
        let sequence = self.sink.publish(&ctx, envelope).await?;
        Ok(Some(sequence))
    }
}

/// 同事务提交的产出（事件与审计是其后续副作用，不算事务内容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationCommit {
    /// 本事务递增后的组织权限版本。
    pub permission_version: i64,
    /// 事务闭包统计的已撤销会话数（纯授权变更时为 0）。
    pub revoked_sessions: u64,
}

/// 发布 + 审计完成后的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationOutcome {
    pub commit: RevocationCommit,
    /// 事件是否已发布（`false` = 发布失败或无下游；提交不受影响）。
    pub published: bool,
}

/// 撤销传播服务。
pub struct RevocationService {
    store: Arc<AdminStore>,
    events: Arc<dyn RevocationEventSink>,
    audit: Arc<dyn AuditSink>,
}

/// 会话快照与组织当前版本的比对结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCheck {
    /// 快照仍等于当前版本：会话携带的授权面未过期。
    Current { permission_version: i64 },
    /// 快照落后：期间发生过授权变更（含本模块的撤销传播）。中间件据此判失效。
    Stale {
        session_version: i64,
        current_version: i64,
    },
}

impl RevocationService {
    pub fn new(
        store: Arc<AdminStore>,
        events: Arc<dyn RevocationEventSink>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            store,
            events,
            audit,
        }
    }

    /// **同事务**组合器（验收核心）：`mutation` 与 `permission_version`
    /// 递增要么都落、要么都不落（见模块文档的 4 步）。`mutation` 拿到
    /// 事务句柄与规范化时刻，返回其统计产物（如撤销会话数）。
    pub fn propagate_sync<F>(
        &self,
        organization_id: &str,
        mutation: F,
    ) -> Result<RevocationCommit, StorageError>
    where
        F: FnOnce(&Transaction<'_>, &str) -> Result<u64, StorageError>,
    {
        let now = super::now_string();
        let mut conn = self.store.lock_for_adapter()?;
        let tx = conn
            .transaction()
            .map_err(|source| StorageError::Query(format!("revocation tx open: {source}")))?;
        // 授权变更先行；Err 经 `?` 返回 → `tx` drop → 回滚（版本不递增）。
        let revoked_sessions = mutation(&tx, &now)?;
        let permission_version = crate::policy::version::bump_tx(&tx, organization_id, &now)?;
        tx.commit()
            .map_err(|source| StorageError::Query(format!("revocation tx commit: {source}")))?;
        Ok(RevocationCommit {
            permission_version,
            revoked_sessions,
        })
    }

    /// async 包装：同事务提交 + 提交后发布撤销事件 + 审计。
    ///
    /// SQLite 部分是单写者互斥锁后的亚毫秒操作（`policy` 的同步形态裁决
    /// 同款），事件发布才是真正的 await 点。
    pub async fn propagate(
        &self,
        ctx: &RequestContext,
        organization_id: &OrganizationId,
        mutation: impl FnOnce(&Transaction<'_>, &str) -> Result<u64, StorageError> + Send + 'static,
    ) -> Result<RevocationOutcome, ApiError> {
        let commit = self
            .propagate_sync(organization_id.as_str(), mutation)
            .map_err(|e| super::storage_error_to_api_error(&e))?;
        self.after_commit(ctx, organization_id, commit).await
    }

    /// 禁用成员 + 撤销其全部存活会话 + 递增版本（§4.5/§6.3 的完整组合），
    /// **同一个事务**。与 `PolicyService::set_membership_enabled(false)` 的
    /// SQL 同源；组合点不同：这里由撤销面直接驱动，不要求调用方另持
    /// `PolicyService` 的事务边界。
    pub async fn disable_principal(
        &self,
        ctx: &RequestContext,
        organization_id: &OrganizationId,
        principal_id: &str,
    ) -> Result<RevocationOutcome, ApiError> {
        let principal = principal_id.to_string();
        let organization = organization_id.as_str().to_string();
        self.propagate(ctx, organization_id, move |tx, now| {
            let changed = tx
                .execute(
                    "UPDATE memberships SET enabled = 0, updated_at = ?3
                     WHERE organization_id = ?1 AND principal_id = ?2",
                    rusqlite::params![organization, principal, now],
                )
                .map_err(|source| StorageError::Query(format!("memberships disable: {source}")))?;
            if changed == 0 {
                return Err(StorageError::Query(format!(
                    "membership ({organization}, {principal}) does not exist"
                )));
            }
            revoke_all_for_principal_tx(
                tx,
                &organization,
                &principal,
                crate::storage::login_sessions::RevokeReason::PermissionRevoked,
                now,
            )
            .map_err(|source| StorageError::Query(format!("login_sessions revoke: {source}")))
        })
        .await
    }

    /// 纯会话撤销传播：不触碰 memberships/acl，只撤销该主体全部存活会话
    /// 并递增版本（同事务）。适用于「角色不变、授权面整体失效」的场景。
    pub async fn revoke_sessions_for_principal(
        &self,
        ctx: &RequestContext,
        organization_id: &OrganizationId,
        principal_id: &str,
    ) -> Result<RevocationOutcome, ApiError> {
        let principal = principal_id.to_string();
        let organization = organization_id.as_str().to_string();
        self.propagate(ctx, organization_id, move |tx, now| {
            revoke_all_for_principal_tx(
                tx,
                &organization,
                &principal,
                crate::storage::login_sessions::RevokeReason::PermissionRevoked,
                now,
            )
            .map_err(|source| StorageError::Query(format!("login_sessions revoke: {source}")))
        })
        .await
    }

    /// 提交后的副作用：发布撤销事件（失败只记日志、`published=false`）+
    /// 落审计（同样不吞回执）。
    async fn after_commit(
        &self,
        ctx: &RequestContext,
        organization_id: &OrganizationId,
        commit: RevocationCommit,
    ) -> Result<RevocationOutcome, ApiError> {
        let published = match self
            .events
            .publish_permission_revoked(
                organization_id,
                Some(ctx.principal_id.as_str()),
                commit.permission_version,
            )
            .await
        {
            Ok(_) => true,
            Err(source) => {
                tracing::error!(error = %source, "revocation event publish failed after commit");
                false
            }
        };
        let event = AuditEvent {
            principal_id: Some(ctx.principal_id.clone()),
            request_id: Some(ctx.request_id.clone()),
            ..AuditEvent::new(
                organization_id.clone(),
                AuditPrincipalKind::User,
                "permission_revoked",
                AuditOutcome::Succeeded,
                Timestamp::new(super::now_string()),
            )
        };
        if let Err(source) = self.audit.record(event) {
            tracing::error!(error = %source, "revocation audit append failed");
        }
        Ok(RevocationOutcome {
            commit,
            published,
        })
    }

    /// 会话校验助手：登录会话的版本快照 vs 组织当前版本（中间件判失效）。
    /// 会话本身的过期/撤销检查走 [`LoginSessions::peek`]（只读，不刷新
    /// 空闲窗）。
    pub fn check_session_permission(
        &self,
        sessions: &LoginSessions,
        session_id: &str,
        now: Timestamp,
    ) -> Result<PermissionCheck, ApiError> {
        let row = sessions
            .peek(session_id, now)
            .map_err(|failure| match failure {
                ValidateFailure::Storage(source) => super::storage_error_to_api_error(&source),
                rejected @ ValidateFailure::Rejected(_) => {
                    // 脱敏：只说「会话无效」，不携带拒绝原因（§3.2 L2 同款）；
                    // 具体拒绝事实留在日志（此处已持有 `rejected`）。
                    tracing::debug!(rejected = %rejected, "session rejected during permission check");
                    ApiError::new(ApiErrorCode::Unauthenticated, "login session rejected")
                }
            })?;
        let current = self.current_permission_version(&row.organization_id)?;
        if current > row.permission_version {
            Ok(PermissionCheck::Stale {
                session_version: row.permission_version,
                current_version: current,
            })
        } else {
            Ok(PermissionCheck::Current {
                permission_version: row.permission_version,
            })
        }
    }

    /// 组织当前权限版本（无行 = 初始 1）。事务只是读的一致性边界。
    pub fn current_permission_version(&self, organization_id: &str) -> Result<i64, ApiError> {
        let mut conn = self
            .store
            .lock_for_adapter()
            .map_err(|e| super::storage_error_to_api_error(&e))?;
        let tx = conn.transaction().map_err(|source| {
            super::storage_error_to_api_error(&StorageError::Query(format!(
                "permission read tx: {source}"
            )))
        })?;
        let version = crate::policy::version::current_tx(&tx, organization_id)
            .map_err(|e| super::storage_error_to_api_error(&e))?;
        tx.commit().map_err(|source| {
            super::storage_error_to_api_error(&StorageError::Query(format!(
                "permission read commit: {source}"
            )))
        })?;
        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PolicyService;

    /// 同事务性（验收核心）：mutation 失败 → 版本不递增（回滚把 bump 也丢了）。
    #[test]
    fn a_failed_mutation_rolls_back_the_version_bump() {
        let store = Arc::new(AdminStore::open_in_memory().expect("store"));
        store.migrate().expect("migrate");
        let events = Arc::new(RecordingEvents::default());
        let service = RevocationService::new(store.clone(), events.clone(), Arc::new(NullAudit));
        let broken = |_: &Transaction<'_>, _: &str| -> Result<u64, StorageError> {
            Err(StorageError::Query("boom".into()))
        };
        let error = service
            .propagate_sync("org-a", broken)
            .expect_err("mutation must fail");
        assert!(error.to_string().contains("boom"));
        let policy = PolicyService::new(store, b"test-server-secret-32-bytes-min!!".to_vec())
            .expect("policy");
        assert_eq!(policy.permission_version("org-a").expect("version"), 1);
        assert!(
            events.calls.lock().expect("lock").is_empty(),
            "无提交则无事件"
        );
    }

    /// 成功提交：版本首建为 2，且跨组织互不干扰。
    #[test]
    fn a_committed_mutation_bumps_monotonically() {
        let store = Arc::new(AdminStore::open_in_memory().expect("store"));
        store.migrate().expect("migrate");
        let events = Arc::new(RecordingEvents::default());
        let service = RevocationService::new(store.clone(), events.clone(), Arc::new(NullAudit));
        let noop = |_: &Transaction<'_>, _: &str| Ok::<u64, StorageError>(0);
        assert_eq!(
            service.propagate_sync("org-a", noop).expect("commit").permission_version,
            2
        );
        let noop = |_: &Transaction<'_>, _: &str| Ok::<u64, StorageError>(0);
        assert_eq!(
            service.propagate_sync("org-a", noop).expect("commit").permission_version,
            3
        );
        assert_eq!(service.current_permission_version("org-a").expect("read"), 3);
        assert_eq!(service.current_permission_version("org-b").expect("read"), 1);
    }

    // ── 测试替身 ──

    #[derive(Default)]
    struct RecordingEvents {
        calls: std::sync::Mutex<Vec<(String, i64)>>,
    }

    #[async_trait]
    impl RevocationEventSink for RecordingEvents {
        async fn publish_permission_revoked(
            &self,
            organization_id: &OrganizationId,
            _principal_id: Option<&str>,
            permission_version: i64,
        ) -> Result<Option<EventSequence>, PortError> {
            self.calls
                .lock()
                .expect("lock")
                .push((organization_id.as_str().to_string(), permission_version));
            Ok(Some(EventSequence::new(1)))
        }
    }

    struct NullAudit;

    impl AuditSink for NullAudit {
        fn record(&self, _event: AuditEvent) -> Result<(), PortError> {
            Ok(())
        }
    }
}
