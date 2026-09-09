//! DataZen path driver: jdbc (external Java Agent).
//!
//! MVP: AgentProcessManager + full session/query/meta over JSON-RPC.
//! Enable with `DATAZEN_DRIVERS=…,jdbc` and place `datazen-jdbc-agent.jar`.
//!
//! Host wiring: `AppSettings.pluginSettings.jdbc` is mirrored via
//! [`apply_plugin_settings`] (see `src-tauri/src/redis_flush_gate.rs`).

use std::sync::Arc;

use datazen_driver_api::*;

mod agent_process;
mod driver;
mod protocol;
mod settings;

pub use agent_process::{AgentLaunchConfig, AgentProcessManager, AgentState};
pub use driver::JdbcDriver;
pub use protocol::{agent_version_info, PROTOCOL_VERSION};
pub use settings::{apply_plugin_settings, config_epoch, resolved_launch_config, SETTINGS_KEY};

struct JdbcFactory;

impl DatabaseDriverFactory for JdbcFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        // One shared agent process for the whole host; settings apply live.
        Arc::new(JdbcDriver::shared())
    }

    fn driver_id(&self) -> &'static str {
        "jdbc"
    }

    fn supports_explain(&self) -> bool {
        false
    }

    fn supports_streaming_results(&self) -> bool {
        // Materialized query_multi path; true stream fetch can be enabled later.
        false
    }
}

datazen_driver_api::register_driver!(&JdbcFactory);
