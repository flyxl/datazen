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

/// Holds registered drivers, in one of two injection forms:
///
/// * **Lazy (link-time) form** — [`DriverRegistry::new`] / [`init_drivers`]:
///   starts empty; call [`DriverRegistry::ensure_type`] (or rely on
///   [`DriverRegistry::get`]) to load a type on demand out of the `inventory`
///   factories the final binary links. This is the desktop host's current
///   wiring; which types exist depends on the link closure.
/// * **Static form** — [`DriverRegistry::with_factories`]: the host hands the
///   factory list to the constructor; everything is admitted eagerly and the
///   registry never consults the link-time scan again. No lazy loading, no
///   hidden global state; a type that was not seeded fails closed.
pub struct DriverRegistry {
    drivers: Arc<RwLock<HashMap<DatabaseType, Arc<dyn DatabaseDriver>>>>,
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
    /// Whether a lookup miss falls back to the link-time `inventory` scan.
    /// `true` for the lazy form, `false` for [`DriverRegistry::with_factories`].
    inventory_fallback: bool,
    /// Canonical ids seeded via [`DriverRegistry::with_factories`], sorted;
    /// `None` for the lazy form. Fixed at construction — a catalog, not
    /// mutable state — so [`DriverRegistry::available_types`] can answer
    /// synchronously and deterministically.
    seeded_types: Option<Vec<DatabaseType>>,
}

/// The outcome of admitting one factory: the driver instance, the driver's
/// own type id (`actual` — it may differ from the key the type was asked
/// under) and the static capabilities captured at admission.
struct FactoryAdmission {
    driver: Arc<dyn DatabaseDriver>,
    actual: DatabaseType,
    capabilities: DriverCapabilities,
}

impl DriverRegistry {
    /// Lazy form: an empty registry that resolves drivers on demand out of
    /// the `inventory` factories linked into the final binary. See
    /// [`DriverRegistry::with_factories`] for the static form.
    pub fn new() -> Self {
        Self {
            drivers: Arc::new(RwLock::new(HashMap::new())),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
            factories: Arc::new(RwLock::new(HashMap::new())),
            inventory_fallback: true,
            seeded_types: None,
        }
    }

    /// Static form: build the registry from an explicitly supplied factory
    /// list. Every factory is admitted at construction — protocol gate,
    /// driver instance, capability capture and alias insertion all happen
    /// here — so afterwards `ensure_type` never loads anything, and a type
    /// that was not seeded fails closed with the same error the lazy form
    /// produces for an absent driver.
    ///
    /// A factory whose `protocol_version()` is below `MIN_PROTOCOL_VERSION`
    /// fails construction with the error the lazy form defers to the first
    /// `ensure_type`. A later duplicate of an already-seeded id wins,
    /// matching map-insert semantics.
    ///
    /// The constructor is synchronous on purpose: a host can assemble its
    /// registry before entering any async runtime, and the catalog cannot
    /// change behind the caller's back.
    pub fn with_factories(
        factories: impl IntoIterator<Item = &'static dyn DatabaseDriverFactory>,
    ) -> Result<Self, String> {
        let mut drivers = HashMap::new();
        let mut capabilities = HashMap::new();
        let mut factory_map: HashMap<DatabaseType, &'static dyn DatabaseDriverFactory> =
            HashMap::new();
        let mut seeded_types: Vec<DatabaseType> = Vec::new();
        for factory in factories {
            let db_type = factory.driver_id().to_string();
            let admission = Self::admit_factory(&db_type, factory)?;
            Self::install_admission(
                &db_type,
                factory,
                admission,
                &mut drivers,
                &mut capabilities,
                &mut factory_map,
            );
            if !seeded_types.contains(&db_type) {
                seeded_types.push(db_type);
            }
        }
        seeded_types.sort();
        Ok(Self {
            drivers: Arc::new(RwLock::new(drivers)),
            capabilities: Arc::new(RwLock::new(capabilities)),
            factories: Arc::new(RwLock::new(factory_map)),
            inventory_fallback: false,
            seeded_types: Some(seeded_types),
        })
    }

    /// Install a resolved admission under the requested key and the driver's
    /// own id, in the inventory path's insertion order: capabilities first,
    /// then the factory, then the driver — under both spellings when the
    /// driver's own type differs from the requested key. Both keys must
    /// resolve to the same driver/factory pair, so the provider a caller
    /// resolves is backed by the driver this registry handed out, never by a
    /// second `create()`.
    fn install_admission(
        db_type: &str,
        factory: &'static dyn DatabaseDriverFactory,
        admission: FactoryAdmission,
        drivers: &mut HashMap<DatabaseType, Arc<dyn DatabaseDriver>>,
        capabilities: &mut HashMap<DatabaseType, DriverCapabilities>,
        factory_map: &mut HashMap<DatabaseType, &'static dyn DatabaseDriverFactory>,
    ) {
        capabilities.insert(db_type.to_string(), admission.capabilities);
        if admission.actual != db_type {
            capabilities.insert(admission.actual.clone(), admission.capabilities);
        }
        factory_map.insert(db_type.to_string(), factory);
        factory_map.insert(admission.actual.clone(), factory);
        if admission.actual != db_type {
            drivers.insert(admission.actual.clone(), admission.driver.clone());
        }
        drivers.insert(db_type.to_string(), admission.driver);
    }

