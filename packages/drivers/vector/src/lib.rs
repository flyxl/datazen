//! DataZen path driver: vector

use std::sync::Arc;

use datazen_driver_api::*;

mod sync_adapter;
mod vector;
pub use sync_adapter::VectorSyncAdapter;
pub use vector::*;

struct VectorFactory;
impl DatabaseDriverFactory for VectorFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(VectorDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "vector"
    }
}
datazen_driver_api::register_driver!(&VectorFactory);
