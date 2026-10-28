//! Plugin factory and auto-registration via `inventory`.

use std::sync::Arc;

use crate::capabilities::CapabilitySet;
use crate::resource::ResourceProvider;
use crate::traits::DatabaseDriver;

/// Factory that plugins implement to register their driver.
/// Use the [`register_driver!`] macro for convenient registration.
pub trait DatabaseDriverFactory: Send + Sync + 'static {
    /// Create an instance of the driver.
    fn create(&self) -> Arc<dyn DatabaseDriver>;

    /// Unique string identifier for this driver (e.g. "kiwi", "redis").
    fn driver_id(&self) -> &'static str;

    /// Protocol version this plugin was compiled against.
    fn protocol_version(&self) -> u32 {
        crate::PROTOCOL_VERSION
    }

    /// Whether this driver supports query cancellation.
    fn supports_cancel_query(&self) -> bool {
        false
    }

    /// Whether this factory explicitly advertises the precise execution-handle
    /// cancellation protocol. This is separate from the legacy capability so
    /// old plugins cannot make the host call their session-wide cancel method.
    fn supports_query_execution_cancel(&self) -> bool {
        false
    }

    /// Whether this driver supports EXPLAIN analysis.
    fn supports_explain(&self) -> bool {
        false
    }

    /// Whether this driver supports streaming results.
    fn supports_streaming_results(&self) -> bool {
        false
    }

    /// The opaque-resource provider this driver implements, if any.
    ///
    /// Defaults to `None`, which means "this driver has not been migrated to
    /// the resource contract yet" — never "this driver supports everything".
    /// A caller must go through [`DatabaseDriverFactory::require_resource_provider`]
    /// so a missing provider surfaces as an error instead of a silent no-op.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        None
    }

    /// The capabilities this driver declares, for drivers that expose a
    /// provider. Defaults to the all-unknown set, so an unmigrated driver
    /// claims nothing.
    fn resource_capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }
}

// Collect all factories registered across the binary (including plugins).
inventory::collect!(&'static dyn DatabaseDriverFactory);

/// Register a driver factory at link time. Usage:
///
/// ```ignore
/// struct MyDriverFactory;
/// impl DatabaseDriverFactory for MyDriverFactory { ... }
/// datazen_driver_api::register_driver!(&MyDriverFactory);
/// ```
#[macro_export]
macro_rules! register_driver {
    ($factory:expr) => {
        $crate::inventory::submit!($factory as &'static dyn $crate::DatabaseDriverFactory);
    };
}

/// Iterate over all registered driver factories.
pub fn iter_driver_factories() -> inventory::iter<&'static dyn DatabaseDriverFactory> {
    inventory::iter::<&'static dyn DatabaseDriverFactory>
}

/// Create a registered driver by its stable driver id.
pub fn create_driver(driver_id: &str) -> Option<Arc<dyn DatabaseDriver>> {
    iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == driver_id)
        .map(|factory| factory.create())
}

/// Protocol version of the driver behind `driver`.
///
/// A migration plan records the protocol version of the driver that produced
/// it, and the apply path refuses a plan whose driver now speaks a different
/// protocol. The lookup therefore has to be identical for Data Sync and Data
/// Transfer — a second copy is a second opinion about the same number.
///
/// Falls back to the compiled-in protocol version when the driver is not
/// registered in this build (an unlinked driver cannot answer for itself).
pub fn driver_protocol_version(driver: &dyn DatabaseDriver) -> u32 {
    let driver_type = driver.driver_type();
    iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == driver_type)
        .map(|factory| factory.protocol_version())
        .unwrap_or(crate::PROTOCOL_VERSION)
}

/// Fail-closed accessor: the provider or an explicit "not migrated" error.
///
/// This exists so that "the driver has no resource provider" can never be
/// mistaken for "the driver has nothing to do" — the defect that lets a driver
/// report a successful no-op cancel. As of this writing only `hbase` and
/// `vector` still answer `Ok(())` there; the other 13 path drivers answer
/// `Err(DriverError::Unsupported(..))`.
pub fn require_resource_provider(
    factory: &dyn DatabaseDriverFactory,
) -> Result<Arc<dyn ResourceProvider>, ResourceProviderMissing> {
    factory.resource_provider().ok_or(ResourceProviderMissing {
        driver_id: factory.driver_id(),
    })
}

/// A factory that has not been migrated to the resource contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("driver {driver_id} does not implement the resource provider contract")]
pub struct ResourceProviderMissing {
    pub driver_id: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::DatabaseDriver;

    struct NoProviderFactory;

    impl DatabaseDriverFactory for NoProviderFactory {
        fn create(&self) -> Arc<dyn DatabaseDriver> {
            unimplemented!("this factory only exists to prove the accessor rejects it")
        }

        fn driver_id(&self) -> &'static str {
            "legacy-fixture"
        }
    }

    #[test]
    fn a_factory_without_a_provider_is_an_error_not_an_empty_provider() {
        let factory = NoProviderFactory;
        let error = match require_resource_provider(&factory) {
            Ok(_) => panic!("a factory with no provider must not yield a provider"),
            Err(error) => error,
        };
        assert_eq!(error.driver_id, "legacy-fixture");
    }

    #[test]
    fn an_unmigrated_factory_declares_no_capabilities() {
        let capabilities = NoProviderFactory.resource_capabilities();
        // Every field defaults to unknown/unsupported, so an unmigrated driver
        // cannot accidentally be read as a capable one.
        assert_eq!(capabilities.stateful_session, Default::default());
        assert_eq!(capabilities.precise_cancel, Default::default());
        assert!(!capabilities.transactions.savepoints.enables_feature());
        assert!(!capabilities.declares_anything());
    }
}
