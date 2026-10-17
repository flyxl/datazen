//! Background monitor subsystem — isolated DB connections for scheduled widget refresh.

mod channels;
mod connections;
mod engine;
pub(crate) mod grants;
mod scheduled;

pub use connections::MonitorConnectionRegistry;
pub use engine::MonitorEngine;
