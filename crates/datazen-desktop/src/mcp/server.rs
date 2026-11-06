//! DataZen MCP Server — orchestration entry (struct, core helpers, tests).

use crate::commands::AppState;
use crate::mcp::session::McpSessionRegistry;
use crate::mcp::tool_help;
use crate::platform::identity::{new_request_id, DesktopIdentity};
use datazen::application::identity_policy::IdentityPolicy;
use datazen::mcp::allowlist;
use datazen::mcp::approval::{approval_preview, McpApprovalDecision, McpApprovalPort};
use datazen::mcp::batch;
use datazen::mcp::permission::McpPermissionMode;
use datazen::mcp::policy::{McpCallPlan, McpToolPolicy};
use datazen::mcp::session_tools::{StatefulCall, StatefulSessionHost};
use datazen::mcp::sql_scope::{attempted_sql_targets, validate_sql_scope, SqlScopeDialect};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::id::ClientInstanceId;
use datazen::workflow::resource_execution::ResourceExecutionHost;
use datazen_driver_api::{ConnectionConfig, QueryResult, Value};
use rmcp::ErrorData as McpError;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

pub use super::types::*;

#[derive(Clone)]
pub struct DataZenMcpServer {
    pub(crate) app_state: Arc<AppState>,
    /// M4: the shared read-only command use cases (`datazen::workflow::runtime_reads`)
    /// run on this port. The desktop implementation is `RuntimeCommandSource`,
    /// assembled once per server instance.
    pub(crate) resource_host: Arc<dyn ResourceExecutionHost>,
    pub(crate) disabled_tools: HashSet<String>,
    pub(crate) permission_mode: McpPermissionMode,
    /// Non-empty allowlist required — empty means deny-all (secure default).
    pub(crate) allowed_connection_ids: Vec<String>,
    /// Additional database scope, intersected with the connection allowlist.
    /// Empty means no additional scope (backward-compatible secure default).
    pub(crate) allowed_databases: Vec<String>,
    /// Additional namespace/schema scope, intersected with the allowlist.
    /// Empty means no additional scope.
    pub(crate) allowed_namespaces: Vec<String>,
    /// Approval presenter; production is the Tauri event broker, tests inject
    /// a deterministic port. Missing presenter fails closed.
    pub(crate) approvals: Arc<dyn McpApprovalPort>,
    /// Stateful tool mappings (memory only). Shared across clones: the tool router
    /// keeps its own clone, and a mapping opened through it must stay visible to
    /// the server that owns the transport.
    pub(crate) sessions: Arc<McpSessionRegistry>,
    /// Backend-generated client instance id (consumer-adapters §4.1). One per MCP
    /// server instance; never taken from the client.
    pub(crate) client_instance_id: String,
}

/// Owner purpose recorded for stateful MCP sessions. Display only — never an
/// authorization input (summary §5.1(3)).
pub const MCP_SESSION_PURPOSE: &str = "mcp-stateful-session";

pub const MCP_ALL_TOOLS: &[&str] = &[
    "list_connections",
    "list_databases",
    "list_tables",
    "search_tables",
    "query",
    "get_schema",
    "explain_query",
    "describe_table",
    "list_workflows",
    "run_workflow",
    // §4.2 stateful protocol: explicit open / use / close.
    "open_session",
    "use_session",
    "close_session",
    // T20: bounded batch and read-only Redis surface.
    "execute_batch",
    "redis_command",
];

impl DataZenMcpServer {
    pub fn new(app_state: Arc<AppState>) -> Self {
        let approvals = default_approval_port(&app_state);
        Self {
            resource_host: crate::services::runtime_command_source::resource_host(&app_state),
            app_state,
            disabled_tools: HashSet::new(),
            permission_mode: McpPermissionMode::default(),
            allowed_connection_ids: Vec::new(),
            allowed_databases: Vec::new(),
            allowed_namespaces: Vec::new(),
            approvals,
            sessions: Arc::new(McpSessionRegistry::new()),
            client_instance_id: crate::platform::identity::new_client_instance_id().to_string(),
        }
    }

    /// Backend-generated client instance id of this MCP server instance.
    pub(crate) fn client_instance_id(&self) -> &str {
        &self.client_instance_id
    }

    /// Call context for this MCP server instance (consumer-adapters §4.1).
    ///
    /// Organization and principal stay the desktop launch subject; only
    /// `clientInstanceId` is this server's own backend-generated id. Another MCP
    /// client is therefore a different owner at the gateway rather than another
    /// call from the same desktop instance, which is what makes a stateful session
    /// unusable by a client that never opened it. `platform::identity` keeps
    /// assembling the launch identity only and must never take an identity as a
    /// parameter, so this assembly lives in the adapter that owns the id.
    pub(crate) fn request_context(&self, identity: &DesktopIdentity) -> RequestContext {
        RequestContext::new(
            identity.organization_id().clone(),
            identity.principal_id().clone(),
            None,
            ClientInstanceId::new(self.client_instance_id.to_owned()),
            new_request_id(),
            None,
        )
    }

    /// Drop every mapping this client opened and close the database sessions
    /// behind them. Called when the transport goes away: a mapping whose client
    /// is gone is not a session to keep alive until its TTL runs out.
    pub(crate) async fn release_sessions(&self) {
        self.sessions
            .disconnected
            .store(true, std::sync::atomic::Ordering::Release);
        self.sessions.close_client(self.client_instance_id());
        self.start_session_cleanup();
    }

    pub fn with_disabled_tools(mut self, disabled: &[String]) -> Self {
        self.disabled_tools = disabled.iter().cloned().collect();
        self
    }

    pub fn with_permission_mode(mut self, mode: McpPermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }

    pub fn with_allowed_connections(mut self, allowed: &[String]) -> Self {
        self.allowed_connection_ids = allowed.to_vec();
        self
    }

    pub fn with_allowed_scopes(mut self, databases: &[String], namespaces: &[String]) -> Self {
        self.allowed_databases = databases.to_vec();
        self.allowed_namespaces = namespaces.to_vec();
        self
    }

