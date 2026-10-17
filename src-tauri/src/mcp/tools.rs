//! MCP tool handlers and helpers.

use super::server::{DataZenMcpServer, MCP_SESSION_PURPOSE};
use super::session::{sanitize_ttl, SessionBinding};
use super::types::*;
use crate::mcp::allowlist;
use crate::mcp::permission;
use crate::mcp::tool_help;
use datazen_application::dto::requests::{
    CloseMode, CloseSessionRequest, CommandCall, ExecuteInSessionRequest, OpenSessionRequest,
};
use datazen_application::identity_policy::IdentityPolicy;
use datazen_application::sessions::ConnectionUseCases;
use datazen_platform_api::context::{OwnerRef, RequestContext};
use datazen_platform_api::dto::execution::{ExecutionState, ExecutionView};
use datazen_platform_api::dto::idempotency::IdempotentOperation;
use datazen_platform_api::id::{ArtifactId, ClientInstanceId, ConnectionId, Counter};
use datazen_platform_api::target::{ExecutionTarget, NamespaceTarget, TargetNamespaceLayer};
use datazen_runtime::application::RuntimeConnectionUseCases;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData as McpError};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) use datazen_feature_core::mcp::describe::format_table_description;

#[tool_router(vis = "pub(crate)")]
impl DataZenMcpServer {
    #[tool(
        description = "List all configured database connections. Returns connection IDs, names, database types, and hosts."
    )]
    pub(crate) async fn list_connections(&self) -> Result<String, McpError> {
        let connections = self.app_state.store.get_connections().await;
        let result: Vec<serde_json::Value> = connections
            .iter()
            .filter(|c| allowlist::is_connection_allowed(&c.id, &self.allowed_connection_ids))
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "name": c.name,
                    "databaseType": format!("{:?}", c.database_type),
                    "host": c.host,
                    "database": c.database,
                })
            })
            .collect();
        serde_json::to_string_pretty(&result).map_err(|e| Self::map_err(format!("Error: {e}")))
    }

    #[tool(description = "List all databases on a connected server.")]
    pub(crate) async fn list_databases(
        &self,
        Parameters(input): Parameters<ListDatabasesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("list_databases", &input.connection_id)?;
        crate::services::db_tools::list_databases(self.app_state.as_ref(), &input.connection_id)
            .await
            .map_err(Self::map_err)
    }

    #[tool(
        description = "List all tables in a database. Returns table names with types and row counts."
    )]
    pub(crate) async fn list_tables(
        &self,
        Parameters(input): Parameters<ListTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("list_tables", &input.connection_id)?;
        crate::services::db_tools::list_tables(
            self.app_state.as_ref(),
            &input.connection_id,
            input.database.as_deref(),
            input.schema.as_deref(),
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Search for tables by name pattern (case-insensitive substring match). Use this instead of list_tables when the database has many tables."
    )]
    pub(crate) async fn search_tables(
        &self,
        Parameters(input): Parameters<SearchTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("search_tables", &input.connection_id)?;
        let limit = input.limit.unwrap_or(20) as usize;
        crate::services::db_tools::search_tables(
            self.app_state.as_ref(),
            &input.connection_id,
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.pattern,
            limit,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Execute a SQL query on a connected database. Returns results as JSON. Use list_connections first to get valid connection IDs. Default row limit is 100 (max 50000)."
    )]
    pub(crate) async fn query(
        &self,
        Parameters(input): Parameters<QueryInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("query", &input.connection_id)?;
        permission::check_sql_allowed(&input.sql, self.permission_mode)
            .map_err(|e| tool_help::tool_error("query", &e))?;
        crate::services::db_tools::query(
            self.app_state.as_ref(),
            &input.connection_id,
            &input.sql,
            input.limit,
            Some(self.permission_mode),
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get detailed schema of a table including columns, types, primary keys, foreign keys, and indexes."
    )]
    pub(crate) async fn get_schema(
        &self,
        Parameters(input): Parameters<GetSchemaInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("get_schema", &input.connection_id)?;
        let tables = vec![input.table.clone()];
        crate::services::db_tools::get_table_schema(
            self.app_state.as_ref(),
            &input.connection_id,
            &tables,
            input.database.as_deref(),
            input.schema.as_deref(),
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get the execution plan (EXPLAIN) for a SQL query. Useful for performance analysis."
    )]
    pub(crate) async fn explain_query(
        &self,
        Parameters(input): Parameters<ExplainQueryInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("explain_query", &input.connection_id)?;
        crate::services::db_tools::explain_query(
            self.app_state.as_ref(),
            &input.connection_id,
            &input.sql,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get a human-readable description of a table including columns, types, constraints, and indexes."
    )]
    pub(crate) async fn describe_table(
        &self,
        Parameters(input): Parameters<DescribeTableInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("describe_table", &input.connection_id)?;
        let schema = crate::services::db_tools::get_single_table_schema(
            self.app_state.as_ref(),
            &input.connection_id,
            &input.table,
            input.database.as_deref(),
            input.schema.as_deref(),
        )
        .await
        .map_err(Self::map_err)?;

        Ok(format_table_description(&input.table, &schema))
    }

    #[tool(
        description = "List all available user-defined workflows. Workflows are reusable AI automations combining prompts and database operations."
    )]
    pub(crate) async fn list_workflows(&self) -> Result<String, McpError> {
        let workflows = self.app_state.workflow_registry.list().await;
        serde_json::to_string_pretty(&workflows)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(
        description = "Execute a user-defined workflow by ID. Workflows are reusable automations combining prompts and database operations. Use list_workflows to see available workflows."
    )]
    pub(crate) async fn run_workflow(
        &self,
        Parameters(input): Parameters<RunWorkflowInput>,
    ) -> Result<String, McpError> {
        let workflow = self
            .app_state
            .workflow_registry
            .get(&input.workflow_id)
            .await
            .ok_or_else(|| {
                tool_help::tool_error(
                    "run_workflow",
                    &format!("Workflow '{}' not found", input.workflow_id),
                )
            })?;

        let result = crate::workflow::WorkflowExecutor::execute_with_options(
            &workflow,
            self.app_state.as_ref(),
            input.connection_id.as_deref(),
            &input.variables,
            crate::workflow::WorkflowExecuteOptions {
                permission_mode: Some(self.permission_mode),
                query_row_limit: Some(crate::workflow::WORKFLOW_QUERY_ROW_LIMIT),
            },
        )
        .await
        .map_err(|e| McpError::internal_error(e, None))?;

        Ok(result.final_output)
    }

    /// Stateful protocol step 1 (summary §4.2): open a session on an **explicit**
    /// target and hand back an opaque reference. Nothing about the GUI active
    /// connection, tab or database is consulted.
    #[tool(
        description = "Open a database session on an explicit target and return an opaque clientSessionRef. Use use_session to run commands on it and close_session to release it. Stateful: the same reference always means the same session."
    )]
    pub(crate) async fn open_session(
        &self,
        Parameters(input): Parameters<OpenSessionInput>,
    ) -> Result<String, McpError> {
        const TOOL: &str = "open_session";
        self.ensure_allowed(TOOL, &input.connection_id)?;

        let namespace = namespace_target(Some(input.database), input.schema);
        // §4.1: an empty explicit target is a target error, not a fallback to the GUI.
        if !namespace.has_value(TargetNamespaceLayer::Database) {
            return Err(tool_help::tool_error(
                TOOL,
                "open_session needs an explicit target: pass database (and schema when the driver has one). The active GUI connection is never used as a fallback.",
            ));
        }
        let ttl = sanitize_ttl(input.ttl_ms.map(Duration::from_millis));

        let call = self.session_call(TOOL)?;
        let target =
            ExecutionTarget::new(ConnectionId::new(input.connection_id.clone()), namespace);
        let token = call
            .runtime
            .issue_submission_token(&call.context, IdempotentOperation::OpenSession, None)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let receipt = call
            .runtime
            .open_session(
                &call.context,
                OpenSessionRequest::new(target, call.owner.clone(), token.idempotency_key),
            )
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let now = Instant::now();
        let binding = self.sessions.open(
            self.client_instance_id(),
            &input.connection_id,
            receipt.session.handle.clone(),
            receipt.session.context_revision,
            ttl,
            now,
        );
        let expires_in_ms = ttl.as_millis().min(u128::from(u64::MAX)) as u64;
        Ok(json!({
            "clientSessionRef": binding.reference,
            "state": receipt.session.state,
            "contextRevision": receipt.session.context_revision,
            "expiresInMs": expires_in_ms,
        })
        .to_string())
    }

    /// Stateful protocol step 2: run one command inside a mapping this client owns.
    #[tool(
        description = "Run one driver command inside a session opened by open_session. Pass the clientSessionRef returned by open_session. The reference is bound to the MCP client that opened it; a reference held by another client, or one that is closed or expired, returns SessionLost."
    )]
    pub(crate) async fn use_session(
        &self,
        Parameters(input): Parameters<UseSessionInput>,
    ) -> Result<String, McpError> {
        const TOOL: &str = "use_session";
        let now = Instant::now();
        let binding = self
            .sessions
            .lookup(&input.client_session_ref, self.client_instance_id(), now)
            .map_err(|reason| McpError::invalid_params(reason.client_message(), None))?;

        // §4.1: stdio mode runs the same SQL guard as the GUI, not a bypass.
        self.guard_session_sql(&input.command, &input.input)?;

        let call = self.session_call(TOOL)?;
        let timeout = Duration::from_millis(
            input
                .timeout_ms
                .unwrap_or(DEFAULT_USE_TIMEOUT_MS)
                .min(MAX_USE_TIMEOUT_MS),
        );
        let receipt = self
            .dispatch_session_command(&call, &binding, &input.command, input.input, timeout)
            .await?;

        let mut report = json!({
            "executionId": receipt.execution_id,
            "state": receipt.state,
            "effectOutcome": receipt.effect_outcome,
            "resultCompleteness": receipt.result_completeness,
            "artifactIds": receipt.artifact_ids,
            "truncationReason": receipt.truncation_reason,
            "errorCode": receipt.error_code,
        });
        if receipt.state == ExecutionState::Succeeded {
            let (columns, rows, truncated) = self.read_rows(&call, &receipt.artifact_ids).await?;
            report["columns"] = columns;
            report["rows"] = json!(rows);
            report["rowsTruncated"] = json!(truncated);
        }
        Ok(report.to_string())
    }

    /// Stateful protocol step 3: release one mapping this client owns.
    #[tool(
        description = "Close a session opened by open_session and release its database session. Unknown, already closed and expired references all report the same thing, so a stale reference never reveals whether it once existed."
    )]
    pub(crate) async fn close_session(
        &self,
        Parameters(input): Parameters<CloseSessionInput>,
    ) -> Result<String, McpError> {
        const TOOL: &str = "close_session";
        let now = Instant::now();
        // Idempotent: a reference this client does not hold is reported as already
        // closed, without distinguishing unknown from foreign (§4.2).
        let Ok(binding) =
            self.sessions
                .lookup(&input.client_session_ref, self.client_instance_id(), now)
        else {
            return Ok(json!({ "closed": false }).to_string());
        };

        let mode = close_mode(input.mode.as_deref())
            .map_err(|reason| tool_help::tool_error(TOOL, &reason))?;
        let call = self.session_call(TOOL)?;
        call.runtime
            .close_session(
                &call.context,
                CloseSessionRequest::new(binding.handle.clone(), mode),
            )
            .await
            // The mapping is kept on failure so the client can retry; dropping it
            // here would leak the database session with no way left to close it.
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        self.sessions
            .close(&input.client_session_ref, self.client_instance_id());
        Ok(json!({ "closed": true }).to_string())
    }
}

