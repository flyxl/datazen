//! Builds a fully wired [`AppState`] for command integration tests.

use std::sync::{Arc, OnceLock};

use crate::commands::AppState;
use crate::store::Store;
use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::{ConnectionConfig, SslMode, TableInfo, TableType};
use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory};
use datazen_runtime::driver_registry::registry::DriverRegistry;

/// Temp store + mock postgres driver + keyring guard for command tests.
pub struct TestAppState {
    pub _keyring: super::FileKeyringGuard,
    pub _temp: tempfile::TempDir,
    pub state: AppState,
    pub store: Arc<Store>,
    pub mock: Arc<MockDriver>,
    pub registry: Arc<DriverRegistry>,
}

/// Supplies the same test driver through the runtime resource contract.
///
/// Runtime SQL and ChangeSet paths open short-lived resources through a
/// `DatabaseDriverFactory`; registering only a `MockDriver` is not enough for
/// those paths. Keep the adapter and command driver backed by the same mock so
/// the tests exercise the real runtime wiring without opening a network
/// connection.
struct TestMockDriverFactory {
    driver_id: &'static str,
    driver: Arc<dyn DatabaseDriver>,
    provider: OnceLock<Arc<dyn ResourceProvider>>,
}

impl TestMockDriverFactory {
    fn new(driver_id: &'static str, driver: Arc<dyn DatabaseDriver>) -> Self {
        Self {
            driver_id,
            driver,
            provider: OnceLock::new(),
        }
    }
}

impl DatabaseDriverFactory for TestMockDriverFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::clone(&self.driver)
    }

    fn driver_id(&self) -> &'static str {
        self.driver_id
    }

    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(
            self.provider
                .get_or_init(|| {
                    Arc::new(
                        super::resource_provider::TestResourceProvider::new_for_driver(
                            Arc::clone(&self.driver),
                            self.driver_id,
                        ),
                    ) as Arc<dyn ResourceProvider>
                })
                .clone(),
        )
    }
}

impl TestAppState {
    pub async fn new() -> Self {
        Self::with_options(MockDriverOptions::default()).await
    }

    pub async fn with_options(opts: MockDriverOptions) -> Self {
        Self::build(opts, false).await
    }

    /// Build a test state with a resource provider for runtime SQL/ChangeSet
    /// tests. Other tests keep using `with_options` so provider-backed session
    /// behavior is opt-in.
    pub async fn with_resource_provider(mut opts: MockDriverOptions) -> Self {
        // This retained-resource fixture supports explain explicitly. Keep the
        // default legacy MockDriver conservative so unsupported-driver tests
        // continue to exercise the absence of this capability.
        if !opts
            .extra_commands
            .iter()
            .any(|command| command.id == "explain")
        {
            opts.extra_commands
                .push(datazen_driver_api::explain_command_definition());
        }
        opts.command_results
            .entry("explain".into())
            .or_insert_with(|| {
                serde_json::to_value(&opts.explain_plan).expect("serialize mock explain plan")
            });
        Self::build(opts, true).await
    }

