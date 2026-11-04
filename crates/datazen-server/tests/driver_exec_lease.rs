//! WR-F7 验收测试：server 驱动执行后端 + runtime 租约源。
//!
//! 钉住的契约（不依赖真数据库；驱动用 `driver-api` 的 `MockDriver` /
//! `DriverRegistry::with_factories` 静态形态，后端/仓储用本文件的最小假体）：
//!
//! 1. **租约源 resolve 成功路径**：`ServerRuntimeLeaseSource::resolve()` 每次
//!    现构造 `RuntimeConnectionUseCases`（两次 resolve 不是同一个 Arc——
//!    端口语义禁止缓存，runtime 可能被吊销）。
//! 2. **无驱动类型 fail-closed 报错文案**：连接声明的 `databaseType` 没有被
//!    装配进 registry → `open` 拒绝，且文案逐字携带 registry 的配置事实
//!    （`driver type '…' is not registered in this build`）。
//! 3. **出站白名单在驱动之前**：egress 闸门拒绝时 `open` 失败，物理资源
//!    一个都没开（浏览器零数据库凭据的同族加固，P7）。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::connection::domain::{
    ConnectionRepository, ConnectionRepositoryError, ConnectionScope, ProfileRecord,
    ProfileRepository, ProfileScope,
};
use datazen::connection::dto::{ProfileDraft, ProfilePatch};
use datazen::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, PolicyChangeStream,
    PolicyService, PolicyVersionChange,
};
use datazen::execution::runtime::connection as rc;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::runtime::registry::audit::CapabilityVersions;
use datazen::execution::runtime::registry::backend::{
    CancelOnResource, CapabilityProvenance, CloseResource, CloseResourceOutcome, ExecuteOnResource,
    FinalizeHandles, HandleFinalization, OpenResource, OpenedResource, ResourceCancel,
    ResourceExecution, SessionBackend,
};
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{ConfigRevision, ConnectionId, PolicyIsolationKey};
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource as dr;
use datazen_driver_api::{ConnectionConfig, DatabaseDriver, DatabaseDriverFactory};
use datazen_server::execution::driver_exec::ServerDriverExecBackend;

const ORG: &str = "org-driver-exec-test";
const CONNECTION: &str = "conn-driver-exec-test";
const CONFIG_REVISION: u64 = 7;

// ── 假体：profiles / connections / policy / budget / backend ──────────────

/// `ProfileRepository` 假体：永远返回 enabled、`CONFIG_REVISION` 的记录。
struct Profiles;

#[async_trait]
impl ProfileRepository for Profiles {
    async fn list(
        &self,
        _ctx: &RequestContext,
        _scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        Ok(vec![])
    }
    async fn get(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        let mut record = ProfileRecord::new(ctx.organization_id.clone(), id, "test-profile");
        record.config_revision = ConfigRevision::new(CONFIG_REVISION);
        record.enabled = true;
        Ok(Some(record))
    }
    async fn create(
        &self,
        _ctx: &RequestContext,
        _draft: ProfileDraft,
        _idem: &datazen::shared::id::IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        Err(PortError::BackendUnavailable("not exercised".into()))
    }
    async fn compare_and_set(
        &self,
        _ctx: &RequestContext,
        _id: ConnectionId,
        _expected: ConfigRevision,
        _patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        Err(PortError::BackendUnavailable("not exercised".into()))
    }
    async fn disable(
        &self,
        _ctx: &RequestContext,
        _id: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        Err(PortError::BackendUnavailable("not exercised".into()))
    }
    async fn advance_credential_revision(
        &self,
        _ctx: &RequestContext,
        _id: ConnectionId,
    ) -> Result<datazen::shared::id::CredentialRevision, PortError> {
        Err(PortError::BackendUnavailable("not exercised".into()))
    }
}

/// 运行时读取半边走 `ProfileReader` 端口——同一假体双实现（读取面
/// 签名与 `ProfileRepository` 一致，直接委托）。
#[async_trait]
impl datazen::connection::domain::profile::ProfileReader for Profiles {
    async fn get(
        &self,
        ctx: &RequestContext,
        connection_id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        <Self as ProfileRepository>::get(self, ctx, connection_id).await
    }
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        <Self as ProfileRepository>::list(self, ctx, scope).await
    }
}

