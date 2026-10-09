//! Driver registry — resolves `DatabaseType` to a concrete `DatabaseDriver`.
//!
//! Drivers are discovered via `inventory` factories from optional path/git
//! driver crates linked into the host binary.

use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Static capabilities advertised by a driver factory.
///
/// These flags describe whether the driver has a meaningful implementation;
/// they are not inferred from the presence of a trait method because the
/// trait intentionally has a default unsupported/no-op implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCapabilities {
    pub supports_cancel_query: bool,
    pub supports_query_execution_cancel: bool,
    pub supports_explain: bool,
    pub supports_streaming_results: bool,
    /// Whether the dialect accepts `OFFSET` in pagination. `false` for
    /// Presto/Hive-family engines, which paginate with `LIMIT` only.
    pub supports_offset: bool,
    /// Whether the engine has a real second namespace level (`schema`).
    /// `true` only for PostgreSQL and SQL Server; every other driver carries the
    /// whole namespace in `database`, so sending it a schema is an error rather
    /// than a hint.
    pub has_schema_level: bool,
    /// Whether one connection can address more than one `database`. Drives
    /// the "you must name a database" rule for workflows, whose steps carry
    /// no session of their own to inherit a default from.
    pub has_multi_database: bool,
}

impl DriverCapabilities {
    fn from_factory(factory: &dyn DatabaseDriverFactory, driver: &dyn DatabaseDriver) -> Self {
        Self {
            supports_cancel_query: factory.supports_cancel_query(),
            supports_query_execution_cancel: factory.supports_query_execution_cancel()
                && driver.supports_query_execution_cancel(),
            supports_explain: factory.supports_explain(),
            supports_streaming_results: factory.supports_streaming_results(),
            supports_offset: driver.supports_offset(),
            has_schema_level: driver.has_schema_level(),
            has_multi_database: driver.has_multi_database(),
        }
    }
}

/// Why a driver type has no resource provider to hand out.
///
/// Both variants are errors. `NotRegistered` means this build cannot supply
/// the driver at all; `Missing` means the driver *is* here and has not been
/// migrated to the resource contract. Callers must not read either as "this
/// driver has nothing to do".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceProviderLookup {
    /// The driver type could not be loaded in this build.
    #[error("driver type '{driver_type}' is not registered in this build: {reason}")]
    NotRegistered { driver_type: String, reason: String },
    /// The driver is registered but ships no resource provider.
    #[error(transparent)]
    Missing(#[from] ResourceProviderMissing),
}

/// Holds registered drivers. Starts empty; call [`DriverRegistry::ensure_type`]
/// (or rely on [`DriverRegistry::get`]) to load a type on demand.
pub struct DriverRegistry {
    drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn DatabaseDriver>>>>,
    kv_drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn KeyValueDriver>>>>,
    capabilities: Arc<RwLock<HashMap<DatabaseType, DriverCapabilities>>>,
    /// The factory each registered driver came from.
    ///
    /// A factory is the only thing that can produce a `ResourceProvider`, so
    /// this is the host's route to one. The factory is stored — never the
    /// provider — because a provider's identity is the factory's to promise:
    /// a driver that memoizes returns the same instance forever, and a driver
    /// that does not would silently invalidate the handles it already issued
    /// if the host cached a snapshot taken at some arbitrary moment.
    /// `every_linked_factory_resolves_or_reports_why_not` pins which drivers
    /// make that promise today.
    factories: Arc<RwLock<HashMap<DatabaseType, &'static dyn DatabaseDriverFactory>>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Arc::new(RwLock::new(HashMap::new())),
            kv_drivers: Arc::new(RwLock::new(HashMap::new())),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
            factories: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Catalog of types this build can provide (inventory factories only).
    /// Does **not** instantiate any driver.
    pub fn available_types(&self) -> Vec<DatabaseType> {
        let mut types: Vec<DatabaseType> = Vec::new();
        for factory in iter_driver_factories() {
            let id = factory.driver_id().to_string();
            if !types.iter().any(|t| t == &id) {
                types.push(id);
            }
        }
        types
    }

