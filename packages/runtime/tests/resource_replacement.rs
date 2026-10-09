//! 候选替换的公开面集成测试（提交闸门）。
//!
//! 替身全部手写：`datazen_runtime::testing` 下的 fixture 由 `cfg(test)` 门控，
//! `tests/` 编译 lib 时并不带 `cfg(test)`，只能按本文件声明的公开接缝自建。
//! 轮换与停用的旅程在 `resource_rotation_and_disable.rs`。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use datazen_runtime::connection::port::CancelDisposition;
use datazen_runtime::connection::types::ClientInstanceId;
use datazen_runtime::connection::{
    ConfigRevision, ConnectionId, DbSessionId, ExecutionId, LeaseId, NamespaceTarget, OwnerRef,
    PoolKeyInputs, ResourceId,
};
use datazen_runtime::resource::{
    BeginOutcome, DirectoryFault, DirectoryPublisher, LeasePurpose, LeaseRequest, MonotonicSource,
    PhysicalTransport, Publication, ReplacementReceipt, ResourceError, ResourceManager,
};

/// 固定时刻的单调时钟：本文件断言的是状态裁决，不依赖时长，因此不需要推进。
struct FixedClock {
    nanos: AtomicU64,
}

impl FixedClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            nanos: AtomicU64::new(1_000),
        })
    }
}

impl MonotonicSource for FixedClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Default)]
struct Journal {
    opened: Vec<ResourceId>,
    closed: Vec<ResourceId>,
    issued: u64,
}

struct ScriptedTransport {
    journal: Mutex<Journal>,
}

impl ScriptedTransport {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            journal: Mutex::new(Journal::default()),
        })
    }

    fn opened(&self) -> Vec<ResourceId> {
        self.lock().opened.clone()
    }

    fn closed(&self) -> Vec<ResourceId> {
        self.lock().closed.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Journal> {
        self.journal
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

impl PhysicalTransport for ScriptedTransport {
    fn open(&self, _request: &LeaseRequest) -> Result<ResourceId, ResourceError> {
        let mut journal = self.lock();
        journal.issued += 1;
        let resource_id = ResourceId::new(format!("res-{}", journal.issued));
        journal.opened.push(resource_id.clone());
        Ok(resource_id)
    }

    fn close(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        self.lock().closed.push(resource_id.clone());
        Ok(())
    }

    fn reset(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        self.lock().closed.push(resource_id.clone());
        Ok(())
    }

    fn cancel(
        &self,
        _resource_id: &ResourceId,
        _execution_id: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError> {
        Ok(CancelDisposition::Requested)
    }
}

/// 目录发布替身：能按注入的故障拒绝发布，并统计可见记录数与被拒令牌。
struct StubDirectory {
    inner: Mutex<DirectoryState>,
}

#[derive(Debug, Default)]
struct DirectoryState {
    fault: Option<DirectoryFault>,
    published: Vec<Publication>,
    rejected_tokens: Vec<String>,
}

impl StubDirectory {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(DirectoryState::default()),
        })
    }

    fn fail_with(&self, fault: DirectoryFault) {
        self.lock().fault = Some(fault);
    }

    fn recover(&self) {
        self.lock().fault = None;
    }

    fn visible_records(&self) -> usize {
        self.lock().published.len()
    }

    fn rejected_tokens(&self) -> Vec<String> {
        self.lock().rejected_tokens.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DirectoryState> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl DirectoryPublisher for StubDirectory {
    fn publish(&self, receipt: &ReplacementReceipt) -> Result<Publication, DirectoryFault> {
        let mut state = self.lock();
        if let Some(fault) = state.fault.clone() {
            state.rejected_tokens.push(receipt.attachment_token.clone());
            return Err(fault);
        }
        let publication = Publication {
            session_id: receipt.new_session_id.clone(),
            attachment_token: receipt.attachment_token.clone(),
            published_at_nanos: receipt.committed_at_nanos + 1,
        };
        state.published.push(publication.clone());
        Ok(publication)
    }

    fn lookup(&self, session_id: &DbSessionId) -> Result<Option<Publication>, DirectoryFault> {
        Ok(self
            .lock()
            .published
            .iter()
            .find(|publication| publication.session_id == *session_id)
            .cloned())
    }
}

fn connection(id: &str) -> ConnectionId {
    ConnectionId::new(id.to_owned())
}

fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-it-1"),
    }
}

