use super::*;
use crate::db::registry::DriverRegistry;
use crate::db::{ConnectionConfig, DatabaseDriver, ServerInfo, SslMode};
use crate::store::Store;
use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
use async_trait::async_trait;

struct StubDriver(String);

#[async_trait]
impl DatabaseDriver for StubDriver {
    fn driver_type(&self) -> DatabaseType {
        self.0.clone()
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        Err(DriverError::QueryFailed("stub".into()))
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Err(DriverError::QueryFailed("stub".into()))
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<crate::db::TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        _table: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<crate::db::TableSchema, DriverError> {
        Err(DriverError::QueryFailed("stub".into()))
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<crate::db::QueryResult, DriverError> {
        Err(DriverError::QueryFailed("stub".into()))
    }

    async fn query_multi(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _limit: Option<u32>,
    ) -> Result<crate::db::MultiQueryResult, DriverError> {
        Err(DriverError::QueryFailed("stub".into()))
    }

    async fn query_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[crate::db::Value],
    ) -> Result<crate::db::QueryResult, DriverError> {
        Err(DriverError::QueryFailed("stub".into()))
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Ok(0)
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

async fn test_manager_stub() -> ConnectionManager {
    std::env::set_var("DATAZEN_KEYRING", "file");
    let dir = tempfile::tempdir().unwrap();
    // Keep tempdir alive for the store path lifetime of this test helper call site.
    let path = dir.path().to_path_buf();
    std::mem::forget(dir);
    let store = Arc::new(Store::init_with_path(&path).await.unwrap());
    let registry = Arc::new(DriverRegistry::new());
    registry
        .register_test_driver("postgresql", Arc::new(StubDriver("postgresql".into())))
        .await;
    ConnectionManager::new(registry, store)
}

/// A persisted connection config whose id (`connection_id`) is `connection_id`.
fn test_config(connection_id: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: connection_id.to_string(),
        name: "test".into(),
        database_type: "postgresql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(5432),
        database: Some("db".into()),
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

#[tokio::test]
async fn resolve_session_for_connection_reuses_existing_runtime() {
    let mgr = test_manager_stub().await;
    let handle = ConnectionHandle {
        id: "rt-2".into(),
        pool_id: "pool-2".into(),
    };
    mgr.insert_test_session("rt-2", "cfg-2", test_config("cfg-2"), handle)
        .await;

    // Input is the connection_id of a config whose session already lives:
    // it must resolve to that existing db_session_id.
    let (db_session_id, _driver, returned) =
        mgr.resolve_session_for_connection("cfg-2").await.unwrap();
    assert_eq!(db_session_id, "rt-2");
    assert_eq!(returned.id, "rt-2");
}

async fn test_manager() -> (
    crate::testing::FileKeyringGuard,
    Arc<ConnectionManager>,
    Arc<Store>,
    Arc<MockDriver>,
) {
    let keyring = crate::testing::FileKeyringGuard::set();
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
    let registry = Arc::new(DriverRegistry::new());
    let mock = MockDriver::new(
        "postgres",
        MockDriverOptions {
            server_version: "PostgreSQL 16".into(),
            ..Default::default()
        },
    );
    registry
        .register_test_driver("postgres", mock.clone())
        .await;
    let mgr = Arc::new(ConnectionManager::new(registry, store.clone()));
    (keyring, mgr, store, mock)
}

fn sample_config(id: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.into(),
        name: "Test".into(),
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

#[tokio::test]
async fn connect_registers_session_and_returns_db_session_id() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let db_session_id = mgr.connect("cfg-1").await.unwrap();
    // db_session_id comes from the driver's runtime handle.
    assert!(db_session_id.starts_with("mock-cfg-1"));
    assert_eq!(mgr.session_owner_map_len().await, 1);
    assert_eq!(
        mgr.owner_connection_id(&db_session_id).await.as_deref(),
        Some("cfg-1")
    );
}

#[tokio::test]
async fn get_or_connect_session_reuses_existing_session() {
    let (_keyring, mgr, store, mock) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let first = mgr.get_or_connect_session("cfg-1").await.unwrap();
    let second = mgr.get_or_connect_session("cfg-1").await.unwrap();
    assert_eq!(first, second);
    assert_eq!(mock.get_columns_calls(), 0);
}

#[tokio::test]
async fn get_session_returns_driver_and_updates_last_used() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let db_session_id = mgr.connect("cfg-1").await.unwrap();
    let (driver, handle) = mgr.get_session(&db_session_id).await.unwrap();
    assert_eq!(driver.driver_type(), "postgres");
    assert_eq!(handle.id, db_session_id);
}

#[tokio::test]
async fn disconnect_removes_session() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let db_session_id = mgr.connect("cfg-1").await.unwrap();
    mgr.disconnect(&db_session_id).await.unwrap();
    assert_eq!(mgr.session_owner_map_len().await, 0);
}

#[tokio::test]
async fn ping_returns_true_for_active_session() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let db_session_id = mgr.connect("cfg-1").await.unwrap();
    assert!(mgr.ping(&db_session_id).await);
    assert!(!mgr.ping("missing").await);
}

#[tokio::test]
async fn get_session_config_returns_stored_config() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let db_session_id = mgr.connect("cfg-1").await.unwrap();
    let cfg = mgr.get_session_config(&db_session_id).await.unwrap();
    // The session's effective config keeps the owning connection_id.
    assert_eq!(cfg.id, "cfg-1");
    assert_eq!(cfg.name, "Test");
}

