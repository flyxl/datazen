//! 连续旅程测试的共享替身。
//!
//! 三件套：
//!
//! * [`TestClock`]：把 `connection::testing::FakeClock` 适配成生产接口 [`MonotonicSource`]。
//!   旅程里**从不** `sleep`，时间只由这里显式推进。
//! * [`RecordingTransport`]：实现本模块自己定义的 [`PhysicalTransport`]，逐条记账
//!   `Open/Reset/Close/Cancel`，并允许注入单点故障。
//! * [`FakeDirectory`]：实现 [`DirectoryPublisher`]，可注入 `Unavailable`/`Rejected` 故障。
//!
//! 这里的替身**只**替代宿主自己的接缝；驱动的 `Clean` 判定仍然是外部输入
//! （`DriverCleanVerdict` 的私有字段），替身不替驱动做裁决。

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use datazen_platform_api::id::{BlockId, RunId};

use crate::connection::port::CancelDisposition;
use crate::connection::testing::FakeClock;
use crate::connection::{
    ConfigRevision, ConnectionId, DbSessionId, ExecutionId, LeaseId, NamespaceTarget, OwnerRef,
    PoolKeyInputs, ResourceId,
};
use crate::resource::cleanup::{PhysicalTransport, SharedTransport};
use crate::resource::lease::{LeasePurpose, LeaseRecord, LeaseRequest};
use crate::resource::publication::{
    DirectoryFault, DirectoryPublisher, Publication, ReplacementReceipt,
};
use crate::resource::replacement::BeginOutcome;
use crate::resource::{MonotonicSource, ResourceError, ResourceManager};

// ---------------------------------------------------------------------------
// 时间
// ---------------------------------------------------------------------------

/// 用 `FakeClock` 驱动的单调时间源。
pub struct TestClock {
    inner: FakeClock,
}

impl TestClock {
    pub fn new() -> Self {
        Self {
            inner: FakeClock::new(),
        }
    }

    /// 显式推进时间。旅程里唯一的「等待」手段，绝不 `sleep`。
    pub fn advance(&self, duration: Duration) {
        let _ = self.inner.advance(duration);
    }
}

impl Default for TestClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonotonicSource for TestClock {
    fn now_nanos(&self) -> u64 {
        let now = self.inner.monotonic();
        now.as_nanos()
    }
}

// ---------------------------------------------------------------------------
// 物理端口
// ---------------------------------------------------------------------------

/// 物理端口上的动作，**按发生顺序**记账。
///
/// 顺序本身就是断言对象：CM-73 要求 `Reset` 必须排在 `Close` **之前**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    Open(ResourceId),
    Reset(ResourceId),
    Close(ResourceId),
    Cancel(ResourceId, ExecutionId),
}

/// 单点注入故障。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fault {
    #[default]
    None,
    Open,
    Reset,
    Close,
    Cancel,
}

/// 物理端口的记账本。
#[derive(Debug, Clone, Default)]
pub struct Journal {
    pub opened: Vec<ResourceId>,
    pub reset: Vec<ResourceId>,
    pub closed: Vec<ResourceId>,
    pub cancelled: Vec<(ResourceId, ExecutionId)>,
    pub events: Vec<TransportEvent>,
    pub fault: Fault,
    pub cancel_disposition: Option<CancelDisposition>,
    next: u64,
}

/// 记录式物理端口：只认账目标识，不碰任何真实句柄。
pub struct RecordingTransport {
    journal: Arc<Mutex<Journal>>,
}

impl RecordingTransport {
    pub fn new() -> Self {
        Self {
            journal: Arc::new(Mutex::new(Journal::default())),
        }
    }

    /// 塞进 `ResourceManager` 的形状。
    pub fn shared(&self) -> SharedTransport {
        Arc::new(Self {
            journal: Arc::clone(&self.journal),
        })
    }

