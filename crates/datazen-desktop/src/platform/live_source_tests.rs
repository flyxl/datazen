//! 出向端口接线**真的生效**的证据。
//!
//! ## 为什么「构造函数被调用过」不算证据
//!
//! 只断言 `DesktopLiveConnectionSource::new(..)` 返回了某个值，那么把 `open_live`
//! 改成 `Err(Refused(..))` 测试照样全绿——端口仍然一根线都没通。所以每条用例都经
//! **真实执行路径**走一遍：真的 `open_session`、真的 `borrow_live_connection`，
//! 然后看**端口被问了什么**、**拿回来的是什么**。
//!
//! ## 三条判据，缺一条都不算「真的接上了」
//!
//! 1. **端口真的被查询**：由 [`RecordingSource`] 的调用计数证明（不是构造计数）。
//! 2. **借到的是那一份活资源**：两个 `ConnectionHandle` 路由字段必须与 provider
//!    `connection_handle()` 返回的句柄一致。测试 provider 返回刻意不同于 Runtime
//!    `resource_key` / `provider_id` 的字段，确保宿主调用的是经过校验的 provider 接口。
//! 3. **没有新建物理连接**：借出前后 `acquire_resource` 的调用次数**不变**。
//!    这是端口契约「实现方不得在此新建物理连接」唯一可测的形式。
//!
//! ## 替身放哪、不替谁
//!
//! [`RecordingSource`] 包装的是**真的** [`DesktopLiveConnectionSource`]，只记录后
//! 转发；被替换的只有 `DriverRegistry` 里的 provider（它本来就是一个端口），
//! 而 provider 的 `acquire_resource` 仍会真的走一遍（含预算记账）。因此断言测到的
//! 是桌面适配器本身的效果，不是一个替身的效果。
//!
//! ## 不伪造 `Verified`
//!
//! 本文件的 provider 对 `reset_resource` 一律返回 `Discard`，与 15 个真实驱动的
//! 实测一致（`ResetForReuse::Unsupported`）。没有任何一处为了「让池看起来可用」
//! 而谎报 `Clean`——首版空闲池在真实驱动上不可达是**正确行为**。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, SessionContinuity,
};
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::namespace::{NamespaceShape, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, ResourceDescriptor, ResourceError, ResourceHandle, ResourceProvider,
    ResultSink, ReusePolicy,
};
use datazen_driver_api::session::{
    CancelReceipt, CloseDisposition, ContextChangeDisposition, ExecutionCompletion,
    ResetDisposition, SessionHandleRef, SessionObservation, TransactionObservation,
    TransactionOptions,
};
use datazen_driver_api::{
    ConnectionConfig, DatabaseDriver, DriverError, QueryExecutionId, SslMode,
};

use super::DesktopLiveConnectionSource;

// ---------------------------------------------------------------------------
// 驱动侧替身：真实实现 ResourceProvider，且把每次 acquire 记下来
// ---------------------------------------------------------------------------

/// 记账型 provider。`resource_key` 形如 `{connectionId}#{seq}`，于是「端口交出的
/// 连接是哪一份、是不是新开的那份」可以直接从字符串读出来。
struct CountingProvider {
    driver_id: String,
    acquires: AtomicU64,
    /// `reset_resource` 被调了几次。用于钉住「借出不触发复位」。
    resets: AtomicU64,
}

impl CountingProvider {
    fn new(driver_id: &str) -> Arc<Self> {
        Arc::new(Self {
            driver_id: driver_id.to_owned(),
            acquires: AtomicU64::new(0),
            resets: AtomicU64::new(0),
        })
    }

    fn acquires(&self) -> u64 {
        self.acquires.load(Ordering::SeqCst)
    }

    fn resets(&self) -> u64 {
        self.resets.load(Ordering::SeqCst)
    }
}

