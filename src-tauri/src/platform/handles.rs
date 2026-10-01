//! owner 作用域的会话级句柄登记（概要 §6.3 的 `SessionRegistry` 登记语义）。
//!
//! ## 为什么另起一张表
//!
//! `AppState::session_transactions` 的类型是
//! `Arc<Mutex<HashMap<String, TransactionHandle>>>`：
//!
//! * 键只有 `dbSessionId`（一个裸 `String`），**类型上就表达不了 owner**；
//! * 值是 driver 层的 `TransactionHandle`，同样没有归属字段。
//!
//! 开发计划 :84 明令："**不得把新 owner 和权限语义伪映射为旧共享 session**"。
//! 于是本模块**不写**那张表，也**不**往那张表塞任何 owner 载荷；它另起一张
//! [`SessionHandleRegistry`]，键是 [`HandleKey`]（owner + `dbSessionId`），
//! 由调用方在 adapter 层完成归属校验后才允许登记。
//!
//! ## 迁移缺口（CM-73 "不改"）
//!
//! 旧表继续被既有调用点使用（`commands/connection.rs`、`commands/query.rs`、
//! `commands/data.rs`、`commands/mcp.rs` 及其测试）。本 track **不改**这些调用点：
//! 改它们等于在同一次发布里同时改语义与调用面，超出 :77「窄适配」的范围。
//! 缺口的准确形状是：**同一 `dbSessionId` 在旧表里只有一条记录，在新表里按 owner 有
//! N 条**。两者并存期间不得互相读取；接线时由后续 track 把调用点整体迁到新表，
//! 旧表随之删除。
//!
//! ## 选型说明
//!
//! * `OwnerRef`（`datazen_platform_api::context`）只派生 `Clone + PartialEq + Eq`，
//!   没有 `Hash` / `Ord`。给它补派生属于改 platform-api 契约，会和其他 track 的实现面
//!   撞车，因此这里用 `Vec` + 线性查。登记条目数 = 活跃会话上的活跃事务句柄，
//!   桌面形态是个位数，线性查不构成热点。若将来 `OwnerRef` 补上 `Hash`，换
//!   `HashMap<HandleKey, _>` 是一次纯局部替换。
//! * 锁用 `std::sync::Mutex`（本 crate 未引 `parking_lot`）。中毒后选择恢复并 `error!`
//!   记日志：登记表里是纯数据，每次加锁期间只做**一次** `push` 或 `retain`，
//!   不存在"改到一半"的可观测中间态，恢复不会破坏不变量。

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{DbSessionId, RuntimeEpoch};

/// owner + 会话 的复合键。**两维缺一不可**：只有 `dbSessionId` 就是旧共享 session。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleKey {
    owner: OwnerRef,
    db_session_id: DbSessionId,
}

impl HandleKey {
    /// 登记前调用方必须已经完成归属校验（见 `identity::OwnerIntent::admit`）。
    /// 本函数**不做**校验：它只做类型封装，避免调用方在登记路径上跳过校验。
    pub fn new(owner: OwnerRef, db_session_id: DbSessionId) -> Self {
        Self {
            owner,
            db_session_id,
        }
    }

    pub fn owner(&self) -> &OwnerRef {
        &self.owner
    }

    pub fn db_session_id(&self) -> &DbSessionId {
        &self.db_session_id
    }
}

/// 会话级句柄的登记条目。
///
/// `runtime_epoch` 是 §6.3 的会话世代：epoch 不一致的请求必须被拒，不能拿同一个
/// `dbSessionId` 里的旧句柄继续用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionScopedHandle {
    key: HandleKey,
    runtime_epoch: RuntimeEpoch,
    /// driver 层句柄的稳定指纹。driver-api 的 `TransactionHandle` 没有 `Debug`，
    /// 这里存字符串指纹而不是整个 driver 值，避免本模块反过来依赖 driver-api 的形状。
    payload: String,
}

