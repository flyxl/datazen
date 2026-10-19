//! Background monitor subsystem — isolated DB connections for scheduled widget refresh.

mod channels;
mod connections;
mod engine;
#[cfg(feature = "webdriver")]
pub(crate) mod evidence;
mod fixed_targets;
pub(crate) mod grants;
mod scheduled;

pub use connections::MonitorConnectionRegistry;
pub use engine::MonitorEngine;
