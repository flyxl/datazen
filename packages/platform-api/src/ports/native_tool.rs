//! Environment port for deployment-pinned native tools. Requests contain no paths,
//! argv, environment, secrets or shell fragments. Implementations report cleanup
//! separately from exit/effect; they never retry an invocation.
use crate::{error::PortError, id::ConnectionId};
use std::sync::{atomic::AtomicBool, Arc};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeToolSelection {
    pub tool_id: String,
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeToolResolution {
    /// Opaque deployment lookup key, never a client executable path.
    pub key: String,
    pub selection: NativeToolSelection,
    /// Upper bounds require deployment evidence. None is unsupported, not zero.
    pub max_processes: Option<u32>,
    pub max_database_sockets: Option<u32>,
    pub connection_id: Option<ConnectionId>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTermination {
    Exited,
    Cancelled,
    TimedOut,
    OutputLimit,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCleanup {
    Confirmed,
    Unconfirmed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeToolReport {
    pub termination: NativeTermination,
    pub exit_code: Option<i32>,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub cleanup: NativeCleanup,
    pub version_verified: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct NativeToolLimits {
    pub max_output_bytes: usize,
    pub timeout_ms: u64,
    pub terminate_grace_ms: u64,
}
impl Default for NativeToolLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: 65_536,
            timeout_ms: 15_000,
            terminate_grace_ms: 100,
        }
    }
}
#[async_trait::async_trait]
pub trait NativeToolPort: Send + Sync + 'static {
    fn resolve(&self, selection: &NativeToolSelection) -> Result<NativeToolResolution, PortError>;
    /// Err guarantees no process was started. Every post-spawn outcome is a report.
    /// Dropping this future must not abandon the process: provider retains its
    /// worker, group and temporary resources until cleanup is confirmed.
    async fn run(
        &self,
        run_key: &str,
        resolution: NativeToolResolution,
        cancel: Arc<AtomicBool>,
        limits: NativeToolLimits,
    ) -> Result<NativeToolReport, PortError>;
    /// Infrastructure cleanup only. Must not start/replay application work.
    async fn cleanup_status(&self, run_key: &str) -> NativeCleanup;
}