    /// Resolve one factory against its requested registry key: apply the
    /// protocol gate, create the driver, capture its capabilities.
    ///
    /// Shared by the inventory path (`register_from_inventory`) and the
    /// static constructor (`with_factories`) so the protocol gate and the
    /// capability capture cannot drift between the two injection paths.
    fn admit_factory(
        _db_type: &str,
        factory: &'static dyn DatabaseDriverFactory,
    ) -> Result<FactoryAdmission, String> {
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
        let capabilities = DriverCapabilities::from_factory(factory, driver.as_ref());
        Ok(FactoryAdmission {
            driver,
            actual,
            capabilities,
        })
    }

    /// Catalog of types this build can provide. For the lazy form this is
    /// the linked `inventory` factories (deduped, link order) and does **not**
    /// instantiate any driver; for the static form it is exactly the seeded
    /// ids, sorted so the answer is deterministic.
    pub fn available_types(&self) -> Vec<DatabaseType> {
        if let Some(seeded) = &self.seeded_types {
            return seeded.clone();
        }
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

        if !self.inventory_fallback {
            // Static form: construction was total, so a miss is a hard,
            // deterministic answer — never a link-time scan behind the
            // caller's back.
            return Err(format!("No driver available for database type '{db_type}'"));
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
            let admission = Self::admit_factory(db_type, *factory)?;
            // The driver and its factory are registered under the same keys, so
            // the provider a caller resolves is backed by the driver this
            // registry handed out, never by a second `create()`.
            let mut capabilities = self.capabilities.write().await;
            let mut factories = self.factories.write().await;
            Self::install_admission(
                db_type,
                *factory,
                admission,
                drivers,
                &mut capabilities,
                &mut factories,
            );
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

/// Create a lazy, inventory-backed registry. Drivers load via
/// [`DriverRegistry::ensure_type`]. For a statically seeded registry — the
/// form a host hands its factory list to explicitly — see
/// [`DriverRegistry::with_factories`].
pub fn init_drivers() -> DriverRegistry {
    DriverRegistry::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::capabilities::CapabilitySet;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
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

    /// A factory compiled against a protocol older than the host minimum.
    /// The inventory path surfaces the gate on the first `ensure_type`; the
    /// static constructor must surface it at construction, before any
    /// `create()`.
    struct AncientProtocolFactory;

    impl DatabaseDriverFactory for AncientProtocolFactory {
        fn create(&self) -> Arc<dyn DatabaseDriver> {
            MockDriver::new("ancient", MockDriverOptions::default())
        }

        fn driver_id(&self) -> &'static str {
            "ancient"
        }

        fn protocol_version(&self) -> u32 {
            // One below the floor (`MIN_PROTOCOL_VERSION` is 1): trips the
            // same gate the inventory path applies.
            0
        }
    }

    /// The static form serves exactly its seed: the catalog is the supplied
    /// factory list — real drivers linked into the test binary must not leak
    /// into it — lookups resolve from the construction-time admission, and a
    /// type that was not seeded fails closed instead of quietly consulting
    /// the link-time scan.
    #[tokio::test]
    async fn a_static_registry_serves_exactly_its_seeded_factories() {
        let driver: Arc<dyn DatabaseDriver> =
            MockDriver::new("memoized", MockDriverOptions::default());
        let factory: &'static dyn DatabaseDriverFactory = Box::leak(Box::new(
            MemoizedProviderFactory::new("memoized", Arc::clone(&driver)),
        ));
        let registry = DriverRegistry::with_factories([factory])
            .expect("a current-protocol factory admits cleanly");

        assert_eq!(registry.available_types(), vec!["memoized".to_string()]);

        let first = registry
            .resource_provider(&"memoized".to_string())
            .await
            .expect("a seeded migrated driver resolves a provider");
        let second = registry
            .resource_provider(&"memoized".to_string())
            .await
            .expect("a seeded migrated driver keeps resolving a provider");
        assert!(Arc::ptr_eq(&first, &second));

        assert!(registry
            .get_capabilities(&"memoized".to_string())
            .await
            .is_some());

        assert_eq!(
            registry.ensure_type("not-seeded").await,
            Err("No driver available for database type 'not-seeded'".to_string())
        );
    }

    /// An empty seed is a valid, closed catalog: nothing exists and nothing
    /// loads on demand — the shape a host without any linked driver takes.
    #[tokio::test]
    async fn an_empty_static_registry_is_a_closed_empty_world() {
        let registry =
            DriverRegistry::with_factories([]).expect("an empty seed is a valid catalog");

        assert!(registry.available_types().is_empty());
        assert!(registry.get(&"absent".to_string()).await.is_none());
        assert!(registry
            .get_capabilities(&"absent".to_string())
            .await
            .is_none());
        assert!(matches!(
            registry.resource_provider(&"absent".to_string()).await,
            Err(ResourceProviderLookup::NotRegistered { .. })
        ));
    }

    /// The protocol gate applies at static construction and names the driver.
    #[test]
    fn a_factory_below_the_minimum_protocol_version_fails_static_construction() {
        let ancient: &'static dyn DatabaseDriverFactory =
            Box::leak(Box::new(AncientProtocolFactory));

        match DriverRegistry::with_factories([ancient]) {
            Ok(_) => panic!("a too-old factory must fail static construction"),
            Err(error) => {
                assert!(
                    error.contains("ancient"),
                    "the error must name the driver: {error}"
                );
                assert!(
                    error.contains("too old"),
                    "the error must carry the protocol gate: {error}"
                );
            }
        }
    }
}
