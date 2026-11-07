//! IdleReclaimer server 接线烟测（设计档 `w8-idle-reclaimer-arming-design.md`
//! §8.2；IdleReclaimer 武装刀）。
//!
//! 覆盖：
//!
//! - t1 到期回收全链：短 deadline（50ms）+ 短 scan（20ms）→ 有界轮询目录
//!   注册位翻假（内核 `reclaim_idle` 驱逐 + 目录摘除）。
//! - t2 未到期不误伤：deadline 未过的会话在扫描后仍注册（负控制；在飞
//!   hold 语义归内核钉子 `p4_idle_activity.rs`，本文件不重复驱动器桩语义）。
//! - t3 吊销后跟随新实例（R5 红灯）：会话 A（实例一）→ `revoke()` →
//!   resolve 新实例 → 会话 B（实例二）→ 驱动器（spawn 于吊销**前**）必须
//!   回收 B——绑死旧实例的实现回收不到 B。
//! - t4 停车：`stop()` → 有界轮询 `is_finished() == true`（不泄漏 task）。
//! - t5 disabled 配置面：`(0, _) → disabled`、扫描间隔下限 60s、默认
//!   1800/60——config 映射单元断言（不等墙钟）。
//!
//! 夹具沿 cm63 真租约源装配模式（空驱动注册表；空闲回收不触驱动面）。

use std::sync::Arc;
use std::time::{Duration, Instant};

use datazen::connection::domain::catalog::ConnectionRepository as _;
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::domain::IdempotencyKey;
use datazen::execution::application::orchestration::{
    RuntimeConnectionUseCases, RuntimeResultSink,
};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::runtime::directory::InMemorySessionDirectory;
use datazen::execution::SessionDirectory as _;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen_driver_api::capabilities;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory};
use datazen_server::config::idle_policy_from_config;
use datazen_server::execution::idle_reclaimer::ServerIdleReclaimer;
use datazen_server::execution::lease::{RuntimeLeaseSource as _, ServerRuntimeLeaseSource};
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::policy::PolicyService;
use datazen_server::storage::{AdminStore, ServerConnections, ServerProfiles};

const SERVER_SECRET: &[u8] = b"idle-reclaimer-wiring-secret";

/// 短期限策略：deadline 50ms / scan 20ms（绕过 config 的 60s 下限——那是
/// 部署配置面的纪律；内核策略本身允许任意量级，桌面夹具同款）。
fn short_policy() -> datazen::execution::application::orchestration::IdlePolicy {
    datazen::execution::application::orchestration::IdlePolicy::with_deadline(
        Duration::from_millis(50),
        Duration::from_millis(20),
    )
}

fn context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("client-http"),
        RequestId::new("req-idle"),
        None,
    )
}

/// 真租约源（空驱动注册表——空闲回收不触驱动面）+ 同实例用例面。
struct Fixture {
    _store: Arc<AdminStore>,
    lease: Arc<ServerRuntimeLeaseSource>,
    cases: Arc<dyn ConnectionUseCases>,
    directory: Arc<InMemorySessionDirectory>,
    ctx: RequestContext,
}

/// 把 [`MockDriver`] 连同 [`LegacyResourceAdapter`] 包成静态工厂（cm63
/// `WireMockFactory` 同款；空闲回收的开会话路径要过驱动 open 闸）。
struct WireMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn DatabaseDriver>,
}

