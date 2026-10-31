//! Shared desktop adapter for read-only commands executed through Runtime.
//!
//! M4 后本类型只剩两个角色：`ResourceExecutionHost` 的桌面实现（AI、
//! MCP 与 Wapp 的读取用例都吃这个端口，共享逻辑在
//! `datazen::workflow::runtime_reads`），以及导出装配的会话 profile 查询。
//! 命令执行本体已用例化进共享层，宿主不再持有读取业务。
use crate::commands::AppState;
use crate::{platform::PlatformEntry, store::DesktopRepository};
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::workflow::{
    error::WorkflowError,
    resource_execution::{PreparedExecution, ResourceExecutionHost},
};
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct RuntimeCommandSource {
    registry: Arc<DriverRegistry>,
    store: Arc<DesktopRepository>,
    platform: PlatformEntry,
}

impl RuntimeCommandSource {
    pub(crate) fn new(
        registry: Arc<DriverRegistry>,
        store: Arc<DesktopRepository>,
        platform: PlatformEntry,
    ) -> Self {
        Self {
            registry,
            store,
            platform,
        }
    }

    pub(crate) async fn profile_for_session(
        &self,
        session_id: &str,
    ) -> Result<(String, ConnectionConfig, u64), String> {
        let adapter = self.platform.require().map_err(|error| error.to_string())?;
        let runtime = adapter.runtime().map_err(|error| error.to_string())?;
        let owner = runtime
            .connection_id_for_session(&datazen::execution::runtime::connection::DbSessionId::new(
                session_id,
            ))
            .ok_or_else(|| "Runtime session is unavailable".to_owned())?;
        let (config, metadata) = self
            .store
            .platform_profile(owner.as_str())
            .await
            .filter(|(_, metadata)| metadata.enabled)
            .ok_or_else(|| "Connection profile unavailable".to_owned())?;
        Ok((owner.as_str().to_owned(), config, metadata.config_revision))
    }

    /// Assemble from the desktop app state (the MCP and Wapp read paths).
    pub(crate) fn from_app_state(state: &AppState) -> Self {
        Self::new(
            state.driver_registry.clone(),
            state.store.clone(),
            state.platform.clone(),
        )
    }

    pub(crate) async fn execution_safe_mode(&self) -> bool {
        self.store.execution_settings().await.safe_mode
    }
}

/// The desktop `ResourceExecutionHost` for the shared read-only command
/// use cases (`datazen::workflow::runtime_reads`).
///
/// M4: the host keeps its legitimate role — one desktop implementation of
/// the port — while every read path (Wapp, MCP, and later web) goes through
/// the shared use case.
pub(crate) fn resource_host(state: &AppState) -> std::sync::Arc<dyn ResourceExecutionHost> {
    std::sync::Arc::new(RuntimeCommandSource::from_app_state(state))
}

#[async_trait::async_trait]
impl ResourceExecutionHost for RuntimeCommandSource {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        self.store
            .platform_profile(id)
            .await
            .map(|(config, metadata)| (config, metadata.config_revision, metadata.enabled))
            .ok_or_else(|| WorkflowError::ConnectionFailed {
                connection_id: id.into(),
                message: "Connection profile unavailable".into(),
            })
    }

    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        self.registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| WorkflowError::Driver("Driver unavailable".into()))
    }

    async fn safe_mode(&self) -> bool {
        self.execution_safe_mode().await
    }

    async fn prepare(
        &self,
        _id: &str,
        revision: u64,
        command: &str,
        target: datazen::execution::target::ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        let adapter = self
            .platform
            .require()
            .map_err(|error| WorkflowError::Validation(error.to_string()))?;
        let call = adapter.begin_call();
        let runtime = adapter
            .runtime()
            .map_err(|error| WorkflowError::Validation(error.to_string()))?;
        let canonical = adapter
            .resolve_execution_target(
                call.context(),
                datazen::shared::id::Counter::new(revision),
                command,
                &target,
            )
            .await
            .map_err(|error| WorkflowError::Validation(error.message))?;
        Ok(PreparedExecution {
            artifacts: crate::platform::feature_ports::HostArtifactChunks::new(runtime.clone())
                .shared(),
            use_cases: runtime,
            context: call.context().clone(),
            target: crate::platform::into_execution_target(canonical),
        })
    }
}