    /// Ensure drivers for every distinct type in `types` are loaded.
    pub async fn ensure_types<I, S>(&self, types: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for t in types {
            let db_type = t.as_ref();
            if let Err(e) = self.ensure_type(db_type).await {
                tracing::warn!(db_type, error = %e, "Failed to preload driver");
            }
        }
    }

    /// Lazily construct and register the driver for `db_type` if missing.
    pub async fn ensure_type(&self, db_type: &str) -> Result<(), String> {
        {
            let drivers = self.drivers.read().await;
            if drivers.contains_key(db_type) {
                return Ok(());
            }
        }

        let mut drivers = self.drivers.write().await;
        if drivers.contains_key(db_type) {
            return Ok(());
        }

        self.register_from_inventory(db_type, &mut drivers).await?;
        tracing::info!(db_type, "Registered driver on demand");
        Ok(())
    }

    async fn register_from_inventory(
        &self,
        db_type: &str,
        drivers: &mut HashMap<DatabaseType, Arc<dyn DatabaseDriver>>,
    ) -> Result<(), String> {
        for factory in iter_driver_factories() {
            if factory.driver_id() != db_type {
                continue;
            }
            let pv = factory.protocol_version();
            if pv < datazen_driver_api::MIN_PROTOCOL_VERSION {
                return Err(format!(
                    "Driver '{}' protocol version {} is too old (minimum {})",
                    factory.driver_id(),
                    pv,
                    datazen_driver_api::MIN_PROTOCOL_VERSION
                ));
            }
            if pv > datazen_driver_api::PROTOCOL_VERSION {
                tracing::warn!(
                    "Driver '{}' protocol version {} is newer than host {}. Loading with possible incompatibility.",
                    factory.driver_id(),
                    pv,
                    datazen_driver_api::PROTOCOL_VERSION
                );
            } else if pv < datazen_driver_api::PROTOCOL_VERSION {
                tracing::warn!(
                    "Driver '{}' protocol version {} < host {}. Running in degraded mode \
                     (cancel_query={}, explain={}, streaming={}).",
                    factory.driver_id(),
                    pv,
                    datazen_driver_api::PROTOCOL_VERSION,
                    factory.supports_cancel_query(),
                    factory.supports_explain(),
                    factory.supports_streaming_results(),
                );
            }

            let driver = factory.create();
            let actual = driver.driver_type();
            let capabilities = DriverCapabilities::from_factory(*factory, driver.as_ref());
            {
                let mut capability_map = self.capabilities.write().await;
                capability_map.insert(db_type.to_string(), capabilities);
                if actual != db_type {
                    capability_map.insert(actual.clone(), capabilities);
                }
            }
            if let Some(kv) = factory.create_kv() {
                let mut kv_map = self.kv_drivers.write().await;
                kv_map.insert(kv.driver_type(), kv);
            }
            // The driver and its factory are registered under the same keys, so
            // the provider a caller resolves is backed by the driver this
            // registry handed out, never by a second `create()`.
            {
                let mut factory_map = self.factories.write().await;
                factory_map.insert(db_type.to_string(), *factory);
                factory_map.insert(actual.clone(), *factory);
            }
            if actual != db_type {
                drivers.insert(actual, driver.clone());
            }
            drivers.insert(db_type.to_string(), driver);
            return Ok(());
        }
        Err(format!("No driver available for database type '{db_type}'"))
    }

    /// Look up a driver, loading it on demand if needed.
    pub async fn get(&self, db_type: &DatabaseType) -> Option<Arc<dyn DatabaseDriver>> {
        if let Err(e) = self.ensure_type(db_type).await {
            tracing::warn!(db_type = %db_type, error = %e, "Driver ensure failed");
            return None;
        }
        let drivers = self.drivers.read().await;
        drivers.get(db_type).cloned()
    }

