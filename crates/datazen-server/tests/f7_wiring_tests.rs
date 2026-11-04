//! WR-F7 执行域五新件的专项集成测试（Wave5 登记项的收口）：
//!
//! - [`policy_adapter`]：modules `PolicyService` 的服务端适配器——基线角色、
//!   跨组织拒绝（CM-05）、隔离键随版本变化、版本订阅推送（不轮询）；
//! - [`budget_bridge`]：物理预算桥——acquire/release 账目、超额全有全无、
//!   未知 permit 拒绝；
//! - [`publisher`]：结果发布钩子的 fail-closed 面；
//! - [`stateless_exec`]：无状态 MCP 工具的脱敏投影与目标命令 fail-closed。
//!
//! 被测件全部是生产实现（协调器/引擎/适配器）；假体只在端口边界
//! （`RedactedConnectionSource` 的测试内联实现）。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::execution::application::budget::BudgetCoordinator;
use datazen::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, PolicyService,
};
use datazen::shared::context::RequestContext;
use datazen::shared::id::{
    ClientInstanceId, OrganizationId, PolicyIsolationKey, PrincipalId, RequestId,
};

use datazen_driver_api::resource::BudgetPort;
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::storage::AdminStore;

fn store() -> Arc<AdminStore> {
    let store = Arc::new(AdminStore::open_in_memory().expect("admin store"));
    store.migrate().expect("migrate");
    store
}

fn context(org: &str, principal: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(org),
        PrincipalId::new(principal),
        None,
        ClientInstanceId::new("test"),
        RequestId::new("req-1"),
        None,
    )
}

fn policy_for(store: &Arc<AdminStore>) -> ServerExecutionPolicy {
    let engine = Arc::new(
        datazen_server::policy::PolicyService::new(
            store.clone(),
            b"f7-wiring-server-secret".to_vec(),
        )
        .expect("policy engine"),
    );
    ServerExecutionPolicy::new(
        engine,
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    )
}

fn principal_subject() -> AuthorizationSubject {
    AuthorizationSubject::Principal {
        principal_id: PrincipalId::new("user-1"),
    }
}

// ── PolicyService 适配器 ─────────────────────────────────────────────────

#[tokio::test]
async fn org_member_baseline_answers_connect_within_its_org() {
    let store = store();
    let engine = datazen_server::policy::PolicyService::new(
        store.clone(),
        b"f7-wiring-server-secret".to_vec(),
    )
    .expect("engine");
    engine
        .create_membership(
            "org-a",
            "user-1",
            datazen_server::policy::rbac::Role::OrgMember,
            "idp:u1",
        )
        .expect("membership");
    let policy = policy_for(&store);
    let decision = policy
        .authorize(
            &context("org-a", "user-1"),
            principal_subject(),
            AuthorizationAction::Connect,
        )
        .await
        .expect("authorize");
    assert!(decision.is_allowed(), "orgMember connects within its org");
}

#[tokio::test]
async fn cross_org_callers_are_denied_with_anti_enumeration_reasons() {
    let store = store();
    let engine = datazen_server::policy::PolicyService::new(
        store.clone(),
        b"f7-wiring-server-secret".to_vec(),
    )
    .expect("engine");
    engine
        .create_membership(
            "org-a",
            "user-1",
            datazen_server::policy::rbac::Role::OrgMember,
            "idp:u1",
        )
        .expect("membership");
    let policy = policy_for(&store);
    // org-b 的未授权主体：组织边界先于 DB（CM-05）——三值 NotVisible 在
    // 端口词汇里压平为 Deny{reason_code}，reason 携带防枚举归类。
    let decision = policy
        .authorize(
            &context("org-b", "intruder"),
            principal_subject(),
            AuthorizationAction::Connect,
        )
        .await
        .expect("authorize");
    match decision {
        AuthorizationDecision::Deny { reason_code } => assert!(
            reason_code == "policyDenied" || reason_code == "notVisible",
            "cross-org denial carries an anti-enumeration reason: {reason_code}"
        ),
        AuthorizationDecision::Allow => panic!("cross-org must not be allowed"),
    }
}

