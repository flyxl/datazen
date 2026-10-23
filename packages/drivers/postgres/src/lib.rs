//! DataZen path driver: postgres

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod admin_commands;
mod backup_stream;
mod catalog;
mod connection;
mod control;
mod execution;
mod fixed_session;
mod migration;
mod numeric;
mod postgres;
mod resource;
mod resource_stream;
mod schema;
mod schema_object_sql;
mod session_connection;
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
pub use schema_object_sql::PostgresSchemaObjectSql;
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

/// A reuse alias factory: its 物理资源契约由本 crate 的资源机制承载，但能力快照
/// 与每个句柄的身份是别名自己的 driver id（能力声明与同一厂商父库一致，按
/// 别名精修与否在 capability matrix 中另行裁决）。
///
/// 三个单例必须同时成立：
/// * `inner` 与 `provider` 绑定同一个 [`PostgresDriver`] 实例（否则 provider
///   发出的句柄会寻址到另一组池）；
/// * `create()` 每次从同一个 `inner` 出发包一层 [`ReuseDriver`]（注册表有
///   自身缓存，池子住在 inner 里，跨调用引用同一批池）；
/// * provider_id 与 driver_id 同名。
macro_rules! postgres_alias_factory {
    ($factory:ident, $inner:ident, $provider:ident, $id:literal) => {
        struct $factory;
        static $inner: OnceLock<Arc<PostgresDriver>> = OnceLock::new();
        static $provider: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

        impl $factory {
            fn inner(&self) -> Arc<PostgresDriver> {
                $inner
                    .get_or_init(|| Arc::new(PostgresDriver::new()))
                    .clone()
            }

            fn provider(&self) -> Arc<dyn ResourceProvider> {
                $provider
                    .get_or_init(|| {
                        Arc::new(PostgresResourceProvider::with_provider_id(
                            self.inner(),
                            $id,
                        )) as Arc<dyn ResourceProvider>
                    })
                    .clone()
            }
        }

        impl DatabaseDriverFactory for $factory {
            fn create(&self) -> Arc<dyn DatabaseDriver> {
                Arc::new(ReuseDriver::new_with_precise_cancel(
                    self.inner(),
                    $id,
                    true,
                ))
            }
            fn driver_id(&self) -> &'static str {
                $id
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
            /// The alias shares the PostgreSQL resource machinery; the provider
            /// is never `None`, so the resource contract is reachable for the
            /// alias database type exactly as it is for the native pool.
            fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
                Some(self.provider())
            }
            fn resource_capabilities(&self) -> CapabilitySet {
                self.provider().capabilities().capabilities.clone()
            }
        }

        datazen_driver_api::register_driver!(&$factory);
    };
}

postgres_alias_factory!(QuestDbFactory, QUESTDB_INNER, QUESTDB_PROVIDER, "questdb");
postgres_alias_factory!(
    CloudberryFactory,
    CLOUDBERRY_INNER,
    CLOUDBERRY_PROVIDER,
    "cloudberry"
);

#[cfg(test)]
mod keyset_paging_tests;

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

    /// A reuse alias must be able to open a platform session: the factory
    /// resolves a resource provider, and pools live in the memoized inner
    /// driver that provider was built from — the same one every `create()`
    /// call wraps, so the provider's handles reach the pools the host's
    /// live-connection port hands out.
    #[test]
    fn every_alias_factory_binds_a_provider_to_the_driver_it_hands_out() {
        for (factory, id) in [
            (&QuestDbFactory as &dyn DatabaseDriverFactory, "questdb"),
            (
                &CloudberryFactory as &dyn DatabaseDriverFactory,
                "cloudberry",
            ),
        ] {
            assert_eq!(factory.driver_id(), id);

            let provider = factory.resource_provider().unwrap_or_else(|| {
                panic!("{id}: alias factories must resolve the resource contract")
            });
            assert!(
                Arc::ptr_eq(&provider, &factory.resource_provider().unwrap()),
                "{id}: the provider must be memoized; a rebuilt provider rejects \
                 every handle issued by the previous one"
            );
            assert_eq!(provider.provider_id(), id, "{id}: alias identity");
            assert!(
                !provider.capabilities().provider_id.is_empty(),
                "{id}: capability snapshot is keyed on the alias identity"
            );

            // The wrapper handed out twice shares the same inner driver —
            // asserted at the wrapper layer because the wrapper is what the
            // registry cache and the live-connection port both hold.
            let first = factory.create();
            let second = factory.create();
            assert_eq!(
                (first.driver_type(), second.driver_type()),
                (id.into(), id.into()),
                "{id}: wrappers wrap this alias id"
            );
        }
    }

    #[tokio::test]
    async fn explain_rejects_multiple_statements_before_using_any_connection() {
        let driver = PostgresDriver::new();
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
