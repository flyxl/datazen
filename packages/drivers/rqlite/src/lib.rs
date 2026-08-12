//! DataZen path driver: rqlite

use std::sync::Arc;

use datazen_driver_api::*;

mod rqlite;
pub use rqlite::*;

struct RqliteFactory;
impl DatabaseDriverFactory for RqliteFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(RqliteDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "rqlite"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&RqliteFactory);
