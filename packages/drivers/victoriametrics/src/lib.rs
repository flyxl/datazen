//! DataZen path driver: victoriametrics

use std::sync::Arc;

use datazen_driver_api::*;

mod victoriametrics;
pub use victoriametrics::*;

struct VictoriaMetricsFactory;
impl DatabaseDriverFactory for VictoriaMetricsFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(VictoriaMetricsDriver::new())
    }
    fn driver_id(&self) -> &'static str { "victoriametrics" }
}
datazen_driver_api::register_driver!(&VictoriaMetricsFactory);