    /// Return the static capabilities advertised by the registered factory.
    /// `None` means the driver was injected by a test/legacy integration that
    /// does not expose capability metadata yet; callers must treat that as
    /// unknown rather than supported.
    pub async fn get_capabilities(&self, db_type: &DatabaseType) -> Option<DriverCapabilities> {
        // Keep capability lookup aligned with driver lookup: a connection-info
        // request must not depend on a separate, eagerly populated registry.
        // `ensure_type` is a no-op for legacy/test registrations, so those
        // drivers deliberately remain `None` (unknown).
        if let Err(error) = self.ensure_type(db_type).await {
            tracing::debug!(db_type = %db_type, %error, "Driver capability lookup failed");
            return None;
        }
        let capabilities = self.capabilities.read().await;
        capabilities.get(db_type).copied()
    }

    /// Types currently loaded in memory (for diagnostics). Prefer
    /// [`available_types`] for UI catalogs.
    pub async fn loaded_types(&self) -> Vec<DatabaseType> {
        let drivers = self.drivers.read().await;
        drivers.keys().cloned().collect()
    }

    /// Resolve the opaque-resource provider of a registered driver.
    ///
    /// Fail-closed, and never a silent no-op: a driver that has not been
    /// migrated to the resource contract comes back as
    /// [`ResourceProviderMissing`], which names the driver. The two error
    /// cases are deliberately *different* — a type this build has not loaded
    /// is not the same as a type that was loaded and has no provider, and
    /// collapsing them would resurrect exactly the mistake the accessor exists
    /// to prevent.
    ///
    /// Every call re-resolves through the registry's own factory rather than
    /// caching a provider. The providers that memoize hand back the same
    /// instance, so their handles stay valid; the ones that do not are a driver
    /// defect this accessor deliberately does not paper over.
    pub async fn resource_provider(
        &self,
        db_type: &DatabaseType,
    ) -> Result<Arc<dyn ResourceProvider>, ResourceProviderLookup> {
        if let Err(error) = self.ensure_type(db_type).await {
            return Err(ResourceProviderLookup::NotRegistered {
                driver_type: db_type.clone(),
                reason: error,
            });
        }
        let factory = *self.factories.read().await.get(db_type).ok_or({
            ResourceProviderLookup::NotRegistered {
                driver_type: db_type.clone(),
                reason: "not backed by an inventory factory".to_string(),
            }
        })?;
        require_resource_provider(factory).map_err(ResourceProviderLookup::Missing)
    }

    pub async fn get_kv_driver(&self, db_type: &DatabaseType) -> Option<Arc<dyn KeyValueDriver>> {
        if let Err(e) = self.ensure_type(db_type).await {
            tracing::warn!(db_type = %db_type, error = %e, "KV driver ensure failed");
            return None;
        }
        let kv_drivers = self.kv_drivers.read().await;
        kv_drivers.get(db_type).cloned()
    }

    /// Look up a SQL driver by its type string, loading on demand.
    pub async fn get_sql_driver_by_name(&self, name: &str) -> Option<Arc<dyn DatabaseDriver>> {
        self.get(&name.to_string()).await
    }

    /// Register a driver instance for unit tests (bypasses inventory).
    #[cfg(any(test, feature = "test-harness"))]
    pub async fn register_test_driver(
        &self,
        db_type: impl Into<DatabaseType>,
        driver: Arc<dyn DatabaseDriver>,
    ) {
        let db_type = db_type.into();
        self.drivers.write().await.insert(db_type.clone(), driver);
        // Replacing a previously factory-registered driver with a legacy test
        // driver must not leave stale capability metadata behind.
        self.capabilities.write().await.remove(&db_type);
    }