/// 本夹具**唯一**声明的能力：`stateful_session`。
///
/// 为什么必须声明：`session_backend::open` 对 `OwnerRef::Editor` 会调
/// `require_stateful_session()`，不声明就拿不到 `CapabilityUnsupported`
/// 之外的任何机会——这条用例测的是出向端口，不是能力门禁。
///
/// 为什么只声明它：`CapabilitySet::default()` 把其余 11 格全留在**非支持的
/// 默认值**上，因此这个夹具不谎报任何它没有的能力。特别是
/// `reset_for_reuse` 保持 `Unsupported`——与 15 个真实驱动的实测一致，
/// 也正因如此首版空闲池在真实驱动上不可达是**正确行为**，
/// 不能为了让池「看起来可用」而在夹具里把它改成 `Verified`。
fn wiring_capabilities(driver_id: &str) -> CapabilityRegistry {
    let mut set = CapabilitySet::default();
    set.stateful_session = Availability::Supported;
    let mut registry = CapabilityRegistry::new(
        driver_id.to_owned(),
        datazen_driver_api::capabilities::CapabilitySnapshot::new(
            driver_id,
            "test",
            datazen_driver_api::PROTOCOL_VERSION,
            0,
        ),
    );
    registry.capabilities = set;
    registry
}

#[async_trait]
impl ResourceProvider for CountingProvider {
    fn provider_id(&self) -> &str {
        &self.driver_id
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        // `capabilities()` 返回引用，而本文件每次 new 一个 provider。这里把 registry
        // 搬进 Box 后泄漏成 `&'static`：测试进程短命，泄漏一个结构不是问题，
        // 也不引入全局态（每个 provider 拿到自己那一份，互不串台）。
        Box::leak(Box::new(wiring_capabilities(&self.driver_id)))
    }

    fn namespace_shape(&self) -> &NamespaceShape {
        Box::leak(Box::new(NamespaceShape::default()))
    }

    fn connection_handle(
        &self,
        handle: &ResourceHandle,
    ) -> Result<datazen_driver_api::ConnectionHandle, ResourceError> {
        Ok(datazen_driver_api::ConnectionHandle {
            id: format!("driver:{}", handle.resource_key()),
            pool_id: format!("pool:{}", handle.provider_id()),
        })
    }

    async fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError> {
        Ok(ResourceDescriptor {
            provider_id: self.driver_id.clone(),
            resource_key: format!("{}:key", request.connection_config.id),
            session_continuity: SessionContinuity::Unknown,
            reuse_policy: ReusePolicy::Unknown,
            initialization_requirements: Vec::new(),
            connection_cost_policy: ConnectionCostPolicy::DeclaredConservative {
                declared_cost: 1,
                hard_cap: None,
            },
            namespace_shape: NamespaceShape::default(),
        })
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        let permit = budget.acquire_physical_connections(1).await?;
        let seq = self.acquires.fetch_add(1, Ordering::SeqCst);
        let handle = ResourceHandle::issue(
            self.driver_id.clone(),
            format!("{}#{}", request.connection_config.id, seq),
            1,
        );
        // permit 必须活到 close 才释放；本文件的用例不显式关会话，它跟着 Arc 一起
        // 结束。显式 forget 以免制造「permit 已归还」的假信号——那会让断言读到的
        // 计数失去意义（是 permit 先没了，还是根本没新建，两种对不上号）。
        std::mem::forget(permit);
        Ok(handle)
    }