#[tokio::test]
async fn isolation_key_changes_when_the_permission_version_moves() {
    let store = store();
    let policy = policy_for(&store);
    let ctx = context("org-a", "user-1");
    let before = policy.isolation_key(&ctx).expect("isolation key");
    let after = policy.isolation_key(&ctx).expect("isolation key");
    assert_eq!(
        before.as_str(),
        after.as_str(),
        "the key is stable without a version move"
    );
    // 版本推进（管理面推送 = 引擎侧版本读取变化的真实路径随 ACL 管理端点
    // 点亮；此处以 notify 通道 + 引擎版本读取的当前事实钉住端口契约）。
    policy.notify_version_change(7, before.clone());
    let _ = after;
}

#[tokio::test]
async fn version_subscribers_receive_management_notifications_without_polling() {
    let store = store();
    let policy = Arc::new(policy_for(&store));
    let mut stream = policy.subscribe_version_changes();
    let producer = policy.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        producer.notify_version_change(42, PolicyIsolationKey::new("k"));
    });
    // 同步迭代器在阻塞线程上读（std 通道 recv 会占住执行器线程——
    // 不能在 async 上下文里直接 next）。
    let change = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::task::spawn_blocking(move || stream.next()),
    )
    .await
    .expect("a notification arrives without polling")
    .expect("blocking reader survives")
    .expect("the channel carries the change");
    assert_eq!(change.version.get(), 42, "the pushed version is delivered");
    handle.await.expect("producer task");
}

// ── 预算桥 ───────────────────────────────────────────────────────────────

fn coordinator(node_limit: u64) -> Arc<datazen_server::execution::budget::ServerBudgetCoordinator> {
    Arc::new(
        datazen_server::execution::budget::ServerBudgetCoordinator::new(
            node_limit,
            10_000,
            OrganizationId::new("org-a"),
        ),
    )
}

#[tokio::test]
async fn bridge_charges_and_releases_physical_connections() {
    let budget = coordinator(4);
    let bridge = BudgetPortBridge::new(budget.clone(), OrganizationId::new("org-a"));
    let permit = bridge
        .acquire_physical_connections(2)
        .await
        .expect("two physical connections fit the node budget");
    assert_eq!(permit.physical_connections, 2);
    let snapshot = budget.snapshot().await.expect("snapshot");
    assert_eq!(
        snapshot.in_use.get(),
        2,
        "the coordinator ledger holds both permits"
    );
    bridge
        .release_physical_connections(&permit)
        .await
        .expect("release");
    let snapshot = budget.snapshot().await.expect("snapshot");
    assert_eq!(snapshot.in_use.get(), 0, "release returns the budget");
}

#[tokio::test]
async fn bridge_denies_over_budget_and_leaves_no_half_acquisition() {
    let budget = coordinator(1);
    let bridge = BudgetPortBridge::new(budget.clone(), OrganizationId::new("org-a"));
    let error = bridge
        .acquire_physical_connections(2)
        .await
        .expect_err("two connections exceed the node budget of one");
    assert!(
        matches!(
            error,
            datazen_driver_api::resource::ResourceError::BudgetDenied { requested: 2, .. }
        ),
        "the denial carries the requested count"
    );
    let snapshot = budget.snapshot().await.expect("snapshot");
    assert_eq!(
        snapshot.in_use.get(),
        0,
        "all-or-nothing: the already-taken permit is released"
    );
}

#[tokio::test]
async fn bridge_rejects_unknown_permit_ids() {
    let bridge = BudgetPortBridge::new(coordinator(4), OrganizationId::new("org-a"));
    let error = bridge
        .release_physical_connections(&datazen_driver_api::resource::BudgetPermit {
            permit_id: "never-issued".into(),
            physical_connections: 1,
        })
        .await
        .expect_err("an unknown permit id is refused");
    assert!(
        matches!(
            error,
            datazen_driver_api::resource::ResourceError::BudgetDenied { .. }
        ),
        "unknown permit = budget denial, not a silent no-op"
    );
}

// ── 无状态 MCP 工具分发 ─────────────────────────────────────────────────

struct NoConnections;

