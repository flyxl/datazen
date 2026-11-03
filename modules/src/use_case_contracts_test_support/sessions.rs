//! sessions 用例的双传输规格：binding 端口、共享夹具与四个 scenario。
//!
//! 规格本体（本文件除 `baseline` 外的一切）不感知 IPC/HTTP 词汇：scenario
//! 只经 [`SessionsContractBinding`] 触达用例，入参出参与断言全部落在
//! `datazen::execution` 的会话词汇（§6.4/§7.1–§7.5：`OpenSessionRequest` /
//! `OpenSessionReceipt` / `SessionView` / `ContextChangeReceipt` /
//! `CloseReceipt`）上。两条宿主腿与无宿主基线各自实现同一个 binding：
//!
//! - 基线（本文件 `baseline`，`cfg(test)`）：直调装配好的
//!   `RuntimeConnectionUseCases`（共享 [`MemoryProfileStore`] + 宽松策略
//!   假体 + 物理后端假体）——宿主整体换掉后规格仍原样过；
//! - HTTP 腿：`server/tests/use_case_contracts.rs`（§6.4 端点形态：POST
//!   `/api/v1/sessions`、GET `/api/v1/sessions/{id}`、PUT `.../context`、
//!   POST `.../close`——attach/detach 路由同形挂载，场景随后续登记）；
//! - IPC 腿：桌面 `commands/sessions_use_case_contract_tests.rs`（真实
//!   `#[tauri::command]` 入口 `open_session` / `get_session` /
//!   `set_session_context` / `close_session`）。
//!
//! 三腿的 scenario 测试与本文件 scenario 函数**逐字同名**，绿名单逐条
//! 对得上——「两腿同绿」因此可核对到每条用例。
//!
//! # 本域的错误词汇：有码 + application 原句
//!
//! 与 connections 域的「宿主装配句」相对照：本域用例失败直接是
//! [`ApiError`]（code + 脱敏 message 是 application 自己的产物），两条腿
//! 拿到的是**同一份错误**——scenario 因此可以逐字断言 message（含
//! `convert::runtime` 的收敛句「connection runtime operation failed」，
//! 那本身就是 application 决定不外传细节的词汇）。
//!
//! # 装配登记（可收窄面）
//!
//! - 会话可见性闸（HTTP 腿 `SessionsState.directory`）：规格假体腿共用
//!   用例面自持的目录；HTTP 腿的闸目录是装配事实源，由各腿 binding 在
//!   open / set_context 之后登记目录条目（生产装配裁决是「同一目录
//!   实例」——测试腿用登记动作模拟该裁决，见 server 腿文件头）。
//! - attach/detach 场景未随本刀登记（场景集收窄到 open → get →
//!   context → close 生命周期）；路由形状已由 HTTP 腿挂载。

use async_trait::async_trait;

use crate::connection::ProfileRecord;
use crate::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
    PolicyChangeStream, PolicyService,
};
use crate::execution::application::requests::{CloseMode, CloseSessionRequest, OpenSessionRequest, SetSessionContextRequest};
use crate::execution::application::use_cases::ConnectionUseCases;
use crate::execution::application::RuntimeConnectionUseCases;
use crate::execution::runtime::connection as rt;
use crate::execution::runtime::registry::{audit::CapabilityVersions, backend::*, SessionBackend};
use crate::execution::target::{ExecutionTarget, NamespaceTarget};
use crate::execution::{
    CloseReceipt, ContextChangeReceipt, IdempotentOperation, OpenSessionReceipt, SessionHandle,
    SessionState, SessionView, SubmissionToken,
};
use crate::shared::context::{OwnerRef, RequestContext};
use crate::shared::error::{ApiError, PortError};
use crate::shared::id::{ConnectionId, Counter, OrganizationId, PolicyIsolationKey};
use crate::use_case_contracts_test_support::connections::{
    contract_context, MemoryProfileStore, CONTRACT_ORGANIZATION,
};
use crate::use_case_contracts_test_support::{ContractFailure, ContractResult};

// ── 传输装配端口 ─────────────────────────────────────────────────────────