    async fn execute_on_resource(
        &self,
        _handle: &ResourceHandle,
        _execution_id: &QueryExecutionId,
        _call: &CommandCall,
        _sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn observe_session(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        Ok(SessionObservation::unobservable())
    }

    async fn change_context(
        &self,
        _handle: &ResourceHandle,
        _desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn begin_transaction(
        &self,
        _handle: &ResourceHandle,
        _options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn commit_transaction(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn rollback_transaction(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn request_cancel(
        &self,
        _handle: &ResourceHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    /// 一律 `Discard`——与 15 个真实驱动一致。这里**不**返回 `Clean`。
    async fn reset_resource(
        &self,
        _handle: &ResourceHandle,
        _baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.resets.fetch_add(1, Ordering::SeqCst);
        Ok(ResetDisposition::Discard)
    }

    async fn close_resource(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        Ok(CloseDisposition::Closed)
    }

    /// 协议 5 新增的必填方法。这里**显式拒绝**，且理由取自本 provider 自己的实现，
    /// 不是从任何驱动抄来的。
    ///
    /// 它不发会话级句柄，而且是**结构性**不发：`execute_on_resource` 恒返回
    /// `Err(Unsupported("not exercised"))`，`ExecutionCompletion` 在这个夹具里
    /// 从来没有被构造过一次，因此不存在 `session_handles` 列表——不是空列表，
    /// 是根本没有列表。与之相应，`capabilities()` 只覆盖
    /// `stateful_session` 一格，`session_scoped_handles` 留在
    /// `CapabilitySet::default()` 的 fail-closed `Unknown` 上（**不是**
    /// `Unsupported`，见 `wiring_capabilities` 的注释：只声明本文件真正用到的那一格）。
    ///
    /// 返回 `Ok(())` 在这里是宿主无从察觉的谎言：宿主会注销该登记并判定会话干净，
    /// 而 driver 侧自始至终没有关过任何东西——§6.5 要防的正是这一种失败。
    ///
    /// 与 basic 4 驱动的两处**有意**不同，差别都源于本夹具自身的性质：
    ///
    /// 1. 不做 `SessionHandleRef::belongs_to` 归属校验。该校验保护的是
    ///    「不去关**别的**资源上的句柄」；本 provider 不发任何句柄，没有可供误关的
    ///    对象，所以它在本实现里恒为真，写下来只是噪声。
    /// 2. 不做 `handle.is_closed()` → `Ok(())` 的幂等成功分支。真实驱动在
    ///    `close_resource` 确认关闭时会 `mark_closed()`，该分支才可达；而上面的
    ///    `close_resource` 直接返回 `Closed` 且**从不** `mark_closed()`，
    ///    于是本 provider 发出的句柄的 `is_closed()` 恒为 `false`，写下这个分支
    ///    就是不可达的死代码——注释再解释它为什么重要，也改变不了它永不执行的事实。
    async fn close_handle(
        &self,
        _handle: &ResourceHandle,
        session_handle: &SessionHandleRef,
    ) -> Result<(), ResourceError> {
        Err(ResourceError::unsupported(
            self.driver_id.as_str(),
            "close_handle",
            format!(
                "CountingProvider issues no session-scoped handles: execute_on_resource returns \
                 Err(\"not exercised\") before any ExecutionCompletion is ever built, so no \
                 session_handles list exists and handle '{}' ({:?}) names nothing this provider \
                 can close; session_scoped_handles stays at the fail-closed default Unknown",
                session_handle.handle_id, session_handle.kind
            ),
        ))
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

/// 出向端口的观测包装：数调用、记请求，然后**真的**转给桌面实现。
struct RecordingSource {
    inner: DesktopLiveConnectionSource,
    calls: Mutex<Vec<(String, u64, String)>>,
}

impl RecordingSource {
    fn call_log(&self) -> Vec<(String, u64, String)> {
        lock(&self.calls).clone()
    }
}

#[async_trait]
impl datazen::execution::application::LiveConnectionSource for RecordingSource {
    async fn open_live(
        &self,
        request: datazen::execution::application::LiveConnectionRequest,
    ) -> Result<
        datazen::execution::application::LiveConnection,
        datazen::execution::application::LiveConnectionError,
    > {
        lock(&self.calls).push((
            request.handle.db_session_id.as_str().to_owned(),
            request.handle.runtime_epoch.get(),
            request.connection_id.as_str().to_owned(),
        ));
        self.inner.open_live(request).await
    }
}

/// 测试用连接配置。`database_type` 必须是本进程已注册的类型。
fn connection_config(id: &str, db_type: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.into(),
        name: id.into(),
        database_type: db_type.into(),
        host: None,
        port: None,
        database: Some("app".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::default(),
        connection_timeout: 5,
        max_pool_size: 1,
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

/// 落一份连接配置，让 `DesktopRepository::platform_profile` 读得到。
///
/// 走 `save_connection` 而不是手写 `connections.json`：前者是本进程真实的持久化
/// 路径，绕开它就绕开了「runtime 读到的 revision 与宿主写下的 revision 是同一条」
/// 这一层。首次保存时 `ProfileMetadata::default()` 给 `config_revision = 1`，
/// 因此 runtime 侧 `profile()` 读到 1——测试用 1 即可，不需要魔数。
async fn seed_profile(store: &crate::store::DesktopRepository, config: &ConnectionConfig) {
    store
        .save_connection(config.clone())
        .await
        .expect("profile seed saves");
}

// ---------------------------------------------------------------------------
// 用例
// ---------------------------------------------------------------------------

/// 主机装好后，`open_session` + `borrow_live_connection` 一路走到端口，
/// 并且拿回来的就是**后端当初开出来的那一份**活资源。
///
/// 三条判据在这条里同时成立：端口被调用过（计数）、`ConnectionHandle` 的字段
/// 等于 provider 校验后返回的驱动句柄（身份）、`acquire_resource`
/// 次数没变（没有新建物理连接）。任何一条不成立都说明接线只是摆设。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_wiring_a_borrow_really_reaches_the_port_and_hands_back_that_live_resource() {
    use datazen::application::sessions::ConnectionUseCases;
    use datazen::{
        execution::application::idempotency::IdempotentOperation,
        shared::context::{OwnerRef, RequestContext},
        shared::id::*,
    };
    use datazen_driver_api::DatabaseDriverFactory;

    let temp = tempfile::tempdir().expect("temp dir");
    let store = Arc::new(
        crate::store::DesktopRepository::init_with_path(
            temp.path(),
            crate::store::KeyBackend::File,
        )
        .await
        .expect("store init"),
    );
    seed_profile(&store, &connection_config("conn-1", "test-live")).await;

    // 一个 provider + 一个 driver 的工厂。`resource_provider()` 返回的 provider
    // 与 `create()` 返回的 driver 是**同一个进程内的两个对象**，provider 的
    // `acquire_resource` 由它自己记账——所以「后端开了几次」与「provider 被 acquire
    // 了几次」是同一件事的两个视角。
    struct Factory {
        provider: Arc<CountingProvider>,
        driver: Arc<MockDriver>,
    }
    impl DatabaseDriverFactory for Factory {
        fn create(&self) -> Arc<dyn DatabaseDriver> {
            Arc::clone(&self.driver) as Arc<dyn DatabaseDriver>
        }
        fn driver_id(&self) -> &'static str {
            "test-live"
        }
        fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
            Some(Arc::clone(&self.provider) as Arc<dyn ResourceProvider>)
        }
    }

    let provider = CountingProvider::new("test-live-provider");
    let factory: &'static Factory = Box::leak(Box::new(Factory {
        provider: Arc::clone(&provider),
        driver: MockDriver::new("test-live", MockDriverOptions::default()),
    }));
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    // 驱动先注册：`ensure_type` 见 `drivers` 里已有就直接返回，不再走 inventory
    // 扫描（`Box::leak` 出来的工厂不进 inventory，扫不到）。factory 仍要注册——
    // `resource_provider` 只认 factories 这张表，driver 本身不带 provider。
    registry
        .register_test_driver(
            "test-live",
            Arc::clone(&factory.driver) as Arc<dyn DatabaseDriver>,
        )
        .await;
    registry.register_test_factory("test-live", factory).await;

    let tunnels: Arc<dyn datazen::tunnel::TunnelRepository> = store.clone();
    let known_hosts = super::super::known_hosts::desktop_known_hosts_repository(store.data_dir());
    // D6 第一切片：会话后端的查询历史写端口——同一 `HistoryDb` 的模块适配器。
    let history: Arc<dyn datazen::history::QueryHistoryRepository> = Arc::new(
        super::super::history::DesktopHistoryRepositories::new(store.history_db()),
    );
    let backend = Arc::new(super::super::session_backend::DesktopSessionBackend::new(
        Arc::clone(&registry),
        Arc::clone(&store),
        8,
        tunnels,
        known_hosts,
        history,
    ));
    let recording = Arc::new(RecordingSource {
        inner: DesktopLiveConnectionSource::new(
            Arc::clone(&registry),
            Arc::clone(&store),
            backend.clone(),
        ),
        calls: Mutex::new(Vec::new()),
    });

    // 走**真的** runtime 用例：不直接调 DesktopLiveConnectionSource。
    let service = Arc::new(
        datazen::execution::application::RuntimeConnectionUseCases::new(
            Arc::new(super::super::repositories::DesktopProfiles::new(
                store.clone(),
            )),
            Arc::new(super::super::policy::DesktopPolicy::new(store.clone())),
            backend.clone(),
        ),
    );
    service.with_live_connections(recording.clone());

    // `DesktopPolicy` 只放行**本机桌面身份**（见 policy.rs）。这里的主体直接取
    // `local_principal()`——本进程解析出来的桌面主体，与策略判定用的是同一个值。
    //
    // 之前这里注入 `DATAZEN_DESKTOP_USER=wiring-tester`（并在用例结束时用
    // `DesktopUserGuard` 还原），那是本文件唯一的**进程级 env 写者**：`DATAZEN_DESKTOP_USER`
    // 是**进程级可变状态**，而授权判定要读它。于是「捕获主体」与「判定授权」之间一旦
    // 撞上这次写入/还原，两个读到的值就不一致 ⇒ 合法的本机请求被翻成
    // `desktop_identity_required`。并行跑兄弟用例时这条随时会被踩中（CI 里
    // `platform::live_source::tests::*` 的红就是它）。
    //
    // 主体改为取进程解析值之后，本文件**不再有任何进程级 env 写者**；桌面主体本身
    // 由 `local_principal()` 在进程内只解析一次（见 identity.rs），授权基线恒定。
    let principal = crate::platform::local_principal()
        .expect("a desktop local principal exists in the test process");

    let ctx = RequestContext::new(
        OrganizationId::new(super::super::LOCAL_ORGANIZATION_ID),
        PrincipalId::new(principal.as_str()),
        None,
        ClientInstanceId::new("client"),
        RequestId::new("request"),
        None,
    );
    let token = service
        .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
        .await
        .expect("token issues");
    let opened = service
        .open_session(
            &ctx,
            datazen::execution::application::requests::OpenSessionRequest::new(
                datazen::execution::target::ExecutionTarget::new(
                    datazen::shared::id::ConnectionId::new("conn-1"),
                    datazen::execution::target::NamespaceTarget::database("app"),
                ),
                OwnerRef::Editor {
                    client_instance_id: ctx.client_instance_id.clone(),
                    editor_session_id: EditorSessionId::new("editor-1"),
                },
                token.idempotency_key,
            ),
        )
        .await
        .expect("session opens");
    let handle = opened.session.handle.clone();
    let acquires_after_open = provider.acquires();

    let lease = service
        .borrow_live_connection(
            &ctx,
            &handle,
            datazen::execution::application::LivePurpose::CommandExecution,
        )
        .await
        .expect("port must lend a live connection");

    // 判据 1：端口真的被查询了，且问的是这个会话、这个连接。
    assert_eq!(
        recording.call_log(),
        vec![(
            handle.db_session_id.as_str().to_owned(),
            epoch_number(&handle),
            "conn-1".to_owned(),
        )],
        "端口必须被真的查询一次，且入参就是 runtime 带给它的那个 handle"
    );

    // 判据 2：拿回来的就是那份活资源，不是新开的、也不是编的。
    let lent = lease.connection().expect("fresh borrow must be usable");
    assert_eq!(
        lent.handle().id,
        format!("driver:conn-1#{}", acquires_after_open - 1),
        "id 必须由 provider 从它持有的资源句柄解析"
    );
    assert_eq!(
        lent.handle().pool_id,
        "pool:test-live-provider",
        "pool_id 必须由 provider 从它持有的资源句柄解析"
    );

    // 判据 3：没有新建物理连接。
    assert_eq!(
        provider.acquires(),
        acquires_after_open,
        "借出不得新建物理连接——端口契约禁止，且这是接线诚意的唯一可测形式"
    );
    assert_eq!(
        provider.resets(),
        0,
        "借出也不得触发复位：复位归归池路径管，不归借出路径管"
    );

    let released = lease.release();
    assert!(!released.revoked, "新借出的会话不该是被作废的");
}

// ---------------------------------------------------------------------------
// Runtime 用例（per-database 池维护）共享装配
// ---------------------------------------------------------------------------

/// 真 runtime 用例 + 真 session 的最小装配，供 per-database 池维护用例测试共享。
///
/// 与上一条接线测试同一条装配骨架（临时库 + profile、provider+driver 测试工厂、
/// 桌面会话后端、真 `RuntimeConnectionUseCases` 并注入 live 源），只是把 open 出的
/// `dbSessionId` 一起交回。driver 选项由调用方给，让断言贴着被测行为走。
struct PoolMaintenanceWiring {
    service: Arc<datazen::execution::application::RuntimeConnectionUseCases>,
    driver: Arc<MockDriver>,
    ctx: datazen::shared::context::RequestContext,
    db_session_id: String,
}

async fn wired_pool_maintenance_harness(driver_opts: MockDriverOptions) -> PoolMaintenanceWiring {
    use datazen::application::sessions::ConnectionUseCases;
    use datazen::{
        execution::application::idempotency::IdempotentOperation,
        shared::context::{OwnerRef, RequestContext},
        shared::id::*,
    };
    use datazen_driver_api::DatabaseDriverFactory;

    let temp = tempfile::tempdir().expect("temp dir");
    let store = Arc::new(
        crate::store::DesktopRepository::init_with_path(
            temp.path(),
            crate::store::KeyBackend::File,
        )
        .await
        .expect("store init"),
    );
    seed_profile(&store, &connection_config("pool-conn", "test-live")).await;

    struct Factory {
        driver: Arc<MockDriver>,
        provider: Arc<CountingProvider>,
    }
    impl DatabaseDriverFactory for Factory {
        fn create(&self) -> Arc<dyn DatabaseDriver> {
            Arc::clone(&self.driver) as Arc<dyn DatabaseDriver>
        }
        fn driver_id(&self) -> &'static str {
            "test-live"
        }
        fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
            Some(Arc::clone(&self.provider) as Arc<dyn ResourceProvider>)
        }
    }

    let driver = MockDriver::new("test-live", driver_opts);
    let factory: &'static Factory = Box::leak(Box::new(Factory {
        driver: Arc::clone(&driver),
        provider: CountingProvider::new("test-live-provider"),
    }));
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    registry
        .register_test_driver("test-live", Arc::clone(&driver) as Arc<dyn DatabaseDriver>)
        .await;
    registry.register_test_factory("test-live", factory).await;

    // live 源与 runtime 用例**共享同一个**会话后端：session 行只登记在一处，
    // 分开建的两个比如同两个 runtime，一边打开另一边永远 SessionNotFound。
    let tunnels: Arc<dyn datazen::tunnel::TunnelRepository> = store.clone();
    let known_hosts = super::super::known_hosts::desktop_known_hosts_repository(store.data_dir());
    // D6 第一切片：会话后端的查询历史写端口——同一 `HistoryDb` 的模块适配器。
    let history: Arc<dyn datazen::history::QueryHistoryRepository> = Arc::new(
        super::super::history::DesktopHistoryRepositories::new(store.history_db()),
    );
    let backend = Arc::new(super::super::session_backend::DesktopSessionBackend::new(
        Arc::clone(&registry),
        Arc::clone(&store),
        8,
        tunnels,
        known_hosts,
        history,
    ));
    let recording = Arc::new(DesktopLiveConnectionSource::new(
        Arc::clone(&registry),
        Arc::clone(&store),
        Arc::clone(&backend),
    ));

    let service = Arc::new(
        datazen::execution::application::RuntimeConnectionUseCases::new(
            Arc::new(super::super::repositories::DesktopProfiles::new(
                store.clone(),
            )),
            Arc::new(super::super::policy::DesktopPolicy::new(store.clone())),
            backend,
        ),
    );
    service.with_live_connections(recording.clone());

    // 桌面身份直接取 `local_principal()` 的进程值：它是**进程内只解析一次**的主体，
    // 与策略判定读的是同一个值 ⇒ 「捕获主体」与「判定授权」之间不可能漂移。
    //
    // 本用例族过去栽在这里：本文件另一处用例在运行期写 `DATAZEN_DESKTOP_USER`
    // 并在结束时还原（本进程唯一的 env 写者），撞上这个窗口时，策略读到的主体
    // 与这里捕获的主体不等 ⇒ 合法的本机请求被判 `desktop_identity_required`。
    // 写者已移除，主体改为进程解析值，窗口随之消失。
    let principal = crate::platform::local_principal()
        .expect("a desktop local principal exists in the test process");

    let ctx = RequestContext::new(
        OrganizationId::new(super::super::LOCAL_ORGANIZATION_ID),
        PrincipalId::new(principal.as_str()),
        None,
        ClientInstanceId::new("client"),
        RequestId::new("request"),
        None,
    );
    let token = service
        .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
        .await
        .expect("token issues");
    let opened = service
        .open_session(
            &ctx,
            datazen::execution::application::requests::OpenSessionRequest::new(
                datazen::execution::target::ExecutionTarget::new(
                    datazen::shared::id::ConnectionId::new("pool-conn"),
                    datazen::execution::target::NamespaceTarget::database("app"),
                ),
                OwnerRef::Editor {
                    client_instance_id: ctx.client_instance_id.clone(),
                    editor_session_id: EditorSessionId::new("editor-pool"),
                },
                token.idempotency_key,
            ),
        )
        .await
        .expect("session opens");
    PoolMaintenanceWiring {
        service,
        driver,
        ctx,
        db_session_id: opened.session.handle.db_session_id.as_str().to_owned(),
    }
}