impl DataZenMcpServer {
    /// Stateful steps share one admission path: build a context whose
    /// `clientInstanceId` is the one this server generated, derive the owner from
    /// it, and let IdentityPolicy decide. MCP never carries its own allow rule for
    /// ownership — if the desktop policy rejects the owner, the tool fails.
    fn session_call(&self, tool: &str) -> Result<SessionCall, McpError> {
        // Expired mappings are dropped here rather than only on the next lookup,
        // so a long-lived server does not accumulate references nobody can use.
        // The database session behind one is not this adapter's to keep: the
        // gateway expires it on its own clock.
        self.sessions.purge_expired(Instant::now());
        let adapter = self
            .app_state
            .platform_require()
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let runtime = adapter
            .runtime()
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let client_instance_id = ClientInstanceId::new(self.client_instance_id().to_owned());
        let context = self.request_context(adapter.identity());
        let owner = OwnerRef::ClientSession {
            client_instance_id,
            purpose: MCP_SESSION_PURPOSE.to_owned(),
        };
        IdentityPolicy::check_owner(&context, &owner, None)
            .map_err(|error| tool_help::tool_error(tool, &error.to_string()))?;
        Ok(SessionCall {
            runtime,
            context,
            owner,
        })
    }

    /// Run the SQL guard on the SQL a stateful command carries. The command name
    /// is the contract, not the payload: `query` and `execute` carry SQL under
    /// different keys depending on the driver, so only a payload that is plainly a
    /// string is guarded and anything else is left to the driver command itself.
    fn guard_session_sql(&self, command: &str, input: &serde_json::Value) -> Result<(), McpError> {
        const TOOL: &str = "use_session";
        if command != "query" && command != "execute" {
            return Ok(());
        }
        let Some(sql) = input.get("sql").and_then(serde_json::Value::as_str) else {
            return Ok(());
        };
        permission::check_sql_allowed(sql, self.permission_mode)
            .map_err(|reason| tool_help::tool_error(TOOL, &reason))
    }