/// sessions 用例的传输装配端口（binding harness）。
///
/// 每条腿实现一次：`seed_profile` 是 setup——把 open 的目标档案**直写**
/// 进本腿装配的档案存储（不经用例）；`client_owner` 交出本腿装配的身份
/// 事实里归属声明的那一半（`OwnerRef` 的 client 半边必须与该腿请求上下文
/// 的 client 一致，可采信性核对在用例层）；其余用例方法经本腿的真实传输
/// （tauri command / axum handler）触达 application。规格对实现方式零假设。
///
/// `?Send` 与既有域同一裁决：binding 由单个测试的栈持有、scenario 顺序
/// await，规格不假设跨线程。
#[async_trait(?Send)]
pub trait SessionsContractBinding {
    /// setup：open 的目标档案直写本腿存储（规格的 scenario 每次只 seed
    /// 一条目标档案）。
    async fn seed_profile(&self, record: ProfileRecord) -> Result<(), String>;

    /// 本腿身份的归属半边：`OwnerRef::ClientSession`，client 实例与该腿
    /// 请求上下文一致。
    async fn client_owner(&self) -> OwnerRef;

    /// `issueSubmissionToken` 的会话操作半边（§6.4 令牌列 ✓ 的签发面）：
    /// `openSession` 不绑定句柄，`setSessionContext` 绑定**当前**句柄。
    async fn issue_session_token(
        &self,
        operation: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> ContractResult<SubmissionToken>;

    /// `openSession` 用例半边（POST /api/v1/sessions 与 `open_session` 的
    /// 用例半边）→ 回执。
    async fn open(&self, request: OpenSessionRequest) -> ContractResult<OpenSessionReceipt>;

    /// `getSession` 用例半边（GET /api/v1/sessions/{id} 与 `get_session` 的
    /// 用例半边）→ 会话视图。
    async fn get(&self, handle: SessionHandle) -> ContractResult<SessionView>;

    /// `setSessionContext` 用例半边（PUT .../context）→ 替换回执。
    async fn set_context(
        &self,
        request: SetSessionContextRequest,
    ) -> ContractResult<ContextChangeReceipt>;

    /// `closeSession` 用例半边（POST .../close）→ 关闭回执（幂等）。
    async fn close(&self, request: CloseSessionRequest) -> ContractResult<CloseReceipt>;
}

// ── 共享夹具（setup 材料）─────────────────────────────────────────────────

/// 规格目标连接 id（open 的 target 必须指向已 seed 的档案）。
pub const CONTRACT_CONNECTION: &str = "conn-contract";

/// 上下文切换场景的期望命名空间（与 open 时的 "app" 区分）。
pub const CHANGED_DATABASE: &str = "reporting";

/// open 的目标档案（driver 词汇 "postgres" 与桌面夹具驱动对齐）。
pub fn contract_profile_record() -> ProfileRecord {
    let mut record = ProfileRecord::new(
        OrganizationId::new(CONTRACT_ORGANIZATION),
        ConnectionId::new(CONTRACT_CONNECTION),
        "postgres",
    );
    record.name = "Contract target".to_owned();
    record.initial_namespace = NamespaceTarget::database("app");
    record
}

/// open 的显式目标（§4.3：缺层即 targetRequired，绝不回退默认库）。
pub fn contract_target() -> ExecutionTarget {
    ExecutionTarget::new(
        ConnectionId::new(CONTRACT_CONNECTION),
        NamespaceTarget::database("app"),
    )
}

/// application 错误 → 形态中立观测（基线腿的装配映射；两条宿主腿各自的
/// 映射必须产出与这里相同的 code——本域用例失败就是 `ApiError`，两腿拿到
/// 同一份错误，message 也逐字一致）。
pub fn application_failure(error: ApiError) -> ContractFailure {
    ContractFailure::from_application(Some(error.code.as_str().to_owned()), error.message)
}

/// 宽松策略假体：本域规格不测授权（授权判据归 policy 域测试），一切
/// Connect/Query 放行，委托校验按端口硬约束拒绝。
pub struct ContractPolicy;

#[async_trait]
impl PolicyService for ContractPolicy {
    async fn authorize(
        &self,
        _ctx: &RequestContext,
        _subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(AuthorizationDecision::Allow)
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        _delegation: crate::shared::context::DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        Err(PortError::TokenInvalid)
    }

