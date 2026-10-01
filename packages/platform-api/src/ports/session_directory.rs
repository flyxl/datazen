//! `SessionDirectory`：物理会话目录端口（**纯内存**）。
//!
//! 词汇表（§4.5）：`SessionOwner`、`ReplacementCommit`、`ReplacementOutcome`、
//! `InvalidationReason`、`CloseDisposition`。
//!
//! 四条不可协商的约束：
//!
//! * **禁止磁盘持久化、快照、append-only 日志、数据库表。** 条目只活在内存，
//!   进程退出即全丢；磁盘上只有 tombstone 墓碑（默认 24h，[连接 §13.1](connection-management.md)），
//!   用于 409 去重，**不是**目录内容。
//! * `invalidate` 之后**不得透明重建**；后续请求必须得到 `SessionLost`。
//! * `commit_replacement` 必须原子：prepared 候选**不可路由**，committed 之后旧条目改关闭路由。
//! * `touch` 只更新业务活动时间；心跳、状态读、订阅、attach 一律不得刷新。

use async_trait::async_trait;

use crate::context::OwnerRef;
use crate::dto::session::SessionHandle;
use crate::error::PortError;
use crate::id::{
    ConnectionId, DbSessionId, ExecutionId, JobId, PrincipalId, RuntimeEpoch, Timestamp,
};

/// 内存中的会话条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOwner {
    pub db_session_id: DbSessionId,
    pub organization_id: crate::id::OrganizationId,
    pub principal_id: PrincipalId,
    pub connection_id: ConnectionId,
    pub owner: OwnerRef,
    /// 拥有该会话的 runtime 进程。跨进程失效时用于定位谁要作废条目。
    pub worker_id: crate::id::WorkerId,
    pub runtime_epoch: RuntimeEpoch,
    /// 当前固定物理资源的 epoch。与 `SessionHandle.runtime_epoch` 同步。
    pub resource_epoch: u64,
    /// 仅在已接受的实际业务操作开始/终结时推进。
    pub last_business_activity: Timestamp,
}

impl SessionOwner {
    pub fn to_handle(&self) -> SessionHandle {
        SessionHandle::new(self.db_session_id.clone(), self.runtime_epoch.clone())
    }
}

/// 替换提交。old/new/operation 三者的状态变更必须原子生效。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementCommit {
    pub old: SessionHandle,
    pub new_owner: SessionOwner,
    pub operation: ReplacementOperation,
}

/// 替换动作。`Prepared` 候选**不可路由**，只有 `Committed` 才切换。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementOperation {
    /// 新会话已建连，等待原子切换。
    Prepared,
    /// 切换完成，旧条目改为关闭路由。
    Committed,
    /// 切换失败，新会话销毁，旧会话保持原状。
    RolledBack,
}

/// 替换结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementOutcome {
    pub operation: ReplacementOperation,
    pub replaced: SessionHandle,
}

/// 作废原因。作废后**禁止**透明重建。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidationReason {
    /// worker 失联。
    WorkerLost,
    /// 物理资源丢失（驱动报错、网络断开）。
    ResourceLost,
    /// 凭据或路由版本推进，旧资源作废。
    IdentityRotated,
    /// 策略变更要求作废（§4.5：policy 变更必须按受影响会话通知）。
    PolicyChanged,
}

/// 关闭处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDisposition {
    /// 正常关闭。
    Closed,
    /// 有活跃执行或未决事务，被强制关闭。
    ForceClosed,
}

