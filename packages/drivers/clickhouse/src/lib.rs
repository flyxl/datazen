//! DataZen path driver: clickhouse

use std::sync::Arc;

use datazen_driver_api::*;

mod clickhouse;
pub use clickhouse::*;

struct ClickHouseFactory;
impl DatabaseDriverFactory for ClickHouseFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ClickHouseDriver::new())
    }
    fn driver_id(&self) -> &'static str { "clickhouse" }
    fn supports_explain(&self) -> bool { true }
}
datazen_driver_api::register_driver!(&ClickHouseFactory);
