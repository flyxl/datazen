//! Registry that maps `DatabaseType` → sync adapters.
//!
//! Adapters are registered lazily: only the source/target types needed for a
//! sync job are created, the first time that pair is requested.
//!
//! Concrete adapters self-register via [`SyncAdapterFactory`] + `inventory`
//! (from path / git driver crates).

use crate::db::{
    BoxedSyncAdapter, DatabaseType, SyncAdapterFactory, SyncSourceAdapter, SyncTargetAdapter,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct SyncAdapterRegistry {
    sources: RwLock<HashMap<DatabaseType, Arc<dyn SyncSourceAdapter>>>,
    targets: RwLock<HashMap<DatabaseType, Arc<dyn SyncTargetAdapter>>>,
}

impl SyncAdapterRegistry {
    pub fn new() -> Self {
        Self {
            sources: RwLock::new(HashMap::new()),
            targets: RwLock::new(HashMap::new()),
        }
    }

    /// Ensure adapters for `src_type` and `tgt_type` are registered.
    /// Only those two (or one, if they match) types are loaded.
    pub fn ensure_pair(&self, src_type: &DatabaseType, tgt_type: &DatabaseType) -> Result<(), String> {
        self.ensure_type(src_type)?;
        if src_type != tgt_type {
            self.ensure_type(tgt_type)?;
        }
        Ok(())
    }

    /// Lazily register the sync adapter for a single database type.
    pub fn ensure_type(&self, db_type: &DatabaseType) -> Result<(), String> {
        {
            let sources = self.sources.read().map_err(|e| e.to_string())?;
            let targets = self.targets.read().map_err(|e| e.to_string())?;
            if sources.contains_key(db_type) && targets.contains_key(db_type) {
                return Ok(());
            }
        }

        self.register_from_inventory(db_type)?;
        Ok(())
    }

    fn register_from_inventory(&self, db_type: &DatabaseType) -> Result<(), String> {
        // Touch residual module; path/git driver crates link via Cargo features / tests.
        crate::sync::adapters::force_link();
        #[cfg(test)]
        force_link_driver_sync_adapters();

        let key = db_type.as_str();
        for factory in inventory::iter::<SyncAdapterFactory> {
            if factory.db_types.iter().any(|t| *t == key) {
                let boxed: BoxedSyncAdapter = (factory.create)();
                self.insert_pair(db_type.clone(), boxed);
                tracing::info!(db_type = %db_type, "Registered sync adapter on demand");
                return Ok(());
            }
        }
        Err(format!("No sync adapter for database type '{key}'"))
    }

    fn insert_pair(&self, db_type: DatabaseType, boxed: BoxedSyncAdapter) {
        let mut sources = self.sources.write().expect("sync sources lock");
        let mut targets = self.targets.write().expect("sync targets lock");
        sources.insert(db_type.clone(), boxed.source);
        targets.insert(db_type, boxed.target);
    }

    pub fn get_source(&self, db_type: &DatabaseType) -> Option<Arc<dyn SyncSourceAdapter>> {
        self.sources.read().ok()?.get(db_type).cloned()
    }

    pub fn get_target(&self, db_type: &DatabaseType) -> Option<Arc<dyn SyncTargetAdapter>> {
        self.targets.read().ok()?.get(db_type).cloned()
    }
}

impl Default for SyncAdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Keep the path-driver sync adapters that are always present in the basic CI
/// driver set linked in unit tests. Optional drivers are linked by their
/// generated driver/plugin wiring when those drivers are selected; referencing
/// them here would make the `basic` feature build depend on crates that are
/// intentionally absent from Cargo.toml.
#[cfg(test)]
#[inline(never)]
fn force_link_driver_sync_adapters() {
    let _ = (
        std::any::type_name::<datazen_driver_postgres::PgSyncAdapter>(),
        std::any::type_name::<datazen_driver_mysql::MysqlSyncAdapter>(),
        std::any::type_name::<datazen_driver_sqlite::SqliteSyncAdapter>(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_type_postgresql_succeeds() {
        let registry = SyncAdapterRegistry::new();
        assert!(registry.ensure_type(&"postgresql".to_string()).is_ok());
        assert!(registry.get_source(&"postgresql".to_string()).is_some());
        assert!(registry.get_target(&"postgresql".to_string()).is_some());
    }

    #[test]
    fn ensure_type_unknown_fails() {
        let registry = SyncAdapterRegistry::new();
        let err = registry
            .ensure_type(&"nosuchdb".to_string())
            .expect_err("unknown type must fail");
        assert!(err.contains("No sync adapter"));
    }

    #[test]
    fn ensure_type_wire_aliases_succeed() {
        let registry = SyncAdapterRegistry::new();
        for db in [
            "postgresql",
            "mysql",
            "mariadb",
            "sqlite",
            "cloudberry",
            "questdb",
            "rqlite",
            "turso",
            "doris",
            "starrocks",
            "manticore",
            "ob_oracle",
            "sqlserver",
            "clickhouse",
            "duckdb",
            "elasticsearch",
            "mongodb",
            "influxdb",
            "victoriametrics",
            "hbase",
            "vector",
            #[cfg(feature = "plugin-olap")]
            "trino",
            #[cfg(feature = "plugin-olap")]
            "presto",
        ] {
            assert!(
                registry.ensure_type(&db.to_string()).is_ok(),
                "expected sync adapter for {db}"
            );
        }
    }
}