    /// Register a test driver together with explicit capability metadata.
    #[cfg(any(test, feature = "test-harness"))]
    pub async fn register_test_driver_with_capabilities(
        &self,
        db_type: impl Into<DatabaseType>,
        driver: Arc<dyn DatabaseDriver>,
        capabilities: DriverCapabilities,
    ) {
        let db_type = db_type.into();
        self.drivers.write().await.insert(db_type.clone(), driver);
        self.capabilities
            .write()
            .await
            .insert(db_type, capabilities);
    }

    /// Register a KV driver instance for unit tests (bypasses inventory).
    #[cfg(test)]
    pub async fn register_test_kv_driver(
        &self,
        db_type: impl Into<DatabaseType>,
        kv: Arc<dyn KeyValueDriver>,
    ) {
        self.kv_drivers.write().await.insert(db_type.into(), kv);
    }

    /// Register the factory a type's driver came from, for unit tests.
    ///
    /// Inventory only supplies real driver factories, so a test that needs a
    /// specific provider behaviour has to hand one over directly. Pair it with
    /// [`Self::register_test_driver`] so `ensure_type` has nothing left to load.
    #[cfg(any(test, feature = "test-harness"))]
    pub async fn register_test_factory(
        &self,
        db_type: impl Into<DatabaseType>,
        factory: &'static dyn DatabaseDriverFactory,
    ) {
        self.factories.write().await.insert(db_type.into(), factory);
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create an empty registry. Drivers load via [`DriverRegistry::ensure_type`].
pub fn init_drivers() -> DriverRegistry {
    DriverRegistry::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::capabilities::CapabilitySet;
    use datazen_driver_api::namespace::NamespaceShape;
    use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
    use std::sync::OnceLock;

    fn capabilities(supports_cancel_query: bool) -> DriverCapabilities {
        DriverCapabilities {
            supports_offset: true,
            has_schema_level: false,
            has_multi_database: false,
            supports_cancel_query,
            supports_query_execution_cancel: supports_cancel_query,
            supports_explain: true,
            supports_streaming_results: true,
        }
    }

    #[tokio::test]
    async fn get_capabilities_returns_registered_metadata_by_type() {
        let registry = DriverRegistry::new();
        registry
            .register_test_driver_with_capabilities(
                "test-driver",
                MockDriver::new("test-driver", MockDriverOptions::default()),
                capabilities(true),
            )
            .await;

        assert_eq!(
            registry.get_capabilities(&"test-driver".to_string()).await,
            Some(capabilities(true))
        );
    }

    #[tokio::test]
    async fn legacy_test_driver_capability_is_unknown() {
        let registry = DriverRegistry::new();
        registry
            .register_test_driver(
                "legacy-driver",
                MockDriver::new("legacy-driver", MockDriverOptions::default()),
            )
            .await;

        assert_eq!(
            registry
                .get_capabilities(&"legacy-driver".to_string())
                .await,
            None
        );
    }

    #[tokio::test]
    async fn replacing_registered_metadata_with_legacy_driver_clears_capability() {
        let registry = DriverRegistry::new();
        registry
            .register_test_driver_with_capabilities(
                "test-driver",
                MockDriver::new("test-driver", MockDriverOptions::default()),
                capabilities(true),
            )
            .await;
        registry
            .register_test_driver(
                "test-driver",
                MockDriver::new("test-driver", MockDriverOptions::default()),
            )
            .await;

        assert!(registry
            .get_capabilities(&"test-driver".to_string())
            .await
            .is_none());
    }

    #[test]
    fn capabilities_serialize_using_frontend_camel_case() {
        let value = serde_json::to_value(capabilities(true)).expect("serialize capabilities");
        assert_eq!(value["supportsCancelQuery"], true);
        assert_eq!(value["supportsQueryExecutionCancel"], true);
        assert_eq!(value["supportsExplain"], true);
        assert_eq!(value["supportsStreamingResults"], true);
        assert!(value.get("supports_cancel_query").is_none());
    }

    /// A driver factory that answers with the same memoized provider every
    /// time, exactly as the un-migrated drivers do
    /// (`packages/drivers/clickhouse/src/resource_provider.rs:88`).
    struct MemoizedProviderFactory {
        driver_id: &'static str,
        driver: Arc<dyn DatabaseDriver>,
        provider: OnceLock<Arc<dyn ResourceProvider>>,
    }

    impl MemoizedProviderFactory {
        fn new(driver_id: &'static str, driver: Arc<dyn DatabaseDriver>) -> Self {
            Self {
                driver_id,
                driver,
                provider: OnceLock::new(),
            }
        }
    }

    impl DatabaseDriverFactory for MemoizedProviderFactory {
        fn create(&self) -> Arc<dyn DatabaseDriver> {
            Arc::clone(&self.driver)
        }

        fn driver_id(&self) -> &'static str {
            self.driver_id
        }

        fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
            Some(
                self.provider
                    .get_or_init(|| {
                        Arc::new(LegacyResourceAdapter::new(
                            Arc::clone(&self.driver),
                            self.driver_id,
                            "test",
                            1,
                            NamespaceShape::default(),
                            // A fixture declares nothing: `CapabilitySet::default`
                            // leaves every cell at its non-supporting value.
                            // The adapter still derives `precise_cancel` from the
                            // driver itself, which is the one claim it refuses to
                            // take on faith.
                            CapabilitySet::default(),
                        ))
                    })
                    .clone(),
            )
        }
    }

