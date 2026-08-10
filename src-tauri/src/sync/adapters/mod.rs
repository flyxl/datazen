//! Concrete sync adapters for each supported database type.
//!
//! Each adapter module registers itself via `inventory::submit!(SyncAdapterFactory)`.

pub mod mysql;
pub mod postgresql;
pub mod sqlite;
pub mod sqlserver;
pub mod trino;

/// Ensure adapter modules stay linked so their `inventory::submit!` statics are present.
#[inline(never)]
pub fn force_link() {
    let _ = (
        std::any::type_name::<postgresql::PgSyncAdapter>(),
        std::any::type_name::<mysql::MysqlSyncAdapter>(),
        std::any::type_name::<sqlite::SqliteSyncAdapter>(),
        std::any::type_name::<sqlserver::SqlServerSyncAdapter>(),
        std::any::type_name::<trino::TrinoSyncAdapter>(),
    );
}

#[cfg(test)]
mod roundtrip_tests;
