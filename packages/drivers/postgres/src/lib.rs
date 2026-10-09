//! DataZen path driver: postgres

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod admin_commands;
mod catalog;
mod connection;
mod execution;
mod migration;
mod numeric;
mod postgres;
mod resource;
mod schema;
mod sql;
mod sql_target;
mod structure;
mod sync_adapter;
mod transfer_identity;
mod type_decode;
mod type_normalizer;
pub use migration::{PostgresMigrationCapabilities, PostgresMigrationRenderer};
pub use postgres::*;
pub use resource::PostgresResourceProvider;
pub use structure::{caps_for_version, plan_structure_changes_with_caps};
pub use sync_adapter::PgSyncAdapter;
pub use type_normalizer::PostgresTypeNormalizer;

/// The PostgreSQL factory.
///
/// The driver and its resource provider are memoized together on purpose: the
/// provider must be bound to *the same* [`PostgresDriver`] the host gets from
/// [`DatabaseDriverFactory::create`], otherwise a handle minted by the provider
/// would address a different set of pools than the one the host is using.
struct PostgresFactory;

/// The one driver every caller shares.
static POSTGRES_DRIVER: OnceLock<Arc<PostgresDriver>> = OnceLock::new();
/// The one provider, bound to [`POSTGRES_DRIVER`].
static POSTGRES_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl PostgresFactory {
    fn driver(&self) -> Arc<PostgresDriver> {
        POSTGRES_DRIVER
            .get_or_init(|| Arc::new(PostgresDriver::new()))
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        POSTGRES_PROVIDER
            .get_or_init(|| {
                Arc::new(PostgresResourceProvider::new(self.driver())) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for PostgresFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "postgresql"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        true
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
    /// PostgreSQL really implements the resource contract: the returned
    /// provider runs this crate's pools, execution registry and control pool.
    /// It is never `None`, so a caller that wants the resource contract always
    /// gets a real one.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }
    /// Declared by [`resource::capabilities`], whose table maps every entry to
    /// the code that backs it. Unsupported entries are refused with an
    /// explicit error by the provider rather than silently accepted.
    fn resource_capabilities(&self) -> CapabilitySet {
        self.provider().capabilities().capabilities.clone()
    }
}

static POSTGRES_FACTORY: PostgresFactory = PostgresFactory;
datazen_driver_api::register_driver!(&POSTGRES_FACTORY);

struct QuestDbFactory;
impl DatabaseDriverFactory for QuestDbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(PostgresDriver::new()),
            "questdb",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "questdb"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&QuestDbFactory);

struct CloudberryFactory;
impl DatabaseDriverFactory for CloudberryFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        // Cloudberry is a PostgreSQL derivative and uses the same backend PID
        // and pg_cancel_backend control protocol as native PostgreSQL.
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(PostgresDriver::new()),
            "cloudberry",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "cloudberry"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&CloudberryFactory);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_factory_advertises_precise_cancellation_by_backend_compatibility() {
        let factories: [&dyn DatabaseDriverFactory; 3] =
            [&PostgresFactory, &QuestDbFactory, &CloudberryFactory];

        assert!(factories[0].supports_cancel_query());
        assert!(factories[0].supports_query_execution_cancel());
        assert!(!factories[1].supports_cancel_query());
        assert!(factories[1].supports_query_execution_cancel());
        assert!(!factories[2].supports_cancel_query());
        assert!(factories[2].supports_query_execution_cancel());

        assert!(QuestDbFactory.create().supports_query_execution_cancel());
        assert!(CloudberryFactory.create().supports_query_execution_cancel());
    }
}