    pub fn inject(&self, fault: Fault) {
        self.locked().fault = fault;
    }

    pub fn cancel_disposition(&self, disposition: CancelDisposition) {
        self.locked().cancel_disposition = Some(disposition);
    }

    pub fn snapshot(&self) -> Journal {
        self.locked().clone()
    }

    fn locked(&self) -> MutexGuard<'_, Journal> {
        // 测试锁中毒不掩盖断言失败：取回内层继续用。
        self.journal
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn issue(journal: &mut Journal) -> ResourceId {
        journal.next += 1;
        ResourceId::new(format!("res-{}", journal.next))
    }
}

impl Default for RecordingTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicalTransport for RecordingTransport {
    fn open(&self, _request: &LeaseRequest) -> Result<ResourceId, ResourceError> {
        let mut journal = self.locked();
        if journal.fault == Fault::Open {
            return Err(ResourceError::TransportRefused(
                "open refused by fault injection",
            ));
        }
        let resource = Self::issue(&mut journal);
        journal.opened.push(resource.clone());
        journal.events.push(TransportEvent::Open(resource.clone()));
        Ok(resource)
    }

    fn close(&self, resource: &ResourceId) -> Result<(), ResourceError> {
        let mut journal = self.locked();
        if journal.fault == Fault::Close {
            return Err(ResourceError::TransportRefused(
                "close refused by fault injection",
            ));
        }
        journal.closed.push(resource.clone());
        journal.events.push(TransportEvent::Close(resource.clone()));
        Ok(())
    }

    fn reset(&self, resource: &ResourceId) -> Result<(), ResourceError> {
        let mut journal = self.locked();
        if journal.fault == Fault::Reset {
            return Err(ResourceError::TransportRefused(
                "reset refused by fault injection",
            ));
        }
        journal.reset.push(resource.clone());
        journal.events.push(TransportEvent::Reset(resource.clone()));
        Ok(())
    }

    fn cancel(
        &self,
        resource: &ResourceId,
        execution: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError> {
        let mut journal = self.locked();
        journal
            .events
            .push(TransportEvent::Cancel(resource.clone(), execution.clone()));
        if journal.fault == Fault::Cancel {
            return Err(ResourceError::TransportRefused(
                "cancel refused by fault injection",
            ));
        }
        journal
            .cancelled
            .push((resource.clone(), execution.clone()));
        Ok(journal
            .cancel_disposition
            .unwrap_or(CancelDisposition::Requested))
    }
}

// ---------------------------------------------------------------------------
// 目录
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DirectoryState {
    fault: Option<DirectoryFault>,
    records: Vec<Publication>,
    rejected: Vec<String>,
}

/// 可注入故障的目录替身。
pub struct FakeDirectory {
    state: Arc<Mutex<DirectoryState>>,
}

impl FakeDirectory {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DirectoryState::default())),
        }
    }

    pub fn shared(&self) -> Arc<dyn DirectoryPublisher> {
        Arc::new(Self {
            state: Arc::clone(&self.state),
        })
    }

    /// 之后的每次 `publish` 都返回这个故障，直到 [`Self::recover`]。
    pub fn fail_with(&self, fault: DirectoryFault) {
        self.locked().fault = Some(fault);
    }

    pub fn recover(&self) {
        self.locked().fault = None;
    }

    /// 目录里现在**可见**的记录数。
    pub fn visible_records(&self) -> usize {
        self.locked().records.len()
    }

    pub fn records_session(&self, session_id: &DbSessionId) -> bool {
        self.locked()
            .records
            .iter()
            .any(|record| &record.session_id == session_id)
    }

    /// 被目录**明确拒绝**过的令牌，用来证明「拒绝发生过且没被吞掉」。
    pub fn rejected_tokens(&self) -> Vec<String> {
        self.locked().rejected.clone()
    }

    fn locked(&self) -> MutexGuard<'_, DirectoryState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl Default for FakeDirectory {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryPublisher for FakeDirectory {
    fn publish(&self, receipt: &ReplacementReceipt) -> Result<Publication, DirectoryFault> {
        let mut state = self.locked();
        if let Some(fault) = state.fault {
            if matches!(fault, DirectoryFault::Rejected(_)) {
                state.rejected.push(receipt.attachment_token.clone());
            }
            return Err(fault);
        }
        let publication = Publication {
            session_id: receipt.new_session_id.clone(),
            attachment_token: receipt.attachment_token.clone(),
            published_at_nanos: receipt
                .published_at_nanos
                .unwrap_or(receipt.committed_at_nanos),
        };
        state.records.push(publication.clone());
        Ok(publication)
    }

    fn lookup(&self, session_id: &DbSessionId) -> Result<Option<Publication>, DirectoryFault> {
        let state = self.locked();
        if let Some(fault) = state.fault {
            return Err(fault);
        }
        Ok(state
            .records
            .iter()
            .find(|record| &record.session_id == session_id)
            .cloned())
    }
}

