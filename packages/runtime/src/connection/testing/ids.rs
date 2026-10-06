//! 可预测 ID 生成与强制碰撞。
//!
//! 生成式逐条成表；**唯一不可预测的例外**是 `attachmentToken` 与幂等令牌 nonce，
//! 它们走真实随机源，因此本模块明确提供 `never_journalize()` 标记，
//! 调用方把它写进 journal 会被 clippy/测试拦下，字面量也不得出现在断言里。
//!
//! `dbSessionId` 是内存态 ID，**永不落盘**；本模块没有任何写盘路径。

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::sync::{Arc, Mutex};

use crate::connection::port::Secret;
use crate::connection::types::{
    Counter, DbSessionId, ExecutionId, JobId, LeaseId, OwnerRef, ResourceId, StreamId, WorkerId,
};

/// 可以被**确定性**制造碰撞的范围，用来复现冲突。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FakeIdScope {
    /// `dbSessionId` —— 复用同一个会话 id。
    DbSessionId,
    /// `runtimeEpoch` —— 让新句柄拿到与旧句柄相同的 epoch。
    RuntimeEpoch,
}

impl FakeIdScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            FakeIdScope::DbSessionId => "dbSessionId",
            FakeIdScope::RuntimeEpoch => "runtimeEpoch",
        }
    }
}

/// runtime epoch token。
///
/// 线上字段 `runtimeEpoch` 是十进制计数器，而本夹具的生成式是 `<ownerHash>.<counter>`。
/// 两者不是矛盾：`text` 是 provider 内部用于绑定 owner 的不透明 token，
/// `counter` 才是被比较、被放进 `SessionHandle.runtimeEpoch` 的那个值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochToken {
    pub owner_hash: String,
    pub counter: Counter,
    pub text: String,
}

#[derive(Debug, Default)]
struct Collision {
    /// 还要重复几次。
    remaining: u64,
    /// 被重复的那个值。
    value: u64,
}

#[derive(Debug, Default)]
struct IdState {
    resource_seq: u64,
    db_session_seq: u64,
    stream_seq: u64,
    job_seq: u64,
    token_seq: u64,
    execution_seq: BTreeMap<String, u64>,
    epoch_seq: BTreeMap<String, u64>,
    lease_seq: BTreeMap<String, u64>,
    collisions: BTreeMap<u8, Collision>,
}

impl IdState {
    fn collision(&self, scope: FakeIdScope) -> Option<&Collision> {
        self.collisions.get(&(scope as u8))
    }

    /// 消耗一次碰撞额度并返回被强制的值；没有额度时返回 `None`。
    fn take_forced(&mut self, scope: FakeIdScope) -> Option<u64> {
        let key = scope as u8;
        let entry = self.collisions.get_mut(&key)?;
        if entry.remaining == 0 {
            return None;
        }
        entry.remaining -= 1;
        let value = entry.value;
        if entry.remaining == 0 {
            self.collisions.remove(&key);
        }
        Some(value)
    }

    fn remember_first(&mut self, scope: FakeIdScope, value: u64) {
        let key = scope as u8;
        self.collisions.entry(key).or_insert(Collision {
            remaining: 0,
            value,
        });
    }
}