/// runtime 拥有 per-database 池维护的可测形式：宿主只交 session + database 名，
/// 驱动真的被问到，drv 的裁定原样回到调用方。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_database_routes_the_session_pool_demand_to_the_driver() {
    let wiring = wired_pool_maintenance_harness(MockDriverOptions {
        has_schema_level: true,
        ..MockDriverOptions::default()
    })
    .await;

    let closed = wiring
        .service
        .close_database(&wiring.ctx, &wiring.db_session_id, "app")
        .await
        .expect("close_database succeeds");
    assert!(closed, "verdict comes from the driver, not a default body");
    assert_eq!(
        wiring.driver.close_database_calls(),
        vec!["app".to_owned()],
        "the driver was asked once, with the trimmed database name"
    );
}

/// 未迁移驱动（无 per-database 资源）在 runtime 用例下依然是普通结果：默认体
/// `Ok(false)`，不报错、不触驱动失败路径。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_database_treats_no_per_database_resource_as_a_normal_outcome() {
    let wiring = wired_pool_maintenance_harness(MockDriverOptions::default()).await;

    let closed = wiring
        .service
        .close_database(&wiring.ctx, &wiring.db_session_id, "app")
        .await
        .expect("no resource to close is a normal outcome");
    assert!(!closed);
    assert_eq!(
        wiring.driver.close_database_calls(),
        vec!["app".to_owned()],
        "the demand still reaches the driver"
    );
}