impl DatabaseDriverFactory for WireMockFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::clone(&self.driver)
    }

    fn driver_id(&self) -> &'static str {
        self.driver_id
    }

    fn resource_provider(&self) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
        let shape = datazen_driver_api::namespace::NamespaceShape {
            levels: vec![
                datazen_driver_api::namespace::NamespaceLevel::present(
                    datazen_driver_api::namespace::NamespaceLevelKind::Database,
                    false,
                ),
                datazen_driver_api::namespace::NamespaceLevel::absent(
                    datazen_driver_api::namespace::NamespaceLevelKind::Catalog,
                ),
                datazen_driver_api::namespace::NamespaceLevel::present(
                    datazen_driver_api::namespace::NamespaceLevelKind::Schema,
                    false,
                ),
            ],
            ..datazen_driver_api::namespace::NamespaceShape::default()
        };
        let mut capabilities = capabilities::CapabilitySet::default();
        capabilities.stateful_session = capabilities::Availability::Supported;
        capabilities.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
        capabilities.context_observation = capabilities::ContextObservation::Full;
        capabilities.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
        capabilities.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
        Some(Arc::new(LegacyResourceAdapter::new(
            Arc::clone(&self.driver),
            self.driver_id,
            "idle-wiring-test",
            1,
            shape,
            capabilities,
        )))
    }
}

async fn fixture(
    lease_policy: datazen::execution::application::orchestration::IdlePolicy,
) -> Fixture {
    let store = Arc::new(AdminStore::open_in_memory().expect("in-memory store"));
    store.migrate().expect("migrations apply");
    // 授权基线：org-1/user-1 是 OrgMember（进程内用例面无登录会话链）。
    datazen_server::policy::PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership(
            "org-1",
            "user-1",
            datazen_server::policy::Role::OrgMember,
            "idp:user-1",
        )
        .expect("membership");
    let vault = Arc::new(
        datazen_server::execution::secret_provider::ServerSecretVault::new(
            store.clone(),
            SERVER_SECRET.to_vec(),
        )
        .expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    // 驱动注册表：懒形 + 测试驱动 + 测试工厂（cm63 同款；空闲回收的开会话
    // 路径要过驱动 open 闸，空表会让 open 失败）。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let driver = MockDriver::new("mockdb", MockDriverOptions::default());
    registry
        .register_test_driver("mockdb", driver.clone())
        .await;
    registry
        .register_test_factory(
            "mockdb",
            Box::leak(Box::new(WireMockFactory {
                driver_id: "mockdb",
                driver: driver.clone(),
            })),
        )
        .await;
    let bridge = Arc::new(
        datazen_server::execution::budget_bridge::BudgetPortBridge::new(
            Arc::new(
                datazen_server::execution::budget::ServerBudgetCoordinator::new(
                    8,
                    10_000,
                    OrganizationId::new("org-1"),
                ),
            ),
            OrganizationId::new("org-1"),
        ),
    );
    let backend = Arc::new(
        datazen_server::execution::driver_exec::ServerDriverExecBackend::new(
            registry,
            connections,
            profiles.clone(),
            datazen::connection::domain::catalog::ConnectionScope::new("org-1"),
            bridge,
            None,
        ),
    );
    let policy = ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    );
    let directory = Arc::new(InMemorySessionDirectory::new());
    let lease = Arc::new(
        ServerRuntimeLeaseSource::new_with_shared_parts(
            profiles,
            Arc::new(policy),
            backend,
            RuntimeResultSink::default(),
            directory.clone(),
        )
        .with_idle_policy(lease_policy),
    );
    let cases: Arc<dyn ConnectionUseCases> = Arc::new(
        datazen_server::execution::lease_use_cases::LeaseConnectionUseCases::new(lease.clone()),
    );
    Fixture {
        _store: store,
        lease,
        cases,
        directory,
        ctx: context(),
    }
}