    fn isolation_key(&self, _ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(PolicyIsolationKey::new("contract-sessions"))
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}

/// 物理后端假体：open 观测到目标命名空间、close 确认关闭（与
/// `ai::application` 契约测试的 FakeBackend 同源语义；execute/cancel/
/// finalize 是端口必填位，规格场景不触达，给出确定性回答）。
#[derive(Default)]
pub struct ContractSessionBackend {
    contexts: std::sync::Mutex<std::collections::HashMap<String, rt::SessionContext>>,
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().expect("contract session backend lock")
}

#[async_trait]
impl SessionBackend for ContractSessionBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rt::ProviderError> {
        let resource_id = format!("resource-{}", request.db_session_id.as_str());
        let context = rt::SessionContext::new(request.initial_target.clone(), "contract-fixture");
        lock(&self.contexts).insert(resource_id.clone(), context.clone());
        Ok(OpenedResource {
            context,
            capabilities: CapabilityVersions::new("contract", "contract"),
            capability_snapshot: CapabilityProvenance::new("contract-driver", "1.0.0", 1),
            resource_id,
            driver_supports_cancel: true,
        })
    }

    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rt::ProviderError> {
        request.cancel_handle_sink.publish("contract-cancel");
        let context = lock(&self.contexts)
            .get(&request.resource_id)
            .cloned()
            .ok_or_else(|| rt::ProviderError::SessionLost("unknown resource".into()))?;
        Ok(ResourceExecution {
            execution_id: request.execution_id.clone(),
            stream_id: rt::StreamId::new("contract-stream"),
            state: rt::ExecutionState::Succeeded,
            effect_outcome: rt::EffectOutcome::Completed,
            context_after: context,
            context_revision: request.expected_context_revision,
            handles: Vec::new(),
            cancel_handle: "contract-cancel".into(),
        })
    }

    async fn cancel(
        &self,
        _request: CancelOnResource,
    ) -> Result<ResourceCancel, rt::ProviderError> {
        Ok(ResourceCancel {
            disposition: rt::port::CancelDisposition::Requested,
            state: rt::ExecutionState::CancelRequested,
        })
    }

    async fn finalize_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, rt::ProviderError> {
        Ok(HandleFinalization {
            finalized: request.handles.len(),
            remaining: 0,
            effect_outcome: rt::EffectOutcome::RolledBack,
        })
    }

