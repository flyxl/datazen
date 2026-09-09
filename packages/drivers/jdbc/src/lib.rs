//! DataZen path driver: jdbc (external Java Agent).
//!
//! Phase 0 skeleton: registers the driver id and returns Unsupported for
//! live operations until AgentProcessManager + protocol are wired (Phase 1+).

use std::sync::Arc;

use datazen_driver_api::*;

mod agent_process;
mod driver;
mod protocol;

pub use agent_process::AgentProcessManager;
pub use driver::JdbcDriver;
pub use protocol::{PROTOCOL_VERSION, agent_version_info};

struct JdbcFactory;

impl DatabaseDriverFactory for JdbcFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(JdbcDriver::new())
    }

    fn driver_id(&self) -> &'static str {
        "jdbc"
    }

    fn supports_explain(&self) -> bool {
        false
    }

    fn supports_streaming_results(&self) -> bool {
        // Will be true once query.fetch streaming is implemented (Phase 2–3).
        false
    }
}

datazen_driver_api::register_driver!(&JdbcFactory);
