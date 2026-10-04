//! CM-54：幂等账本。
//!
//! §7.2 第 3 步「原子登记幂等请求与 executionId」，第 3 步同时是
//! 「返回回执**早于** SQL 跑完」的那一步。因此网关必须做到：
//!
//! - 同一个 `idempotencyKey` 重发 → **同一个 `executionId`**，不是新的一次执行；
//! - 那条持久化记录**只写一次**，重发不再写；
//! - 记录**读不出来**时，要求调用方核验，**绝不**换个新 key 自动再写一条。
//!
//! 最后一条是本模块存在的全部理由。如果读失败被当成「没有这条记录」，
//! 网关就会用同一个语义请求写第二条记录，而两次请求都会真的打到库上——
//! 这在写路径上就是**重复写入**，在重试风暴里还会放大。宁可失败，也不重复。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::connection::types::fnv1a64_hex;
use crate::connection::{CommandCall, Counter, DbSessionId, ExecutionId, SessionHandle};
use crate::gateway::provenance::ExecutionSource;

/// 幂等键的作用域：会话内唯一。
///
/// 之所以带 `db_session_id` 而不是全局唯一：幂等的语义边界是「这一次会话执行」，
/// 跨会话用同一个 key 撞车会让两个毫不相干的执行互相返回对方的 executionId。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdempotencyScope {
    db_session_id: DbSessionId,
    runtime_epoch: Counter,
    key: String,
}

impl IdempotencyScope {
    pub fn new(db_session_id: DbSessionId, runtime_epoch: Counter, key: impl Into<String>) -> Self {
        Self {
            db_session_id,
            runtime_epoch,
            key: key.into(),
        }
    }

    /// 从句柄 + 请求键构造。
    pub fn from_handle(handle: &SessionHandle, key: impl Into<String>) -> Self {
        Self::new(handle.db_session_id.clone(), handle.runtime_epoch, key)
    }

    pub fn db_session_id(&self) -> &DbSessionId {
        &self.db_session_id
    }

    pub fn runtime_epoch(&self) -> Counter {
        self.runtime_epoch
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// 键是否可用。空键会退化成「所有请求共享一个记录」。
    pub fn is_usable(&self) -> bool {
        !self.key.trim().is_empty()
    }
}

/// 请求指纹。用来区分「同一个 key 的同一次重发」与「同一个 key 的另一个请求」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestFingerprint(String);

impl RequestFingerprint {
    /// 由**语义**载荷算出：`command` + `input` + `expected_context_revision` + `source`。
    ///
    /// 刻意**不含** `dbSessionId`（已在作用域里）与 `idempotencyKey` 本身。
    ///
    /// 计入 `source`（CM-61）是有代价的：同一个 key 换来源发起，会被判成
    /// [`IdempotencyLookup::Conflict`] 而不是重发。这是对的——受理回执里的来源
    /// 是**这次执行**的来源，而执行是重发时并没有新建的那个。
    /// 若不计入来源，重发就能用同一个 key 把来源悄悄换成另一个。
    pub fn of(
        call: &CommandCall,
        expected_context_revision: Counter,
        source: &ExecutionSource,
    ) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(call.command.as_bytes());
        bytes.push(0x1f);
        bytes.extend_from_slice(
            serde_json::to_string(&call.input)
                .unwrap_or_else(|_| String::from("<unserializable>"))
                .as_bytes(),
        );
        bytes.push(0x1f);
        bytes.extend_from_slice(expected_context_revision.get().to_string().as_bytes());
        bytes.push(0x1f);
        bytes.extend_from_slice(
            serde_json::to_string(&source.to_persistable_json())
                .unwrap_or_else(|_| String::from("<unserializable>"))
                .as_bytes(),
        );
        Self(fnv1a64_hex(&bytes))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RequestFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 一条幂等记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyRecord {
    pub execution_id: ExecutionId,
    pub fingerprint: RequestFingerprint,
    /// 首次受理时刻（单调纳秒），重发时原样保留——它衡量的是「第一次被受理」，
    /// 不是「最近一次被重发」。
    pub first_accepted_at_nanos: u64,
}

/// 存储故障。**读失败与写失败是两回事**：读失败要求核验，写失败要求重试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreFailureKind {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyStoreError {
    pub kind: StoreFailureKind,
    pub message: String,
}

impl IdempotencyStoreError {
    pub fn read(message: impl Into<String>) -> Self {
        Self {
            kind: StoreFailureKind::Read,
            message: message.into(),
        }
    }

