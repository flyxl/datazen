//! Concrete sync adapters for each supported database type.

pub mod mysql;
pub mod postgresql;
pub mod sqlite;
pub mod trino;

#[cfg(test)]
mod roundtrip_tests;

use super::adapter_registry::SyncAdapterRegistry;
use std::sync::Arc;

/// Build a fully populated `SyncAdapterRegistry` with all built-in adapters.
pub fn init_sync_adapters() -> SyncAdapterRegistry {
    let mut reg = SyncAdapterRegistry::new();

    let pg = Arc::new(postgresql::PgSyncAdapter);
    reg.register_both("postgresql".to_string(), pg);

    let mysql = Arc::new(mysql::MysqlSyncAdapter { is_mariadb: false });
    reg.register_both("mysql".to_string(), mysql);

    let maria = Arc::new(mysql::MysqlSyncAdapter { is_mariadb: true });
    reg.register_both("mariadb".to_string(), maria);

    let sqlite = Arc::new(sqlite::SqliteSyncAdapter);
    reg.register_both("sqlite".to_string(), sqlite);

    let trino = Arc::new(trino::TrinoSyncAdapter);
    reg.register_both("trino".to_string(), trino.clone());
    reg.register_both("presto".to_string(), trino);

    reg
}
