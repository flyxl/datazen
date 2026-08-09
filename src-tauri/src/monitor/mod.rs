//! Background monitor subsystem — isolated DB connections for scheduled widget refresh.

mod connections;

pub use connections::MonitorConnectionRegistry;
pub use connections::monitor_registry_key;
