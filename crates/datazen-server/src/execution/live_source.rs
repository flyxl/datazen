//! W8 刀2：活连接**出向端口**的 server 适配器——把一个仍登记在 runtime
//! 台账里的会话，映射到它**已经开着**的那份物理资源。
//!
//! 镜像桌面 `platform/live_source.rs` 的四步与失败方向；server 侧的事实源
//! 替换如下（对应关系逐条登记，桌面参照面见设计档 `w8-knife2-live-handle-design.md`
//! §1.3）：
//!
//! | 步骤 | 桌面事实源 | server 事实源 |
//! | --- | --- | --- |
//! | ① 反查物理资源 | `DesktopSessionBackend::live_resource_for_session` | [`ServerDriverExecBackend::live_resource_for_session`]（`by_session` 反向索引在 `open_resource` 与资源登记**同帧**写入——server 无桌面的「绑定晚于 open」窗口，`backend.rs` 的 `db_session_id` 是唯一来源） |
//! | ② 权威 driver type | `store.platform_profile`（本地库） | `ConnectionRepository::get(&scope, id)`——profile 是唯一权威，取不到必须拒绝，不许「随便挑一个驱动」。server 与桌面的差异：适配器无请求主体可用（`LiveConnectionRequest` 不携带主体事实），组织/主体闸已由借出路径完成（`borrow_live_connection_by_id` 的 authorize + session_entry 过滤），故 scope 走构造注入的 v1 单组织裁决（与 [`ServerDriverExecBackend`] 同款） |
//! | ③ 取同一驱动 Arc | `DriverRegistry::get` | `DriverRegistry::get_sql_driver_by_name`（按名薄壳）——**同实例性质**是驱动工厂的义务（`factory.create()` 与 `factory.resource_provider()` 必须共享进程内单例），进测试钉住（t11） |
//! | ④ 借出句柄 | `Resource::driver_handle` | [`super::driver_exec::Resource::driver_handle`]（`ResourceProvider::connection_handle` 契约） |
//!
//! 端口契约（`orchestration/live.rs`）在本适配器的落地：**绝不新建物理
//! 连接**——第①步只读 `by_session` 反查 + `resources` 表，不存在 acquire
//! 调用路径；epoch 比对（INV-07）在第①步内完成；驱动加载（第③步）取的是
//! **驱动注册表项**的共享引用，不是物理连接（驱动内部的池子才是 socket 的
//! 家，而那个池子此刻已经开着上面那份资源）。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::connection::domain::catalog::{ConnectionRepository, ConnectionScope};
use datazen::execution::application::{
    LiveConnection, LiveConnectionError, LiveConnectionRequest, LiveConnectionSource,
};
use datazen::execution::runtime::driver_registry::DriverRegistry;

use super::driver_exec::ServerDriverExecBackend;

/// server 侧出向端口实现。三件协作者由 bootstrap 显式递交，`scope` 沿
/// [`ServerDriverExecBackend`] 的 v1 单组织裁决构造注入。
pub struct ServerLiveConnectionSource {
    /// 反查物理资源（`by_session` 反向索引 + epoch 比对）。必须与租约源
    /// 持有的 backend 是**同一** `Arc`——索引随资源登记写入，跨实例不可见。
    backend: Arc<ServerDriverExecBackend>,
    /// 权威 database_type 事实源（`ConnectionConfig`；与执行后端同一连接
    /// 目录实例）。
    connections: Arc<dyn ConnectionRepository>,
    /// 同实例驱动 `Arc`（registry 项的共享引用，非物理连接）。
    registry: Arc<DriverRegistry>,
    /// 连接目录 scope（v1 单组织；owner 不携带组织事实的端口缺口同款）。
    scope: ConnectionScope,
}

impl ServerLiveConnectionSource {
    /// 构造（bootstrap 装配入口）。
    pub fn new(
        backend: Arc<ServerDriverExecBackend>,
        connections: Arc<dyn ConnectionRepository>,
        registry: Arc<DriverRegistry>,
        scope: ConnectionScope,
    ) -> Self {
        Self {
            backend,
            connections,
            registry,
            scope,
        }
    }

