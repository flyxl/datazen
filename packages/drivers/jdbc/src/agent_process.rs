//! Lifecycle manager for the external JDBC Java Agent process.
//!
//! Phase 0: type and API surface only. Spawn / JSON-RPC IO lands in Phase 1.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

/// Where to find `java` and the agent jar.
#[derive(Debug, Clone)]
pub struct AgentLaunchConfig {
    /// Override for `java` executable; None -> JAVA_HOME / PATH.
    pub java_path: Option<PathBuf>,
    /// Path to `datazen-jdbc-agent-*.jar`.
    pub agent_jar: PathBuf,
    /// Idle timeout before shutdown when no sessions remain (seconds).
    pub idle_timeout_secs: u64,
}

impl Default for AgentLaunchConfig {
    fn default() -> Self {
        Self {
            java_path: None,
            agent_jar: PathBuf::from("datazen-jdbc-agent.jar"),
            idle_timeout_secs: 600,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Stopped,
    Starting,
    Running,
    Failed,
}

/// Manages a single shared Java Agent child process for the whole app.
///
/// Phase 0 stub: does not spawn yet. Callers should treat `ensure_running` as
/// Unsupported until Phase 1.
pub struct AgentProcessManager {
    config: AgentLaunchConfig,
    state: Mutex<AgentState>,
}

impl AgentProcessManager {
    pub fn new(config: AgentLaunchConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            state: Mutex::new(AgentState::Stopped),
        })
    }

    pub async fn state(&self) -> AgentState {
        *self.state.lock().await
    }

    pub fn config(&self) -> &AgentLaunchConfig {
        &self.config
    }

    /// Ensure the agent is running and has completed `agent.hello`.
    ///
    /// Phase 0: always errors with Unsupported.
    pub async fn ensure_running(&self) -> Result<(), String> {
        Err(
            "JDBC Agent process manager is not implemented yet (Phase 1). \
             See docs/todo/jdbc-agent-implementation-plan.md"
                .into(),
        )
    }

    /// Send a JSON-RPC request and wait for the matching response id.
    ///
    /// Phase 0: always errors.
    pub async fn rpc(
        &self,
        _method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err("JDBC Agent RPC is not implemented yet (Phase 1)".into())
    }

    /// Graceful shutdown or kill.
    pub async fn shutdown(&self) -> Result<(), String> {
        let mut st = self.state.lock().await;
        *st = AgentState::Stopped;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_state_is_stopped() {
        let mgr = AgentProcessManager::new(AgentLaunchConfig::default());
        assert_eq!(mgr.state().await, AgentState::Stopped);
    }

    #[tokio::test]
    async fn ensure_running_phase0_is_unsupported() {
        let mgr = AgentProcessManager::new(AgentLaunchConfig::default());
        let err = mgr.ensure_running().await.unwrap_err();
        assert!(err.contains("Phase 1"));
    }
}
