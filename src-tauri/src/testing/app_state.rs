//! Builds a fully wired [`AppState`] for command integration tests.

use std::sync::{Arc, OnceLock};

use crate::commands::AppState;
use crate::db::registry::DriverRegistry;
use crate::db::{ConnectionConfig, SslMode, TableInfo, TableType};
use crate::store::Store;
use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory};

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
    driver: Arc<dyn DatabaseDriver>,
    provider: OnceLock<Arc<dyn ResourceProvider>>,
}

impl TestMockDriverFactory {
    fn new(driver: Arc<dyn DatabaseDriver>) -> Self {
        Self {
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
        "postgres"
    }

    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(
            self.provider
                .get_or_init(|| {
                    Arc::new(super::resource_provider::TestResourceProvider::new(
                        Arc::clone(&self.driver),
                    )) as Arc<dyn ResourceProvider>
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
    pub async fn with_resource_provider(opts: MockDriverOptions) -> Self {
        Self::build(opts, true).await
    }

    async fn build(opts: MockDriverOptions, with_resource_provider: bool) -> Self {
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
        if with_resource_provider {
            let driver: Arc<dyn DatabaseDriver> = mock.clone();
            let factory: &'static dyn DatabaseDriverFactory =
                Box::leak(Box::new(TestMockDriverFactory::new(driver)));
            registry.register_test_factory("postgres", factory).await;
        }
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

    pub async fn save_connection(&self, id: &str) -> ConnectionConfig {
        let config = sample_postgres_config(id);
        self.store
            .save_connection(config.clone())
            .await
            .expect("save_connection");
        config
    }

    /// Establish (or reuse) the db session for persisted connection config `connection_id`.
    pub async fn connect_config(&self, connection_id: &str) -> String {
        self.state
            .connection_manager
            .get_or_connect_session(connection_id)
            .await
            .expect("connect")
    }

    pub async fn save_and_connect(&self, id: &str) -> (ConnectionConfig, String) {
        let config = self.save_connection(id).await;
        let conn_id = self.connect_config(id).await;
        (config, conn_id)
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
    use crate::db::Value;

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