/// `ConnectionRepository` 假体：只有一条配置，`database_type` 由测试指定。
struct OneConnection {
    database_type: &'static str,
}

#[async_trait]
impl ConnectionRepository for OneConnection {
    async fn list(
        &self,
        _scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        Ok(vec![self.config()])
    }
    async fn get(
        &self,
        _scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        if connection_id == CONNECTION {
            Ok(Some(self.config()))
        } else {
            Ok(None)
        }
    }
    async fn save(
        &self,
        _scope: &ConnectionScope,
        _config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "not exercised".into(),
        ))
    }
    async fn delete(
        &self,
        _scope: &ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "not exercised".into(),
        ))
    }
    async fn reorder(
        &self,
        _scope: &ConnectionScope,
        _ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "not exercised".into(),
        ))
    }
    async fn touch_last_connected_at(
        &self,
        _scope: &ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "not exercised".into(),
        ))
    }
    async fn groups(
        &self,
        _scope: &ConnectionScope,
    ) -> Result<Vec<String>, ConnectionRepositoryError> {
        Ok(vec![])
    }
    async fn save_groups(
        &self,
        _scope: &ConnectionScope,
        _groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "not exercised".into(),
        ))
    }
}

impl OneConnection {
    fn config(&self) -> ConnectionConfig {
        ConnectionConfig {
            id: CONNECTION.into(),
            name: "driver-exec-test".into(),
            database_type: self.database_type.into(),
            host: Some("db.internal".into()),
            port: Some(5432),
            database: Some("app".into()),
            schema: None,
            username: Some("alice".into()),
            password: Some("secret-never-on-the-wire".into()),
            ssl_mode: Default::default(),
            connection_timeout: 30,
            max_pool_size: 5,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }
}

/// `PolicyService` 假体：全允许（用例面只验证 resolve 形状，不做授权裁决）。
struct AllowAllPolicy;

#[async_trait]
impl PolicyService for AllowAllPolicy {
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
        _delegation: datazen::shared::context::DelegationRef,
    ) -> Result<datazen::execution::application::policy::DelegationGrant, PortError> {
        Err(PortError::TokenInvalid)
    }
    fn isolation_key(&self, _ctx: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(PolicyIsolationKey::new("driver-exec-test"))
    }
    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty::<PolicyVersionChange>())
    }
}

/// `dr::BudgetPort` 假体：有求必应（预算桥接归主线接线，见 WR-F7 报告）。
struct OpenBudget;

#[async_trait]
impl dr::BudgetPort for OpenBudget {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<dr::BudgetPermit, dr::ResourceError> {
        Ok(dr::BudgetPermit {
            permit_id: "test-permit".into(),
            physical_connections: requested,
        })
    }
    async fn release_physical_connections(
        &self,
        _permit: &dr::BudgetPermit,
    ) -> Result<(), dr::ResourceError> {
        Ok(())
    }
}

/// `SessionBackend` 假体：租约源测试只验证 resolve 形状，物理面不开。
struct StubBackend;

#[async_trait]
impl SessionBackend for StubBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        Ok(OpenedResource {
            context: rc::SessionContext::new(request.initial_target, "stub"),
            capabilities: CapabilityVersions {
                contract: "1".into(),
                driver_api: "1".into(),
            },
            capability_snapshot: CapabilityProvenance::new("stub-driver", "0.0.0", 1),
            resource_id: "res_stub".into(),
            driver_supports_cancel: false,
        })
    }
    async fn execute(
        &self,
        _request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        Err(rc::ProviderError::HostRejected("unexpectedExecute".into()))
    }
    async fn cancel(
        &self,
        _request: CancelOnResource,
    ) -> Result<ResourceCancel, rc::ProviderError> {
        Err(rc::ProviderError::HostRejected("unexpectedCancel".into()))
    }
    async fn finalize_handles(
        &self,
        _request: FinalizeHandles,
    ) -> Result<HandleFinalization, rc::ProviderError> {
        Ok(HandleFinalization {
            finalized: 0,
            remaining: 0,
            effect_outcome: rc::EffectOutcome::NotStarted,
        })
    }
    async fn close(
        &self,
        _request: CloseResource,
    ) -> Result<CloseResourceOutcome, rc::ProviderError> {
        Ok(CloseResourceOutcome::Closed)
    }
}

