//! Background monitor subsystem — Runtime-backed scheduled widget refresh.

mod channels;
mod engine;
#[cfg(feature = "webdriver")]
pub(crate) mod evidence;
mod fixed_targets;
pub(crate) mod grants;
mod scheduled;

pub use engine::MonitorEngine;
