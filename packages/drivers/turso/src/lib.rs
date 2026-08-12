//! DataZen path driver: turso

use std::sync::Arc;

use datazen_driver_api::*;

mod turso;
pub use turso::*;

struct TursoFactory;
impl DatabaseDriverFactory for TursoFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(TursoDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "turso"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&TursoFactory);
