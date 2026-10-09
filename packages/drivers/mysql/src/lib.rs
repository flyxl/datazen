//! DataZen path driver: mysql
//!
//! Every factory in this crate memoizes its driver and its resource provider
//! together. The reason is identity, not performance: a `ResourceHandle` is
//! only meaningful against the provider that issued it, so a provider must
//! stay the same instance for the life of the process, and it must be bound
//! to the very driver [`DatabaseDriverFactory::create`] hands out. Building a
//! fresh pair per `resource_provider()` call satisfies neither — the host
//! holds one driver while the provider drives another, and a handle saved
//! across two lookups is rejected by the second.
//!
//! One `create()` per factory, once, is therefore part of the contract.
//! Calling `create()` again — or letting the provider call it — is not.

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod admin_commands;
mod migration;
mod mysql;
mod resource;
mod resource_capabilities;
mod schema_scope_mapping;
mod sql_target;
mod structure;
mod sync_adapter;
mod type_normalizer;
pub use migration::{MysqlMigrationCapabilities, MysqlMigrationRenderer};
pub use mysql::*;
use resource::MysqlResourceProvider;
use resource_capabilities::mysql_capability_set;
pub use sync_adapter::MysqlSyncAdapter;
pub use type_normalizer::MysqlTypeNormalizer;

struct MysqlFactory;

