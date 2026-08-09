//! Registry that maps `DatabaseType` → sync adapters.
//!
//! Adapters are registered lazily: only the source/target types needed for a
//! sync job are created, the first time that pair is requested.
//!
//! Concrete adapters self-register via [`SyncAdapterFactory`] + `inventory`.

use super::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::db::DatabaseType;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Host sync-adapter factory discovered via `inventory`.
pub struct SyncAdapterFactory {
    pub db_types: &'static [&'static str],
    pub register: fn(registry: &SyncAdapterRegistry, db_type: DatabaseType),
}

inventory::collect!(SyncAdapterFactory);

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
        // Touch adapter modules so their `inventory::submit!` statics are linked.
        crate::sync::adapters::force_link();

        let key = db_type.as_str();
        for factory in inventory::iter::<SyncAdapterFactory> {
            if factory.db_types.iter().any(|t| *t == key) {
                (factory.register)(self, db_type.clone());
                tracing::info!(db_type = %db_type, "Registered sync adapter on demand");
                return Ok(());
            }
        }
        Err(format!("No sync adapter for database type '{key}'"))
    }

    pub(crate) fn register_both<T>(&self, db_type: DatabaseType, adapter: Arc<T>)
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
        for db in ["cloudberry", "rqlite", "turso"] {
            assert!(
                registry.ensure_type(&db.to_string()).is_ok(),
                "expected sync adapter for {db}"
            );
        }
    }
}