    pub fn write(message: impl Into<String>) -> Self {
        Self {
            kind: StoreFailureKind::Write,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for IdempotencyStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            StoreFailureKind::Read => write!(f, "idempotency read failed: {}", self.message),
            StoreFailureKind::Write => write!(f, "idempotency write failed: {}", self.message),
        }
    }
}

/// 幂等记录存储。网关**必须**通过它访问幂等记录，不允许绕开。
pub trait IdempotencyStore: Send + Sync + 'static {
    fn read(
        &self,
        scope: &IdempotencyScope,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError>;

    /// 必须**恰好一次**。同一 `scope` 重复写是调用方 bug，账本不做去重，
    /// 去重会把「重发」和「重复写」这两件事在存储层混成一个。
    fn write(
        &self,
        scope: &IdempotencyScope,
        record: IdempotencyRecord,
    ) -> Result<(), IdempotencyStoreError>;
}

/// 查重结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyLookup {
    /// 没有这条记录 → 可以新建。
    Miss,
    /// 已有记录且指纹一致 → 同一个语义请求的重发，返回原 `executionId`。
    Hit(IdempotencyRecord),
    /// 已有记录但指纹不同 → 同一个 key 被复用于另一个请求，拒绝。
    Conflict {
        existing: IdempotencyRecord,
        incoming: RequestFingerprint,
    },
    /// **读不出来** → 要求调用方核验。绝不降级成 `Miss`。
    Unreadable { message: String },
}

/// 进程内幂等存储。
///
/// 单进程网关（§2 的范围界定）用它就够了：记录的生命周期与网关进程一致。
/// 需要跨进程共享的部署形态应替换实现，而不是在本类型上叠锁。
#[derive(Debug, Default)]
pub struct InMemoryIdempotencyStore {
    inner: Mutex<HashMap<IdempotencyScope, IdempotencyRecord>>,
    write_count: Mutex<u64>,
}

impl InMemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// 累计写入次数。CM-54「只写一次」的判据。
    pub fn write_count(&self) -> u64 {
        match self.write_count.lock() {
            Ok(guard) => *guard,
            // 锁中毒意味着已经有别的线程在 panic；计数在测试里只会因此失真，
            // 这里如实返回 0 而不是 unwrap 把 panic 传播到网关路径上。
            Err(_) => 0,
        }
    }

    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(guard) => guard.len(),
            Err(_) => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 门禁用例专用：注入一条「写了一半但读不出来」的记录状态。
    pub fn force_insert(&self, scope: IdempotencyScope, record: IdempotencyRecord) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(scope, record);
        }
    }
}

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn read(
        &self,
        scope: &IdempotencyScope,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError> {
        match self.inner.lock() {
            Ok(guard) => Ok(guard.get(scope).cloned()),
            Err(_) => Err(IdempotencyStoreError::read("store lock poisoned")),
        }
    }

    fn write(
        &self,
        scope: &IdempotencyScope,
        record: IdempotencyRecord,
    ) -> Result<(), IdempotencyStoreError> {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(IdempotencyStoreError::write("store lock poisoned")),
        };
        guard.insert(scope.clone(), record);
        if let Ok(mut count) = self.write_count.lock() {
            *count += 1;
        }
        Ok(())
    }
}

/// 幂等账本。
///
/// 本身**不做锁**：check-then-write 的原子性由调用方（网关）在受理关键区里保证。
/// 把锁下沉到账本内部会诱使调用方以为「查过了」就等于「不会被并发插队」。
pub struct IdempotencyLedger {
    store: Arc<dyn IdempotencyStore>,
}

impl IdempotencyLedger {
    pub fn new(store: Arc<dyn IdempotencyStore>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<dyn IdempotencyStore> {
        &self.store
    }

    /// 查重。读失败如实返回 [`IdempotencyLookup::Unreadable`]。
    pub fn lookup(
        &self,
        scope: &IdempotencyScope,
        incoming: &RequestFingerprint,
    ) -> IdempotencyLookup {
        match self.store.read(scope) {
            Ok(None) => IdempotencyLookup::Miss,
            Ok(Some(existing)) => {
                if &existing.fingerprint == incoming {
                    IdempotencyLookup::Hit(existing)
                } else {
                    IdempotencyLookup::Conflict {
                        existing,
                        incoming: incoming.clone(),
                    }
                }
            }
            Err(err) => IdempotencyLookup::Unreadable {
                message: err.message,
            },
        }
    }

    /// 登记一条**新**记录。只在 [`IdempotencyLookup::Miss`] 之后调用。
    pub fn reserve(
        &self,
        scope: &IdempotencyScope,
        record: IdempotencyRecord,
    ) -> Result<(), IdempotencyStoreError> {
        self.store.write(scope, record)
    }
}

impl std::fmt::Debug for IdempotencyLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdempotencyLedger").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::provenance::{ExecutionSource, SourceKind};
    use serde_json::json;