#[async_trait]
pub trait SessionDirectory: Send + Sync + 'static {
    /// `db_session_id` 为随机 128 位以上；登记 owner/epoch 原子完成，碰撞重生成。
    async fn register(&self, owner: SessionOwner) -> Result<SessionHandle, PortError>;

    async fn lookup(&self, db_session_id: DbSessionId) -> Result<Option<SessionOwner>, PortError>;

    /// 只更新业务活动时间，且只在已接受的实际业务操作开始与终结时更新；
    /// 登录心跳、读取状态、订阅与 attach 均**不得**刷新
    /// （[连接 §6.4](connection-management.md)、[§9.2](connection-management.md)）。
    async fn touch(
        &self,
        handle: &SessionHandle,
        business_activity: Timestamp,
    ) -> Result<(), PortError>;

    /// old/new/operation 状态必须原子提交：prepared 候选不可路由，
    /// committed 后旧条目改关闭路由。
    async fn commit_replacement(
        &self,
        commit: ReplacementCommit,
    ) -> Result<ReplacementOutcome, PortError>;

    /// worker 失联/资源丢失时作废条目；调用方必须让后续请求得到 `SessionLost`，
    /// **不得透明重建**。
    async fn invalidate(
        &self,
        db_session_id: DbSessionId,
        reason: InvalidationReason,
    ) -> Result<(), PortError>;

    async fn release(
        &self,
        handle: &SessionHandle,
        disposition: CloseDisposition,
    ) -> Result<(), PortError>;

    /// 清扫超期条目，返回被清扫的 owner 供调用方关闭物理资源。
    async fn sweep_expired(&self, now: Timestamp) -> Result<Vec<SessionOwner>, PortError>;
}

/// 归属校验辅助：CM §4 要求后端确认 editor 属于当前 client、job/block 属于已授权 Job，
/// **不能允许用户声称任意 job owner**。这里给出唯一的判定入口。
pub fn owner_matches_client(
    owner: &OwnerRef,
    client_instance_id: &crate::id::ClientInstanceId,
) -> bool {
    match owner {
        OwnerRef::Editor {
            client_instance_id: owner_client,
            ..
        }
        | OwnerRef::ClientSession {
            client_instance_id: owner_client,
            ..
        } => owner_client == client_instance_id,
        // job / workflowBlock 的归属以服务端记录为准，前端声称不算数。
        OwnerRef::Job { .. } | OwnerRef::WorkflowBlock { .. } => false,
    }
}

/// 归属校验辅助：job 阶段的 owner 必须指向该 job。
pub fn owner_matches_job(owner: &OwnerRef, job_id: &JobId) -> bool {
    match owner {
        OwnerRef::Job {
            job_id: owner_job, ..
        }
        | OwnerRef::WorkflowBlock {
            job_id: owner_job, ..
        } => owner_job == job_id,
        _ => false,
    }
}