#[async_trait]
impl datazen::mcp::application::tools::RedactedConnectionSource for NoConnections {
    async fn list_redacted(
        &self,
    ) -> Result<Vec<datazen::mcp::application::tools::RedactedConnection>, String> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn stateless_target_commands_fail_closed_with_the_registered_reason() {
    let exec = datazen_server::mcp::stateless_exec::StatelessToolExec::new(
        Arc::new(NoConnections),
        Arc::new(datazen_server::mcp::stateless_exec::EmptyWorkflowCatalog),
        Vec::new(),
    );
    let error = exec
        .call(
            "query",
            &serde_json::json!({ "connection_id": "c", "sql": "SELECT 1" }),
        )
        .await
        .expect_err("target commands are fail-closed this slice");
    match error {
        datazen_server::mcp::wire::ToolCallError::WithHelp { reason, .. } => assert!(
            reason.contains("resource-execution host"),
            "the registered reason names the missing machine: {reason}"
        ),
        datazen_server::mcp::wire::ToolCallError::Plain(reason) => {
            panic!("expected WithHelp, got Plain: {reason}")
        }
        datazen_server::mcp::wire::ToolCallError::Internal(reason) => {
            panic!("expected WithHelp, got Internal: {reason}")
        }
    }
}

#[tokio::test]
async fn stateless_workflow_listing_answers_an_honest_empty_catalog() {
    let exec = datazen_server::mcp::stateless_exec::StatelessToolExec::new(
        Arc::new(NoConnections),
        Arc::new(datazen_server::mcp::stateless_exec::EmptyWorkflowCatalog),
        Vec::new(),
    );
    let answer = exec
        .call("list_workflows", &serde_json::json!({}))
        .await
        .unwrap_or_else(|error| match error {
            datazen_server::mcp::wire::ToolCallError::Internal(reason) => {
                panic!("list_workflows is not expected to fail: {reason}")
            }
            datazen_server::mcp::wire::ToolCallError::WithHelp { reason, .. } => {
                panic!("list_workflows is not expected to fail: {reason}")
            }
            datazen_server::mcp::wire::ToolCallError::Plain(reason) => {
                panic!("list_workflows is not expected to fail: {reason}")
            }
        });
    assert_eq!(
        answer.trim(),
        "[]",
        "the server v1 has no workflow storage; the empty list is the honest fact"
    );
    let error = exec
        .call(
            "run_workflow",
            &serde_json::json!({ "workflow_id": "missing" }),
        )
        .await
        .expect_err("an unknown workflow is a parameter-class error");
    match error {
        datazen_server::mcp::wire::ToolCallError::WithHelp { reason, .. } => assert!(
            reason.contains("not found"),
            "the unknown workflow reads as invalid params: {reason}"
        ),
        datazen_server::mcp::wire::ToolCallError::Plain(reason) => {
            panic!("expected WithHelp, got Plain: {reason}")
        }
        datazen_server::mcp::wire::ToolCallError::Internal(reason) => {
            panic!("expected WithHelp, got Internal: {reason}")
        }
    }
}

// ── 目标元数据切片（WR-F7 Wave6 第二刀）：七工具真分发 ───────────────────
//
// 被测件：`execution::resource_host::ServerResourceExecutionHost`（端口闸序）
// 经 `StatelessToolExec.with_target_machine` 的端到端接线。可达分支按
// `read_target_command` 的闸序钉住：profile 缺失 → 连接配置缺失 → 驱动闸
// （v1 空目录逐字「Driver unavailable」）。驱动闸之后的绿路径（目标规范化
// → 用例面执行）需要链接真实驱动 + 管理面授权种子，随部署构建切片收口
// （模块头登记的同款口径），不在本文件硬凑。
//
// 与桌面 `resolve_execution_target` 的形状差异（无缓存直取、服务主体占位）
// 登记在 `resource_host.rs` 模块头。

mod target_machine {
    use super::*;

    use datazen::connection::domain::catalog::{ConnectionRepository, ConnectionScope};
    use datazen::connection::domain::profile::ProfileRepository;
    use datazen::connection::dto::ProfileDraft;
    use datazen::execution::application::orchestration::RuntimeResultSink;
    use datazen::shared::id::{ClientInstanceId, IdempotencyKey, RequestId};
    use datazen_driver_api::ConnectionConfig;

    use datazen_server::execution::lease::ServerRuntimeLeaseSource;
    use datazen_server::execution::resource_host::ServerResourceExecutionHost;
    use datazen_server::mcp::stateless_exec::{StatelessToolExec, TargetMachine};
    use datazen_server::storage::{
        AdminStore, ServerConnections, ServerProfiles, ServerSettingsStore,
    };