    /// Submit one command into an already owned session and wait for a terminal
    /// state. The wait is bounded: a client that stops waiting gets its
    /// execution id back and can ask again, instead of holding the call open.
    async fn dispatch_session_command(
        &self,
        call: &SessionCall,
        binding: &SessionBinding,
        command: &str,
        input: serde_json::Value,
        timeout: Duration,
    ) -> Result<ExecutionView, McpError> {
        let session = call
            .runtime
            .get_session(&call.context, binding.handle.clone())
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        // CAS against the revision recorded when the mapping was opened: a context
        // switch made through another path is a conflict, not something to overwrite.
        let expected = match session.context_revision == binding.context_revision {
            true => session.context_revision,
            false => binding.context_revision,
        };
        let token = call
            .runtime
            .issue_submission_token(
                &call.context,
                IdempotentOperation::ExecuteInSession,
                Some(&binding.handle),
            )
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let receipt = call
            .runtime
            .execute_in_session(
                &call.context,
                ExecuteInSessionRequest::new(
                    binding.handle.clone(),
                    CommandCall::new(command, input),
                    token.idempotency_key,
                )
                .with_expected_context_revision(expected),
            )
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let deadline = Instant::now() + timeout;
        loop {
            let view = call
                .runtime
                .get_execution(&call.context, receipt.execution_id.clone())
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            if view.state.is_terminal() || Instant::now() >= deadline {
                return Ok(view);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Read the result rows back out of the artifacts the execution published.
    /// `ExecutionView` carries no inline rows by design, so the MCP surface has
    /// to fetch chunks itself; the row cap keeps one call from flooding a model
    /// context, and the caller is told the payload was cut.
    async fn read_rows(
        &self,
        call: &SessionCall,
        artifact_ids: &[ArtifactId],
    ) -> Result<(serde_json::Value, Vec<serde_json::Value>, bool), McpError> {
        let mut columns = serde_json::Value::Null;
        let mut rows: Vec<serde_json::Value> = Vec::new();
        let mut truncated = false;
        for artifact in artifact_ids {
            let first = call
                .runtime
                .read_artifact_chunk(&call.context, artifact.clone(), Counter::ZERO)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            if columns.is_null() {
                columns = first.columns.clone();
            }
            for index in 0..first.published_chunk_count.get() {
                let chunk = if index == 0 {
                    first.clone()
                } else {
                    call.runtime
                        .read_artifact_chunk(&call.context, artifact.clone(), Counter::new(index))
                        .await
                        .map_err(|error| McpError::internal_error(error.to_string(), None))?
                };
                for row in chunk.rows {
                    if rows.len() >= MCP_SESSION_ROW_LIMIT {
                        truncated = true;
                        return Ok((columns, rows, truncated));
                    }
                    rows.push(row);
                }
            }
        }
        Ok((columns, rows, truncated))
    }
}

/// Everything a stateful tool needs after admission.
struct SessionCall {
    runtime: Arc<RuntimeConnectionUseCases>,
    context: RequestContext,
    owner: OwnerRef,
}

/// How long `use_session` waits for a terminal state before handing the
/// execution id back to the caller.
const DEFAULT_USE_TIMEOUT_MS: u64 = 60_000;
const MAX_USE_TIMEOUT_MS: u64 = 300_000;
/// Row cap on one `use_session` reply.
const MCP_SESSION_ROW_LIMIT: usize = 200;

/// Explicit namespace for an opened session. An empty value is meaningful: it
/// says the client did not name a database, which is an error for a stateful
/// session rather than a licence to fall back to the GUI target.
fn namespace_target(database: Option<String>, schema: Option<String>) -> NamespaceTarget {
    NamespaceTarget::new(
        database.filter(|value| !value.trim().is_empty()),
        None,
        schema.filter(|value| !value.trim().is_empty()),
        Vec::new(),
    )
}

/// Parse the close mode. Unknown strings are rejected instead of defaulting:
/// a typo must not silently downgrade to "discard uncommitted work".
fn close_mode(requested: Option<&str>) -> Result<CloseMode, String> {
    match requested.unwrap_or("requireNoTransaction") {
        "requireNoTransaction" => Ok(CloseMode::RequireNoTransaction),
        "rollbackAndClose" => Ok(CloseMode::RollbackAndClose),
        other => Err(format!(
            "{other} is not a close mode; use requireNoTransaction or rollbackAndClose"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unnamed_database_stays_unnamed_in_the_session_target() {
        let namespace = namespace_target(None, None);
        assert!(
            !namespace.has_value(TargetNamespaceLayer::Database),
            "a session target must never fall back to the GUI active database"
        );
        assert!(!namespace.has_value(TargetNamespaceLayer::Schema));
    }

    #[test]
    fn a_blank_database_is_dropped_instead_of_becoming_an_empty_namespace_layer() {
        let namespace = namespace_target(Some("   ".to_owned()), Some("public".to_owned()));
        assert!(!namespace.has_value(TargetNamespaceLayer::Database));
        assert!(
            namespace.has_value(TargetNamespaceLayer::Schema),
            "the named schema still reaches the target"
        );
    }

    #[test]
    fn the_named_database_and_schema_reach_the_session_target() {
        let namespace = namespace_target(Some("app".to_owned()), Some("billing".to_owned()));
        assert!(namespace.has_value(TargetNamespaceLayer::Database));
        assert!(namespace.has_value(TargetNamespaceLayer::Schema));
    }

    #[test]
    fn closing_without_a_mode_refuses_to_discard_uncommitted_work() {
        assert!(matches!(
            close_mode(None),
            Ok(CloseMode::RequireNoTransaction)
        ));
        assert!(matches!(
            close_mode(Some("requireNoTransaction")),
            Ok(CloseMode::RequireNoTransaction)
        ));
    }

    #[test]
    fn the_client_can_ask_for_a_rollback_and_close() {
        assert!(matches!(
            close_mode(Some("rollbackAndClose")),
            Ok(CloseMode::RollbackAndClose)
        ));
    }

    #[test]
    fn an_unknown_close_mode_is_rejected_rather_than_defaulted() {
        let reason = close_mode(Some("force"))
            .expect_err("an unknown close mode must not be silently mapped");
        assert!(
            reason.contains("force"),
            "the rejection must name what the client asked for, got: {reason}"
        );
    }

    #[test]
    fn a_requested_ttl_shorter_than_the_default_is_honoured_and_a_long_one_is_capped() {
        assert_eq!(sanitize_ttl(None), super::super::session::DEFAULT_TTL);
        assert_eq!(
            sanitize_ttl(Some(Duration::from_secs(30))),
            Duration::from_secs(30)
        );
        assert_eq!(
            sanitize_ttl(Some(Duration::from_secs(9_999))),
            super::super::session::MAX_TTL
        );
        assert_eq!(
            sanitize_ttl(Some(Duration::ZERO)),
            super::super::session::DEFAULT_TTL,
            "an explicit zero would expire every session instantly; fall back to the default"
        );
    }
}