impl SessionScopedHandle {
    pub fn new(key: HandleKey, runtime_epoch: RuntimeEpoch, payload: impl Into<String>) -> Self {
        Self {
            key,
            runtime_epoch,
            payload: payload.into(),
        }
    }

    pub fn key(&self) -> &HandleKey {
        &self.key
    }

    pub fn runtime_epoch(&self) -> &RuntimeEpoch {
        &self.runtime_epoch
    }

    pub fn payload(&self) -> &str {
        &self.payload
    }
}

/// owner 作用域的会话级句柄登记表。
#[derive(Debug, Default)]
pub struct SessionHandleRegistry {
    entries: std::sync::Mutex<Vec<SessionScopedHandle>>,
}

impl SessionHandleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一条会话级句柄。同一 `(owner, dbSessionId)` 重复登记 ⇒ CAS 冲突，
    /// 不覆盖：覆盖会让"旧 epoch 的句柄"被新登记顶掉，是静默的数据损坏。
    pub fn register(&self, entry: SessionScopedHandle) -> Result<(), PortError> {
        let mut guard = self.lock();
        if guard.iter().any(|e| e.key() == entry.key()) {
            return Err(PortError::cas_conflict(
                "session_handle",
                entry.key().db_session_id().as_str(),
            ));
        }
        guard.push(entry);
        Ok(())
    }

    /// 读取句柄并校验 epoch。epoch 不符 ⇒ CAS 冲突（调用方转 `RuntimeEpochMismatch`）。
    pub fn get(
        &self,
        key: &HandleKey,
        expected_epoch: &RuntimeEpoch,
    ) -> Result<SessionScopedHandle, PortError> {
        let guard = self.lock();
        let found = guard.iter().find(|e| e.key() == key).ok_or_else(|| {
            PortError::NotFound(format!("会话句柄未登记：session={}", key.db_session_id()))
        })?;
        if found.runtime_epoch() != expected_epoch {
            return Err(PortError::cas_conflict(
                "runtime_epoch",
                expected_epoch.as_str(),
            ));
        }
        Ok(found.clone())
    }

    /// 解除登记。返回是否真的移除过（供调用方对齐幂等语义）。
    pub fn remove(&self, key: &HandleKey) -> bool {
        let mut guard = self.lock();
        let before = guard.len();
        guard.retain(|e| e.key() != key);
        guard.len() != before
    }

    /// 解除某个会话在**所有** owner 下的登记（会话真正关闭时用）。
    pub fn remove_session(&self, db_session_id: &DbSessionId) -> usize {
        let mut guard = self.lock();
        let before = guard.len();
        guard.retain(|e| e.key().db_session_id() != db_session_id);
        before - guard.len()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// 中毒后恢复并记 `error!`。理由见模块头：登记表是纯数据，每次加锁期间只做
    /// 一次 `push` / `retain`，不存在可观测的部分更新。
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<SessionScopedHandle>> {
        self.entries.lock().unwrap_or_else(|poisoned| {
            tracing::error!("会话句柄登记表锁中毒，按纯数据语义恢复；条目内容未被部分更新");
            poisoned.into_inner()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::{BlockId, ClientInstanceId, EditorSessionId, JobId, StageId};

    fn editor(client: &str) -> OwnerRef {
        OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new(client),
            editor_session_id: EditorSessionId::new("tab-1"),
        }
    }

    fn job(job_id: &str) -> OwnerRef {
        OwnerRef::Job {
            job_id: JobId::new(job_id),
            stage_id: StageId::new("stage-1"),
        }
    }

    fn block(job_id: &str, block_id: &str) -> OwnerRef {
        OwnerRef::WorkflowBlock {
            job_id: JobId::new(job_id),
            block_id: BlockId::new(block_id),
        }
    }

    fn key(owner: OwnerRef) -> HandleKey {
        HandleKey::new(owner, DbSessionId::new("db-session-1"))
    }

    fn epoch(n: u64) -> RuntimeEpoch {
        RuntimeEpoch::new(n.to_string())
    }

    /// :84 条款一的**可执行**证据：同一个 `dbSessionId` 在不同 owner 下是**不同条目**。
    /// 旧共享表的 `HashMap<String, _>` 在这里只能存一条。
    #[test]
    fn same_db_session_id_under_different_owners_stays_separate() {
        let registry = SessionHandleRegistry::new();
        let alice = key(editor("client-alice"));
        let bob = key(editor("client-bob"));
        let job_a = key(job("job-a"));

        for (k, payload) in [
            (alice.clone(), "tx-alice"),
            (bob.clone(), "tx-bob"),
            (job_a.clone(), "tx-job-a"),
        ] {
            registry
                .register(SessionScopedHandle::new(k, epoch(1), payload))
                .expect("登记");
        }

        assert_eq!(registry.len(), 3);
        assert_eq!(
            registry.get(&alice, &epoch(1)).expect("alice").payload(),
            "tx-alice"
        );
        assert_eq!(
            registry.get(&bob, &epoch(1)).expect("bob").payload(),
            "tx-bob"
        );
        assert_eq!(
            registry.get(&job_a, &epoch(1)).expect("job-a").payload(),
            "tx-job-a"
        );
    }

    /// 与旧共享表的形状对照：一个 `dbSessionId` 键在旧表里只能命中最后写入的那一条。
    /// 这里显式复现该形状，证明"塞进共享槽位"确实会丢 owner。
    #[test]
    fn the_legacy_shared_shape_loses_owner_information() {
        // 旧表形状：HashMap<String, String>，键只有 dbSessionId。
        let mut legacy: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let session = "db-session-1";
        legacy.insert(session.to_owned(), "tx-alice".to_owned());
        legacy.insert(session.to_owned(), "tx-bob".to_owned());

        // bob 的登记把 alice 的顶掉了：alice 的句柄静默消失，且没有任何地方会报出来。
        assert_eq!(legacy.get(session).map(String::as_str), Some("tx-bob"));
        assert_eq!(legacy.len(), 1, "旧表只有一条，alice 的登记被静默吞掉");

        // 同样的两次登记在 owner 作用域表里两条都在。
        let registry = SessionHandleRegistry::new();
        for (owner, payload) in [
            (editor("client-alice"), "tx-alice"),
            (editor("client-bob"), "tx-bob"),
        ] {
            registry
                .register(SessionScopedHandle::new(key(owner), epoch(1), payload))
                .expect("登记");
        }
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn duplicate_registration_is_rejected_instead_of_overwriting() {
        let registry = SessionHandleRegistry::new();
        let k = key(editor("client-alice"));
        registry
            .register(SessionScopedHandle::new(k.clone(), epoch(1), "first"))
            .expect("首次登记");
        let err = registry
            .register(SessionScopedHandle::new(k.clone(), epoch(2), "second"))
            .expect_err("重复登记必须报错而不是覆盖");
        assert!(
            matches!(
                err,
                PortError::CasConflict {
                    entity: "session_handle",
                    ..
                }
            ),
            "期望 session_handle CAS 冲突，实际 {err:?}"
        );
        // 原登记保持不变：epoch 2 的那次登记没有生效。
        assert_eq!(
            registry.get(&k, &epoch(1)).expect("仍可读").payload(),
            "first"
        );
    }

    #[test]
    fn epoch_mismatch_is_refused_instead_of_serving_a_stale_handle() {
        let registry = SessionHandleRegistry::new();
        let k = key(editor("client-alice"));
        registry
            .register(SessionScopedHandle::new(k.clone(), epoch(1), "payload"))
            .expect("登记");
        let err = registry.get(&k, &epoch(2)).expect_err("epoch 不符必须被拒");
        assert!(
            matches!(
                err,
                PortError::CasConflict {
                    entity: "runtime_epoch",
                    ..
                }
            ),
            "实际 {err:?}"
        );
    }

    #[test]
    fn unknown_handle_is_not_found_not_a_default() {
        let registry = SessionHandleRegistry::new();
        let err = registry
            .get(&key(editor("nobody")), &epoch(1))
            .expect_err("未登记必须报错");
        assert!(matches!(err, PortError::NotFound(_)), "实际 {err:?}");
    }

    #[test]
    fn job_and_editor_owners_on_one_session_do_not_see_each_other() {
        let registry = SessionHandleRegistry::new();
        let editor_key = key(editor("client-alice"));
        let job_key = key(job("job-1"));
        let block_key = key(block("job-1", "block-1"));
        for (k, payload) in [
            (editor_key.clone(), "tx-editor"),
            (job_key.clone(), "tx-job"),
            (block_key.clone(), "tx-block"),
        ] {
            registry
                .register(SessionScopedHandle::new(k, epoch(7), payload))
                .expect("登记");
        }
        assert_eq!(registry.len(), 3);
        assert_eq!(
            registry
                .get(&block_key, &epoch(7))
                .expect("block")
                .payload(),
            "tx-block"
        );
        // 删除 editor 只影响 editor。
        assert!(registry.remove(&editor_key));
        assert_eq!(registry.len(), 2);
        assert!(registry.get(&job_key, &epoch(7)).is_ok());
    }

    #[test]
    fn closing_a_session_drops_every_owner_under_it() {
        let registry = SessionHandleRegistry::new();
        for owner in [editor("client-a"), editor("client-b"), job("job-1")] {
            registry
                .register(SessionScopedHandle::new(key(owner), epoch(1), "tx"))
                .expect("登记");
        }
        assert_eq!(
            registry.remove_session(&DbSessionId::new("db-session-1")),
            3
        );
        assert!(registry.is_empty());
        // 幂等：再清一次返回 0。
        assert_eq!(
            registry.remove_session(&DbSessionId::new("db-session-1")),
            0
        );
    }

    #[test]
    fn different_sessions_are_independent() {
        let registry = SessionHandleRegistry::new();
        let s1 = HandleKey::new(editor("client-a"), DbSessionId::new("s-1"));
        let s2 = HandleKey::new(editor("client-a"), DbSessionId::new("s-2"));
        for (k, payload) in [(s1.clone(), "tx-1"), (s2.clone(), "tx-2")] {
            registry
                .register(SessionScopedHandle::new(k, epoch(1), payload))
                .expect("登记");
        }
        assert_eq!(registry.remove(&s1), true);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get(&s2, &epoch(1)).expect("s2").payload(), "tx-2");
    }

    /// :84 条款一的**静态**证据：platform 目录的任何文件都不得触碰旧共享槽位。
    #[test]
    fn no_platform_file_writes_into_the_legacy_shared_slot() {
        for (file, source) in [
            ("identity.rs", include_str!("identity.rs")),
            ("error.rs", include_str!("error.rs")),
            ("handles.rs", include_str!("handles.rs")),
            ("bridge.rs", include_str!("bridge.rs")),
            ("adapter.rs", include_str!("adapter.rs")),
            ("mod.rs", include_str!("mod.rs")),
        ] {
            // 注释里必须允许提到这些名字（文档要解释"为什么不共用"），
            // 所以扫描的是去掉注释后的生产代码。
            let production = crate::platform::production_source(source);
            for forbidden in [
                "session_transactions",
                "query_executions",
                "ref_counts",
                "session_owner_map",
            ] {
                assert!(
                    !production.contains(forbidden),
                    "{file} 的生产代码不得引用旧共享槽位 {forbidden}"
                );
            }
        }
    }

    /// 反向对照：注释里确实提到这些名字，说明上一条不是靠"改名"通过的。
    #[test]
    fn the_legacy_slot_names_stay_documented_in_comments() {
        let production = crate::platform::production_source(include_str!("handles.rs"));
        let commented = include_str!("handles.rs");
        assert!(!production.contains("session_transactions"));
        assert!(
            commented.contains("session_transactions"),
            "迁移缺口必须留在文档里，不能连注释一起删掉"
        );
    }
}