    fn source() -> ExecutionSource {
        ExecutionSource::new(SourceKind::Editor, "edt_a", None, None)
    }

    fn scope() -> IdempotencyScope {
        IdempotencyScope::new(DbSessionId::new("dbse_a"), Counter::new(7), "key-1")
    }

    fn call(command: &str, value: serde_json::Value) -> CommandCall {
        CommandCall {
            command: command.to_owned(),
            input: value,
        }
    }

    fn record(fingerprint: RequestFingerprint, at: u64) -> IdempotencyRecord {
        IdempotencyRecord {
            execution_id: ExecutionId::new("exe_a_1"),
            fingerprint,
            first_accepted_at_nanos: at,
        }
    }

    #[test]
    fn an_empty_or_blank_key_is_not_usable() {
        assert!(!IdempotencyScope::new(DbSessionId::new("d"), Counter::ZERO, "").is_usable());
        assert!(
            !IdempotencyScope::new(DbSessionId::new("d"), Counter::ZERO, "   ").is_usable(),
            "空白键同样是退化键"
        );
        assert!(scope().is_usable());
    }

    #[test]
    fn the_same_key_in_two_sessions_does_not_collide() {
        let a = IdempotencyScope::new(DbSessionId::new("dbse_a"), Counter::new(1), "key");
        let b = IdempotencyScope::new(DbSessionId::new("dbse_b"), Counter::new(1), "key");
        assert_ne!(a, b);
    }

    #[test]
    fn the_same_key_in_two_epochs_does_not_collide() {
        let a = IdempotencyScope::new(DbSessionId::new("dbse_a"), Counter::new(1), "key");
        let b = IdempotencyScope::new(DbSessionId::new("dbse_a"), Counter::new(2), "key");
        assert_ne!(a, b, "资源被替换后 epoch 变化，旧记录不得命中");
    }

    #[test]
    fn fingerprint_separates_command_and_input_and_revision() {
        let base = RequestFingerprint::of(
            &call("query", json!({"sql": "select 1"})),
            Counter::new(3),
            &source(),
        );
        let same = RequestFingerprint::of(
            &call("query", json!({"sql": "select 1"})),
            Counter::new(3),
            &source(),
        );
        let other_input = RequestFingerprint::of(
            &call("query", json!({"sql": "select 2"})),
            Counter::new(3),
            &source(),
        );
        let other_command = RequestFingerprint::of(
            &call("admin", json!({"sql": "select 1"})),
            Counter::new(3),
            &source(),
        );
        let other_revision = RequestFingerprint::of(
            &call("query", json!({"sql": "select 1"})),
            Counter::new(4),
            &source(),
        );
        assert_eq!(base, same);
        assert_ne!(base, other_input);
        assert_ne!(base, other_command);
        assert_ne!(base, other_revision);
        assert!(!base.as_str().is_empty());
    }

    #[test]
    fn a_field_separator_prevents_concatenation_collisions() {
        // 不加分隔符的话 ("ab","c") 与 ("a","bc") 会撞成同一个字节串。
        let left = RequestFingerprint::of(&call("ab", json!("c")), Counter::ZERO, &source());
        let right = RequestFingerprint::of(&call("a", json!("bc")), Counter::ZERO, &source());
        assert_ne!(left, right);
    }

