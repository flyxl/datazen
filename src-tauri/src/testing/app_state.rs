//! Builds a fully wired [`AppState`] for command integration tests.

use std::sync::Arc;

use crate::commands::AppState;
use crate::db::registry::DriverRegistry;
use crate::db::{ConnectionConfig, SslMode, TableInfo, TableType};
use crate::store::Store;
use crate::sync::adapter_registry::SyncAdapterRegistry;
use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

/// Temp store + mock postgres driver + keyring guard for command tests.
pub struct TestAppState {
    pub _keyring: super::FileKeyringGuard,
    pub _temp: tempfile::TempDir,
    pub state: AppState,
    pub store: Arc<Store>,
    pub mock: Arc<MockDriver>,
    pub registry: Arc<DriverRegistry>,
}

impl TestAppState {
    pub async fn new() -> Self {
        Self::with_options(MockDriverOptions::default()).await
    }

    pub async fn with_options(opts: MockDriverOptions) -> Self {
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

    pub async fn save_connection(&self, id: &str) -> ConnectionConfig {
        let config = sample_postgres_config(id);
        self.store
            .save_connection(config.clone())
            .await
            .expect("save_connection");
        config
    }

    pub async fn connect_config(&self, config_id: &str) -> String {
        self.state
            .connection_manager
            .get_or_connect(config_id)
            .await
            .expect("connect")
    }

    pub async fn save_and_connect(&self, id: &str) -> (ConnectionConfig, String) {
        let config = self.save_connection(id).await;
        let conn_id = self.connect_config(id).await;
        (config, conn_id)
    }

    /// Register a redis SQL driver + mock KV driver for kv command tests.
    pub async fn register_redis_kv(&self, kv_opts: crate::testing::mock_kv_driver::MockKvDriverOptions) {
        use crate::testing::mock_kv_driver::MockKvDriver;

        let redis_driver = MockDriver::new("redis", MockDriverOptions::default());
        self.registry
            .register_test_driver("redis", redis_driver)
            .await;
        let kv = MockKvDriver::new("redis", kv_opts);
        self.registry.register_test_kv_driver("redis", kv).await;
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
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
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
    crate::finish_app_state(
        store,
        registry,
        Arc::new(SyncAdapterRegistry::new()),
        None,
    )
}
