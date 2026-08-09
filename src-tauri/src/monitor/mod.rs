//! Background monitor subsystem — isolated DB connections for scheduled widget refresh.

mod connections;
mod engine;

pub use connections::MonitorConnectionRegistry;
pub use connections::monitor_registry_key;
pub use engine::{build_schedule_table, MonitorEngine, ScheduledWidget};
