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
mod backup_stream;
mod fixed_session;
mod migration;
mod mysql;
mod resource;
mod resource_capabilities;
mod schema_object_sql;
mod schema_scope_mapping;
mod session_connection;
mod sql_target;
mod structure;
mod sync_adapter;
mod type_normalizer;
pub use migration::{MysqlMigrationCapabilities, MysqlMigrationRenderer};
pub use mysql::*;
use resource::MysqlResourceProvider;
use resource_capabilities::mysql_capability_set;
pub use schema_object_sql::MysqlSchemaObjectSql;
pub use sync_adapter::MysqlSyncAdapter;
pub use type_normalizer::MysqlTypeNormalizer;

/// A wire-family member’s protocol adapter. Only the compatibility
/// aliases use ReuseDriver; MariaDB has distinct discovery on MysqlDriver.
#[derive(Clone, Copy)]
enum FamilyProtocol {
    Mysql,
    Mariadb,
    ReusedMysql,
}

/// One registration owns its own lazy pair. Keeping construction here makes
/// cancellation and provider wiring a shared contract instead of six copies
/// that can diverge. Different registrations never share a driver or epoch.
struct MysqlFamilyFactory {
    id: &'static str,
    protocol: FamilyProtocol,
    driver: OnceLock<Arc<dyn DatabaseDriver>>,
    provider: OnceLock<Arc<dyn ResourceProvider>>,
}

impl MysqlFamilyFactory {
    const fn new(id: &'static str, protocol: FamilyProtocol) -> Self {
        Self {
            id,
            protocol,
            driver: OnceLock::new(),
            provider: OnceLock::new(),
        }
    }
}

impl DatabaseDriverFactory for MysqlFamilyFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver
            .get_or_init(|| match self.protocol {
                FamilyProtocol::Mysql => Arc::new(MysqlDriver::new(false)),
                FamilyProtocol::Mariadb => Arc::new(MysqlDriver::new(true)),
                FamilyProtocol::ReusedMysql => Arc::new(ReuseDriver::new_with_precise_cancel(
                    Arc::new(MysqlDriver::new(false)),
                    self.id,
                    self.supports_query_execution_cancel(),
                )),
            })
            .clone()
    }

    fn driver_id(&self) -> &'static str {
        self.id
    }

    fn supports_explain(&self) -> bool {
        true
    }

    fn supports_cancel_query(&self) -> bool {
        matches!(self.protocol, FamilyProtocol::Mysql)
    }

    fn supports_query_execution_cancel(&self) -> bool {
        true
    }

    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(
            self.provider
                .get_or_init(|| {
                    Arc::new(MysqlResourceProvider::new(
                        self.create(),
                        self.driver_id(),
                        self.resource_capabilities(),
                    ))
                })
                .clone(),
        )
    }

    fn resource_capabilities(&self) -> CapabilitySet {
        mysql_capability_set(self.supports_query_execution_cancel())
    }
}

static MYSQL_FACTORY: MysqlFamilyFactory = MysqlFamilyFactory::new("mysql", FamilyProtocol::Mysql);
static MARIADB_FACTORY: MysqlFamilyFactory =
    MysqlFamilyFactory::new("mariadb", FamilyProtocol::Mariadb);
static DORIS_FACTORY: MysqlFamilyFactory =
    MysqlFamilyFactory::new("doris", FamilyProtocol::ReusedMysql);
static STARROCKS_FACTORY: MysqlFamilyFactory =
    MysqlFamilyFactory::new("starrocks", FamilyProtocol::ReusedMysql);
static MANTICORE_FACTORY: MysqlFamilyFactory =
    MysqlFamilyFactory::new("manticore", FamilyProtocol::ReusedMysql);
static OB_ORACLE_FACTORY: MysqlFamilyFactory =
    MysqlFamilyFactory::new("ob_oracle", FamilyProtocol::ReusedMysql);

datazen_driver_api::register_driver!(&MYSQL_FACTORY);
datazen_driver_api::register_driver!(&MARIADB_FACTORY);
datazen_driver_api::register_driver!(&DORIS_FACTORY);
datazen_driver_api::register_driver!(&STARROCKS_FACTORY);
datazen_driver_api::register_driver!(&MANTICORE_FACTORY);
datazen_driver_api::register_driver!(&OB_ORACLE_FACTORY);

/// The host's entry into this crate (E4): the six MySQL-protocol families —
/// mysql, mariadb, doris, starrocks, manticore and ob_oracle — the same
/// factories `register_driver!` submits, in the list form a host chains
/// with its other drivers for static registry assembly
/// (`DriverRegistry::with_factories`) instead of inventory scanning at
/// link time.
pub fn factories() -> Vec<&'static dyn datazen_driver_api::DatabaseDriverFactory> {
    vec![
        &MYSQL_FACTORY,
        &MARIADB_FACTORY,
        &DORIS_FACTORY,
        &STARROCKS_FACTORY,
        &MANTICORE_FACTORY,
        &OB_ORACLE_FACTORY,
    ]
}

#[cfg(test)]
mod keyset_paging_tests;

#[cfg(test)]
mod result_origin_tests;

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
            &MYSQL_FACTORY,
            &MARIADB_FACTORY,
            &DORIS_FACTORY,
            &STARROCKS_FACTORY,
            &MANTICORE_FACTORY,
            &OB_ORACLE_FACTORY,
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
            &MYSQL_FACTORY,
            &MARIADB_FACTORY,
            &DORIS_FACTORY,
            &STARROCKS_FACTORY,
            &MANTICORE_FACTORY,
            &OB_ORACLE_FACTORY,
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
            &MYSQL_FACTORY,
            &MARIADB_FACTORY,
            &DORIS_FACTORY,
            &STARROCKS_FACTORY,
            &MANTICORE_FACTORY,
            &OB_ORACLE_FACTORY,
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
        assert!(DORIS_FACTORY.create().supports_query_execution_cancel());
        assert!(STARROCKS_FACTORY.create().supports_query_execution_cancel());
        assert!(MANTICORE_FACTORY.create().supports_query_execution_cancel());
        assert!(OB_ORACLE_FACTORY.create().supports_query_execution_cancel());
    }

    #[tokio::test]
    async fn explain_rejects_multiple_statements_before_using_any_connection() {
        let driver = MysqlDriver::new(false);
        let handle = ConnectionHandle {
            id: "absent".into(),
            pool_id: "absent".into(),
        };
        for sql in ["", "SELECT 1; DELETE FROM synthetic_probe"] {
            let error = driver
                .explain(&handle, sql)
                .await
                .expect_err("one statement required");
            assert!(matches!(error, DriverError::InvalidConfig(_)));
        }
    }
}