    pub fn with_approval_port(mut self, approvals: Arc<dyn McpApprovalPort>) -> Self {
        self.approvals = approvals;
        self
    }

    pub(crate) fn tool_policy(&self) -> McpToolPolicy {
        McpToolPolicy {
            permission_mode: self.permission_mode,
            disabled_tools: self.disabled_tools.clone(),
            allowed_connection_ids: self.allowed_connection_ids.clone(),
            allowed_databases: self.allowed_databases.clone(),
            allowed_namespaces: self.allowed_namespaces.clone(),
        }
    }

    /// Pre-dispatch gate. A plan can only come back after permission, scope,
    /// SQL-target validation, and (for writes) an explicit Allow once. Denied
    /// calls are audited here and never reach a tool body.
    pub(crate) async fn prepare_tool_call(
        &self,
        tool_name: &str,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<PreparedToolCall, McpError> {
        let request_id = new_request_id().to_string();
        let scoped = self.with_scope_context(tool_name, arguments).await;
        if let Some(error) = scoped.error.as_deref() {
            let target_arguments = scoped.arguments.as_ref().or(arguments);
            let sql = target_arguments
                .and_then(|args| args.get("sql"))
                .and_then(serde_json::Value::as_str);
            self.audit_denied_call(
                tool_name,
                None,
                target_arguments,
                sql,
                "deny",
                error,
                &request_id,
            )
            .await;
            return Err(tool_help::tool_error(tool_name, error));
        }
        let target_arguments = scoped.arguments.as_ref().or(arguments);
        let mut plan = match self.tool_policy().evaluate(tool_name, target_arguments) {
            Ok(plan) => plan,
            Err(error) => {
                let sql = target_arguments
                    .and_then(|args| args.get("sql"))
                    .and_then(serde_json::Value::as_str);
                self.audit_denied_call(
                    tool_name,
                    None,
                    target_arguments,
                    sql,
                    "deny",
                    &error,
                    &request_id,
                )
                .await;
                return Err(tool_help::tool_error(tool_name, &error));
            }
        };
        if plan.connection_id.is_none() {
            plan.connection_id = scoped.connection_id.clone();
        }
        if plan.database.is_none() {
            plan.database = scoped.default_database.clone();
        }
        if plan.namespace.is_none() {
            plan.namespace = scoped.default_namespace.clone();
        }

        if self.sql_scope_is_active() && sql_carrying_tool(tool_name, target_arguments) {
            let Some(sql) = plan.sql.as_deref() else {
                self.audit_denied_call(
                    tool_name,
                    Some(&plan),
                    target_arguments,
                    None,
                    "deny",
                    "MCP SQL scope: SQL is required and could not be verified",
                    &request_id,
                )
                .await;
                return Err(tool_help::tool_error(
                    tool_name,
                    "MCP SQL scope: SQL is required and could not be verified",
                ));
            };
            let Some(dialect) = scoped.dialect else {
                self.audit_denied_call(
                    tool_name,
                    Some(&plan),
                    target_arguments,
                    Some(sql),
                    "deny",
                    "MCP SQL scope: this driver's SQL target shape cannot be verified",
                    &request_id,
                )
                .await;
                return Err(tool_help::tool_error(
                    tool_name,
                    "MCP SQL scope: this driver's SQL target shape cannot be verified",
                ));
            };
            let statements = if tool_name == "execute_batch" {
                match batch::parse_sql_batch(sql) {
                    Ok(statements) => statements,
                    Err(error) => {
                        let reason = format!("MCP SQL scope: {error}");
                        self.audit_denied_call(
                            tool_name,
                            Some(&plan),
                            target_arguments,
                            Some(sql),
                            "deny",
                            &reason,
                            &request_id,
                        )
                        .await;
                        return Err(tool_help::tool_error(tool_name, &reason));
                    }
                }
            } else {
                vec![sql.to_owned()]
            };
            let mut resolved_target = None;
            for (index, statement) in statements.iter().enumerate() {
                match validate_sql_scope(
                    statement,
                    dialect,
                    scoped.default_database.as_deref(),
                    scoped.default_namespace.as_deref(),
                    &self.allowed_databases,
                    &self.allowed_namespaces,
                ) {
                    Ok(targets) => {
                        if resolved_target.is_none() {
                            resolved_target = Some(targets);
                        }
                    }
                    Err(error) => {
                        // Audit the target the statement actually attempted,
                        // not the allowed default.
                        let attempted = attempted_sql_targets(
                            statement,
                            dialect,
                            scoped.default_database.as_deref(),
                            scoped.default_namespace.as_deref(),
                        )
                        .or(resolved_target);
                        if let Some(attempted) = attempted {
                            if let Some(database) = attempted.database_label() {
                                plan.database = Some(database);
                            }
                            if let Some(namespace) = attempted.namespace_label() {
                                plan.namespace = Some(namespace);
                            }
                        }
                        let reason = if tool_name == "execute_batch" {
                            format!("MCP SQL scope: statement {}: {error}", index + 1)
                        } else {
                            format!("MCP SQL scope: {error}")
                        };
                        self.audit_denied_call(
                            tool_name,
                            Some(&plan),
                            target_arguments,
                            Some(sql),
                            "deny",
                            &reason,
                            &request_id,
                        )
                        .await;
                        return Err(tool_help::tool_error(tool_name, &reason));
                    }
                }
            }
            if let Some(targets) = resolved_target {
                if let Some(database) = targets.database_label() {
                    plan.database = Some(database);
                }
                if let Some(namespace) = targets.namespace_label() {
                    plan.namespace = Some(namespace);
                }
            }
        }

        if plan.requires_approval {
            let timeout = Duration::from_secs(5 * 60);
            let now = chrono::Utc::now();
            let request = approval_preview(
                &request_id,
                tool_name,
                plan.connection_id.as_deref(),
                plan.database.as_deref(),
                plan.namespace.as_deref(),
                plan.sql.as_deref(),
                arguments,
                &now.to_rfc3339(),
                &(now + chrono::Duration::from_std(timeout).unwrap_or_default()).to_rfc3339(),
                timeout.as_millis() as u64,
            );
            let decision = self.approvals.decide(&request).await;
            if !decision.is_allowed() {
                self.audit_denied_call(
                    tool_name,
                    Some(&plan),
                    target_arguments,
                    plan.sql.as_deref(),
                    decision.audit_decision(),
                    decision.audit_decision(),
                    &request_id,
                )
                .await;
                let reason = match decision {
                    McpApprovalDecision::Deny => "denied by the user",
                    McpApprovalDecision::Timeout => "approval timed out (5 minutes)",
                    McpApprovalDecision::Cancel => "cancelled by the user",
                    McpApprovalDecision::AllowOnce => unreachable!(),
                };
                return Err(tool_help::tool_error(
                    tool_name,
                    &format!("MCP call was not executed: {reason}"),
                ));
            }
        }

        Ok(PreparedToolCall {
            plan,
            request_id,
            default_database: scoped.default_database,
            default_namespace: scoped.default_namespace,
        })
    }

    fn sql_scope_is_active(&self) -> bool {
        !self.allowed_databases.is_empty() || !self.allowed_namespaces.is_empty()
    }

    /// Resolve the call target before policy evaluation. Scope must use the
    /// connection's *actual* default database/schema: profiles saved before the
    /// fields existed often have `null`, and copying only `Some` values made
    /// valid defaults look out of scope. The driver is probed for the missing
    /// dimension; with no active scope a probe failure is non-fatal and only
    /// leaves the audit target empty.
    async fn with_scope_context(
        &self,
        tool_name: &str,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> ScopeContext {
        let mut context = ScopeContext {
            arguments: arguments.cloned(),
            ..ScopeContext::default()
        };
        let connection_id = arguments
            .and_then(|args| args.get("connection_id"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .or_else(|| {
                if tool_name != "use_session" {
                    return None;
                }
                let reference = argument_string(arguments, "client_session_ref")?;
                self.sessions
                    .lookup(
                        &reference,
                        &self.client_instance_id,
                        std::time::Instant::now(),
                    )
                    .ok()
                    .map(|binding| binding.connection_id)
            });
        context.connection_id = connection_id.clone();
        let scope_active = self.sql_scope_is_active();
        let audit_target_needed = connection_id.is_some();
        if !scope_active && !audit_target_needed {
            return context;
        }
        let Some(connection_id) = connection_id else {
            if scope_active && sql_carrying_tool(tool_name, arguments) {
                context.error = Some(
                    "MCP SQL scope: connection_id is required to resolve the actual default target"
                        .to_string(),
                );
            }
            return context;
        };

        let (config, _, enabled) = match self.resource_host.profile(&connection_id).await {
            Ok(profile) => profile,
            Err(error) => {
                if scope_active {
                    context.error = Some(format!(
                        "MCP SQL scope: connection profile could not be resolved: {error}"
                    ));
                }
                return context;
            }
        };
        if !enabled {
            if scope_active {
                context.error = Some("MCP SQL scope: connection profile is disabled".to_string());
            }
            return context;
        }
        context.dialect = SqlScopeDialect::from_driver_id(&config.database_type);
        if tool_name == "redis_command" {
            if scope_active && !self.allowed_namespaces.is_empty() {
                context.error = Some(
                    "MCP namespace scope: Redis has no namespace level; refusing the call"
                        .to_string(),
                );
                return context;
            }
            let database = argument_string(arguments, "database")
                .or_else(|| nonblank(config.database.clone()))
                .or_else(|| Some("0".to_string()));
            if scope_active && !self.allowed_databases.is_empty() && database.is_none() {
                context.error = Some(
                    "MCP database scope: the Redis database index is unknown; refusing the call"
                        .to_string(),
                );
                return context;
            }
            context.default_database = database.clone();
            if let (Some(arguments), Some(database)) = (context.arguments.as_mut(), database) {
                arguments
                    .entry("database".to_string())
                    .or_insert(serde_json::Value::String(database));
            }
            return context;
        }
        if scope_active && context.dialect.is_none() {
            context.error = Some(
                "MCP SQL scope: this driver's SQL target shape cannot be verified".to_string(),
            );
            return context;
        }
        if scope_active
            && !self.allowed_namespaces.is_empty()
            && context
                .dialect
                .is_some_and(|dialect| !dialect.has_namespace_level())
        {
            context.error =
                Some("MCP SQL scope: this driver has no verifiable namespace level".to_string());
            return context;
        }

        let mut database =
            argument_string(arguments, "database").or_else(|| nonblank(config.database.clone()));
        let mut namespace = argument_string(arguments, "schema")
            .or_else(|| argument_string(arguments, "namespace"))
            .or_else(|| nonblank(config.schema.clone()));
        let needs_probe =
            database.is_none() || (namespace.is_none() && !self.allowed_namespaces.is_empty());
        if needs_probe {
            match self
                .probe_connection_defaults(&connection_id, &config)
                .await
            {
                Ok((probed_database, probed_namespace)) => {
                    if database.is_none() {
                        database = probed_database;
                    }
                    if namespace.is_none() {
                        namespace = probed_namespace;
                    }
                }
                Err(error) => {
                    let missing_required_database =
                        !self.allowed_databases.is_empty() && database.is_none();
                    let missing_required_namespace =
                        !self.allowed_namespaces.is_empty() && namespace.is_none();
                    if scope_active && (missing_required_database || missing_required_namespace) {
                        context.error = Some(format!(
                            "MCP SQL scope: actual default database/schema could not be resolved: {error}"
                        ));
                        return context;
                    }
                }
            }
        }
        if database.is_none()
            && config
                .database_type
                .to_ascii_lowercase()
                .starts_with("sqlite")
        {
            database = Some("main".to_string());
        }
        if scope_active && !self.allowed_databases.is_empty() && database.is_none() {
            context.error = Some(
                "MCP SQL scope: the actual default database is unknown; refusing the call"
                    .to_string(),
            );
            return context;
        }
        if scope_active
            && !self.allowed_namespaces.is_empty()
            && namespace.is_none()
            && context.dialect.is_some_and(|dialect| {
                matches!(
                    dialect,
                    SqlScopeDialect::PostgreSql | SqlScopeDialect::MsSql | SqlScopeDialect::DuckDb
                )
            })
        {
            context.error = Some(
                "MCP SQL scope: the actual default schema is unknown; refusing the call"
                    .to_string(),
            );
            return context;
        }
        context.default_database = database.clone();
        context.default_namespace = namespace.clone();
        if let Some(arguments) = context.arguments.as_mut() {
            if let Some(database) = database {
                arguments
                    .entry("database".to_string())
                    .or_insert(serde_json::Value::String(database));
            }
            if let Some(namespace) = namespace {
                arguments
                    .entry("schema".to_string())
                    .or_insert(serde_json::Value::String(namespace));
            }
        }
        context
    }

    /// Probe the live connection for its actual default target. The profile
    /// fields are authoritative when present; the probe fills only the missing
    /// dimension, so a declared target is never silently replaced.
    pub(crate) async fn probe_connection_defaults(
        &self,
        connection_id: &str,
        config: &ConnectionConfig,
    ) -> Result<(Option<String>, Option<String>), String> {
        let dialect = SqlScopeDialect::from_driver_id(&config.database_type)
            .ok_or_else(|| "unsupported SQL driver".to_string())?;
        let probe_sql = default_target_probe_sql(dialect);
        // Resolve the defaults from the live driver handle. A profile with a
        // null database must not go through `read_target_command`, which
        // requires an explicit database for multi-database drivers.
        let direct = tokio::time::timeout(Duration::from_secs(3), async {
            let driver = self
                .resource_host
                .driver(config)
                .await
                .map_err(|error| format!("driver resolution failed: {error}"))?;
            let handle = driver
                .connect(config)
                .await
                .map_err(|error| format!("driver connect failed: {error}"))?;
            let result = driver.query(&handle, probe_sql).await;
            let _ = driver.disconnect(handle).await;
            result.map_err(|error| format!("driver query failed: {error}"))
        })
        .await;
        if let Ok(Ok(result)) = direct {
            if let Some(defaults) = find_driver_probe_row(&result) {
                return Ok(defaults);
            }
        }

        // Some runtime-only test/runtime hosts expose a pooled handle that is
        // not connected on this path. Keep the command fallback for those
        // hosts; real drivers resolve above without requiring a database.
        let value = datazen::workflow::runtime_reads::read_target_command(
            self.resource_host.as_ref(),
            connection_id,
            "query",
            serde_json::json!({ "sql": probe_sql }),
        )
        .await
        .map_err(|error| format!("default target probe failed: {error}"))?;
        let (database, namespace) = find_default_probe_row(&value)
            .ok_or_else(|| "default target probe returned no row".to_string())?;
        Ok((database, namespace))
    }

    /// Persist the final allow/deny outcome. Audit failures are logged and
    /// emitted as a visible event, never propagated into the tool result.
    pub(crate) async fn audit_tool_call(
        &self,
        tool_name: &str,
        connection_id: Option<String>,
        database: Option<String>,
        namespace: Option<String>,
        decision: &str,
        row_count: Option<i64>,
        request_id: Option<String>,
    ) {
        self.audit_tool_call_with_reason(
            tool_name,
            connection_id,
            database,
            namespace,
            decision,
            row_count,
            None,
            request_id,
        )
        .await;
    }

    /// Audit one decision with an optional reason (batch statement index,
    /// Redis command/key). Reasons never carry credentials or full SQL.
    pub(crate) async fn audit_tool_call_with_reason(
        &self,
        tool_name: &str,
        connection_id: Option<String>,
        database: Option<String>,
        namespace: Option<String>,
        decision: &str,
        row_count: Option<i64>,
        reason: Option<String>,
        request_id: Option<String>,
    ) {
        let (organization_id, principal_id) = self.audit_identity();
        let mut record = crate::store::new_audit_record(
            organization_id,
            principal_id,
            connection_id,
            database,
            namespace,
            tool_name,
            decision,
            row_count,
            request_id,
        );
        record.reason = reason;
        if self
            .app_state
            .store
            .mcp_audit_log()
            .append(&record)
            .await
            .is_err()
        {
            // Do not put the storage path or backend error text in a log,
            // error, or event visible to the user.
            tracing::warn!(tool = tool_name, "MCP audit append failed");
            self.emit_audit_error("MCP audit write failed");
        }
    }

    pub(crate) async fn audit_allowed_call(
        &self,
        tool_name: &str,
        prepared: &PreparedToolCall,
        result_text: &str,
    ) {
        self.audit_tool_call(
            tool_name,
            prepared.plan.connection_id.clone(),
            prepared.plan.database.clone(),
            prepared.plan.namespace.clone(),
            "allow",
            matches!(
                tool_name,
                "query" | "use_session" | "run_workflow" | "execute_batch"
            )
            .then(|| row_count_from_output(result_text))
            .flatten(),
            Some(prepared.request_id.clone()),
        )
        .await;
    }

    /// Audit a tool body outcome. Redis denials and batch statements get
    /// dedicated rows; every batch row carries a non-null rowCount.
    pub(crate) async fn audit_call_outcome(
        &self,
        tool_name: &str,
        prepared: &PreparedToolCall,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
        result_text: Option<&str>,
        error: Option<&str>,
    ) {
        match tool_name {
            "redis_command" => {
                self.audit_redis_outcome(prepared, arguments, error).await;
            }
            "execute_batch" => {
                self.audit_batch_outcome(prepared, arguments, result_text, error)
                    .await;
            }
            _ => {
                // Generic tool bodies keep the historical contract: the call
                // passed the pre-dispatch gate and execution was attempted, so
                // a driver/body error still records allow. Redis and batch are
                // handled above with explicit deny/rollback rows.
                self.audit_allowed_call(tool_name, prepared, result_text.unwrap_or(""))
                    .await;
            }
        }
    }

    async fn audit_redis_outcome(
        &self,
        prepared: &PreparedToolCall,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
        error: Option<&str>,
    ) {
        let command = arguments
            .and_then(|args| args.get("command"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing>");
        let keys: Vec<&str> = arguments
            .and_then(|args| args.get("args"))
            .and_then(serde_json::Value::as_array)
            .map(|args| args.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();
        let key_summary = if keys.is_empty() {
            "<none>".to_string()
        } else {
            keys.join(",")
        };
        let (decision, reason) = match error {
            Some(error) => (
                "deny",
                format!("redis command {command} rejected: {error}; keys={key_summary}"),
            ),
            None => (
                "allow",
                format!("redis command {command} executed; keys={key_summary}"),
            ),
        };
        self.audit_tool_call_with_reason(
            "redis_command",
            prepared.plan.connection_id.clone(),
            prepared.plan.database.clone(),
            prepared.plan.namespace.clone(),
            decision,
            None,
            Some(reason),
            Some(prepared.request_id.clone()),
        )
        .await;
    }

    async fn audit_batch_outcome(
        &self,
        prepared: &PreparedToolCall,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
        result_text: Option<&str>,
        error: Option<&str>,
    ) {
        let report =
            result_text.and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
        if let Some(report) = report {
            let overall_ok = report
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let transaction = report
                .get("transaction")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let rolled_back = report
                .get("rolledBack")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let mut total: i64 = 0;
            let statements: &[serde_json::Value] = report
                .get("statements")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut reported: std::collections::BTreeMap<usize, Vec<&serde_json::Value>> =
                std::collections::BTreeMap::new();
            for (position, statement) in statements.iter().enumerate() {
                let index = statement
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(position);
                reported.entry(index).or_default().push(statement);
            }
            let parsed = arguments
                .and_then(|args| args.get("sql"))
                .and_then(serde_json::Value::as_str)
                .map(batch::parse_sql_batch)
                .and_then(Result::ok)
                .unwrap_or_default();
            let mut indexes: Vec<usize> = (0..parsed.len()).collect();
            indexes.extend(reported.keys().copied());
            indexes.sort_unstable();
            indexes.dedup();
            for index in indexes {
                let Some(frames) = reported.get(&index) else {
                    // Parsed but never attempted because an earlier statement
                    // failed. Keep one audit row per statement regardless.
                    self.audit_tool_call_with_reason(
                        "execute_batch",
                        prepared.plan.connection_id.clone(),
                        prepared.plan.database.clone(),
                        prepared.plan.namespace.clone(),
                        "deny",
                        Some(0),
                        Some(format!(
                            "batch statement {} not executed after an earlier failure",
                            index + 1
                        )),
                        Some(prepared.request_id.clone()),
                    )
                    .await;
                    continue;
                };
                let row_count = frames
                    .iter()
                    .filter_map(|frame| frame.get("rowCount").and_then(serde_json::Value::as_i64))
                    .sum::<i64>();
                total += row_count;
                let statement_ok = frames.iter().all(|frame| {
                    frame
                        .get("ok")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                });
                let detail = frames
                    .iter()
                    .find_map(|frame| frame.get("error").and_then(serde_json::Value::as_str))
                    .unwrap_or("batch statement failed");
                let (decision, reason) = if statement_ok && overall_ok {
                    ("allow", format!("batch statement {} executed", index + 1))
                } else if statement_ok && !transaction {
                    (
                        "allow",
                        format!(
                            "batch statement {} executed before a later statement failed",
                            index + 1
                        ),
                    )
                } else if statement_ok && rolled_back {
                    (
                        "deny",
                        format!(
                            "batch statement {} rolled back by the transaction",
                            index + 1
                        ),
                    )
                } else {
                    (
                        "deny",
                        format!("batch statement {} denied: {detail}", index + 1),
                    )
                };
                self.audit_tool_call_with_reason(
                    "execute_batch",
                    prepared.plan.connection_id.clone(),
                    prepared.plan.database.clone(),
                    prepared.plan.namespace.clone(),
                    decision,
                    Some(row_count),
                    Some(reason),
                    Some(prepared.request_id.clone()),
                )
                .await;
            }
            let summary_rows = report
                .get("rowCount")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(total);
            let summary_reason = if overall_ok {
                "batch committed".to_string()
            } else if rolled_back {
                "batch rolled back".to_string()
            } else {
                report
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("batch did not complete")
                    .to_string()
            };
            self.audit_tool_call_with_reason(
                "execute_batch",
                prepared.plan.connection_id.clone(),
                prepared.plan.database.clone(),
                prepared.plan.namespace.clone(),
                if overall_ok { "allow" } else { "deny" },
                Some(summary_rows),
                Some(summary_reason),
                Some(prepared.request_id.clone()),
            )
            .await;
            return;
        }

        // The body failed before a report existed: audit every parsed statement
        // as denied rather than leaving a single allow row.
        let sql = arguments
            .and_then(|args| args.get("sql"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let reason = error.unwrap_or("batch did not execute");
        let statements = batch::parse_sql_batch(sql).unwrap_or_default();
        for (index, _) in statements.iter().enumerate() {
            self.audit_tool_call_with_reason(
                "execute_batch",
                prepared.plan.connection_id.clone(),
                prepared.plan.database.clone(),
                prepared.plan.namespace.clone(),
                "deny",
                Some(0),
                Some(format!("batch statement {} denied: {reason}", index + 1)),
                Some(prepared.request_id.clone()),
            )
            .await;
        }
        self.audit_tool_call_with_reason(
            "execute_batch",
            prepared.plan.connection_id.clone(),
            prepared.plan.database.clone(),
            prepared.plan.namespace.clone(),
            "deny",
            Some(0),
            Some(reason.to_string()),
            Some(prepared.request_id.clone()),
        )
        .await;
    }

    /// Per-statement denial evidence when the pre-dispatch gate rejects a
    /// batch (default write guard, SQL scope, or policy). The batch summary is
    /// recorded after the statement rows.
    pub(crate) async fn audit_batch_denied(
        &self,
        plan: &McpCallPlan,
        sql: Option<&str>,
        decision: &str,
        reason: &str,
        request_id: &str,
    ) {
        let statements = sql
            .map(batch::parse_sql_batch)
            .and_then(Result::ok)
            .unwrap_or_default();
        for (index, _) in statements.iter().enumerate() {
            self.audit_tool_call_with_reason(
                "execute_batch",
                plan.connection_id.clone(),
                plan.database.clone(),
                plan.namespace.clone(),
                decision,
                Some(0),
                Some(format!("batch statement {} denied: {reason}", index + 1)),
                Some(request_id.to_string()),
            )
            .await;
        }
        self.audit_tool_call_with_reason(
            "execute_batch",
            plan.connection_id.clone(),
            plan.database.clone(),
            plan.namespace.clone(),
            decision,
            Some(0),
            Some(reason.to_string()),
            Some(request_id.to_string()),
        )
        .await;
    }

    /// Audit a pre-dispatch denial. Batch denials get one row per parsed
    /// statement plus a summary; other tools keep the single-row behavior.
    async fn audit_denied_call(
        &self,
        tool_name: &str,
        plan: Option<&McpCallPlan>,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
        sql: Option<&str>,
        decision: &str,
        reason: &str,
        request_id: &str,
    ) {
        if tool_name == "execute_batch" {
            let (connection_id, database, namespace) = plan
                .map(|plan| {
                    (
                        plan.connection_id.clone(),
                        plan.database.clone(),
                        plan.namespace.clone(),
                    )
                })
                .unwrap_or_else(|| audit_target(arguments));
            let owned = McpCallPlan {
                connection_id,
                database,
                namespace,
                sql: sql.map(str::to_owned),
                requires_approval: false,
            };
            self.audit_batch_denied(&owned, sql, decision, reason, request_id)
                .await;
            return;
        }
        let (connection_id, database, namespace) = plan
            .map(|plan| {
                (
                    plan.connection_id.clone(),
                    plan.database.clone(),
                    plan.namespace.clone(),
                )
            })
            .unwrap_or_else(|| audit_target(arguments));
        let reason = if tool_name == "redis_command" {
            let command = arguments
                .and_then(|args| args.get("command"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing>");
            let keys: Vec<&str> = arguments
                .and_then(|args| args.get("args"))
                .and_then(serde_json::Value::as_array)
                .map(|args| args.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            format!(
                "{reason}; redis command {command}; keys={}",
                if keys.is_empty() {
                    "<none>".to_string()
                } else {
                    keys.join(",")
                }
            )
        } else {
            reason.to_string()
        };
        self.audit_tool_call_with_reason(
            tool_name,
            connection_id,
            database,
            namespace,
            decision,
            None,
            Some(reason),
            Some(request_id.to_string()),
        )
        .await;
    }

    fn audit_identity(&self) -> (String, String) {
        if let Ok(adapter) = self.app_state.platform_require() {
            return (
                adapter.identity().organization_id().to_string(),
                adapter.identity().principal_id().to_string(),
            );
        }
        (
            self.app_state
                .organization_scope
                .organization_id()
                .to_string(),
            "unknown".to_string(),
        )
    }

    fn emit_audit_error(&self, message: &str) {
        if let Some(handle) = self.app_state.app_handle.get().cloned() {
            use tauri::Emitter;
            let _ = handle.emit(crate::mcp::MCP_AUDIT_ERROR_EVENT, message);
        }
    }

    pub(crate) fn map_err(e: String) -> McpError {
        McpError::internal_error(e, None)
    }

    pub(crate) fn ensure_allowed(
        &self,
        tool_name: &str,
        connection_id: &str,
    ) -> Result<(), McpError> {
        allowlist::ensure_connection_allowed(connection_id, &self.allowed_connection_ids)
            .map_err(|e| tool_help::tool_error(tool_name, &e))
    }

    pub(crate) fn tool_is_registered(&self, name: &str) -> bool {
        Self::tool_router().get(name).is_some()
    }
}

fn default_approval_port(app_state: &Arc<AppState>) -> Arc<dyn McpApprovalPort> {
    let broker = crate::mcp::approval_broker();
    if let Some(handle) = app_state.app_handle.get().cloned() {
        broker.attach_app_handle(handle);
    }
    broker
}

/// Inject the resolved default target into tool arguments that accept a
/// database/schema. Query tools have no public namespace field, so the host
/// uses the private optional fields on their input types; metadata tools keep
/// any explicit caller target and only fill the missing dimension.
pub(crate) fn inject_resolved_target(
    tool_name: &str,
    arguments: &mut serde_json::Map<String, serde_json::Value>,
    plan: &McpCallPlan,
    default_database: Option<&str>,
    default_namespace: Option<&str>,
) {
    if !matches!(
        tool_name,
        "query"
            | "execute_batch"
            | "explain_query"
            | "redis_command"
            | "list_tables"
            | "search_tables"
            | "get_schema"
            | "describe_table"
    ) {
        return;
    }
    let (database, namespace) = if tool_name == "redis_command" {
        (plan.database.as_deref(), None)
    } else if matches!(tool_name, "query" | "execute_batch" | "explain_query") {
        (default_database, default_namespace)
    } else {
        (plan.database.as_deref(), plan.namespace.as_deref())
    };
    if let Some(database) = database {
        arguments
            .entry("database".to_string())
            .or_insert_with(|| serde_json::Value::String(database.to_string()));
    }
    if let Some(namespace) = namespace {
        arguments
            .entry("schema".to_string())
            .or_insert_with(|| serde_json::Value::String(namespace.to_string()));
    }
}

/// The policy result retained across execution so the allow audit can record
/// the exact target and rowcount.
pub(crate) struct PreparedToolCall {
    pub(crate) plan: McpCallPlan,
    pub(crate) request_id: String,
    /// Actual connection defaults resolved before SQL validation. SQL tools
    /// execute against these when the caller omitted an explicit target.
    pub(crate) default_database: Option<String>,
    pub(crate) default_namespace: Option<String>,
}

#[derive(Default)]
struct ScopeContext {
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
    connection_id: Option<String>,
    default_database: Option<String>,
    default_namespace: Option<String>,
    dialect: Option<SqlScopeDialect>,
    error: Option<String>,
}

fn sql_carrying_tool(
    tool_name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    matches!(tool_name, "query" | "execute_batch" | "explain_query")
        || (tool_name == "use_session"
            && matches!(
                argument_string(arguments, "command").as_deref(),
                Some("query" | "execute")
            ))
}

fn argument_string(
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<String> {
    arguments
        .and_then(|args| args.get(key))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn nonblank(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn find_driver_probe_row(result: &QueryResult) -> Option<(Option<String>, Option<String>)> {
    let row = result.rows.first()?;
    let text = |index: usize| {
        row.get(index)
            .and_then(Option::as_ref)
            .and_then(driver_value_text)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    Some((text(0), text(1)))
}

fn driver_value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) | Value::Timestamp(value) => Some(value.clone()),
        Value::Json(serde_json::Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn find_default_probe_row(value: &serde_json::Value) -> Option<(Option<String>, Option<String>)> {
    let rows = value
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .or_else(|| {
            value
                .get("results")
                .and_then(serde_json::Value::as_array)
                .and_then(|results| results.first())
                .and_then(|first| first.get("rows"))
                .and_then(serde_json::Value::as_array)
        })?;
    let row = rows.first()?.as_array()?;
    let text = |index: usize| {
        row.get(index)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Some((text(0), text(1)))
}

fn default_target_probe_sql(dialect: SqlScopeDialect) -> &'static str {
    match dialect {
        SqlScopeDialect::PostgreSql => {
            "/* datazen:mcp-defaults */ SELECT current_database() AS database_name, current_schema() AS schema_name"
        }
        SqlScopeDialect::DuckDb => {
            "/* datazen:mcp-defaults */ SELECT current_database() AS database_name, current_schema() AS schema_name"
        }
        SqlScopeDialect::MsSql => {
            "/* datazen:mcp-defaults */ SELECT DB_NAME() AS database_name, SCHEMA_NAME() AS schema_name"
        }
        SqlScopeDialect::MySql => {
            "/* datazen:mcp-defaults */ SELECT DATABASE() AS database_name, NULL AS schema_name"
        }
        SqlScopeDialect::ClickHouse => {
            "/* datazen:mcp-defaults */ SELECT currentDatabase() AS database_name, NULL AS schema_name"
        }
        SqlScopeDialect::Sqlite => {
            "/* datazen:mcp-defaults */ SELECT 'main' AS database_name, NULL AS schema_name"
        }
    }
}

fn audit_target(
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> (Option<String>, Option<String>, Option<String>) {
    let value = |key: &str| {
        arguments
            .and_then(|args| args.get(key))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    };
    (
        value("connection_id"),
        value("database"),
        value("schema").or_else(|| value("namespace")),
    )
}

pub(crate) fn row_count_from_output(text: &str) -> Option<i64> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    find_row_count(&value)
}

fn find_row_count(value: &serde_json::Value) -> Option<i64> {
    if let Some(values) = value.as_array() {
        return values.iter().find_map(find_row_count);
    }
    let object = value.as_object()?;
    // A SELECT carries its row count in `rows`; MySQL reports 0 in
    // `rowsAffected` for SELECT, so that field must never win over real rows.
    if let Some(rows) = object.get("rows").and_then(|v| v.as_array()) {
        return Some(rows.len() as i64);
    }
    for key in ["rowCount", "row_count", "rowsAffected", "rows_affected"] {
        if let Some(count) = object.get(key).and_then(|v| v.as_i64()) {
            return Some(count.max(0));
        }
    }
    for nested in object.values() {
        if let Some(count) = find_row_count(nested) {
            return Some(count);
        }
    }
    None
}

/// M2 主体归位后本机是共享有状态会话端口的形态实现：runtime 租约解析、
/// 归属裁决（IdentityPolicy 是 application 词汇，由形态裁决）与 cleanup
/// worker 调度钩子。admission/编排/守卫语义在 `datazen::mcp::session_tools`。
#[async_trait::async_trait]
impl StatefulSessionHost for DataZenMcpServer {
    async fn resolve_call(&self) -> Result<StatefulCall, String> {
        let adapter = self
            .app_state
            .platform_require()
            .map_err(|error| error.to_string())?;
        let runtime = adapter.runtime().map_err(|error| error.to_string())?;
        Ok(StatefulCall {
            runtime,
            context: self.request_context(adapter.identity()),
        })
    }

    fn authorize_owner(&self, context: &RequestContext, owner: &OwnerRef) -> Result<(), String> {
        // Stateful steps share one admission path: build a context whose
        // `clientInstanceId` is the one this server generated, derive the owner
        // from it, and let IdentityPolicy decide. MCP never carries its own allow
        // rule for ownership — if the desktop policy rejects the owner, the tool
        // fails.
        IdentityPolicy::check_owner(context, owner, None).map_err(|error| error.to_string())
    }

    fn ensure_cleanup_worker(&self) {
        self.start_session_cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::format_table_description;
    use rmcp::model::ResourceContents;

    #[test]
    fn test_uri_encoding_decoding() {
        let conn_id = urlencoding::encode("my connection");
        let db = urlencoding::encode("test db");
        let uri = format!("datazen://schema/{conn_id}/{db}");

        let rest = uri.strip_prefix("datazen://schema/").unwrap();
        let decoded = urlencoding::decode(rest).unwrap();
        let parts: Vec<&str> = decoded.splitn(2, '/').collect();
        assert_eq!(parts[0], "my connection");
        assert_eq!(parts[1], "test db");
    }

    #[test]
    fn test_simple_connection_id_parsing() {
        let uri = "datazen://schema/abc123/testdb";
        let rest = uri.strip_prefix("datazen://schema/").unwrap();
        let decoded = urlencoding::decode(rest).unwrap();
        let parts: Vec<&str> = decoded.splitn(2, '/').collect();
        assert_eq!(parts[0], "abc123");
        assert_eq!(parts[1], "testdb");
    }

    #[test]
    fn test_schema_uri_no_database() {
        let uri = "datazen://schema/abc123";
        let rest = uri.strip_prefix("datazen://schema/").unwrap();
        let decoded = urlencoding::decode(rest).unwrap();
        let parts: Vec<&str> = decoded.splitn(2, '/').collect();
        assert_eq!(parts[0], "abc123");
        assert_eq!(parts.get(1).copied().unwrap_or(""), "");
    }

    #[test]
    fn query_input_accepts_connection_id() {
        let json = r#"{"connection_id":"c1","sql":"SELECT 1"}"#;
        let parsed: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.connection_id, "c1");
    }

    #[test]
    fn query_input_rejects_removed_config_id_field() {
        let json = r#"{"config_id":"c1","sql":"SELECT 1"}"#;
        let parsed = serde_json::from_str::<QueryInput>(json);
        assert!(
            parsed.is_err(),
            "removed config_id field must not deserialize"
        );
    }

    #[test]
    fn test_list_databases_input_deserialization() {
        let json = r#"{"connection_id": "test-id"}"#;
        let input: ListDatabasesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "test-id");
    }

    #[test]
    fn test_query_input_defaults() {
        let json = r#"{"connection_id": "c1", "sql": "SELECT 1"}"#;
        let input: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
        assert_eq!(input.sql, "SELECT 1");
        assert_eq!(input.limit, None);
    }

    #[test]
    fn test_query_input_with_limit() {
        let json = r#"{"connection_id": "c1", "sql": "SELECT 1", "limit": 50}"#;
        let input: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, Some(50));
    }

    #[test]
    fn test_resolve_query_limit_none_defaults_to_100() {
        assert_eq!(
            datazen::services::db_tools::resolve_query_limit(None),
            Some(100)
        );
    }

    #[test]
    fn test_resolve_query_limit_some_passthrough() {
        assert_eq!(
            datazen::services::db_tools::resolve_query_limit(Some(50)),
            Some(50)
        );
    }

    #[test]
    fn test_resolve_query_limit_some_capped_at_max() {
        assert_eq!(
            datazen::services::db_tools::resolve_query_limit(Some(999_999)),
            Some(50_000)
        );
    }

    #[test]
    fn mcp_all_tools_list_is_stable() {
        assert_eq!(MCP_ALL_TOOLS.len(), 15);
        assert!(MCP_ALL_TOOLS.contains(&"list_connections"));
        assert!(MCP_ALL_TOOLS.contains(&"search_tables"));
        assert!(MCP_ALL_TOOLS.contains(&"run_workflow"));
        assert!(MCP_ALL_TOOLS.contains(&"open_session"));
        assert!(MCP_ALL_TOOLS.contains(&"use_session"));
        assert!(MCP_ALL_TOOLS.contains(&"close_session"));
    }

    #[test]
    fn list_tables_input_deserializes_optional_database() {
        let json = r#"{"connection_id":"c1","database":"mydb"}"#;
        let input: ListTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
        assert_eq!(input.database.as_deref(), Some("mydb"));
    }

    #[test]
    fn search_tables_input_deserializes() {
        let json = r#"{"connection_id":"c1","pattern":"user","database":"app","limit":10}"#;
        let input: SearchTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
        assert_eq!(input.pattern, "user");
        assert_eq!(input.database.as_deref(), Some("app"));
        assert_eq!(input.limit, Some(10));
    }

    #[test]
    fn search_tables_input_defaults_optional_fields() {
        let json = r#"{"connection_id":"c1","pattern":"order"}"#;
        let input: SearchTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
        assert_eq!(input.pattern, "order");
        assert!(input.database.is_none());
        assert!(input.limit.is_none());
    }

    #[test]
    fn run_workflow_input_defaults_variables_to_null() {
        let json = r#"{"workflow_id":"wf-1"}"#;
        let input: RunWorkflowInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.workflow_id, "wf-1");
        assert!(input.variables.is_null());
        assert!(input.connection_id.is_none());
    }

    #[test]
    fn nl2sql_args_deserialize() {
        let json = r#"{"connection_id":"c1","question":"count users","database":"app"}"#;
        let args: Nl2SqlArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.connection_id, "c1");
        assert_eq!(args.question, "count users");
        assert_eq!(args.database.as_deref(), Some("app"));
    }

    #[test]
    fn format_table_description_includes_columns_and_pk() {
        use datazen_driver_api::{ColumnSchema, IndexInfo, TableSchema};

        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: true,
            }],
            primary_keys: vec!["id".into()],
            indexes: vec![IndexInfo {
                name: "users_pkey".into(),
                columns: vec!["id".into()],
                is_unique: true,
                is_primary: true,
                index_type: "btree".into(),
            }],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };

        let desc = format_table_description("users", &schema);
        assert!(desc.contains("Table: users"));
        assert!(desc.contains("id integer PK NOT NULL"));
        assert!(desc.contains("Primary Key: (id)"));
        assert!(desc.contains("users_pkey"));
    }

    fn mcp_server_allowing(
        state: std::sync::Arc<crate::commands::AppState>,
        connection_ids: &[&str],
    ) -> DataZenMcpServer {
        DataZenMcpServer::new(state).with_allowed_connections(
            &connection_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>(),
        )
    }

    // Stateful session tests; include from extracted block below.
    include!("server/tests_stateful.rs");

    // Remaining integration tests live in server.rs; include from extracted block below.
    include!("server/tests_integration.rs");
}
