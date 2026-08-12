//! DataZen path driver: mysql

use std::sync::Arc;

use datazen_driver_api::*;

mod mysql;
mod structure;
mod sync_adapter;
pub use mysql::*;
pub use sync_adapter::MysqlSyncAdapter;

struct MysqlFactory;
impl DatabaseDriverFactory for MysqlFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(MysqlDriver::new(false))
    }
    fn driver_id(&self) -> &'static str {
        "mysql"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&MysqlFactory);

struct MariadbFactory;
impl DatabaseDriverFactory for MariadbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(MysqlDriver::new(true))
    }
    fn driver_id(&self) -> &'static str {
        "mariadb"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&MariadbFactory);

struct DorisFactory;
impl DatabaseDriverFactory for DorisFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new(Arc::new(MysqlDriver::new(false)), "doris"))
    }
    fn driver_id(&self) -> &'static str {
        "doris"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&DorisFactory);

struct StarrocksFactory;
impl DatabaseDriverFactory for StarrocksFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new(
            Arc::new(MysqlDriver::new(false)),
            "starrocks",
        ))
    }
    fn driver_id(&self) -> &'static str {
        "starrocks"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&StarrocksFactory);

struct ManticoreFactory;
impl DatabaseDriverFactory for ManticoreFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new(
            Arc::new(MysqlDriver::new(false)),
            "manticore",
        ))
    }
    fn driver_id(&self) -> &'static str {
        "manticore"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&ManticoreFactory);

struct ObOracleFactory;
impl DatabaseDriverFactory for ObOracleFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new(
            Arc::new(MysqlDriver::new(false)),
            "ob_oracle",
        ))
    }
    fn driver_id(&self) -> &'static str {
        "ob_oracle"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&ObOracleFactory);
