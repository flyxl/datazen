//! Fixtures shared across the store test modules.

use super::super::*;
use chrono::Utc;
use datazen_driver_api::{ConnectionConfig, SshTunnelConfig, SslMode};

/// Serialize tests that mutate process env vars.
///
/// Process-wide lock: `std::env::set_var` is process-global, so every env
/// writer **and** reader in the test binary shares one mutex (see
/// `crate::testing::ENV_LOCK`).
pub(super) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    crate::testing::ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Open a store on the **file** backend.
///
/// The backend is passed explicitly: tests must not depend on the ambient
/// environment (`DATAZEN_KEYRING`), and must never write it — a process-wide
/// env write races with every other test in the binary.
pub(super) async fn init_store_for_test(dir: &std::path::Path) -> DesktopRepository {
    DesktopRepository::init_with_path(dir, KeyBackend::File)
        .await
        .unwrap()
}

pub(super) fn sample_connection_with_ssh() -> ConnectionConfig {
    ConnectionConfig {
        id: "test-ssh-1".into(),
        name: "SSH Test".into(),
        database_type: "postgresql".into(),
        host: Some("localhost".into()),
        port: Some(5432),
        database: Some("mydb".into()),
        schema: None,
        username: Some("dbuser".into()),
        password: Some("db-secret".into()),
        ssl_mode: SslMode::Disable,
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel: Some(SshTunnelConfig {
            enabled: true,
            host: "jump.example.com".into(),
            port: 22,
            username: "sshuser".into(),
            auth_method: "password".into(),
            password: Some("ssh-secret-password".into()),
            private_key_path: None,
            passphrase: Some("key-passphrase".into()),
            jump: None,
        }),
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

pub(super) fn sample_history_entry(sql: &str) -> QueryHistoryEntry {
    QueryHistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        connection_id: "cfg-1".into(),
        database: "db".into(),
        schema: None,
        sql: sql.into(),
        executed_at: Utc::now(),
        execution_time_ms: 10,
        rows_affected: Some(1),
        success: true,
        error_message: None,
    }
}
