//! DataZen path driver: jdbc (external Java Agent).
//!
//! Phase 1: AgentProcessManager can spawn the agent and complete `agent.hello`.
//! Live DatabaseDriver ops still return Unsupported until Phase 2–3.

use std::sync::Arc;

use datazen_driver_api::*;

mod agent_process;
mod driver;
mod protocol;

pub use agent_process::{AgentLaunchConfig, AgentProcessManager, AgentState};
pub use driver::JdbcDriver;
pub use protocol::{agent_version_info, PROTOCOL_VERSION};

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
        false
    }
}

datazen_driver_api::register_driver!(&JdbcFactory);
