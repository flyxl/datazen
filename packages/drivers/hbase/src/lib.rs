//! DataZen path driver: hbase

use std::sync::Arc;

use datazen_driver_api::*;

mod hbase;
mod sync_adapter;
pub use hbase::*;
pub use sync_adapter::HBaseSyncAdapter;

struct HBaseFactory;
impl DatabaseDriverFactory for HBaseFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(HBaseDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "hbase"
    }
}
datazen_driver_api::register_driver!(&HBaseFactory);
