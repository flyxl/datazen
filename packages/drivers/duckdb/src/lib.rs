//! DataZen path driver: duckdb

use std::sync::Arc;

use datazen_driver_api::*;

mod duckdb;
mod sync_adapter;
pub use duckdb::*;
pub use sync_adapter::DuckDbSyncAdapter;

struct DuckDbFactory;
impl DatabaseDriverFactory for DuckDbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(DuckDbDriver::new())
    }
    fn driver_id(&self) -> &'static str { "duckdb" }
    fn supports_explain(&self) -> bool { true }
}
datazen_driver_api::register_driver!(&DuckDbFactory);
