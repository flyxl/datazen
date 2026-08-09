//! Background monitor subsystem — isolated DB connections for scheduled widget refresh.

mod channels;
mod connections;
mod engine;

pub use channels::{AlertPayload, AlertChannelState};
pub use connections::MonitorConnectionRegistry;
pub use connections::monitor_registry_key;
pub use engine::{build_schedule_table, MonitorEngine, ScheduledWidget};
