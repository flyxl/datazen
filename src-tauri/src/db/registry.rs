//! Driver registry — resolves `DatabaseType` to a concrete `DatabaseDriver`.

use super::postgres::PostgresDriver;
use super::{DatabaseDriver, DatabaseType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Holds all registered drivers.
pub struct DriverRegistry {
    drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn DatabaseDriver>>>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, driver: Arc<dyn DatabaseDriver>) {
        let mut drivers = self.drivers.write().await;
        drivers.insert(driver.driver_type(), driver);
    }

    pub async fn get(&self, db_type: &DatabaseType) -> Option<Arc<dyn DatabaseDriver>> {
        let drivers = self.drivers.read().await;
        drivers.get(db_type).cloned()
    }

    pub async fn supported_types(&self) -> Vec<DatabaseType> {
        let drivers = self.drivers.read().await;
        drivers.keys().cloned().collect()
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers built-in drivers (currently PostgreSQL stub only).
pub async fn init_drivers() -> DriverRegistry {
    let registry = DriverRegistry::new();
    registry
        .register(Arc::new(PostgresDriver::new()))
        .await;
    registry
}