impl Fixture {
    async fn seed_and_open(&self, idem: &str) -> datazen::execution::SessionHandle {
        use datazen::connection::domain::{NamespaceTarget, ProfileDraft};
        let profiles = ServerProfiles::new(self._store.clone());
        let record = profiles
            .create(
                &self.ctx,
                ProfileDraft::new("idle-conn", "mockdb"),
                &IdempotencyKey::new(idem),
            )
            .await
            .expect("profile create");
        // 连接配置行（同一连接 id 两半都齐——profile 行 + 连接目录行，
        // cm63 同款；缺半 open 的闸序不可达驱动层）。
        let connections = ServerConnections::new(
            self._store.clone(),
            Arc::new(
                datazen_server::execution::secret_provider::ServerSecretVault::new(
                    self._store.clone(),
                    SERVER_SECRET.to_vec(),
                )
                .expect("secret vault"),
            ),
        );
        let scope = datazen::connection::domain::catalog::ConnectionScope::new("org-1");
        connections
            .save(
                &scope,
                datazen_driver_api::ConnectionConfig {
                    id: record.connection_id.as_str().to_owned(),
                    name: "idle conn".into(),
                    database_type: "mockdb".into(),
                    host: Some("localhost".into()),
                    port: Some(1),
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
            .expect("connection config saved");
        let token = self
            .cases
            .issue_submission_token(
                &self.ctx,
                datazen::execution::IdempotentOperation::OpenSession,
                None,
            )
            .await
            .expect("open token");
        let receipt = self
            .cases
            .open_session(
                &self.ctx,
                datazen::execution::application::requests::OpenSessionRequest::new(
                    datazen::execution::ExecutionTarget::new(
                        record.connection_id.clone(),
                        NamespaceTarget::database("app"),
                    ),
                    datazen::shared::context::OwnerRef::ClientSession {
                        client_instance_id: ClientInstanceId::new("client-http"),
                        purpose: "idle-wiring".to_owned(),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .expect("open session");
        receipt.session.handle
    }

    async fn is_registered(&self, handle: &datazen::execution::SessionHandle) -> bool {
        // 回收的观测面 = 用例层 get_session（runtime 内部注册表摘除 →
        // 会话不可用）；**不用**目录 lookup——共享目录条目的清扫是设计档
        // Q4 登记的已知产线缺口（本刀不做，不拿它当判据）。
        self.cases
            .get_session(&self.ctx, handle.clone())
            .await
            .is_ok()
    }
}

/// 有界轮询（5s 上限；20ms 步进）——不赌固定 sleep 轮次。
async fn wait_until(mut predicate: impl AsyncFnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if predicate().await {
            return;
        }
        assert!(Instant::now() < deadline, "bounded poll exhausted: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// t1 到期回收全链：短 deadline 会话在驱动器接管后被驱逐（目录摘除）。
#[tokio::test]
async fn t1_expired_session_is_reclaimed_end_to_end() {
    let fx = fixture(short_policy()).await;
    let handle = fx.seed_and_open("idem-t1").await;
    assert!(
        fx.is_registered(&handle).await,
        "session registered on open"
    );
    let _reclaimer = ServerIdleReclaimer::spawn(fx.lease.clone(), short_policy());
    let fx_ref = &fx;
    let handle_ref = &handle;
    wait_until(
        || async { !fx_ref.is_registered(handle_ref).await },
        "expired session leaves the directory",
    )
    .await;
}

/// t2 未到期不误伤：deadline 未过的会话在扫描轮次后仍注册（负控制）。
#[tokio::test]
async fn t2_session_within_deadline_survives_scans() {
    let fx = fixture(
        datazen::execution::application::orchestration::IdlePolicy::with_deadline(
            Duration::from_secs(1800),
            Duration::from_millis(20),
        ),
    )
    .await;
    // 长期限策略随实例装配——驱动器循环用短间隔（扫描频繁、期限未到）。
    let handle = fx.seed_and_open("idem-t2").await;
    let reclaimer = ServerIdleReclaimer::spawn(
        fx.lease.clone(),
        datazen::execution::application::orchestration::IdlePolicy::with_deadline(
            Duration::from_secs(1800),
            Duration::from_millis(20),
        ),
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        fx.is_registered(&handle).await,
        "session within deadline survives scans"
    );
    reclaimer.stop();
}

/// t3 吊销后跟随新实例（R5 红灯）：驱动器 spawn 于吊销前，必须回收
/// 吊销后在新实例上建的会话——绑死旧实例的实现回收不到它。
#[tokio::test]
async fn t3_reclaimer_follows_the_instance_across_revocation() {
    let fx = fixture(short_policy()).await;
    let _reclaimer = ServerIdleReclaimer::spawn(fx.lease.clone(), short_policy());

    // 实例一：会话 A（吊销后随实例作废）。
    let _handle_a = fx.seed_and_open("idem-t3a").await;
    fx.lease.revoke();
    // 吊销后 resolve 出新实例（cell 空 → 慢路重建），会话 B 落新实例。
    let _runtime_two = fx.lease.resolve().await.expect("resolve after revoke");
    let handle_b = fx.seed_and_open("idem-t3b").await;

    let fx_ref = &fx;
    let handle_b_ref = &handle_b;
    wait_until(
        || async { !fx_ref.is_registered(handle_b_ref).await },
        "session on the NEW instance gets reclaimed (reclaimer followed revocation)",
    )
    .await;
}

/// t4 停车：stop() 后下一轮循环边界退出，task 不泄漏。
#[tokio::test]
async fn t4_stop_terminates_the_loop_and_the_task_finishes() {
    let fx = fixture(short_policy()).await;
    let reclaimer = ServerIdleReclaimer::spawn(fx.lease.clone(), short_policy());
    reclaimer.stop();
    let reclaimer_ref = &reclaimer;
    wait_until(
        || async { reclaimer_ref.is_finished() },
        "reclaimer task finishes after stop()",
    )
    .await;
}

/// t5 disabled 配置面：`(0, _) → disabled`、默认 1800/60、扫描下限 60——
/// config 映射单元断言（不等墙钟）。
#[tokio::test]
async fn t5_config_mapping_disabled_and_defaults() {
    use datazen::execution::application::orchestration::IdlePolicy;

    // TTL=0 → disabled（deadline=None 是配置事实）。
    let disabled = idle_policy_from_config(0, 60);
    assert_eq!(disabled.deadline_ms(), None, "TTL=0 maps to disabled");
    // 默认映射：1800s deadline + 60s scan。
    let enabled = idle_policy_from_config(1800, 60);
    assert_eq!(enabled.deadline_ms(), Some(1_800_000));
    assert_eq!(enabled.scan_interval(), Duration::from_secs(60));
    // 非默认 TTL。
    let tuned = idle_policy_from_config(7200, 120);
    assert_eq!(tuned.deadline_ms(), Some(7_200_000));
    assert_eq!(tuned.scan_interval(), Duration::from_secs(120));
    // 与内核缺省同族 sanity：disabled 的 scan_interval 仍是合法正量。
    assert!(disabled.scan_interval() > Duration::ZERO);

    // config 解析面：TTL=0 合法（disabled）、扫描 <60 拒绝。
    let store_dir = std::env::temp_dir().join(format!(
        "idle-wiring-t5-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&store_dir).expect("temp dir");
    let db_path = store_dir.join("config.sqlite");

    // 扫描下限拒绝。
    // SAFETY（测试进程内一次性注入）：仓库既有 config 测试同款模式。
    std::env::set_var("DATAZEN_SERVER_DB", &db_path);
    std::env::set_var("DATAZEN_SERVER_RUNTIME_IDLE_SCAN_SECS", "30");
    let rejected = datazen_server::config::ServerConfig::from_env();
    assert!(
        rejected.is_err(),
        "scan interval below 60s must be rejected"
    );
    std::env::remove_var("DATAZEN_SERVER_RUNTIME_IDLE_SCAN_SECS");

    // TTL=0 接受（disabled 是配置事实）。
    std::env::set_var("DATAZEN_SERVER_RUNTIME_IDLE_TTL_SECS", "0");
    let accepted = datazen_server::config::ServerConfig::from_env();
    assert!(
        accepted.is_ok(),
        "TTL=0 (disabled) is a legal configuration fact: {:?}",
        accepted.err()
    );
    let config = accepted.expect("config");
    assert_eq!(config.runtime_idle_ttl_secs, 0);
    assert_eq!(config.runtime_idle_scan_secs, 60);
    std::env::remove_var("DATAZEN_SERVER_RUNTIME_IDLE_TTL_SECS");
    let _ = IdlePolicy::disabled();
    let _ = std::fs::remove_dir_all(store_dir);
}
