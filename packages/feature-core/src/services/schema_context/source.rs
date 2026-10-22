//! Runtime-backed metadata source: session identity plus independent metadata reads.

use crate::services::ports::SchemaContextSource;
use crate::workflow::command::WorkflowCommandStep;
use crate::workflow::error::WorkflowError;
use crate::workflow::resource_execution::{self, ResourceExecutionHost};
use datazen_driver_api::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, TableInfo, TableSchema,
};
use datazen_runtime::application::RuntimeConnectionUseCases;
use datazen_runtime::connection::DbSessionId;
use datazen_runtime::schema_cache::SchemaCache;
use std::sync::Arc;

/// Step id of every metadata read.
///
/// The id labels the execution for logs and audit; it never reaches the
/// database.
const METADATA_STEP_ID: &str = "runtime-read";

/// The port's refusal to hand out a physical connection handle.
///
/// The Runtime path addresses sessions logically, so the three default
/// methods of the port — which are all built on a physical
/// [`ConnectionHandle`] — cannot be used here and are overridden instead.
/// The wording is part of the contract and must not drift.
const NO_PHYSICAL_HANDLE: &str =
    "The Runtime metadata source does not expose physical session handles";

/// A metadata source that reads through Runtime sessions and the shared
/// execution gateway.
///
/// The source owns exactly two dependencies, both form-neutral: the Runtime
/// facade (for the session-to-connection mapping) and the execution host (for
/// the profile, the driver and the command itself).
pub struct RuntimeSchemaContextSource {
    runtime: Arc<RuntimeConnectionUseCases>,
    host: Arc<dyn ResourceExecutionHost>,
}

impl RuntimeSchemaContextSource {
    pub fn new(
        runtime: Arc<RuntimeConnectionUseCases>,
        host: Arc<dyn ResourceExecutionHost>,
    ) -> Self {
        Self { runtime, host }
    }

    /// The connection that owns a runtime session.
    ///
    /// A closed or unknown session has no owner projection, and neither has a
    /// session that is no longer attached to its directory entry.
    fn connection_for_session(&self, session_id: &str) -> Result<String, String> {
        self.runtime
            .connection_id_for_session(&DbSessionId::new(session_id))
            .map(|connection| connection.as_str().to_owned())
            .ok_or_else(|| "Runtime session is unavailable".to_owned())
    }

    /// The profile of the connection that owns the session.
    ///
    /// A disabled profile is reported as unavailable: the metadata path has
    /// no business reading a connection the user turned off, and the shared
    /// execution gateway refuses it a moment later anyway.
    async fn profile_for_session(
        &self,
        session_id: &str,
    ) -> Result<(String, ConnectionConfig), String> {
        let connection = self.connection_for_session(session_id)?;
        let (config, _revision, enabled) =
            self.host.profile(&connection).await.map_err(host_message)?;
        if !enabled {
            return Err("Connection profile unavailable".into());
        }
        Ok((connection, config))
    }

    /// Run one read-only metadata command against the session's connection.
    ///
    /// The command travels through the shared execution gateway, so the
    /// profile revision, the target resolution, the cancellation and the
    /// result transport are the shared ones — a metadata read is an ordinary
    /// independent Runtime target, not a second execution path.
    async fn read_session_command(
        &self,
        session_id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (connection, _) = self.profile_for_session(session_id).await?;
        let step =
            WorkflowCommandStep::new(METADATA_STEP_ID, command, Some(connection.clone()), input);
        resource_execution::execute(self.host.as_ref(), &step, &connection, None, None, false)
            .await
            .map(|result| result.data)
            .map_err(|error| error.to_string())
    }
}

/// The message a caller sees when the host cannot supply a profile or a
/// driver.
///
/// `ConnectionFailed` carries the host's own wording; keeping it verbatim
/// preserves the text callers already handle. Every other variant keeps its
/// own display form.
fn host_message(error: WorkflowError) -> String {
    match error {
        WorkflowError::ConnectionFailed { message, .. } => message,
        other => other.to_string(),
    }
}

#[async_trait::async_trait]
impl SchemaContextSource for RuntimeSchemaContextSource {
    async fn get_session(
        &self,
        _id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        Err(NO_PHYSICAL_HANDLE.into())
    }

    async fn driver_for_session(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        let (_connection, config) = self.profile_for_session(id).await?;
        self.host.driver(&config).await.map_err(host_message)
    }

    async fn session_schema(&self, id: &str) -> Option<String> {
        self.profile_for_session(id)
            .await
            .ok()
            .and_then(|(_connection, config)| config.schema)
    }

    async fn tables(
        &self,
        id: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String> {
        let output = self
            .read_session_command(
                id,
                "list_tables",
                serde_json::json!({ "database": database, "schema": schema }),
            )
            .await?;
        Ok(datazen_driver_api::parse_tables_from_command(&output))
    }

    async fn table_schema(
        &self,
        _cache: &SchemaCache,
        id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<TableSchema, String> {
        let output = self
            .read_session_command(
                id,
                "get_table_schema",
                serde_json::json!({ "database": database, "schema": schema, "table": table }),
            )
            .await?;
        // The shared parser is the primary path. The fallback repeats the
        // deserialization of the raw payload so a malformed answer still
        // fails with the very same message callers already handle instead of
        // a generic "unavailable".
        match datazen_driver_api::parse_table_schema_from_command(&output) {
            Some(table_schema) => Ok(table_schema),
            None => {
                serde_json::from_value(output["schema"].clone()).map_err(|error| error.to_string())
            }
        }
    }
}

/// The metadata source of a form whose Runtime is not wired.
///
/// The shared source needs a Runtime facade; an assembly that failed to
/// provide one has no metadata to read. Binding this source keeps that
/// failure explicit instead of pretending the database is empty.
pub struct UnavailableSchemaContextSource {
    reason: String,
}

impl UnavailableSchemaContextSource {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait::async_trait]
impl SchemaContextSource for UnavailableSchemaContextSource {
    async fn get_session(
        &self,
        _id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        Err(NO_PHYSICAL_HANDLE.into())
    }

    async fn driver_for_session(&self, _id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        Err(self.reason.clone())
    }

    async fn session_schema(&self, _id: &str) -> Option<String> {
        None
    }

    async fn tables(
        &self,
        _id: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, String> {
        Err(self.reason.clone())
    }

    async fn table_schema(
        &self,
        _cache: &SchemaCache,
        _id: &str,
        _database: &str,
        _schema: Option<&str>,
        _table: &str,
    ) -> Result<TableSchema, String> {
        Err(self.reason.clone())
    }
}