#[tokio::test]
async fn test_connection_uses_driver() {
    let (_keyring, mgr, _, _) = test_manager().await;
    let info = mgr.test_connection(&sample_config("cfg-1")).await.unwrap();
    assert_eq!(info.server_version, "PostgreSQL 16");
}

#[tokio::test]
async fn connect_errors_when_connection_config_missing() {
    let (_keyring, mgr, _, _) = test_manager().await;
    let err = mgr.connect("missing").await.unwrap_err();
    assert!(matches!(err, ConnectionError::ConnectionConfigNotFound(_)));
    // The message must make clear which kind of id was not found.
    assert!(err
        .to_string()
        .contains("Connection config 'missing' not found"));
}

/// Invariant: an idle-evicted session is auto-reconnected through
/// `session_owner_map` and keeps its original `db_session_id`.
#[tokio::test]
async fn evicted_session_auto_reconnects_preserving_db_session_id() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    // `cleanup_idle_connections` now skips sessions that still hold a ref
    // (`ref_counts > 0`), and `get_or_connect_session` takes one — so the
    // eviction path is exercised through the unreferenced `connect` entry.
    let db_session_id = mgr.connect("cfg-1").await.unwrap();
    assert_eq!(
        mgr.owner_connection_id(&db_session_id).await.as_deref(),
        Some("cfg-1")
    );

    // Force idle eviction of the live session.
    mgr.expire_test_session(&db_session_id).await;
    mgr.cleanup_idle_connections().await;
    assert!(!mgr.ping(&db_session_id).await);
    // The ownership mapping survives eviction — this is what enables
    // auto-reconnect under the same db_session_id.
    assert_eq!(
        mgr.owner_connection_id(&db_session_id).await.as_deref(),
        Some("cfg-1")
    );

    // Auto-reconnect rebuilds the session from the latest persisted config
    // and reuses the exact same db_session_id.
    let (driver, handle) = mgr.get_session(&db_session_id).await.unwrap();
    assert_eq!(driver.driver_type(), "postgres");
    assert_eq!(handle.id, db_session_id);
    assert!(mgr.ping(&db_session_id).await);
    assert_eq!(
        mgr.owner_connection_id(&db_session_id).await.as_deref(),
        Some("cfg-1")
    );
}

/// Invariant: `resolve_session_for_connection` accepts only a persisted
/// `connection_id`. Passing a runtime `db_session_id` must fail.
#[tokio::test]
async fn resolve_session_for_connection_rejects_db_session_id() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let db_session_id = mgr.connect("cfg-1").await.unwrap();

    match mgr.resolve_session_for_connection(&db_session_id).await {
        Err(e) => assert!(matches!(e, ConnectionError::ConnectionConfigNotFound(_))),
        Ok(_) => panic!("db_session_id must not be accepted as connection_id"),
    }
}

/// Invariant: a valid `connection_id` creates or reuses a session and
/// returns its `db_session_id` with the owner mapping recorded.
#[tokio::test]
async fn resolve_session_for_connection_creates_session_from_connection_id() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store
        .save_connection(sample_config("cfg-fb"))
        .await
        .unwrap();

    let (db_session_id, driver, handle) =
        mgr.resolve_session_for_connection("cfg-fb").await.unwrap();
    assert_ne!(db_session_id, "cfg-fb");
    assert!(db_session_id.starts_with("mock-cfg-fb"));
    assert_eq!(handle.id, db_session_id);
    assert_eq!(driver.driver_type(), "postgres");
    assert_eq!(
        mgr.owner_connection_id(&db_session_id).await.as_deref(),
        Some("cfg-fb")
    );
}

#[tokio::test]
async fn driver_not_found_when_type_unregistered() {
    let (_keyring, mgr, store, _) = test_manager().await;
    let mut cfg = sample_config("cfg-x");
    cfg.database_type = "unknown-db".into();
    store.save_connection(cfg).await.unwrap();
    let err = mgr.connect("cfg-x").await.unwrap_err();
    assert!(matches!(err, ConnectionError::DriverNotFound(_)));
}

#[tokio::test]
async fn shutdown_disconnects_all() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store.save_connection(sample_config("cfg-1")).await.unwrap();
    let _ = mgr.connect("cfg-1").await.unwrap();
    mgr.shutdown().await;
    assert_eq!(mgr.session_owner_map_len().await, 0);
}

/// [tester] Poisoned `connect_locks` must surface `ConnectionError::Internal`, not panic.
#[tokio::test]
async fn test_tester_connect_lock_poison_returns_internal() {
    let (_keyring, mgr, store, _) = test_manager().await;
    store
        .save_connection(sample_config("cfg-poison"))
        .await
        .unwrap();

    let mgr_for_poison = Arc::clone(&mgr);
    let poison_thread = std::thread::spawn(move || {
        let _guard = mgr_for_poison.connect_locks.lock().unwrap();
        panic!("tester: intentional connect_locks poison");
    });
    assert!(
        poison_thread.join().is_err(),
        "poison thread must panic to leave the mutex poisoned"
    );

    let err = mgr.get_or_connect_session("cfg-poison").await.unwrap_err();
    match err {
        ConnectionError::Internal(msg) => {
            assert!(
                msg.contains("connect lock poisoned"),
                "unexpected internal message: {msg}"
            );
        }
        other => panic!("expected ConnectionError::Internal, got {other:?}"),
    }
}