/// 便捷：会话是否已被某个执行占用（运行期判定的读侧事实，非端口契约）。
pub fn has_active_execution(active: Option<&ExecutionId>) -> bool {
    active.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ClientInstanceId, OrganizationId};

    fn owner(client: &str) -> SessionOwner {
        SessionOwner {
            db_session_id: DbSessionId::new("sess-1"),
            organization_id: OrganizationId::new("org-1"),
            principal_id: PrincipalId::new("user-1"),
            connection_id: ConnectionId::new("conn-1"),
            owner: OwnerRef::Editor {
                client_instance_id: ClientInstanceId::new(client),
                editor_session_id: crate::id::EditorSessionId::new("ed-1"),
            },
            worker_id: crate::id::WorkerId::new("worker-1"),
            runtime_epoch: RuntimeEpoch::new("epoch-1"),
            resource_epoch: 1,
            last_business_activity: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    #[test]
    fn editor_owner_must_belong_to_the_claiming_client() {
        let claim = ClientInstanceId::new("client-1");
        assert!(owner_matches_client(&owner("client-1").owner, &claim));
        assert!(!owner_matches_client(&owner("client-2").owner, &claim));
    }

    #[test]
    fn job_owner_is_never_satisfied_by_a_client_claim() {
        let job_owner = OwnerRef::Job {
            job_id: JobId::new("job-1"),
            stage_id: crate::id::StageId::new("stage-1"),
        };
        let block_owner = OwnerRef::WorkflowBlock {
            job_id: JobId::new("job-1"),
            block_id: crate::id::BlockId::new("b1"),
        };
        let client = ClientInstanceId::new("client-1");
        assert!(!owner_matches_client(&job_owner, &client));
        assert!(!owner_matches_client(&block_owner, &client));
    }

    #[test]
    fn job_owner_matches_only_its_own_job() {
        let job_owner = OwnerRef::Job {
            job_id: JobId::new("job-1"),
            stage_id: crate::id::StageId::new("s"),
        };
        let block_owner = OwnerRef::WorkflowBlock {
            job_id: JobId::new("job-1"),
            block_id: crate::id::BlockId::new("b1"),
        };
        assert!(owner_matches_job(&job_owner, &JobId::new("job-1")));
        assert!(owner_matches_job(&block_owner, &JobId::new("job-1")));
        assert!(!owner_matches_job(&job_owner, &JobId::new("job-2")));
        assert!(!owner_matches_job(
            &owner("client-1").owner,
            &JobId::new("job-1")
        ));
    }

    #[test]
    fn owner_projects_a_handle_with_the_same_epoch() {
        let owner = owner("client-1");
        let handle = owner.to_handle();
        assert_eq!(handle.db_session_id, owner.db_session_id);
        assert_eq!(handle.runtime_epoch, owner.runtime_epoch);
    }

    #[test]
    fn prepared_and_committed_are_distinct_operations() {
        assert_ne!(
            ReplacementOperation::Prepared,
            ReplacementOperation::Committed
        );
        assert_ne!(
            ReplacementOperation::Committed,
            ReplacementOperation::RolledBack
        );
        // 只有 Prepared 之后才允许 Committed；两者不可互相取代。
        let outcome = ReplacementOutcome {
            operation: ReplacementOperation::Committed,
            replaced: SessionHandle::new(DbSessionId::new("sess-1"), RuntimeEpoch::new("epoch-1")),
        };
        assert_eq!(outcome.operation, ReplacementOperation::Committed);
        assert_eq!(outcome.replaced.db_session_id, DbSessionId::new("sess-1"));
    }

    #[test]
    fn invalidation_reasons_are_distinguishable_so_they_are_logged_separately() {
        let reasons = [
            InvalidationReason::WorkerLost,
            InvalidationReason::ResourceLost,
            InvalidationReason::IdentityRotated,
            InvalidationReason::PolicyChanged,
        ];
        let mut unique = reasons.to_vec();
        unique.sort_by_key(|r| format!("{r:?}"));
        unique.dedup();
        assert_eq!(unique.len(), reasons.len());
    }

    #[test]
    fn directory_source_has_no_disk_persistence_surface() {
        // 目录必须是纯内存：签名里不得出现路径、文件、快照或数据库句柄。
        // 先切掉 `#[cfg(test)]` 之后的区域，否则本测试的标记字符串会命中自己。
        let source = include_str!("session_directory.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        for forbidden in [
            "std::fs",
            "File::",
            "snapshot(",
            "append_only",
            "Sqlite",
            "sqlx",
        ] {
            assert!(
                !source.contains(forbidden),
                "SessionDirectory 不得引入磁盘持久化面 {forbidden}"
            );
        }
    }

    #[test]
    fn session_owner_keys_on_db_session_id_never_on_connection_id_alone() {
        let owner = owner("client-1");
        assert_eq!(owner.db_session_id, DbSessionId::new("sess-1"));
        // 同一连接的两个会话是两个条目：连接 id 不是主键。
        let other = SessionOwner {
            db_session_id: DbSessionId::new("sess-2"),
            ..owner.clone()
        };
        assert_ne!(owner, other);
    }

    #[test]
    fn active_execution_is_readable_without_a_port_call() {
        assert!(has_active_execution(Some(&ExecutionId::new("exec-1"))));
        assert!(!has_active_execution(None));
    }
}
