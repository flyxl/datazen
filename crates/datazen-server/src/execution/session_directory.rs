//! `SessionDirectory` 的服务端适配器：**单进程内存**实现（WR-D4，v1 形态）。
//!
//! [web-server-host §10.1/§10.3](../../../../../docs/architecture/platform/web-server-host.md)
//! 的「单进程内存」首版：与 `MemoryEventSink` 同一条登记口径——团队服务首版
//! 是单实例（API + admin + worker 同进程），本实现即 v1 形态的物理会话目录，
//! 多实例共享目录归 P9。
//!
//! 端口域文档（`crates/datazen/src/execution/domain/session_directory.rs`）的四条
//! 不可协商约束逐条落地：
//!
//! 1. **纯内存**：条目只活在 `HashMap`，进程退出即全丢；没有任何磁盘、
//!    快照、append-only 日志或数据库表面（源码里连 AdminStore 都不出现）。
//!    409 去重的墓碑是**宿主**的对象（幂等回执存储），不是目录内容。
//! 2. **invalidate 之后不透明重建**：条目被移除，后续 `lookup` 返回
//!    `None`，用例层把它翻成 `SessionLost`。
//! 3. **替换原子提交**：`Prepared` 候选放在独立的 prepared 表里
//!    （**不可路由**——`lookup` 对它返回 `None`），`Committed` 在同一把锁内
//!    「promote 候选 + 移除旧条目」一次完成；`RolledBack` 只丢候选。
//! 4. **touch 只改业务活动时间**：实现里没有任何「区分心跳/状态读」的
//!    逻辑——那是调用方纪律；本实现只保证被调用时**只**更新
//!    `last_business_activity` 一个字段。
//!
//! ## 形态裁决（登记）
//!
//! * `register` 碰撞重生成：端口要求「db_session_id 随机 128 位以上，碰撞
//!   重生成」。db_session_id 在入参里，目录对**活条目/候选**的 id 冲突
//!   就地铸造新的随机 id（uuid v4）并返回新句柄。
//! * `sweep_expired` 的过期判据是 `last_business_activity + idle_ttl`：
//!   TTL 是构造参数（[`ServerSessionDirectory::with_idle_ttl`]；默认值是
//!   §9.2「可调初始值」的首版配置起点，不是协议常量）。
//! * `release` 幂等：对不存在的句柄返回 `Ok`（关闭重试与 lost 竞态下
//!   重复 close 不得报错）；`disposition` 在 v1 只被接受（内存目录没有
//!   可挂的处置账本，物理关闭上报由宿主审计承接）。
//! * `invalidate` 的 `reason` 同样只被接受：单进程内没有可寻址的 worker
//!   拓扑，原因分类留给调用方日志/审计。
//! * 并发模型沿用 `MemoryEventSink` 先例：`std::sync` 锁 + 短临界区，
//!   async fn 内不持锁跨 await（本实现的临界区只有查表/换表/克隆）。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use datazen::execution::domain::session_directory::{
    CloseDisposition, InvalidationReason, ReplacementCommit, ReplacementOperation,
    ReplacementOutcome, SessionDirectory, SessionOwner,
};
use datazen::execution::domain::SessionHandle;
use datazen::shared::error::PortError;
use datazen::shared::id::{DbSessionId, Timestamp};

/// `sweep_expired` 的默认空闲 TTL（§9.2 首版配置起点；部署覆盖用
/// [`ServerSessionDirectory::with_idle_ttl`]）。
pub const DEFAULT_IDLE_TTL_SECS: u64 = 3600;

/// 目录状态：活条目与 prepared 候选分表——「候选不可路由」由分表结构性保证。
#[derive(Default)]
struct DirectoryState {
    live: HashMap<DbSessionId, SessionOwner>,
    prepared: HashMap<DbSessionId, SessionOwner>,
}

/// `SessionDirectory` 的单进程内存实现。线程安全；可跨 tokio 任务共享（`Arc`）。
pub struct ServerSessionDirectory {
    state: Mutex<DirectoryState>,
    idle_ttl: Duration,
}

impl Default for ServerSessionDirectory {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerSessionDirectory {
    /// 默认 TTL（见 [`DEFAULT_IDLE_TTL_SECS`]）。
    pub fn new() -> Self {
        Self::with_idle_ttl(Duration::from_secs(DEFAULT_IDLE_TTL_SECS))
    }