    async fn build(opts: MockDriverOptions, _with_resource_provider: bool) -> Self {
        let keyring = super::FileKeyringGuard::set();
        let temp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::init_with_path(temp.path())
                .await
                .expect("store init"),
        );
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new("postgres", opts);
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        // Runtime-backed command tests need the same mock provider as the
        // direct driver path. Register it for every fixture so a test cannot
        // accidentally create a legacy-only session that the production
        // command path would reject.
        let driver: Arc<dyn DatabaseDriver> = mock.clone();
        let factory: &'static dyn DatabaseDriverFactory =
            Box::leak(Box::new(TestMockDriverFactory::new("postgres", driver)));
        registry.register_test_factory("postgres", factory).await;
        let state = build_app_state(store.clone(), registry.clone());
        Self {
            _keyring: keyring,
            _temp: temp,
            state,
            store,
            mock,
            registry,
        }
    }

    pub async fn with_tables() -> Self {
        Self::with_options(rich_mock_options()).await
    }

    pub async fn with_tables_and_resource_provider() -> Self {
        Self::with_resource_provider(rich_mock_options()).await
    }

    /// Register a Runtime resource factory for a driver spelling introduced by
    /// a focused fixture (for example `postgresql` versus `postgres`).
    pub async fn register_runtime_driver(
        &self,
        driver_id: &'static str,
        driver: Arc<dyn DatabaseDriver>,
    ) {
        let factory: &'static dyn DatabaseDriverFactory =
            Box::leak(Box::new(TestMockDriverFactory::new(driver_id, driver)));
        self.registry
            .register_test_factory(driver_id, factory)
            .await;
    }

    pub async fn save_connection(&self, id: &str) -> ConnectionConfig {
        let config = sample_postgres_config(id);
        self.store
            .save_connection(config.clone())
            .await
            .expect("save_connection");
        config
    }

    /// Persisted connection config must exist; establish it as a Runtime session.
    pub async fn connect_config(&self, connection_id: &str) -> String {
        self.open_runtime_session(connection_id).await
    }

    pub async fn save_and_connect(&self, id: &str) -> (ConnectionConfig, String) {
        // Keep the historical fixture name, but make it exercise the Runtime
        // session boundary used by migrated command paths.
        self.save_and_connect_runtime(id).await
    }

    /// Persist a profile and open it through the Runtime session boundary.
    /// Opens a session through the Runtime boundary used by production paths.
    pub async fn save_and_connect_runtime(&self, id: &str) -> (ConnectionConfig, String) {
        let config = self.save_connection(id).await;
        let session = self.open_runtime_session(id).await;
        (config, session)
    }

    /// Persist a profile and open a real platform runtime session for command tests.
    pub async fn save_and_open_runtime_session(&self, id: &str) -> String {
        self.save_connection(id).await;
        self.open_runtime_session(id).await
    }

    /// Open a runtime session for a profile that the test has already persisted.
    pub async fn open_runtime_session(&self, id: &str) -> String {
        use datazen_application::{
            dto::requests::OpenSessionRequest, sessions::ConnectionUseCases,
        };
        use datazen_platform_api::{
            context::OwnerRef,
            dto::idempotency::IdempotentOperation,
            id::ConnectionId,
            target::{ExecutionTarget, NamespaceTarget},
        };

        let config = self.store.get_connection(id).await.expect("profile exists");
        let adapter = self.state.platform_require().expect("platform is ready");
        let call = adapter.begin_call();
        let runtime = adapter.runtime().expect("runtime is ready");
        let token = runtime
            .issue_submission_token(call.context(), IdempotentOperation::OpenSession, None)
            .await
            .expect("runtime session token issues");
        let opened = runtime
            .open_session(
                call.context(),
                OpenSessionRequest::new(
                    ExecutionTarget::new(
                        ConnectionId::new(id),
                        NamespaceTarget::database(config.database.as_deref().unwrap_or("app")),
                    ),
                    OwnerRef::ClientSession {
                        client_instance_id: call.context().client_instance_id.clone(),
                        purpose: "commandTest".into(),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .expect("runtime session opens");
        opened.session.handle.db_session_id.as_str().to_owned()
    }
}

pub fn sample_postgres_config(id: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.into(),
        name: format!("Test {id}"),
        database_type: "postgres".into(),
        host: Some("localhost".into()),
        port: Some(5432),
        database: Some("app".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Prefer,
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
    }
}

pub fn rich_mock_options() -> MockDriverOptions {
    use datazen_driver_api::Value;

    MockDriverOptions {
        databases: vec!["app".into()],
        tables: vec![TableInfo {
            name: "users".into(),
            schema: None,
            table_type: TableType::Table,
            row_count: Some(2),
        }],
        count_total: 2,
        query_rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::String("alice".into())),
        ]],
        server_version: "PostgreSQL 16".into(),
        ..Default::default()
    }
}

pub fn build_app_state(store: Arc<Store>, registry: Arc<DriverRegistry>) -> AppState {
    crate::finish_app_state(store, registry, Arc::new(SyncAdapterRegistry::new()), None)
}
