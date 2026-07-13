//! Driver registry — resolves `DatabaseType` to a concrete `DatabaseDriver`.

use super::kiwi::KiwiDriver;
use super::mysql::MysqlDriver;
use super::postgres::PostgresDriver;
use super::redis_driver::RedisDriver;
use super::sqlite::SqliteDriver;
use super::traits::KeyValueDriver;
use super::{DatabaseDriver, DatabaseType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Holds all registered drivers.
pub struct DriverRegistry {
    drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn DatabaseDriver>>>>,
    /// Same [`Arc`] identity as the registered [`DatabaseType::Redis`] driver — use for `kv_*` commands.
    pub redis: Arc<RedisDriver>,
}

impl DriverRegistry {
    fn new(redis: Arc<RedisDriver>) -> Self {
        Self {
            drivers: Arc::new(RwLock::new(HashMap::new())),
            redis,
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

    pub async fn get_kv_driver(&self, db_type: &DatabaseType) -> Option<Arc<dyn KeyValueDriver>> {
        match db_type {
            DatabaseType::Redis => Some(self.redis.clone() as Arc<dyn KeyValueDriver>),
            _ => None,
        }
    }
}

/// Registers built-in drivers.
pub async fn init_drivers() -> DriverRegistry {
    let redis = Arc::new(RedisDriver::new());
    let registry = DriverRegistry::new(redis.clone());
    let redis_dyn: Arc<dyn DatabaseDriver> = redis.clone();
    registry
        .register(redis_dyn)
        .await;
    registry
        .register(Arc::new(PostgresDriver::new()))
        .await;
    registry
        .register(Arc::new(MysqlDriver::new(false)))
        .await;
    registry
        .register(Arc::new(MysqlDriver::new(true)))
        .await;
    registry
        .register(Arc::new(SqliteDriver::new()))
        .await;
    registry
        .register(Arc::new(KiwiDriver::new()))
        .await;
    registry
}