/// 静态驱动工厂（driver-api 的 MockDriver 包装；`with_factories` 的种子）。
struct StubFactory;

impl DatabaseDriverFactory for StubFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        MockDriver::new("mock-stub", MockDriverOptions::default())
    }
    fn driver_id(&self) -> &'static str {
        "mock-stub"
    }
}

static STUB_FACTORY: StubFactory = StubFactory;

fn stub_registry() -> Arc<DriverRegistry> {
    Arc::new(
        DriverRegistry::with_factories([&STUB_FACTORY as &'static dyn DatabaseDriverFactory])
            .expect("stub factory admits cleanly"),
    )
}

fn open_request() -> OpenResource {
    OpenResource {
        connection_id: rc::ConnectionId::new(CONNECTION),
        config_revision: rc::ConfigRevision::new(CONFIG_REVISION),
        owner: rc::OwnerRef::ClientSession {
            client_instance_id: datazen::shared::id::ClientInstanceId::new("driver-exec-test"),
        },
        initial_target: rc::NamespaceTarget {
            database: "app".into(),
            catalog: String::new(),
            schema: String::new(),
            path: String::new(),
        },
        runtime_epoch: 1,
        db_session_id: rc::DbSessionId::new("dbs-driver-exec-test"),
    }
}

fn backend_with(
    registry: Arc<DriverRegistry>,
    database_type: &'static str,
    egress: Option<datazen_server::execution::driver_exec::EgressGate>,
) -> ServerDriverExecBackend {
    ServerDriverExecBackend::new(
        registry,
        Arc::new(OneConnection { database_type }),
        Arc::new(Profiles),
        ConnectionScope::new(ORG),
        Arc::new(OpenBudget),
        egress,
    )
}

// ── 验收 2：无驱动类型 fail-closed 报错文案 ────────────────────────────────

#[tokio::test]
async fn open_fails_closed_for_unregistered_driver_type_with_actionable_text() {
    let backend = backend_with(stub_registry(), "nosuch-driver", None);
    let error = backend
        .open(open_request())
        .await
        .expect_err("an unregistered driver type must fail closed");
    let text = error.to_string();
    assert!(
        text.contains("not registered in this build"),
        "fail-closed text must carry the registry fact verbatim: {text}"
    );
    assert!(
        text.contains("nosuch-driver"),
        "fail-closed text must name the missing driver type: {text}"
    );
}

// ── 验收 3：出站白名单在驱动之前 ──────────────────────────────────────────

#[tokio::test]
async fn egress_gate_blocks_open_before_any_driver_call() {
    let gate: datazen_server::execution::driver_exec::EgressGate =
        Arc::new(|_config: &ConnectionConfig| Err("egress blocked by whitelist".to_string()));
    let backend = backend_with(stub_registry(), "mock-stub", Some(gate));
    let error = backend
        .open(open_request())
        .await
        .expect_err("a whitelisted-out egress must be refused");
    assert!(
        error.to_string().contains("egress blocked"),
        "the refusal must come from the gate, not the driver: {error}"
    );
}

// ── 验收 1：租约源 resolve 成功路径（mcp feature 门控，与 runtime.rs 同门）──

#[cfg(feature = "mcp")]
mod lease_source {
    use super::*;
    use datazen_server::mcp::runtime::{RuntimeLeaseSource, ServerRuntimeLeaseSource};

    #[tokio::test]
    async fn resolve_returns_a_fresh_runtime_per_call() {
        let source = ServerRuntimeLeaseSource::new(
            Arc::new(Profiles),
            Arc::new(AllowAllPolicy),
            Arc::new(StubBackend),
        );
        let first = source
            .resolve()
            .await
            .expect("resolve must assemble the runtime");
        let second = source
            .resolve()
            .await
            .expect("second resolve must assemble again");
        // 每调用现取：两次 resolve 不得共享同一个 runtime 实例（端口语义
        // 禁止缓存——runtime 可能被吊销）。
        assert!(
            !Arc::ptr_eq(&first, &second),
            "lease source must not cache the runtime across calls"
        );
    }
}
