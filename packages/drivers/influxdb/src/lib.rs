//! DataZen path driver: influxdb

use std::sync::Arc;

use datazen_driver_api::*;

mod influxdb;
mod sync_adapter;
pub use influxdb::*;
pub use sync_adapter::InfluxDbSyncAdapter;

struct InfluxDbFactory;
impl DatabaseDriverFactory for InfluxDbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(InfluxDbDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "influxdb"
    }
}
datazen_driver_api::register_driver!(&InfluxDbFactory);