    /// 以部署配置的 TTL 构建（`sweep_expired` 的过期判据）。
    pub fn with_idle_ttl(idle_ttl: Duration) -> Self {
        Self {
            state: Mutex::new(DirectoryState::default()),
            idle_ttl,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DirectoryState> {
        // 单条目操作不会留下半更新状态；毒锁恢复与 MemoryEventSink 同款。
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl SessionDirectory for ServerSessionDirectory {
    async fn register(&self, mut owner: SessionOwner) -> Result<SessionHandle, PortError> {
        let mut state = self.lock();
        if state.live.contains_key(&owner.db_session_id)
            || state.prepared.contains_key(&owner.db_session_id)
        {
            // 碰撞重生成：调用方给的 id 已被占用时，目录铸造新的随机 id
            //（uuid v4，128 位），登记照常原子完成。
            owner.db_session_id = DbSessionId::new(uuid::Uuid::new_v4().to_string());
        }
        let handle = owner.to_handle();
        state.live.insert(owner.db_session_id.clone(), owner);
        Ok(handle)
    }

    async fn lookup(&self, db_session_id: DbSessionId) -> Result<Option<SessionOwner>, PortError> {
        let state = self.lock();
        // prepared 候选不可路由：只查活表。
        Ok(state.live.get(&db_session_id).cloned())
    }

    async fn lookup_owner(
        &self,
        db_session_id: DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError> {
        // 本实现只维护活表（closed 即移除）；墓碑闸在单进程 v1 由用例的
        // 会话账承载，这里如实回答活表事实。
        let state = self.lock();
        Ok(state.live.get(&db_session_id).cloned())
    }

    async fn touch(
        &self,
        handle: &SessionHandle,
        business_activity: Timestamp,
    ) -> Result<(), PortError> {
        let mut state = self.lock();
        let owner = state
            .live
            .get_mut(&handle.db_session_id)
            .filter(|owner| owner.runtime_epoch == handle.runtime_epoch)
            .ok_or_else(|| PortError::NotFound("session".into()))?;
        // 只更新业务活动时间：其余字段一律不动。
        owner.last_business_activity = business_activity;
        Ok(())
    }

    async fn commit_replacement(
        &self,
        commit: ReplacementCommit,
    ) -> Result<ReplacementOutcome, PortError> {
        let mut state = self.lock();
        match commit.operation {
            ReplacementOperation::Prepared => {
                let mut candidate = commit.new_owner;
                if state.live.contains_key(&candidate.db_session_id)
                    || state.prepared.contains_key(&candidate.db_session_id)
                {
                    candidate.db_session_id = DbSessionId::new(uuid::Uuid::new_v4().to_string());
                }
                // 记录候选落位后的 id：候选 id 若被重生成，调用方拿到的
                // outcome.replaced 仍是旧句柄（Prepared 的 replaced 语义是
                // 「被替换者」，候选的新 id 由它自己的 SessionOwner 带回）。
                state
                    .prepared
                    .insert(candidate.db_session_id.clone(), candidate);
                Ok(ReplacementOutcome {
                    operation: ReplacementOperation::Prepared,
                    replaced: commit.old,
                })
            }
            ReplacementOperation::Committed => {
                // 原子提交：promote 候选 + 移除旧条目在同一临界区完成。
                let candidate = state
                    .prepared
                    .remove(&commit.new_owner.db_session_id)
                    .ok_or_else(|| PortError::NotFound("prepared candidate".into()))?;
                let replaced = state
                    .live
                    .remove(&commit.old.db_session_id)
                    .map(|old| old.to_handle())
                    .unwrap_or_else(|| commit.old.clone());
                state
                    .live
                    .insert(candidate.db_session_id.clone(), candidate);
                Ok(ReplacementOutcome {
                    operation: ReplacementOperation::Committed,
                    replaced,
                })
            }
            ReplacementOperation::RolledBack => {
                // 切换失败：新会话销毁（丢候选），旧会话保持原状。
                state.prepared.remove(&commit.new_owner.db_session_id);
                Ok(ReplacementOutcome {
                    operation: ReplacementOperation::RolledBack,
                    replaced: commit.old,
                })
            }
        }
    }

    async fn invalidate(
        &self,
        db_session_id: DbSessionId,
        reason: InvalidationReason,
    ) -> Result<(), PortError> {
        // v1：reason 只被接受（调用方日志/审计消费）；目录的动作是移除条目，
        // 后续请求因此必须得到 SessionLost（透明重建被结构性排除）。
        let _ = reason;
        let mut state = self.lock();
        state
            .live
            .remove(&db_session_id)
            .or_else(|| state.prepared.remove(&db_session_id))
            .map(|_| ())
            .ok_or_else(|| PortError::NotFound("session".into()))
    }

    async fn release(
        &self,
        handle: &SessionHandle,
        disposition: CloseDisposition,
    ) -> Result<(), PortError> {
        // v1：disposition 只被接受；释放幂等（不存在的句柄 = 已释放）。
        let _ = disposition;
        let mut state = self.lock();
        if let Some(owner) = state.live.get(&handle.db_session_id) {
            if owner.runtime_epoch != handle.runtime_epoch {
                // 旧纪元的句柄不得释放新纪元的条目（INV-07/08 同源语义）。
                return Ok(());
            }
        }
        state.live.remove(&handle.db_session_id);
        state.prepared.remove(&handle.db_session_id);
        Ok(())
    }

    async fn sweep_expired(&self, now: Timestamp) -> Result<Vec<SessionOwner>, PortError> {
        let now = parse_ts(&now);
        let mut state = self.lock();
        let ttl = self.idle_ttl;
        let mut swept = Vec::new();
        let is_expired = |owner: &SessionOwner| match (parse_ts(&owner.last_business_activity), now)
        {
            (Some(activity), Some(now)) => {
                now - activity > chrono::Duration::from_std(ttl).unwrap_or_default()
            }
            // 形状不合规的时间戳按「不可判定」保留（保守不误扫）。
            _ => false,
        };
        let live_keys: Vec<DbSessionId> = state
            .live
            .iter()
            .filter(|(_, owner)| is_expired(owner))
            .map(|(id, _)| id.clone())
            .collect();
        for id in live_keys {
            if let Some(owner) = state.live.remove(&id) {
                swept.push(owner);
            }
        }
        let prepared_keys: Vec<DbSessionId> = state
            .prepared
            .iter()
            .filter(|(_, owner)| is_expired(owner))
            .map(|(id, _)| id.clone())
            .collect();
        for id in prepared_keys {
            if let Some(owner) = state.prepared.remove(&id) {
                swept.push(owner);
            }
        }
        Ok(swept)
    }
}

fn parse_ts(value: &Timestamp) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value.as_str())
        .ok()
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
}

#[cfg(test)]
#[path = "session_directory_tests.rs"]
mod tests;
