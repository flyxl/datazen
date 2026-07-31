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