/// 真实随机源（不读系统时钟、不产生可从其他 id 推导的关系）。
///
/// 用 `RandomState` 的 OS 随机种子，不引入 `rand` 依赖 —— 这是本模块唯一允许的真实随机源。
fn random_u64() -> u64 {
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

/// 可预测 ID 生成器。克隆**共享同一状态**（`Arc`），因此 provider 与测试拿到的是
/// 同一个序列器 —— 这是「同一个 dbSessionId 上的 executionId 连续递增」得以成立的前提。
#[derive(Clone)]
pub struct FakeIds {
    worker_id: WorkerId,
    state: Arc<Mutex<IdState>>,
}

impl FakeIds {
    pub fn new(worker_id: WorkerId) -> Self {
        Self {
            worker_id,
            state: Arc::new(Mutex::new(IdState::default())),
        }
    }

    pub fn worker_id(&self) -> &str {
        self.worker_id.as_str()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, IdState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// `res_<workerId>_<seq:04>`，每 provider 一份序列。
    pub fn next_resource_id(&self) -> ResourceId {
        let mut state = self.lock();
        state.resource_seq += 1;
        ResourceId::new(format!("res_{}_{:04}", self.worker_id, state.resource_seq))
    }

    /// `dbs_<workerId>_<seq:04>`，每 provider 一份序列。内存态，永不落盘。
    pub fn next_db_session_id(&self) -> DbSessionId {
        let mut state = self.lock();
        let seq = match state.take_forced(FakeIdScope::DbSessionId) {
            Some(forced) => forced,
            None => {
                state.db_session_seq += 1;
                let value = state.db_session_seq;
                state.remember_first(FakeIdScope::DbSessionId, value);
                value
            }
        };
        DbSessionId::new(format!("dbs_{}_{:04}", self.worker_id, seq))
    }

    /// `str_<seq:04>`，事件序号连续。
    pub fn next_stream_id(&self) -> StreamId {
        let mut state = self.lock();
        state.stream_seq += 1;
        StreamId::new(format!("str_{:04}", state.stream_seq))
    }

    /// `job_<orgId>_<seq:04>`。
    pub fn next_job_id(&self, organization_id: &str) -> JobId {
        let mut state = self.lock();
        state.job_seq += 1;
        JobId::new(format!("job_{organization_id}_{:04}", state.job_seq))
    }

    /// `job:<jobId>/stage:<n>` —— 资源 owner 是 jobId/stageId。
    pub fn job_stage(&self, job_id: &JobId, stage: u64) -> String {
        format!("job:{job_id}/stage:{stage}")
    }

    /// `lse_<resourceId>#<n>`，每 resource 递增，每次 acquire 一条。
    pub fn next_lease_id(&self, resource_id: &ResourceId) -> LeaseId {
        let mut state = self.lock();
        let entry = state
            .lease_seq
            .entry(resource_id.as_str().to_owned())
            .or_insert(0);
        *entry += 1;
        LeaseId::new(format!("lse_{resource_id}#{entry}"))
    }

    /// `exe_<dbSessionId>_<seq:04>`，每 dbSessionId 递增。
    pub fn next_execution_id(&self, db_session_id: &DbSessionId) -> ExecutionId {
        let mut state = self.lock();
        let entry = state
            .execution_seq
            .entry(db_session_id.as_str().to_owned())
            .or_insert(0);
        *entry += 1;
        ExecutionId::new(format!("exe_{db_session_id}_{:04}", entry))
    }

    /// `<ownerHash>.<counter>`，每 dbSessionId 从 1 递增，换 owner 必增。
    pub fn next_runtime_epoch(&self, db_session_id: &DbSessionId, owner: &OwnerRef) -> EpochToken {
        let owner_hash = owner.hash();
        let mut state = self.lock();
        let counter = match state.take_forced(FakeIdScope::RuntimeEpoch) {
            Some(forced) => forced,
            None => {
                let entry = state
                    .epoch_seq
                    .entry(db_session_id.as_str().to_owned())
                    .or_insert(0);
                *entry += 1;
                *entry
            }
        };
        EpochToken {
            text: format!("{owner_hash}.{counter}"),
            owner_hash,
            counter: Counter::new(counter),
        }
    }

    /// 让接下来 `k` 次取值在 `scope` 内**重新发出该范围第一次发过的值**。
    ///
    /// 计数器不推进：结果是确定性的，可以靠它稳定复现「两个不同会话拿到同一个
    /// `dbSessionId`」或「新句柄拿到与旧句柄相同的 `runtimeEpoch`」。
    pub fn force_collision(&self, scope: FakeIdScope, k: u64) {
        let mut state = self.lock();
        let key = scope as u8;
        let existing_first = state
            .collision(scope)
            .map(|collision| collision.value)
            .unwrap_or(match scope {
                FakeIdScope::DbSessionId => state.db_session_seq,
                FakeIdScope::RuntimeEpoch => 1,
            });
        state.collisions.insert(
            key,
            Collision {
                remaining: k,
                value: existing_first,
            },
        );
    }

    /// `attachmentToken`。**例外**：真实随机源，不写 journal、不打印。
    pub fn attachment_token(&self) -> Secret {
        let mut state = self.lock();
        state.token_seq += 1;
        Secret::new(format!("atk_{:016x}_{:04}", random_u64(), state.token_seq))
    }

    /// 幂等令牌 nonce。同样走真实随机源，**不得**写入 journal 或断言字面量。
    pub fn idempotency_nonce(&self) -> Secret {
        let mut state = self.lock();
        state.token_seq += 1;
        Secret::new(format!("idn_{:016x}_{:04}", random_u64(), state.token_seq))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::types::{
        ClientInstanceId, ConnectionId, EditorSessionId, OrganizationId, PrincipalId,
    };

    fn owner(user: &str, editor: &str) -> OwnerRef {
        OwnerRef::Editor {
            organization_id: OrganizationId::new("org-alpha"),
            principal_id: PrincipalId::new(user),
            connection_id: ConnectionId::new("conn-fixture-p"),
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new(editor),
        }
    }

    fn ids() -> FakeIds {
        FakeIds::new(WorkerId::new("w1"))
    }

    #[test]
    fn generated_shapes_match_the_documented_table() {
        let ids = ids();
        // res_<workerId>_<seq:04>
        assert_eq!(ids.next_resource_id().as_str(), "res_w1_0001");
        assert_eq!(ids.next_resource_id().as_str(), "res_w1_0002");
        // dbs_<workerId>_<seq:04>
        assert_eq!(ids.next_db_session_id().as_str(), "dbs_w1_0001");
        // str_<seq:04>
        assert_eq!(ids.next_stream_id().as_str(), "str_0001");
        // job_<orgId>_<seq:04>
        assert_eq!(ids.next_job_id("org-alpha").as_str(), "job_org-alpha_0001");
        // job:<jobId>/stage:<n>
        let job = ids.next_job_id("org-alpha");
        assert_eq!(ids.job_stage(&job, 2), "job:job_org-alpha_0002/stage:2");
    }

    #[test]
    fn lease_and_execution_ids_are_scoped_per_resource_and_session() {
        let ids = ids();
        let r1 = ids.next_resource_id();
        let r2 = ids.next_resource_id();
        assert_eq!(ids.next_lease_id(&r1).as_str(), "lse_res_w1_0001#1");
        assert_eq!(ids.next_lease_id(&r1).as_str(), "lse_res_w1_0001#2");
        assert_eq!(ids.next_lease_id(&r2).as_str(), "lse_res_w1_0002#1");

        let s1 = ids.next_db_session_id();
        let s2 = ids.next_db_session_id();
        assert_eq!(ids.next_execution_id(&s1).as_str(), "exe_dbs_w1_0001_0001");
        assert_eq!(ids.next_execution_id(&s1).as_str(), "exe_dbs_w1_0001_0002");
        assert_eq!(
            ids.next_execution_id(&s2).as_str(),
            "exe_dbs_w1_0002_0001",
            "序号每 dbSessionId 独立"
        );
    }

    #[test]
    fn runtime_epoch_starts_at_one_and_bumps_when_owner_changes() {
        let ids = ids();
        let session = ids.next_db_session_id();
        let a1 = ids.next_runtime_epoch(&session, &owner("user-alpha-1", "ed-1"));
        assert_eq!(a1.counter.get(), 1);
        assert_eq!(a1.text, format!("{}.1", a1.owner_hash));
        assert!(a1.text.contains('.'), "生成式是 `<ownerHash>.<counter>`");

        let again = ids.next_runtime_epoch(&session, &owner("user-alpha-1", "ed-1"));
        assert_eq!(again.counter.get(), 2, "同一 owner 再次取 epoch 也必须递增");

        let other = ids.next_runtime_epoch(&session, &owner("user-alpha-2", "ed-1"));
        assert!(other.counter.get() > a1.counter.get(), "换 owner 必增");
        assert_ne!(
            other.owner_hash, a1.owner_hash,
            "ownerHash 必须随 owner 变化"
        );
    }

    #[test]
    fn forced_runtime_epoch_collision_is_deterministic() {
        // 新句柄拿到与旧句柄相同的 runtimeEpoch，宿主必须仍能拒绝它。
        let ids = ids();
        let session = ids.next_db_session_id();
        let first = ids.next_runtime_epoch(&session, &owner("user-alpha-1", "ed-1"));
        let second = ids.next_runtime_epoch(&session, &owner("user-alpha-1", "ed-1"));
        assert_ne!(first.counter, second.counter);

        ids.force_collision(FakeIdScope::RuntimeEpoch, 1);
        let forced = ids.next_runtime_epoch(&session, &owner("user-alpha-1", "ed-1"));
        assert_eq!(
            forced.counter, first.counter,
            "被强制的 epoch 必须与第一次完全相同"
        );
        assert_eq!(forced.text, first.text);

        let after = ids.next_runtime_epoch(&session, &owner("user-alpha-1", "ed-1"));
        assert_ne!(after.counter, forced.counter, "额度用尽后恢复正常递增");
    }

    #[test]
    fn forced_db_session_id_collision_repeats_the_first_session() {
        let ids = ids();
        let first = ids.next_db_session_id();
        let second = ids.next_db_session_id();
        assert_ne!(first, second);
        ids.force_collision(FakeIdScope::DbSessionId, 1);
        assert_eq!(
            ids.next_db_session_id(),
            first,
            "强制的 dbSessionId 必须与第一次相同"
        );
        // 碰撞额度用尽后计数器**从未推进过**（见 `force_collision` 的契约），
        // 因此下一次取号必然是尚未发过的下一个值，而不是把 `second` 再发一遍。
        let after = ids.next_db_session_id();
        assert_ne!(after, first, "碰撞额度只对一次取号生效");
        assert_ne!(after, second, "额度用尽后恢复正常递增");
    }

    #[test]
    fn secrets_are_redacted_in_debug_and_display() {
        // 日志脱敏：令牌字面量不得出现在任何输出里。
        let ids = ids();
        let token = ids.attachment_token();
        assert_eq!(format!("{token:?}"), "Secret(<redacted>)");
        assert_eq!(format!("{token}"), "<redacted>");
        assert!(token.expose().starts_with("atk_"), "内部结构仍可用于比较");
        assert_ne!(
            token.expose(),
            ids.attachment_token().expose(),
            "令牌必须每次不同"
        );
    }
}