    #[test]
    fn miss_then_hit_returns_the_same_execution_id() {
        let store = InMemoryIdempotencyStore::shared();
        let ledger = IdempotencyLedger::new(store.clone());
        let fingerprint =
            RequestFingerprint::of(&call("query", json!(1)), Counter::new(2), &source());

        assert_eq!(
            ledger.lookup(&scope(), &fingerprint),
            IdempotencyLookup::Miss
        );
        ledger
            .reserve(&scope(), record(fingerprint.clone(), 100))
            .map_err(|e| e.to_string())
            .unwrap_or_else(|m| panic!("reserve 应当成功: {m}"));

        match ledger.lookup(&scope(), &fingerprint) {
            IdempotencyLookup::Hit(hit) => {
                assert_eq!(hit.execution_id, ExecutionId::new("exe_a_1"));
                assert_eq!(hit.first_accepted_at_nanos, 100, "重发不刷新首次受理时刻");
            }
            other => panic!("重发应当命中原记录，实际 {other:?}"),
        }
        assert_eq!(store.write_count(), 1, "CM-54：重发不得二次写入");
    }

    #[test]
    fn reusing_a_key_for_a_different_request_is_a_conflict_not_a_replay() {
        let store = InMemoryIdempotencyStore::shared();
        let ledger = IdempotencyLedger::new(store);
        let original = RequestFingerprint::of(&call("query", json!(1)), Counter::new(2), &source());
        ledger
            .reserve(&scope(), record(original, 1))
            .map_err(|e| e.to_string())
            .unwrap_or_else(|m| panic!("reserve 应当成功: {m}"));

        let impostor = RequestFingerprint::of(&call("query", json!(2)), Counter::new(2), &source());
        match ledger.lookup(&scope(), &impostor) {
            IdempotencyLookup::Conflict { existing, incoming } => {
                assert_eq!(existing.execution_id, ExecutionId::new("exe_a_1"));
                assert_eq!(incoming, impostor);
            }
            other => panic!("同 key 不同载荷必须是冲突，实际 {other:?}"),
        }
    }

    #[test]
    fn an_unreadable_record_is_never_downgraded_to_miss() {
        struct BrokenStore;
        impl IdempotencyStore for BrokenStore {
            fn read(
                &self,
                _scope: &IdempotencyScope,
            ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError> {
                Err(IdempotencyStoreError::read("connection reset"))
            }
            fn write(
                &self,
                _scope: &IdempotencyScope,
                _record: IdempotencyRecord,
            ) -> Result<(), IdempotencyStoreError> {
                Ok(())
            }
        }

        let ledger = IdempotencyLedger::new(Arc::new(BrokenStore));
        let fingerprint =
            RequestFingerprint::of(&call("query", json!(1)), Counter::ZERO, &source());
        match ledger.lookup(&scope(), &fingerprint) {
            IdempotencyLookup::Unreadable { message } => assert_eq!(message, "connection reset"),
            other => panic!("读失败必须要求核验，实际 {other:?}"),
        }
    }

    #[test]
    fn a_write_failure_surfaces_as_a_write_error() {
        struct FailingStore;
        impl IdempotencyStore for FailingStore {
            fn read(
                &self,
                _scope: &IdempotencyScope,
            ) -> Result<Option<IdempotencyRecord>, IdempotencyStoreError> {
                Ok(None)
            }
            fn write(
                &self,
                _scope: &IdempotencyScope,
                _record: IdempotencyRecord,
            ) -> Result<(), IdempotencyStoreError> {
                Err(IdempotencyStoreError::write("disk full"))
            }
        }

        let ledger = IdempotencyLedger::new(Arc::new(FailingStore));
        let fingerprint =
            RequestFingerprint::of(&call("query", json!(1)), Counter::ZERO, &source());
        let err = ledger
            .reserve(&scope(), record(fingerprint, 0))
            .err()
            .unwrap_or_else(|| panic!("写失败必须冒泡"));
        assert_eq!(err.kind, StoreFailureKind::Write);
        assert!(err.to_string().contains("disk full"));
    }

    #[test]
    fn store_len_tracks_distinct_scopes() {
        let fingerprint =
            RequestFingerprint::of(&call("query", json!(1)), Counter::ZERO, &source());
        let s1 = IdempotencyScope::new(DbSessionId::new("d1"), Counter::new(1), "k");
        let s2 = IdempotencyScope::new(DbSessionId::new("d2"), Counter::new(1), "k");

        let store = InMemoryIdempotencyStore::new();
        assert!(store.is_empty());
        store.force_insert(s1, record(fingerprint.clone(), 1));
        store.force_insert(s2, record(fingerprint, 2));

        assert_eq!(store.len(), 2);
        assert_eq!(
            store.write_count(),
            0,
            "force_insert 是夹具注入，不是 write，不该计入『只写一次』"
        );
    }
}
