//! Plugin factory and auto-registration via `inventory`.

use std::sync::Arc;

use crate::traits::{DatabaseDriver, KeyValueDriver};

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

    /// If this driver also implements KeyValueDriver, return it.
    /// Default returns None.
    fn create_kv(&self) -> Option<Arc<dyn KeyValueDriver>> {
        None
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
        .find(|factory| factory.driver_id() == driver_id)
        .map(|factory| factory.create())
}