    /// 解析这个会话**此刻**对应的权威 driver type。
    ///
    /// 走 `ConnectionRepository::get` 而不是 registry 的反查：会话的
    /// `connectionId` 是持久化配置的 id，driver type 只有配置里那一份是权威
    /// 的。取不到（miss 或目录故障）⇒ 会话已被删除/禁用，这时**必须**拒绝，
    /// 不允许回落到「随便挑一个驱动」。
    async fn driver_type_of(&self, connection_id: &str) -> Option<String> {
        self.connections
            .get(&self.scope, connection_id)
            .await
            .ok()
            .flatten()
            .map(|config| config.database_type.as_str().to_owned())
    }
}

#[async_trait]
impl LiveConnectionSource for ServerLiveConnectionSource {
    async fn open_live(
        &self,
        request: LiveConnectionRequest,
    ) -> Result<LiveConnection, LiveConnectionError> {
        let epoch = request.handle.runtime_epoch.get();
        let db_session_id = request.handle.db_session_id.as_str();

        // 1. 反查物理资源。句柄不认识 ⇒ 会话已关/已驱逐（或资源已真关）；
        //    epoch 不匹配 ⇒ 上一个纪元的残留行（INV-07）。两种都不新建、
        //    不透明替换。
        let resource = self
            .backend
            .live_resource_for_session(db_session_id, epoch)
            .await
            .ok_or_else(|| {
                LiveConnectionError::SessionLost(format!(
                    "no live physical resource is bound to session {db_session_id} at epoch {epoch}"
                ))
            })?;

        // 2. 解析权威 driver type。
        let driver_type = self
            .driver_type_of(request.connection_id.as_str())
            .await
            .ok_or_else(|| {
                LiveConnectionError::Refused(format!(
                    "connection profile {} is unavailable; refusing to guess a driver",
                    request.connection_id.as_str()
                ))
            })?;

        // 3. 取驱动引用。这一步可能真的加载驱动（首次），但加载的是**驱动
        //    注册表项**，不是物理连接：驱动内部的池子才是 socket 的家，而
        //    那个池子此刻已经开着上面那份资源。因此这一步不违反「不得新建
        //    物理连接」。
        let driver = self
            .registry
            .get_sql_driver_by_name(&driver_type)
            .await
            .ok_or_else(|| {
                LiveConnectionError::Refused(format!(
                    "driver {driver_type} is not loaded; refusing to substitute another driver"
                ))
            })?;

        // 4. Provider validates the live resource and lends its actual driver handle.
        let handle = resource.driver_handle().map_err(|_| {
            LiveConnectionError::Refused(
                "the runtime resource provider cannot lend its live driver handle".into(),
            )
        })?;
        Ok(LiveConnection::new(driver, handle))
    }
}

