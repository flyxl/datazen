//! Driver protocol version lookup, shared by the migration wizards.
//!
//! A plan records the protocol version of the driver that produced it, and the
//! apply path refuses a plan whose driver now speaks a different protocol. The
//! lookup therefore has to be identical for Data Sync and Data Transfer — a
//! second copy is a second opinion about the same number.

use datazen_driver_api::{iter_driver_factories, DatabaseDriver, PROTOCOL_VERSION};

/// Protocol version of the driver behind `driver`.
///
/// Falls back to the compiled-in protocol version when the driver is not
/// registered in this build (an unlinked driver cannot answer for itself).
pub(crate) fn driver_protocol_version(driver: &dyn DatabaseDriver) -> u32 {
    let driver_type = driver.driver_type();
    iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == driver_type)
        .map(|factory| factory.protocol_version())
        .unwrap_or(PROTOCOL_VERSION)
}
