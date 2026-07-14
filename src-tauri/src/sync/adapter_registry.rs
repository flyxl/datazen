//! Registry that maps `DatabaseType` → sync adapters.

use super::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::db::DatabaseType;
use std::collections::HashMap;
use std::sync::Arc;

pub struct SyncAdapterRegistry {
    sources: HashMap<DatabaseType, Arc<dyn SyncSourceAdapter>>,
    targets: HashMap<DatabaseType, Arc<dyn SyncTargetAdapter>>,
}

impl SyncAdapterRegistry {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            targets: HashMap::new(),
        }
    }

    pub fn register_source(&mut self, db_type: DatabaseType, adapter: Arc<dyn SyncSourceAdapter>) {
        self.sources.insert(db_type, adapter);
    }

    pub fn register_target(&mut self, db_type: DatabaseType, adapter: Arc<dyn SyncTargetAdapter>) {
        self.targets.insert(db_type, adapter);
    }

    /// Register the same struct as both source and target adapter.
    pub fn register_both<T>(&mut self, db_type: DatabaseType, adapter: Arc<T>)
    where
        T: SyncSourceAdapter + SyncTargetAdapter + 'static,
    {
        self.sources
            .insert(db_type.clone(), adapter.clone() as Arc<dyn SyncSourceAdapter>);
        self.targets
            .insert(db_type, adapter as Arc<dyn SyncTargetAdapter>);
    }

    pub fn get_source(&self, db_type: &DatabaseType) -> Option<Arc<dyn SyncSourceAdapter>> {
        self.sources.get(db_type).cloned()
    }

    pub fn get_target(&self, db_type: &DatabaseType) -> Option<Arc<dyn SyncTargetAdapter>> {
        self.targets.get(db_type).cloned()
    }
}
