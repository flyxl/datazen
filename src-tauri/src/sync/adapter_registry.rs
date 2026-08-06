//! Registry that maps `DatabaseType` → sync adapters.
//!
//! Adapters are registered lazily: only the source/target types needed for a
//! sync job are created, the first time that pair is requested.

use super::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::db::DatabaseType;
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

        self.register_builtin(db_type)?;
        Ok(())
    }

    fn register_builtin(&self, db_type: &DatabaseType) -> Result<(), String> {
        use super::adapters::{mysql, postgresql, sqlite, trino};

        match db_type.as_str() {
            "postgresql" => {
                self.register_both(db_type.clone(), Arc::new(postgresql::PgSyncAdapter));
            }
            "mysql" => {
                self.register_both(
                    db_type.clone(),
                    Arc::new(mysql::MysqlSyncAdapter { is_mariadb: false }),
                );
            }
            "mariadb" => {
                self.register_both(
                    db_type.clone(),
                    Arc::new(mysql::MysqlSyncAdapter { is_mariadb: true }),
                );
            }
            "sqlite" => {
                self.register_both(db_type.clone(), Arc::new(sqlite::SqliteSyncAdapter));
            }
            "trino" | "presto" => {
                self.register_both(db_type.clone(), Arc::new(trino::TrinoSyncAdapter));
            }
            other => {
                return Err(format!("No sync adapter for database type '{other}'"));
            }
        }
        tracing::info!(db_type = %db_type, "Registered sync adapter on demand");
        Ok(())
    }

    fn register_both<T>(&self, db_type: DatabaseType, adapter: Arc<T>)
    where
        T: SyncSourceAdapter + SyncTargetAdapter + 'static,
    {
        let mut sources = self.sources.write().expect("sync sources lock");
        let mut targets = self.targets.write().expect("sync targets lock");
        sources.insert(db_type.clone(), adapter.clone() as Arc<dyn SyncSourceAdapter>);
        targets.insert(db_type, adapter as Arc<dyn SyncTargetAdapter>);
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
