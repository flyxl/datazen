//! DataZen path driver: mysql

use std::sync::Arc;

use datazen_driver_api::*;

mod admin_commands;
mod migration;
mod mysql;
mod sql_target;
mod structure;
mod sync_adapter;
pub use migration::{MysqlMigrationCapabilities, MysqlMigrationRenderer};
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
    fn supports_cancel_query(&self) -> bool {
        true
    }
    fn supports_query_execution_cancel(&self) -> bool {
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
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&MariadbFactory);

struct DorisFactory;
impl DatabaseDriverFactory for DorisFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(MysqlDriver::new(false)),
            "doris",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "doris"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&DorisFactory);

struct StarrocksFactory;
impl DatabaseDriverFactory for StarrocksFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(MysqlDriver::new(false)),
            "starrocks",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "starrocks"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&StarrocksFactory);

struct ManticoreFactory;
impl DatabaseDriverFactory for ManticoreFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(MysqlDriver::new(false)),
            "manticore",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "manticore"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&ManticoreFactory);

struct ObOracleFactory;
impl DatabaseDriverFactory for ObOracleFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(MysqlDriver::new(false)),
            "ob_oracle",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "ob_oracle"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&ObOracleFactory);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_factory_advertises_precise_cancellation_for_mysql_family_servers() {
        let factories: [&dyn DatabaseDriverFactory; 6] = [
            &MysqlFactory,
            &MariadbFactory,
            &DorisFactory,
            &StarrocksFactory,
            &ManticoreFactory,
            &ObOracleFactory,
        ];

        assert!(factories[0].supports_cancel_query());
        assert!(factories[0].supports_query_execution_cancel());
        assert!(!factories[1].supports_cancel_query());
        assert!(factories[1].supports_query_execution_cancel());
        assert!(!factories[2].supports_cancel_query());
        assert!(factories[2].supports_query_execution_cancel());
        assert!(!factories[3].supports_cancel_query());
        assert!(factories[3].supports_query_execution_cancel());
        assert!(!factories[4].supports_cancel_query());
        assert!(factories[4].supports_query_execution_cancel());
        assert!(!factories[5].supports_cancel_query());
        assert!(factories[5].supports_query_execution_cancel());

        assert!(MysqlDriver::new(false).supports_query_execution_cancel());
        assert!(MysqlDriver::new(true).supports_query_execution_cancel());
        assert!(DorisFactory.create().supports_query_execution_cancel());
        assert!(StarrocksFactory.create().supports_query_execution_cancel());
        assert!(ManticoreFactory.create().supports_query_execution_cancel());
        assert!(ObOracleFactory.create().supports_query_execution_cancel());
    }
}
