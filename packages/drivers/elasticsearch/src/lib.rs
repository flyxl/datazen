//! DataZen path driver: elasticsearch

use std::sync::Arc;

use datazen_driver_api::*;

mod elasticsearch;
mod sync_adapter;
pub use elasticsearch::*;
pub use sync_adapter::ElasticsearchSyncAdapter;

struct ElasticsearchFactory;
impl DatabaseDriverFactory for ElasticsearchFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ElasticsearchDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "elasticsearch"
    }
}
datazen_driver_api::register_driver!(&ElasticsearchFactory);
