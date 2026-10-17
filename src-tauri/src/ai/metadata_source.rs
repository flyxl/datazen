//! Desktop session identity lookup bound to independent Runtime metadata reads.
use crate::{
    db::DriverRegistry, platform::PlatformEntry, services::ConnectionManager, store::Store,
};
use datazen_driver_api::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, TableInfo, TableSchema,
};
use datazen_feature_core::{
    services::ports::SchemaContextSource,
    workflow::{
        command::WorkflowCommandStep,
        error::WorkflowError,
        resource_execution::{PreparedExecution, ResourceExecutionHost},
    },
};
use std::sync::Arc;
pub struct RuntimeSchemaSource {
    sessions: Arc<ConnectionManager>,
    registry: Arc<DriverRegistry>,
    store: Arc<Store>,
    platform: PlatformEntry,
}
impl RuntimeSchemaSource {
    pub fn new(
        sessions: Arc<ConnectionManager>,
        registry: Arc<DriverRegistry>,
        store: Arc<Store>,
        platform: PlatformEntry,
    ) -> Self {
        Self {
            sessions,
            registry,
            store,
            platform,
        }
    }
    pub(crate) async fn read(
        &self,
        session_id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let config = self
            .sessions
            .get_session_config(session_id)
            .await
            .map_err(|error| error.to_string())?;
        let step = WorkflowCommandStep::new("ai-metadata", command, Some(config.id.clone()), input);
        datazen_feature_core::workflow::resource_execution::execute(
            self, &step, &config.id, None, None, false,
        )
        .await
        .map(|result| result.data)
        .map_err(|error| error.to_string())
    }
}
#[async_trait::async_trait]
impl ResourceExecutionHost for RuntimeSchemaSource {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        self.store
            .platform_profile(id)
            .await
            .map(|(config, metadata)| (config, metadata.config_revision, metadata.enabled))
            .ok_or_else(|| WorkflowError::ConnectionFailed {
                connection_id: id.into(),
                message: "profile unavailable".into(),
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
        self.store.execution_settings().await.safe_mode
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
#[async_trait::async_trait]
impl SchemaContextSource for RuntimeSchemaSource {
    async fn get_session(
        &self,
        _id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        Err("The Runtime metadata source does not expose physical session handles".into())
    }
    async fn driver_for_session(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        let config = self
            .sessions
            .get_session_config(id)
            .await
            .map_err(|error| error.to_string())?;
        self.registry
            .get(&config.database_type)
            .await
            .ok_or("Driver unavailable".into())
    }
    async fn session_schema(&self, id: &str) -> Option<String> {
        self.sessions
            .get_session_config(id)
            .await
            .ok()
            .and_then(|config| config.schema)
    }
    async fn tables(
        &self,
        id: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String> {
        let output = self
            .read(
                id,
                "list_tables",
                serde_json::json!({ "database": database, "schema": schema }),
            )
            .await?;
        Ok(datazen_driver_api::parse_tables_from_command(&output))
    }
    async fn table_schema(
        &self,
        _cache: &datazen_runtime::schema_cache::SchemaCache,
        id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<TableSchema, String> {
        let output = self
            .read(
                id,
                "get_table_schema",
                serde_json::json!({ "database": database, "schema": schema, "table": table }),
            )
            .await?;
        serde_json::from_value(output["schema"].clone()).map_err(|error| error.to_string())
    }
}
