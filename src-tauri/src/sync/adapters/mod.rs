//! Concrete sync adapters for each supported database type.

pub mod mysql;
pub mod postgresql;
pub mod sqlite;
pub mod trino;

#[cfg(test)]
mod roundtrip_tests;

use super::adapter_registry::SyncAdapterRegistry;
use crate::db::DatabaseType;
use std::sync::Arc;

/// Build a fully populated `SyncAdapterRegistry` with all built-in adapters.
pub fn init_sync_adapters() -> SyncAdapterRegistry {
    let mut reg = SyncAdapterRegistry::new();

    let pg = Arc::new(postgresql::PgSyncAdapter);
    reg.register_both(DatabaseType::PostgreSQL, pg);

    let mysql = Arc::new(mysql::MysqlSyncAdapter { is_mariadb: false });
    reg.register_both(DatabaseType::MySQL, mysql);

    let maria = Arc::new(mysql::MysqlSyncAdapter { is_mariadb: true });
    reg.register_both(DatabaseType::MariaDB, maria);

    let sqlite = Arc::new(sqlite::SqliteSyncAdapter);
    reg.register_both(DatabaseType::SQLite, sqlite);

    let trino = Arc::new(trino::TrinoSyncAdapter);
    reg.register_both(DatabaseType::Trino, trino.clone());
    reg.register_both(DatabaseType::Presto, trino);

    reg
}
