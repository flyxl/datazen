//! DataZen path driver: sqlite
//!
//! The factory memoizes its driver and its resource provider together. The
//! reason is identity, not performance: a `ResourceHandle` is only meaningful
//! against the provider that issued it, so the provider must stay the same
//! instance for the life of the process, and it must be bound to the very
//! driver [`DatabaseDriverFactory::create`] hands out. Building a fresh pair
//! per `resource_provider()` call satisfies neither — the host holds one driver
//! while the provider drives another, and a handle saved across two lookups is
//! rejected by the second.
//!
//! One `create()`, once, is therefore part of the contract. Calling `create()`
//! again — or letting the provider call it — is not.

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod adb;
mod migration;
mod resource;
mod resource_capabilities;
mod schema_object_sql;
mod sql_target;
mod sqlite;
mod structure;
mod sync_adapter;
mod type_normalizer;
pub use migration::{SqliteMigrationCapabilities, SqliteMigrationRenderer};
use resource::SqliteResourceProvider;
pub use schema_object_sql::SqliteSchemaObjectSql;
pub use sqlite::*;
pub use sync_adapter::SqliteSyncAdapter;
pub use type_normalizer::SqliteTypeNormalizer;

struct SqliteFactory;

/// The one driver every caller shares, and the one [`SQLITE_PROVIDER`] is bound to.
static SQLITE_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`SQLITE_DRIVER`].
static SQLITE_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl SqliteFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        SQLITE_DRIVER
            .get_or_init(|| Arc::new(SqliteDriver::new()))
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        SQLITE_PROVIDER
            .get_or_init(|| {
                Arc::new(SqliteResourceProvider::new(self.driver(), self.driver_id()))
                    as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for SqliteFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "sqlite"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    /// Memoized, and bound to [`SqliteFactory::driver`] — the same instance
    /// `create()` returns. Rebuilding it per call would leave the host holding
    /// one driver while the provider drives another, and would reject every
    /// handle saved across two lookups.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }
    fn resource_capabilities(&self) -> CapabilitySet {
        resource_capabilities::sqlite_capability_set()
    }
}
datazen_driver_api::register_driver!(&SqliteFactory);

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract, asserted: one driver per factory and one provider per
    /// factory. The host calls `resource_provider()` once per resource lookup,
    /// so a provider that is rebuilt per call fails here — and would then
    /// reject every handle saved across two lookups.
    #[test]
    fn create_and_resource_provider_are_memoized() {
        let factory = SqliteFactory;

        let first_driver = factory.create();
        let second_driver = factory.create();
        assert!(
            Arc::ptr_eq(&first_driver, &second_driver),
            "create() must not build a second driver: the provider is bound to the one it hands out"
        );

        let first = factory
            .resource_provider()
            .expect("sqlite implements the resource provider contract");
        let second = factory
            .resource_provider()
            .expect("sqlite implements the resource provider contract");
        assert!(
            Arc::ptr_eq(&first, &second),
            "resource_provider() must be memoized: a rebuilt provider rejects every handle issued by the previous one"
        );
        assert_eq!(first.provider_id(), factory.driver_id());
    }

    /// CM-19 / P4 Explain boundary. The pooled `supports_explain()` is real
    /// (`SqliteDriver::explain` exists and stays advertised), but the
    /// session-bound `explain` **Command** is opt-in: `execute_standard_sql_command`
    /// only routes `explain` to the driver when the driver declares the command,
    /// so an older driver's pooled method can never stand in for a retained
    /// editor resource.
    ///
    /// This pins the *command surface*: if someone declares `explain` here to make
    /// the Explain panel go green, this test turns red instead of silently
    /// planning on a one-shot pool connection while the driver still reports
    /// `stateful_session = Unsupported`.
    #[tokio::test]
    async fn explain_is_pooled_only_and_not_a_declared_session_command() {
        let factory = SqliteFactory;
        assert!(
            factory.supports_explain(),
            "the pooled capability is real and must stay honest: this test pins the command surface, not the existence of `SqliteDriver::explain`"
        );

        let driver = factory.create();
        let ids: Vec<String> = driver
            .command_definitions()
            .into_iter()
            .map(|definition| definition.id)
            .collect();
        assert!(
            !ids.iter().any(|id| id == "explain"),
            "sqlite declares stateful_session = Unsupported, so it must not advertise the \
             session-bound `explain` Command; declared commands: {ids:?}"
        );

        let handle = ConnectionHandle {
            id: "session".into(),
            pool_id: "pool".into(),
        };
        let error = driver
            .execute_command(&handle, "explain", serde_json::json!({"sql": "SELECT 1"}))
            .await
            .expect_err("an undeclared `explain` must be refused, not answered from the pool");
        assert!(
            matches!(error, DriverError::Unsupported(_)),
            "expected Unsupported for an undeclared command, got {error:?}"
        );
    }
}
