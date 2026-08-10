//! Background monitor subsystem — isolated DB connections for scheduled widget refresh.

mod channels;
mod connections;
mod engine;

pub use connections::MonitorConnectionRegistry;
pub use engine::MonitorEngine;
