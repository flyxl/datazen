//! DataZen path driver: sqlserver

use std::sync::Arc;

use datazen_driver_api::*;

mod sqlserver;
pub use sqlserver::*;

struct SqlServerFactory;
impl DatabaseDriverFactory for SqlServerFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(SqlServerDriver::new())
    }
    fn driver_id(&self) -> &'static str { "sqlserver" }
    fn supports_explain(&self) -> bool { true }
}
datazen_driver_api::register_driver!(&SqlServerFactory);