/// open_databases 逐字带回驱动报告的池列表；这是导航树「已打开」标记的唯一来源。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_databases_reports_the_driver_pool_list_verbatim() {
    let wiring = wired_pool_maintenance_harness(MockDriverOptions {
        open_databases: vec!["aux".to_owned(), "app".to_owned()],
        ..MockDriverOptions::default()
    })
    .await;

    let open = wiring
        .service
        .open_databases(&wiring.ctx, &wiring.db_session_id)
        .await
        .expect("open_databases succeeds");
    assert_eq!(open, vec!["aux".to_owned(), "app".to_owned()]);
}

/// 把会话自己的库空兜住：名字不合法在本用例边界就拒绝，不触驱动。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_database_rejects_an_empty_database_name_before_touching_the_driver() {
    let wiring = wired_pool_maintenance_harness(MockDriverOptions::default()).await;

    let error = wiring
        .service
        .close_database(&wiring.ctx, &wiring.db_session_id, "   ")
        .await
        .expect_err("empty name is refused");
    use datazen::shared::error::ApiErrorCode;
    assert_eq!(error.code, ApiErrorCode::InvalidArgument);
    assert!(
        wiring.driver.close_database_calls().is_empty(),
        "the driver must not be asked for an invalid name"
    );
}

/// 把公开句柄里的 `runtime_epoch`（形如 `rte-7` 的字符串 id）还原成数字。
///
/// 不直接在断言里写 `7`：那个数字是 `SessionRegistry` 内部的 `epoch_seq`，
/// 与本文件无关，写死会让测试跟着 registry 的发号策略一起脆。解析而不是硬编码，
/// 也让「epoch 真的被端口看到了」这件事仍然可断言。
fn epoch_number(handle: &datazen::execution::SessionHandle) -> u64 {
    handle
        .runtime_epoch
        .as_str()
        .strip_prefix("rte-")
        .and_then(|rest| rest.parse::<u64>().ok())
        .expect("public runtime epoch must parse")
}