/// The one driver every caller shares, and the one [`MYSQL_PROVIDER`] is bound to.
static MYSQL_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`MYSQL_DRIVER`].
static MYSQL_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl MysqlFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        MYSQL_DRIVER
            .get_or_init(|| Arc::new(MysqlDriver::new(false)))
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        MYSQL_PROVIDER
            .get_or_init(|| {
                Arc::new(MysqlResourceProvider::new(
                    self.driver(),
                    self.driver_id(),
                    self.resource_capabilities(),
                )) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for MysqlFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "mysql"
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

    /// A real P2 resource provider over the real driver: opaque handles, a
    /// per-provider runtime epoch, and a budget permit released exactly once.
    /// `None` here would mean "not migrated", so it is never returned.
    ///
    /// Memoized, and bound to [`MysqlFactory::driver`] — the same instance
    /// `create()` returns. Rebuilding it per call would leave the host holding
    /// one driver while the provider drives another, and would reject every
    /// handle saved across two lookups.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }

    /// Derived from the factory's own declarations, so the two can never drift.
    fn resource_capabilities(&self) -> CapabilitySet {
        mysql_capability_set(self.supports_query_execution_cancel())
    }
}
datazen_driver_api::register_driver!(&MysqlFactory);

/// See [`MysqlFactory`] for why the driver and the provider are memoized
/// together rather than rebuilt per call.
struct MariadbFactory;

/// The one driver every caller shares, and the one [`MARIADB_PROVIDER`] is bound to.
static MARIADB_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`MARIADB_DRIVER`].
static MARIADB_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl MariadbFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        MARIADB_DRIVER
            .get_or_init(|| Arc::new(MysqlDriver::new(true)))
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        MARIADB_PROVIDER
            .get_or_init(|| {
                Arc::new(MysqlResourceProvider::new(
                    self.driver(),
                    self.driver_id(),
                    self.resource_capabilities(),
                )) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for MariadbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "mariadb"
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

    /// A real P2 resource provider over the real driver: opaque handles, a
    /// per-provider runtime epoch, and a budget permit released exactly once.
    /// `None` here would mean "not migrated", so it is never returned.
    ///
    /// Memoized, and bound to [`MariadbFactory::driver`] — the same instance
    /// `create()` returns. Its epoch stays separate from the other five ids,
    /// so a capability granted to one family member is never inherited by
    /// another.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }

    /// Derived from the factory's own declarations, so the two can never drift.
    fn resource_capabilities(&self) -> CapabilitySet {
        mysql_capability_set(self.supports_query_execution_cancel())
    }
}
datazen_driver_api::register_driver!(&MariadbFactory);

/// See [`MysqlFactory`] for why the driver and the provider are memoized
/// together rather than rebuilt per call.
struct DorisFactory;

/// The one driver every caller shares, and the one [`DORIS_PROVIDER`] is bound to.
static DORIS_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`DORIS_DRIVER`].
static DORIS_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl DorisFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        DORIS_DRIVER
            .get_or_init(|| {
                Arc::new(ReuseDriver::new_with_precise_cancel(
                    Arc::new(MysqlDriver::new(false)),
                    "doris",
                    true,
                ))
            })
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        DORIS_PROVIDER
            .get_or_init(|| {
                Arc::new(MysqlResourceProvider::new(
                    self.driver(),
                    self.driver_id(),
                    self.resource_capabilities(),
                )) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for DorisFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "doris"
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

    /// A real P2 resource provider over the real driver: opaque handles, a
    /// per-provider runtime epoch, and a budget permit released exactly once.
    /// `None` here would mean "not migrated", so it is never returned.
    ///
    /// Memoized, and bound to [`DorisFactory::driver`] — the same instance
    /// `create()` returns. Its epoch stays separate from the other five ids,
    /// so a capability granted to one family member is never inherited by
    /// another.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }

    /// Derived from the factory's own declarations, so the two can never drift.
    fn resource_capabilities(&self) -> CapabilitySet {
        mysql_capability_set(self.supports_query_execution_cancel())
    }
}
datazen_driver_api::register_driver!(&DorisFactory);

/// See [`MysqlFactory`] for why the driver and the provider are memoized
/// together rather than rebuilt per call.
struct StarrocksFactory;

/// The one driver every caller shares, and the one [`STARROCKS_PROVIDER`] is bound to.
static STARROCKS_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`STARROCKS_DRIVER`].
static STARROCKS_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl StarrocksFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        STARROCKS_DRIVER
            .get_or_init(|| {
                Arc::new(ReuseDriver::new_with_precise_cancel(
                    Arc::new(MysqlDriver::new(false)),
                    "starrocks",
                    true,
                ))
            })
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        STARROCKS_PROVIDER
            .get_or_init(|| {
                Arc::new(MysqlResourceProvider::new(
                    self.driver(),
                    self.driver_id(),
                    self.resource_capabilities(),
                )) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for StarrocksFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "starrocks"
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

    /// A real P2 resource provider over the real driver: opaque handles, a
    /// per-provider runtime epoch, and a budget permit released exactly once.
    /// `None` here would mean "not migrated", so it is never returned.
    ///
    /// Memoized, and bound to [`StarrocksFactory::driver`] — the same instance
    /// `create()` returns. Its epoch stays separate from the other five ids,
    /// so a capability granted to one family member is never inherited by
    /// another.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }

    /// Derived from the factory's own declarations, so the two can never drift.
    fn resource_capabilities(&self) -> CapabilitySet {
        mysql_capability_set(self.supports_query_execution_cancel())
    }
}
datazen_driver_api::register_driver!(&StarrocksFactory);

/// See [`MysqlFactory`] for why the driver and the provider are memoized
/// together rather than rebuilt per call.
struct ManticoreFactory;

/// The one driver every caller shares, and the one [`MANTICORE_PROVIDER`] is bound to.
static MANTICORE_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`MANTICORE_DRIVER`].
static MANTICORE_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl ManticoreFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        MANTICORE_DRIVER
            .get_or_init(|| {
                Arc::new(ReuseDriver::new_with_precise_cancel(
                    Arc::new(MysqlDriver::new(false)),
                    "manticore",
                    true,
                ))
            })
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        MANTICORE_PROVIDER
            .get_or_init(|| {
                Arc::new(MysqlResourceProvider::new(
                    self.driver(),
                    self.driver_id(),
                    self.resource_capabilities(),
                )) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for ManticoreFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "manticore"
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

    /// A real P2 resource provider over the real driver: opaque handles, a
    /// per-provider runtime epoch, and a budget permit released exactly once.
    /// `None` here would mean "not migrated", so it is never returned.
    ///
    /// Memoized, and bound to [`ManticoreFactory::driver`] — the same instance
    /// `create()` returns. Its epoch stays separate from the other five ids,
    /// so a capability granted to one family member is never inherited by
    /// another.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }

    /// Derived from the factory's own declarations, so the two can never drift.
    fn resource_capabilities(&self) -> CapabilitySet {
        mysql_capability_set(self.supports_query_execution_cancel())
    }
}
datazen_driver_api::register_driver!(&ManticoreFactory);

/// See [`MysqlFactory`] for why the driver and the provider are memoized
/// together rather than rebuilt per call.
struct ObOracleFactory;

/// The one driver every caller shares, and the one [`OB_ORACLE_PROVIDER`] is bound to.
static OB_ORACLE_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`OB_ORACLE_DRIVER`].
static OB_ORACLE_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl ObOracleFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        OB_ORACLE_DRIVER
            .get_or_init(|| {
                Arc::new(ReuseDriver::new_with_precise_cancel(
                    Arc::new(MysqlDriver::new(false)),
                    "ob_oracle",
                    true,
                ))
            })
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        OB_ORACLE_PROVIDER
            .get_or_init(|| {
                Arc::new(MysqlResourceProvider::new(
                    self.driver(),
                    self.driver_id(),
                    self.resource_capabilities(),
                )) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for ObOracleFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "ob_oracle"
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

    /// A real P2 resource provider over the real driver: opaque handles, a
    /// per-provider runtime epoch, and a budget permit released exactly once.
    /// `None` here would mean "not migrated", so it is never returned.
    ///
    /// Memoized, and bound to [`ObOracleFactory::driver`] — the same instance
    /// `create()` returns. Its epoch stays separate from the other five ids,
    /// so a capability granted to one family member is never inherited by
    /// another.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }

    /// Derived from the factory's own declarations, so the two can never drift.
    fn resource_capabilities(&self) -> CapabilitySet {
        mysql_capability_set(self.supports_query_execution_cancel())
    }
}
datazen_driver_api::register_driver!(&ObOracleFactory);

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract, asserted per id: one driver and one provider each. The
    /// host calls `resource_provider()` once per resource lookup, so a
    /// provider rebuilt per call fails here — and would then reject every
    /// handle saved across two lookups.
    #[test]
    fn every_family_factory_memoizes_its_driver_and_its_provider() {
        let factories: [&dyn DatabaseDriverFactory; 6] = [
            &MysqlFactory,
            &MariadbFactory,
            &DorisFactory,
            &StarrocksFactory,
            &ManticoreFactory,
            &ObOracleFactory,
        ];

        for factory in factories {
            let id = factory.driver_id();
            assert!(
                Arc::ptr_eq(&factory.create(), &factory.create()),
                "{id}: create() must not build a second driver, the provider is bound to the one it hands out"
            );

            let first = factory
                .resource_provider()
                .unwrap_or_else(|| panic!("{id} implements the resource provider contract"));
            let second = factory
                .resource_provider()
                .unwrap_or_else(|| panic!("{id} implements the resource provider contract"));
            assert!(
                Arc::ptr_eq(&first, &second),
                "{id}: resource_provider() must be memoized, a rebuilt provider rejects every handle issued by the previous one"
            );
            assert_eq!(first.provider_id(), id);
        }
    }

    /// Memoizing is per id, not per crate: the six providers stay distinct, so
    /// the epoch isolation the family relies on is untouched.
    #[test]
    fn each_family_id_keeps_its_own_provider() {
        let factories: [&dyn DatabaseDriverFactory; 6] = [
            &MysqlFactory,
            &MariadbFactory,
            &DorisFactory,
            &StarrocksFactory,
            &ManticoreFactory,
            &ObOracleFactory,
        ];

        for (index, factory) in factories.iter().enumerate() {
            for other in &factories[index + 1..] {
                assert!(
                    !Arc::ptr_eq(
                        &factory.resource_provider().expect("migrated"),
                        &other.resource_provider().expect("migrated")
                    ),
                    "{} and {} must not share one provider: a capability granted to one id is never inherited by another",
                    factory.driver_id(),
                    other.driver_id()
                );
            }
        }
    }

    #[test]
    fn mysql_factory_advertises_precise_cancellation_for_mysql_family_servers() {
        let factories: [&dyn DatabaseDriverFactory; 6] = [
            &MysqlFactory,
            &MariadbFactory,
            &DorisFactory,
            &StarrocksFactory,
            &ManticoreFactory,
            &ObOracleFactory,
        ];

        assert!(factories[0].supports_cancel_query());
        assert!(factories[0].supports_query_execution_cancel());
        assert!(!factories[1].supports_cancel_query());
        assert!(factories[1].supports_query_execution_cancel());
        assert!(!factories[2].supports_cancel_query());
        assert!(factories[2].supports_query_execution_cancel());
        assert!(!factories[3].supports_cancel_query());
        assert!(factories[3].supports_query_execution_cancel());
        assert!(!factories[4].supports_cancel_query());
        assert!(factories[4].supports_query_execution_cancel());
        assert!(!factories[5].supports_cancel_query());
        assert!(factories[5].supports_query_execution_cancel());

        assert!(MysqlDriver::new(false).supports_query_execution_cancel());
        assert!(MysqlDriver::new(true).supports_query_execution_cancel());
        assert!(DorisFactory.create().supports_query_execution_cancel());
        assert!(StarrocksFactory.create().supports_query_execution_cancel());
        assert!(ManticoreFactory.create().supports_query_execution_cancel());
        assert!(ObOracleFactory.create().supports_query_execution_cancel());
    }
}