    async fn close(
        &self,
        request: CloseResource,
    ) -> Result<CloseResourceOutcome, rt::ProviderError> {
        lock(&self.contexts).remove(&request.resource_id);
        Ok(CloseResourceOutcome::Closed)
    }
}

// ── scenarios（规格本体：setup → request → 断言）──────────────────────────

async fn open_a_contract_session(
    binding: &dyn SessionsContractBinding,
) -> OpenSessionReceipt {
    binding
        .seed_profile(contract_profile_record())
        .await
        .expect("seed the target profile");
    let token = binding
        .issue_session_token(IdempotentOperation::OpenSession, None)
        .await
        .expect("an openSession token issues");
    let request = OpenSessionRequest::new(
        contract_target(),
        binding.client_owner().await,
        token.idempotency_key,
    );
    binding.open(request).await.expect("the session opens")
}

/// 正常路径：open 的回执回答了新会话的全部身份事实，且 get 用**同一个
/// 句柄**看到同一份视图（§6.4：`{id}` = `SessionHandle.dbSessionId`，回执
/// 与读回互为表里，两腿一致）。
pub async fn opening_a_session_answers_a_receipt_that_get_session_sees(
    binding: &dyn SessionsContractBinding,
) {
    let receipt = open_a_contract_session(binding).await;

    assert!(
        !receipt
            .session
            .handle
            .db_session_id
            .as_str()
            .trim()
            .is_empty(),
        "the runtime mints a non-empty db session id"
    );
    assert_eq!(
        receipt.session.connection_id.as_str(),
        CONTRACT_CONNECTION,
        "the receipt answers the addressed profile"
    );
    assert_eq!(
        receipt.session.context_revision,
        Counter::new(1),
        "a new session starts at contextRevision 1（views.rs 的既有 wire 语义）"
    );
    assert!(
        !receipt.attachment_token.is_empty(),
        "the receipt carries the attachment token"
    );

    let view = binding
        .get(receipt.session.handle.clone())
        .await
        .expect("get after open");
    assert_eq!(
        view.handle, receipt.session.handle,
        "the same handle answers for the opened session"
    );
    assert_eq!(view.connection_id, receipt.session.connection_id);
    assert_eq!(view.initial_target, receipt.session.initial_target);
    assert_eq!(
        view.observed_context.namespace.database.as_deref(),
        Some("app"),
        "the observed namespace is the opened target"
    );
}

/// 正常路径：两阶段候选 → 服务端提交替换——回执报告被替换的旧会话与
/// **新**会话，新会话的句柄与命名空间随即经 get 可见（§7.4；旧 dbSessionId
/// 不再是当前会话）。
pub async fn a_context_change_answers_a_receipt_the_new_session_answers_for(
    binding: &dyn SessionsContractBinding,
) {
    let opened = open_a_contract_session(binding).await;
    let current = binding
        .get(opened.session.handle.clone())
        .await
        .expect("get the opened session");
    let old_db_session_id = opened.session.handle.db_session_id.clone();

    let token = binding
        .issue_session_token(
            IdempotentOperation::SetSessionContext,
            Some(&opened.session.handle),
        )
        .await
        .expect("a setSessionContext token issues");
    let request = SetSessionContextRequest::new(
        opened.session.handle.clone(),
        current.context_revision,
        NamespaceTarget::database(CHANGED_DATABASE),
        token.idempotency_key,
    );
    let change = binding.set_context(request).await.expect("the context changes");

    assert_eq!(
        change.replaced_session_id.as_ref().map(|id| id.as_str()),
        Some(old_db_session_id.as_str()),
        "the old session id is reported as replaced"
    );
    assert_ne!(
        change.session.handle.db_session_id, old_db_session_id,
        "the replacement is a new db session"
    );
    assert_eq!(
        change.session.observed_context.namespace.database.as_deref(),
        Some(CHANGED_DATABASE),
        "the receipt reflects the desired namespace"
    );

    let view = binding
        .get(change.session.handle.clone())
        .await
        .expect("get after the change");
    assert_eq!(
        view.handle, change.session.handle,
        "the new session answers for the receipt's handle"
    );
    assert_eq!(
        view.observed_context.namespace.database.as_deref(),
        Some(CHANGED_DATABASE),
        "the new session serves the changed context"
    );
}

/// 正常路径：close 幂等（§7.5）——重复 close 查墓碑，同键回答**原**
/// `CloseReceipt`（`PartialEq` 逐字段一致，不是形状相似）。
pub async fn closing_a_session_answers_the_same_receipt_twice(
    binding: &dyn SessionsContractBinding,
) {
    let opened = open_a_contract_session(binding).await;
    let handle = opened.session.handle.clone();
    let request = CloseSessionRequest::new(handle.clone(), CloseMode::RollbackAndClose);

    let first = binding.close(request.clone()).await.expect("the first close closes");
    assert_eq!(
        first.db_session_id, handle.db_session_id,
        "the receipt names the closed session"
    );
    assert_eq!(
        first.state,
        SessionState::Closed,
        "a healthy close answers closed"
    );

    let second = binding
        .close(request)
        .await
        .expect("the retry close answers the tombstone");
    assert_eq!(
        second, first,
        "§7.5: the same key answers the original receipt"
    );
}

/// 错误路径（乐观并发族）：过期 `expectedContextRevision` 的上下文切换在
/// **任何物理替换之前**被拒，拒绝携带契约码 `contextConflict` 与 application
/// 收敛句（两腿同一份 `ApiError`），存量会话不受牵连。
pub async fn a_stale_expected_context_revision_is_refused_as_context_conflict(
    binding: &dyn SessionsContractBinding,
) {
    let opened = open_a_contract_session(binding).await;
    let current = binding
        .get(opened.session.handle.clone())
        .await
        .expect("get the opened session");

    let token = binding
        .issue_session_token(
            IdempotentOperation::SetSessionContext,
            Some(&opened.session.handle),
        )
        .await
        .expect("a setSessionContext token issues");
    let request = SetSessionContextRequest::new(
        opened.session.handle.clone(),
        current.context_revision.saturating_increment(),
        NamespaceTarget::database(CHANGED_DATABASE),
        token.idempotency_key,
    );
    let failure = binding
        .set_context(request)
        .await
        .expect_err("the stale expected context revision is refused");
    assert_eq!(
        failure.code.as_deref(),
        Some("contextConflict"),
        "the CAS conflict carries the contract code on both legs"
    );
    assert_eq!(
        failure.message, "connection runtime operation failed",
        "the collapsed application sentence travels verbatim"
    );

    let still = binding
        .get(opened.session.handle)
        .await
        .expect("get after the refusal");
    assert_eq!(
        still.observed_context.namespace.database.as_deref(),
        Some("app"),
        "the refused replacement did not touch the session"
    );
}

// ── 无宿主基线（判据本体：宿主整体换掉，规格原样过）───────────────────────

#[cfg(test)]
mod baseline {
    use super::*;
    use std::sync::Arc;

