//! Shared desktop adapter for read-only commands executed through Runtime.
use crate::{platform::PlatformEntry, store::DesktopRepository};
use datazen_driver_api::{
    check_command_access, CommandAccessLevel, CommandCatalog, ConnectionConfig, DatabaseDriver,
};
use datazen_feature_core::workflow::{
    command::WorkflowCommandStep,
    error::WorkflowError,
    resource_execution::{PreparedExecution, ResourceExecutionHost},
};
use datazen_runtime::driver_registry::DriverRegistry;
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
            .connection_id_for_session(&datazen_runtime::connection::DbSessionId::new(session_id))
            .ok_or_else(|| "Runtime session is unavailable".to_owned())?;
        let (config, metadata) = self
            .store
            .platform_profile(owner.as_str())
            .await
            .filter(|(_, metadata)| metadata.enabled)
            .ok_or_else(|| "Connection profile unavailable".to_owned())?;
        Ok((owner.as_str().to_owned(), config, metadata.config_revision))
    }

    pub(crate) async fn read_target_command(
        &self,
        connection_id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (config, _, enabled) = self
            .profile(connection_id)
            .await
            .map_err(|error| error.to_string())?;
        if !enabled {
            return Err("Connection profile is disabled".into());
        }
        let driver = self
            .driver(&config)
            .await
            .map_err(|error| error.to_string())?;
        let catalog = datazen_driver_api::LegacyCommandCatalog::new(driver.as_ref());
        let definition = catalog
            .command_definitions()
            .into_iter()
            .chain(datazen_driver_api::schema_catalog_command_definitions())
            .chain(crate::services::schema_metadata::command_definitions())
            .find(|definition| definition.id == command)
            .ok_or_else(|| format!("Unsupported command: {command}"))?;
        check_command_access(&definition, CommandAccessLevel::Read)
            .map_err(|_| "Wapps may invoke read-only driver commands".to_owned())?;

        let step = WorkflowCommandStep::new(
            "wapp-command",
            command,
            Some(connection_id.to_owned()),
            input,
        );
        datazen_feature_core::workflow::resource_execution::execute(
            &WappReadOnlyHost(self),
            &step,
            connection_id,
            Some(datazen_feature_core::mcp::permission::McpPermissionMode::SafeWrite),
            None,
            false,
        )
        .await
        .map(|result| result.data)
        .map_err(|error| error.to_string())
    }

    /// Resolve the configured driver for a persisted connection profile.
    ///
    /// Callers that execute a command must still use `read_target_command` so
    /// Runtime performs the authoritative profile, revision and target checks.
    /// This helper is only for driver metadata needed to render prompts.
    pub(crate) async fn driver_for_connection(
        &self,
        connection_id: &str,
    ) -> Result<Arc<dyn DatabaseDriver>, String> {
        let (config, metadata) = self
            .store
            .platform_profile(connection_id)
            .await
            .ok_or_else(|| "Connection profile unavailable".to_owned())?;
        if !metadata.enabled {
            return Err("Connection profile is disabled".to_owned());
        }
        self.registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| "Driver unavailable".to_owned())
    }

    pub(crate) async fn execution_safe_mode(&self) -> bool {
        self.store.execution_settings().await.safe_mode
    }
}

struct WappReadOnlyHost<'a>(&'a RuntimeCommandSource);

#[async_trait::async_trait]
impl ResourceExecutionHost for WappReadOnlyHost<'_> {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        let (mut config, revision, enabled) = self.0.profile(id).await?;
        config.read_only = true;
        Ok((config, revision, enabled))
    }

    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        self.0.driver(config).await
    }

    async fn safe_mode(&self) -> bool {
        self.0.safe_mode().await
    }

    async fn prepare(
        &self,
        id: &str,
        revision: u64,
        command: &str,
        target: datazen_platform_api::target::ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        self.0.prepare(id, revision, command, target).await
    }
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
        target: datazen_platform_api::target::ExecutionTarget,
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
                datazen_platform_api::id::Counter::new(revision),
                command,
                &target,
            )
            .await
            .map_err(|error| WorkflowError::Validation(error.message))?;
        Ok(PreparedExecution {
            runtime,
            context: call.context().clone(),
            target: crate::platform::into_execution_target(canonical),
        })
    }
}