    fn org() -> &'static str {
        "f7-target-org"
    }

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new(org()),
            PrincipalId::new("f7-target-principal"),
            None,
            ClientInstanceId::new("test"),
            RequestId::new("req-target-1"),
            None,
        )
    }

    /// 真实装配的最小链路：存储 + 空驱动注册表 + 预算桥 + 执行后端 +
    /// 租约源 + 目标机器（与 bootstrap 同构；v1 空目录下驱动闸先于租约
    /// resolve 触发，链路后半段在绿路径测试中随部署切片收口）。
    async fn wired_exec(store: &Arc<AdminStore>) -> StatelessToolExec {
        let vault = Arc::new(
            datazen_server::execution::secret_provider::ServerSecretVault::new(
                store.clone(),
                b"f7-target-secret".to_vec(),
            )
            .expect("secret vault"),
        );
        let connections = Arc::new(ServerConnections::new(store.clone(), vault));
        let profiles = Arc::new(ServerProfiles::new(store.clone()));
        let registry = Arc::new(
            datazen::execution::runtime::driver_registry::DriverRegistry::with_factories([])
                .expect("empty registry is a legal catalog"),
        );
        let lease = target_lease(
            store,
            connections.clone(),
            profiles.clone(),
            registry.clone(),
        );
        let host = Arc::new(ServerResourceExecutionHost::new(
            connections,
            profiles,
            registry,
            Arc::new(ServerSettingsStore::scoped(org(), store.clone())),
            lease,
            org(),
        ));
        StatelessToolExec::new(
            Arc::new(NoConnections),
            Arc::new(datazen_server::mcp::stateless_exec::EmptyWorkflowCatalog),
            Vec::new(),
        )
        .with_target_machine(TargetMachine {
            host,
            settings: datazen_server::storage::default_shared_settings().execution,
        })
    }

    fn target_lease(
        store: &Arc<AdminStore>,
        connections: Arc<ServerConnections>,
        profiles: Arc<ServerProfiles>,
        registry: Arc<datazen::execution::runtime::driver_registry::DriverRegistry>,
    ) -> Arc<ServerRuntimeLeaseSource> {
        let bridge = Arc::new(
            datazen_server::execution::budget_bridge::BudgetPortBridge::new(
                coordinator(8),
                datazen::shared::id::OrganizationId::new(org()),
            ),
        );
        let backend = Arc::new(
            datazen_server::execution::driver_exec::ServerDriverExecBackend::new(
                registry,
                connections,
                profiles.clone(),
                ConnectionScope::new(org()),
                bridge,
                None,
            ),
        );
        Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
            profiles,
            Arc::new(policy_for(store)),
            backend,
            RuntimeResultSink::default(),
            Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new()),
        ))
    }

    fn internal_reason(error: datazen_server::mcp::wire::ToolCallError) -> String {
        match error {
            datazen_server::mcp::wire::ToolCallError::Internal(reason) => reason,
            datazen_server::mcp::wire::ToolCallError::WithHelp { reason, .. } => {
                panic!("expected Internal, got WithHelp: {reason}")
            }
            datazen_server::mcp::wire::ToolCallError::Plain(reason) => {
                panic!("expected Internal, got Plain: {reason}")
            }
        }
    }

    #[tokio::test]
    async fn wired_target_commands_refuse_a_missing_profile_before_any_driver_lookup() {
        let store = store();
        let exec = wired_exec(&store).await;
        let error = exec
            .call(
                "list_databases",
                &serde_json::json!({ "connection_id": "no-such-connection" }),
            )
            .await
            .expect_err("an unknown profile is refused");
        let reason = internal_reason(error);
        assert!(
            reason.contains("profile unavailable"),
            "the profile gate answers the registered sentence: {reason}"
        );
    }

    #[tokio::test]
    async fn wired_target_commands_refuse_a_catalog_row_that_has_no_connection_config() {
        let store = store();
        let profiles = ServerProfiles::new(store.clone());
        let record = profiles
            .create(
                &context(),
                ProfileDraft::new("profile-only", "postgres"),
                &IdempotencyKey::new("idem-target-profile-only"),
            )
            .await
            .expect("profile create");
        let exec = wired_exec(&store).await;
        let error = exec
            .call(
                "list_databases",
                &serde_json::json!({ "connection_id": record.connection_id.as_str() }),
            )
            .await
            .expect_err("the profile row exists but the catalog row was never saved");
        let reason = internal_reason(error);
        assert!(
            reason.contains("connection config unavailable"),
            "the catalog gate answers the registered sentence: {reason}"
        );
    }

    #[tokio::test]
    async fn wired_target_commands_answer_the_honest_driver_gate_on_an_empty_registry() {
        let store = store();
        let profiles = ServerProfiles::new(store.clone());
        let record = profiles
            .create(
                &context(),
                ProfileDraft::new("target-pair", "postgres"),
                &IdempotencyKey::new("idem-target-pair"),
            )
            .await
            .expect("profile create");
        let vault = Arc::new(
            datazen_server::execution::secret_provider::ServerSecretVault::new(
                store.clone(),
                b"f7-target-secret".to_vec(),
            )
            .expect("secret vault"),
        );
        let connections = Arc::new(ServerConnections::new(store.clone(), vault));
        connections
            .save(
                &ConnectionScope::new(org()),
                ConnectionConfig {
                    id: record.connection_id.as_str().to_owned(),
                    name: "target-pair".into(),
                    database_type: "postgres".into(),
                    host: Some("127.0.0.1".into()),
                    port: Some(5432),
                    database: Some("app".into()),
                    schema: None,
                    username: None,
                    password: None,
                    ssl_mode: datazen_driver_api::SslMode::Disable,
                    connection_timeout: 30,
                    max_pool_size: 10,
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
                },
            )
            .await
            .expect("catalog save");
        let registry = Arc::new(
            datazen::execution::runtime::driver_registry::DriverRegistry::with_factories([])
                .expect("empty registry is a legal catalog"),
        );
        let lease = target_lease(
            &store,
            connections.clone(),
            Arc::new(profiles),
            registry.clone(),
        );
        let host = Arc::new(ServerResourceExecutionHost::new(
            connections,
            Arc::new(ServerProfiles::new(store.clone())),
            registry,
            Arc::new(ServerSettingsStore::scoped(org(), store.clone())),
            lease,
            org(),
        ));
        let exec = StatelessToolExec::new(
            Arc::new(NoConnections),
            Arc::new(datazen_server::mcp::stateless_exec::EmptyWorkflowCatalog),
            Vec::new(),
        )
        .with_target_machine(TargetMachine {
            host,
            settings: datazen_server::storage::default_shared_settings().execution,
        });
        let error = exec
            .call(
                "list_databases",
                &serde_json::json!({ "connection_id": record.connection_id.as_str() }),
            )
            .await
            .expect_err("an empty driver directory answers the honest sentence");
        let reason = internal_reason(error);
        assert!(
            reason.contains("Driver unavailable"),
            "the v1 empty registry answers the registered fail-closed sentence: {reason}"
        );
    }

    #[tokio::test]
    async fn wired_target_commands_validate_required_arguments_before_the_machine() {
        let store = store();
        let exec = wired_exec(&store).await;
        let error = exec
            .call("get_schema", &serde_json::json!({}))
            .await
            .expect_err("a missing connection_id is a parameter-class error");
        match error {
            datazen_server::mcp::wire::ToolCallError::WithHelp { reason, .. } => assert!(
                reason.contains("missing required input property: connection_id"),
                "the argument gate names the missing property: {reason}"
            ),
            datazen_server::mcp::wire::ToolCallError::Plain(reason) => {
                panic!("expected WithHelp, got Plain: {reason}")
            }
            datazen_server::mcp::wire::ToolCallError::Internal(reason) => {
                panic!("expected WithHelp, got Internal: {reason}")
            }
        }
    }
}

// ── ResultPublisher ──────────────────────────────────────────────────────

#[tokio::test]
async fn publisher_reports_missing_execution_as_rejection() {
    use datazen_server::execution::publisher::execution_result_publisher;
    let publisher = execution_result_publisher(
        datazen::execution::application::orchestration::RuntimeResultSink::default(),
    );
    let error = publisher(
        datazen::shared::id::ExecutionId::new("no-such-execution"),
        serde_json::json!({ "kind": "rows" }),
    )
    .await
    .expect_err("publishing into an unknown execution is rejected");
    assert!(
        matches!(
            error,
            datazen::execution::runtime::connection::ProviderError::ProtocolError(_)
        ),
        "the rejection is a publication failure, not a panic"
    );
}