// ---------------------------------------------------------------------------
// 池键输入
// ---------------------------------------------------------------------------

/// 一个带 database + policy 的池键输入（CM-67：按库与策略分片）。
pub fn pool_key_inputs(
    connection_id: &ConnectionId,
    database: &str,
    policy: &str,
) -> PoolKeyInputs {
    PoolKeyInputs {
        connection_id: connection_id.clone(),
        config_revision: ConfigRevision::new(1),
        driver_id: "postgres".to_owned(),
        namespace: NamespaceTarget {
            database: database.to_owned(),
            catalog: "public".to_owned(),
            schema: "public".to_owned(),
            path: format!("public.{database}"),
        },
        execution_identity_key: "exec-identity-1".to_owned(),
        policy_isolation_key: policy.to_owned(),
    }
}

/// 旅程里最常用的归属主体。
pub fn workflow_owner() -> OwnerRef {
    OwnerRef::WorkflowBlock {
        run_id: RunId::new("run-1"),
        block_id: BlockId::new("blk-1"),
    }
}

// ---------------------------------------------------------------------------
// 场景台架
// ---------------------------------------------------------------------------

/// 一个 `ResourceManager` + 记账式物理端口 + 可注入故障的目录。
pub struct Stage {
    pub manager: ResourceManager,
    pub transport: RecordingTransport,
    pub directory: FakeDirectory,
    pub connection: ConnectionId,
}

impl Stage {
    pub fn new() -> Self {
        let clock: Arc<dyn MonotonicSource> = Arc::new(TestClock::new());
        let transport = RecordingTransport::new();
        let directory = FakeDirectory::new();
        let manager =
            ResourceManager::new(transport.shared(), clock).with_directory(directory.shared());
        Self {
            manager,
            transport,
            directory,
            connection: ConnectionId::new("conn-stage"),
        }
    }

    pub fn request(&self) -> LeaseRequest {
        LeaseRequest::new(
            pool_key_inputs(&self.connection, "appdb", "policy-a"),
            workflow_owner(),
            LeasePurpose::ShortOperation,
        )
    }

    pub fn acquire(&mut self) -> LeaseRecord {
        let request = self.request();
        self.manager
            .acquire(&request)
            .expect("a fresh request must open a resource")
    }

    /// 打开一个候选，返回它的租约 id（`candidate-{key}`）。
    pub fn open_candidate(&mut self, key: &str, old: &DbSessionId, new: &DbSessionId) -> LeaseId {
        let request = self.request();
        let outcome = self
            .manager
            .begin_replacement(
                key,
                old.clone(),
                new.clone(),
                ResourceId::new("res-old"),
                &request,
            )
            .expect("begin a replacement candidate");
        assert!(matches!(outcome, BeginOutcome::Started { .. }));
        LeaseId::new(format!("candidate-{key}"))
    }
}