fn request(connection_id: &ConnectionId, database: &str) -> LeaseRequest {
    LeaseRequest::new(
        PoolKeyInputs {
            connection_id: connection_id.clone(),
            config_revision: ConfigRevision::new(1),
            driver_id: "postgres".to_owned(),
            namespace: NamespaceTarget {
                database: database.to_owned(),
                catalog: "appdb".to_owned(),
                schema: "public".to_owned(),
                path: "appdb.public".to_owned(),
            },
            execution_identity_key: "owner-hash-1".to_owned(),
            policy_isolation_key: "policy-a".to_owned(),
        },
        owner(),
        LeasePurpose::ShortOperation,
    )
}

fn manager() -> (
    ResourceManager,
    Arc<ScriptedTransport>,
    Arc<StubDirectory>,
    ConnectionId,
) {
    let transport = ScriptedTransport::new();
    let directory = StubDirectory::new();
    let connection_id = connection("conn-replace");
    let manager = ResourceManager::new(transport.clone(), FixedClock::new())
        .with_directory(directory.clone());
    (manager, transport, directory, connection_id)
}

fn reason_of(error: ResourceError) -> &'static str {
    error.reason()
}

// ---------------------------------------------------------------------------
// 候选替换的提交闸门
// ---------------------------------------------------------------------------

#[test]
fn a_candidate_is_not_published_before_commit_and_cannot_take_executions() {
    let (mut manager, transport, directory, connection_id) = manager();
    let old_session = DbSessionId::new("sess-old");
    let new_session = DbSessionId::new("sess-new");
    let old = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the old session holds a resource");
    let candidate_lease = LeaseId::new("candidate-key-1");

    let outcome = manager
        .begin_replacement(
            "key-1",
            old_session.clone(),
            new_session.clone(),
            old.resource_id.clone(),
            &request(&connection_id, "appdb"),
        )
        .expect("a candidate may be built");
    assert!(matches!(outcome, BeginOutcome::Started { .. }));

    let refusal = manager
        .accepts_execution(&candidate_lease)
        .expect_err("an unpublished candidate must not serve executions");
    assert_eq!(reason_of(refusal), "resourceCandidateNotPublished");
    assert_eq!(
        directory.visible_records(),
        0,
        "no directory record exists before the commit barrier opens"
    );
    assert_eq!(
        transport.opened().len(),
        2,
        "the candidate holds its own resource"
    );
    assert!(
        manager.old_session_executable(&old_session),
        "before commit the old session is untouched"
    );
    assert!(manager.accepts_execution(&old.lease_id).is_ok());
}

#[test]
fn a_failure_before_commit_leaves_the_old_session_executable_and_closes_the_candidate() {
    let (mut manager, transport, directory, connection_id) = manager();
    let old_session = DbSessionId::new("sess-old");
    let new_session = DbSessionId::new("sess-new");
    let old = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the old session holds a resource");
    let candidate_lease = LeaseId::new("candidate-key-1");
    manager
        .begin_replacement(
            "key-1",
            old_session.clone(),
            new_session,
            old.resource_id.clone(),
            &request(&connection_id, "appdb"),
        )
        .expect("a candidate may be built");

    manager
        .abandon_replacement("key-1")
        .expect("an uncommitted candidate is abandoned, not left dangling");

    assert!(
        manager.old_session_executable(&old_session),
        "a failure before commit must leave the old session valid"
    );
    assert!(manager.accepts_execution(&old.lease_id).is_ok());
    assert_eq!(
        manager.lease(&candidate_lease),
        None,
        "the abandoned candidate is cleaned off the ledger"
    );
    assert_eq!(
        transport.closed(),
        vec![ResourceId::new("res-2")],
        "the candidate's own resource is closed, never the old one"
    );
    assert_eq!(directory.visible_records(), 0);
}