    /// A driver that was never migrated to the resource contract.
    struct UnmigratedFactory {
        driver_id: &'static str,
        driver: Arc<dyn DatabaseDriver>,
    }

    impl DatabaseDriverFactory for UnmigratedFactory {
        fn create(&self) -> Arc<dyn DatabaseDriver> {
            Arc::clone(&self.driver)
        }

        fn driver_id(&self) -> &'static str {
            self.driver_id
        }
        // `resource_provider` is left at its default: this driver has nothing
        // to offer, and the host must say so.
    }

    /// The same provider instance comes back on every lookup.
    ///
    /// This is the guardrail for the whole wiring: the host re-resolves on each
    /// call instead of caching, and that is only sound because the factory
    /// memoizes. If a factory ever rebuilt its provider per call, the handles
    /// it had already issued would be rejected on their next use.
    #[tokio::test]
    async fn resource_provider_returns_the_same_instance_on_every_lookup() {
        let registry = DriverRegistry::new();
        let driver: Arc<dyn DatabaseDriver> =
            MockDriver::new("memoized", MockDriverOptions::default());
        registry
            .register_test_driver("memoized", Arc::clone(&driver))
            .await;
        registry
            .register_test_factory(
                "memoized",
                Box::leak(Box::new(MemoizedProviderFactory::new("memoized", driver))),
            )
            .await;

        let first = registry
            .resource_provider(&"memoized".to_string())
            .await
            .expect("a migrated driver resolves a provider");
        let second = registry
            .resource_provider(&"memoized".to_string())
            .await
            .expect("a migrated driver keeps resolving a provider");

        assert!(
            Arc::ptr_eq(&first, &second),
            "two lookups returned different provider instances"
        );
    }

    /// A driver without a provider is an error, not an empty success.
    ///
    /// If this ever degrades into `Ok`, "no provider" becomes
    /// indistinguishable from "nothing to do" — the exact mistake the
    /// fail-closed accessor exists to prevent.
    #[tokio::test]
    async fn resource_provider_reports_missing_for_an_unmigrated_driver() {
        let registry = DriverRegistry::new();
        let driver: Arc<dyn DatabaseDriver> =
            MockDriver::new("unmigrated", MockDriverOptions::default());
        registry
            .register_test_driver("unmigrated", Arc::clone(&driver))
            .await;
        registry
            .register_test_factory(
                "unmigrated",
                Box::leak(Box::new(UnmigratedFactory {
                    driver_id: "unmigrated",
                    driver,
                })),
            )
            .await;

        // `dyn ResourceProvider` is not comparable, so match the error out.
        match registry.resource_provider(&"unmigrated".to_string()).await {
            Ok(_) => panic!("a driver with no provider must not yield a provider"),
            Err(ResourceProviderLookup::Missing(error)) => {
                assert_eq!(error.driver_id, "unmigrated");
            }
            Err(other) => panic!("wrong failure for an unmigrated driver: {other}"),
        }
    }

    /// A driver with no factory behind it is reported as its own failure.
    ///
    /// "This build has no such driver" and "this driver has no provider" are
    /// different defects; collapsing them would tell a caller to go fix the
    /// driver migration when the real problem is the build.
    #[tokio::test]
    async fn resource_provider_separates_an_unregistered_type_from_a_missing_provider() {
        let registry = DriverRegistry::new();
        registry
            .register_test_driver(
                "no-factory",
                MockDriver::new("no-factory", MockDriverOptions::default()),
            )
            .await;

        assert!(matches!(
            registry.resource_provider(&"no-factory".to_string()).await,
            Err(ResourceProviderLookup::NotRegistered { .. })
        ));
        assert!(matches!(
            registry.resource_provider(&"absent".to_string()).await,
            Err(ResourceProviderLookup::NotRegistered { .. })
        ));
    }

    /// Every factory linked into this build resolves or explains itself, and
    /// every one that resolves hands out the *same* provider on every lookup.
    ///
    /// Runs against real drivers rather than fixtures, so a driver that starts
    /// answering `Ok` with a provider it cannot keep alive — or a driver that
    /// drops a fresh provider mid-build — fails here. Which drivers are linked
    /// depends on the build's driver features, so the census is reported rather
    /// than pinned; only the *properties* are pinned.
    ///
    /// There is no exemption list. A driver that has not been migrated answers
    /// `Missing` and is legitimately absent from the stable set; a driver that
    /// answers `Ok` has claimed the resource contract, and a claim it cannot
    /// honour across two lookups is a defect, not a documented exception.
    #[tokio::test]
    async fn every_linked_factory_resolves_or_reports_why_not() {
        let registry = DriverRegistry::new();
        let mut stable: Vec<&'static str> = Vec::new();
        let mut rebuilt_per_lookup: Vec<&'static str> = Vec::new();
        let mut missing: Vec<&'static str> = Vec::new();

        for factory in iter_driver_factories() {
            let id = factory.driver_id();
            match registry.resource_provider(&id.to_string()).await {
                Ok(provider) => {
                    let again = registry
                        .resource_provider(&id.to_string())
                        .await
                        .expect("a factory that resolved once keeps resolving");
                    if Arc::ptr_eq(&provider, &again) {
                        stable.push(id);
                    } else {
                        rebuilt_per_lookup.push(id);
                    }
                }
                Err(ResourceProviderLookup::Missing(error)) => {
                    assert_eq!(
                        error.driver_id, id,
                        "the error must name the driver asked for"
                    );
                    missing.push(id);
                }
                Err(other) => panic!("driver '{id}' failed for the wrong reason: {other}"),
            }
        }

        // The host links postgres, mysql and sqlite unconditionally
        // (`src-tauri/Cargo.toml`), so an empty stable set would mean the loop
        // never ran — not that there is nothing to check.
        assert!(
            !stable.is_empty(),
            "no linked driver resolved a memoized provider; the census proved nothing"
        );
        assert!(
            rebuilt_per_lookup.is_empty(),
            "driver(s) {rebuilt_per_lookup:?} rebuild their provider on every lookup. \
             The host resolves per call, so the handles one instance issued are \
             rejected by the next — and the provider drives a DatabaseDriver the \
             host never registered. Memoize the driver and its provider together \
             in the driver crate; caching it here would only hide the churn \
             behind one lucky instance."
        );
        println!(
            "{stable:?} keep one provider across lookups; {} rebuilt per call: \
             {rebuilt_per_lookup:?}; {} reported missing {missing:?}",
            stable.len(),
            missing.len(),
        );
    }
}