    /// 无传输绑定：直调装配好的 `RuntimeConnectionUseCases`。实现与本文件
    /// scenario 用的是同一用例面、同一夹具——这就是「把宿主从 Tauri 换成
    /// axum，测试还能原样过」的极限形态（连宿主都没有）。
    struct DirectRuntimeBinding {
        store: Arc<MemoryProfileStore>,
        use_cases: RuntimeConnectionUseCases,
        ctx: RequestContext,
    }

    #[async_trait(?Send)]
    impl SessionsContractBinding for DirectRuntimeBinding {
        async fn seed_profile(&self, record: ProfileRecord) -> Result<(), String> {
            self.store.seed(vec![record]).await;
            Ok(())
        }

        async fn client_owner(&self) -> OwnerRef {
            OwnerRef::ClientSession {
                client_instance_id: self.ctx.client_instance_id.clone(),
                purpose: "contract".to_owned(),
            }
        }

        async fn issue_session_token(
            &self,
            operation: IdempotentOperation,
            handle: Option<&SessionHandle>,
        ) -> ContractResult<SubmissionToken> {
            self.use_cases
                .issue_submission_token(&self.ctx, operation, handle)
                .await
                .map_err(application_failure)
        }

        async fn open(&self, request: OpenSessionRequest) -> ContractResult<OpenSessionReceipt> {
            self.use_cases
                .open_session(&self.ctx, request)
                .await
                .map_err(application_failure)
        }

        async fn get(&self, handle: SessionHandle) -> ContractResult<SessionView> {
            self.use_cases
                .get_session(&self.ctx, handle)
                .await
                .map_err(application_failure)
        }

        async fn set_context(
            &self,
            request: SetSessionContextRequest,
        ) -> ContractResult<ContextChangeReceipt> {
            self.use_cases
                .set_session_context(&self.ctx, request)
                .await
                .map_err(application_failure)
        }

        async fn close(&self, request: CloseSessionRequest) -> ContractResult<CloseReceipt> {
            self.use_cases
                .close_session(&self.ctx, request)
                .await
                .map_err(application_failure)
        }
    }

    fn harness() -> DirectRuntimeBinding {
        let store = Arc::new(MemoryProfileStore::default());
        let use_cases = RuntimeConnectionUseCases::new(
            store.clone(),
            Arc::new(ContractPolicy),
            Arc::new(ContractSessionBackend::default()),
        );
        DirectRuntimeBinding {
            store,
            use_cases,
            ctx: contract_context(),
        }
    }

    #[tokio::test]
    async fn opening_a_session_answers_a_receipt_that_get_session_sees() {
        super::opening_a_session_answers_a_receipt_that_get_session_sees(&harness()).await;
    }

    #[tokio::test]
    async fn a_context_change_answers_a_receipt_the_new_session_answers_for() {
        super::a_context_change_answers_a_receipt_the_new_session_answers_for(&harness()).await;
    }

    #[tokio::test]
    async fn closing_a_session_answers_the_same_receipt_twice() {
        super::closing_a_session_answers_the_same_receipt_twice(&harness()).await;
    }

    #[tokio::test]
    async fn a_stale_expected_context_revision_is_refused_as_context_conflict() {
        super::a_stale_expected_context_revision_is_refused_as_context_conflict(&harness()).await;
    }
}