// ── t11 单元腿（桌面 `live_source_tests.rs` 同构）──────────────────────────
//
// 五条判据：反查 miss / epoch 漂移 → `SessionLost`；profile 缺席 / 驱动缺席
// → `Refused`；同实例钉（registry 两次取出的 `Arc` 指针相等）。装配沿
// `table_data_routes.rs` 的 `WireMockFactory` 词汇：真 SQLite 管理库 +
// MockDriver + `LegacyResourceAdapter` 资源提供者 + `ServerDriverExecBackend`。
// 会话→资源绑定由 backend.open 直接写入（`db_session_id` 同帧登记），无需
// 走 runtime 的授权链——被测面是适配器四步，不是借出路径。

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::connection::domain::profile::ProfileRepository as _;
    use datazen::connection::dto::ProfileDraft;
    use datazen::execution::runtime::connection::{ConfigRevision, DbSessionId, NamespaceTarget, OwnerRef};
    use datazen::execution::runtime::registry::backend::{OpenResource, SessionBackend as _};
    use datazen::shared::context::RequestContext;
    use datazen::shared::id::{
        ClientInstanceId, ConnectionId as PublicConnectionId, OrganizationId, PrincipalId,
        RequestId,
    };
    use datazen_driver_api::capabilities;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
    use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory, SslMode};
    use crate::execution::budget::ServerBudgetCoordinator;
    use crate::execution::budget_bridge::BudgetPortBridge;
    use crate::execution::secret_provider::ServerSecretVault;
    use crate::storage::{AdminStore, ServerConnections, ServerProfiles};

    /// 把 [`MockDriver`] 连同 [`LegacyResourceAdapter`] 包成静态工厂
    /// （`register_test_factory` 吃 `&'static dyn DatabaseDriverFactory`）。
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
            let mut caps = capabilities::CapabilitySet::default();
            caps.stateful_session = capabilities::Availability::Supported;
            caps.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
            caps.context_observation = capabilities::ContextObservation::Full;
            caps.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
            caps.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
            Some(Arc::new(LegacyResourceAdapter::new(
                Arc::clone(&self.driver),
                self.driver_id,
                "live-source-test",
                1,
                shape,
                caps,
            )))
        }
    }

    /// 四步适配器的最小装配（管理库行 + backend + 真资源一份）。
    struct Fixture {
        _dir: std::path::PathBuf,
        _store: Arc<AdminStore>,
        backend: Arc<ServerDriverExecBackend>,
        connections: Arc<ServerConnections>,
        registry: Arc<DriverRegistry>,
        _driver: Arc<MockDriver>,
        source: ServerLiveConnectionSource,
        scope: ConnectionScope,
        connection_id: String,
    }

    async fn fixture() -> Fixture {
        let dir: std::path::PathBuf =
            std::env::temp_dir().join(format!("dz-live-source-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
        store.migrate().expect("migrations apply v1..=v9");

        let ctx = RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("live-source-test"),
            RequestId::new("req-live-source"),
            None,
        );

        let registry = Arc::new(DriverRegistry::new());
        let driver = MockDriver::new("mockdb", MockDriverOptions::default());
        registry.register_test_driver("mockdb", driver.clone()).await;
        registry
            .register_test_factory(
                "mockdb",
                Box::leak(Box::new(WireMockFactory {
                    driver_id: "mockdb",
                    driver: driver.clone(),
                })),
            )
            .await;

        let vault = Arc::new(
            ServerSecretVault::new(store.clone(), b"live-source-secret".to_vec())
                .expect("secret vault"),
        );
        let connections = Arc::new(ServerConnections::new(store.clone(), vault));
        let profiles = Arc::new(ServerProfiles::new(store.clone()));

        let record = profiles
            .create(
                &ctx,
                ProfileDraft::new("live-conn", "mockdb"),
                &datazen::shared::id::IdempotencyKey::new("idem-live-source-profile"),
            )
            .await
            .expect("profile create");
        let connection_id = record.connection_id.as_str().to_owned();
        let scope = ConnectionScope::new("org-1");
        connections
            .save(
                &scope,
                datazen_driver_api::ConnectionConfig {
                    id: connection_id.clone(),
                    name: "live conn".into(),
                    database_type: "mockdb".into(),
                    host: Some("localhost".into()),
                    port: Some(1),
                    database: Some("app".into()),
                    schema: None,
                    username: None,
                    password: None,
                    ssl_mode: SslMode::Disable,
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

        let backend = Arc::new(ServerDriverExecBackend::new(
            registry.clone(),
            connections.clone(),
            profiles,
            scope.clone(),
            Arc::new(BudgetPortBridge::new(
                Arc::new(ServerBudgetCoordinator::new(
                    8,
                    10_000,
                    OrganizationId::new("org-1"),
                )),
                OrganizationId::new("org-1"),
            )),
            None,
        ));

        let source = ServerLiveConnectionSource::new(
            backend.clone(),
            connections.clone(),
            registry.clone(),
            scope.clone(),
        );

        Fixture {
            _dir: dir,
            _store: store,
            backend,
            connections,
            registry,
            _driver: driver,
            source,
            scope,
            connection_id,
        }
    }

    /// 经 `backend.open` 开一份真物理资源（同帧写入 `by_session` 反向索引）。
    async fn open_resource_for(fx: &Fixture, db_session_id: &str, epoch: u64) {
        fx.backend
            .open(OpenResource {
                connection_id: PublicConnectionId::new(fx.connection_id.clone()),
                config_revision: ConfigRevision(1),
                owner: OwnerRef::Editor {
                    organization_id: OrganizationId::new("org-1"),
                    principal_id: PrincipalId::new("user-1"),
                    connection_id: PublicConnectionId::new(fx.connection_id.clone()),
                    client_instance_id: ClientInstanceId::new("live-source-test"),
                    editor_session_id: datazen::shared::id::EditorSessionId::new("editor-1"),
                },
                initial_target: NamespaceTarget {
                    database: "app".into(),
                    catalog: String::new(),
                    schema: String::new(),
                    path: String::new(),
                },
                runtime_epoch: epoch,
                db_session_id: DbSessionId::new(db_session_id),
            })
            .await
            .expect("resource opens through the real backend");
    }

    fn request(db_session_id: &str, epoch: u64, connection_id: &str) -> LiveConnectionRequest {
        LiveConnectionRequest {
            handle: datazen::execution::runtime::connection::SessionHandle {
                db_session_id: DbSessionId::new(db_session_id),
                runtime_epoch: datazen::execution::runtime::connection::Counter::new(epoch),
            },
            connection_id: PublicConnectionId::new(connection_id),
            purpose: datazen::execution::application::LivePurpose::CommandExecution,
        }
    }

    /// 反查 miss：未登记的 dbSessionId → `SessionLost`，绝不新建连接。
    #[tokio::test]
    async fn an_unknown_session_yields_session_lost() {
        let fx = fixture().await;
        let error = fx
            .source
            .open_live(request("s-none", 7, &fx.connection_id))
            .await
            .expect_err("unknown session refuses");
        assert!(matches!(error, LiveConnectionError::SessionLost(_)), "{error}");
    }

    /// epoch 漂移：上一个纪元的残留行 → `SessionLost`（INV-07 在第①步内
    /// 拒绝，绝不透明替换）。
    #[tokio::test]
    async fn an_epoch_mismatch_yields_session_lost() {
        let fx = fixture().await;
        open_resource_for(&fx, "s-1", 7).await;
        let error = fx
            .source
            .open_live(request("s-1", 8, &fx.connection_id))
            .await
            .expect_err("stale epoch refuses");
        assert!(matches!(error, LiveConnectionError::SessionLost(_)), "{error}");
    }

    /// profile 缺席：连接目录答不出 driver type → `Refused`，绝不猜驱动。
    #[tokio::test]
    async fn a_missing_profile_refuses_to_guess_a_driver() {
        let fx = fixture().await;
        open_resource_for(&fx, "s-1", 7).await;
        let error = fx
            .source
            .open_live(request("s-1", 7, "conn-missing"))
            .await
            .expect_err("missing profile refuses");
        match &error {
            LiveConnectionError::Refused(message) => {
                assert!(message.contains("conn-missing"), "{message}");
                assert!(message.contains("refusing to guess a driver"), "{message}");
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    /// 驱动缺席：profile 在场但 registry 无方言 → `Refused`，绝不换驱动。
    #[tokio::test]
    async fn a_missing_driver_refuses_to_substitute_another() {
        let fx = fixture().await;
        open_resource_for(&fx, "s-1", 7).await;
        // 第二份连接配置：database_type 指向未装配的方言。
        fx.connections
            .save(
                &fx.scope,
                datazen_driver_api::ConnectionConfig {
                    id: "conn-ghost".into(),
                    name: "ghost conn".into(),
                    database_type: "ghostdb".into(),
                    host: Some("localhost".into()),
                    port: Some(1),
                    database: Some("app".into()),
                    schema: None,
                    username: None,
                    password: None,
                    ssl_mode: SslMode::Disable,
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
            .expect("ghost config saved");
        let error = fx
            .source
            .open_live(request("s-1", 7, "conn-ghost"))
            .await
            .expect_err("missing driver refuses");
        match &error {
            LiveConnectionError::Refused(message) => {
                assert!(message.contains("ghostdb"), "{message}");
                assert!(message.contains("refusing to substitute"), "{message}");
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    /// 同实例钉：`registry.get_sql_driver_by_name` 两次取出的是**同一个**
    /// `Arc`——`factory.create()` 与 `factory.resource_provider()` 共享进程内
    /// 单例（桌面模块头的工厂义务，server 端镜像）。绿路径顺带钉住：适配器
    /// 交出的驱动与 registry 项指针相等，句柄来自 provider 的真实持有。
    #[tokio::test]
    async fn the_registry_hands_out_one_driver_instance_and_the_green_path_lends_it() {
        let fx = fixture().await;
        open_resource_for(&fx, "s-1", 7).await;

        let first = fx
            .registry
            .get_sql_driver_by_name("mockdb")
            .await
            .expect("driver loaded");
        let second = fx
            .registry
            .get_sql_driver_by_name("mockdb")
            .await
            .expect("driver loaded");
        assert!(
            Arc::ptr_eq(&first, &second),
            "same registry entry must yield the same Arc"
        );

        let connection = fx
            .source
            .open_live(request("s-1", 7, &fx.connection_id))
            .await
            .expect("live connection lends");
        assert!(
            Arc::ptr_eq(connection.driver(), &first),
            "the adapter must hand out the registry's own driver Arc"
        );
        assert!(
            !connection.handle().id.is_empty(),
            "the provider lent its actual routing handle"
        );
    }
}