#[test]
fn a_commit_whose_publish_fails_is_never_reported_as_success() {
    let (mut manager, _transport, directory, connection_id) = manager();
    let old_session = DbSessionId::new("sess-old");
    let new_session = DbSessionId::new("sess-new");
    let old = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the old session holds a resource");
    let candidate_lease = LeaseId::new("candidate-key-1");
    manager
        .begin_replacement(
            "key-1",
            old_session.clone(),
            new_session.clone(),
            old.resource_id.clone(),
            &request(&connection_id, "appdb"),
        )
        .expect("a candidate may be built");
    directory.fail_with(DirectoryFault::Rejected("attachment token already bound"));

    let error = manager
        .confirm_replacement("key-1", connection_id.clone(), "attach-2")
        .expect_err("a commit whose publish failed must not be reported as success");
    assert_eq!(reason_of(error), "resourceCandidateNotPublished");
    assert_eq!(directory.visible_records(), 0);
    assert_eq!(directory.rejected_tokens(), vec!["attach-2".to_owned()]);

    // 非空跑：提交后的状态与提交前**不同** —— 旧 id 已停用，候选仍不能执行，
    // 且只能靠重放拿到**同一个**新会话的回执。
    assert!(
        !manager.old_session_executable(&old_session),
        "after the barrier the old id must not execute"
    );
    let refusal = manager
        .accepts_execution(&candidate_lease)
        .expect_err("an unpublished candidate must not serve executions");
    assert_eq!(reason_of(refusal), "resourceCandidateNotPublished");

    directory.recover();
    let receipt = manager
        .recover_replacement("key-1")
        .expect("the committed candidate recovers its own receipt");
    assert_eq!(receipt.new_session_id, new_session);
    assert_eq!(receipt.old_session_id, old_session);
    assert!(
        receipt.published_at_nanos.is_some(),
        "a replayed receipt is published once the directory accepts it"
    );
    assert_eq!(directory.visible_records(), 1);
    assert!(
        manager.accepts_execution(&candidate_lease).is_ok(),
        "only a published candidate may serve executions"
    );
}

#[test]
fn the_same_idempotency_key_resumes_the_candidate_instead_of_opening_a_second_resource() {
    let (mut manager, transport, _directory, connection_id) = manager();
    let old_session = DbSessionId::new("sess-old");
    let old = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the old session holds a resource");
    let base = request(&connection_id, "appdb");
    manager
        .begin_replacement(
            "key-1",
            old_session.clone(),
            DbSessionId::new("sess-new"),
            old.resource_id.clone(),
            &base,
        )
        .expect("the first attempt builds a candidate");

    let resumed = manager
        .begin_replacement(
            "key-1",
            old_session.clone(),
            DbSessionId::new("sess-new"),
            old.resource_id.clone(),
            &base,
        )
        .expect("the same key resumes rather than failing");
    assert!(matches!(resumed, BeginOutcome::Resumed { .. }));
    assert_eq!(
        transport.opened().len(),
        2,
        "a resumed candidate must not open a duplicate resource"
    );

    // 同一个幂等键指向**另一个**旧会话时是另一件事：宁可拒绝也不产出第二个候选。
    let clash = manager.begin_replacement(
        "key-1",
        DbSessionId::new("sess-other"),
        DbSessionId::new("sess-new"),
        old.resource_id.clone(),
        &base,
    );
    assert_eq!(
        reason_of(clash.expect_err("a reused key must not fork")),
        "resourceDuplicateCandidate"
    );
    assert_eq!(transport.opened().len(), 2);
}
