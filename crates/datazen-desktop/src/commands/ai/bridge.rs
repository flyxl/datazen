//! Opt-in plugin / workflow tool bridge (T22).
//!
//! Definitions are resolved from current settings on every turn, so disabling
//! a plugin or workflow removes its tools immediately. Execution re-resolves
//! the declaration after approval (TOCTOU-safe) and reuses the T19 approval
//! broker plus the MCP audit log; no second approval/audit surface exists.

use crate::commands::AppState;
use datazen::ai::application::tool_bridge::{
    count_result_rows, parse_and_validate_arguments, parse_bridge_tool, plugin_tool_definition,
    render_input_template_with_dialect, render_sql_template_with_resolver, truncate_bridge_result,
    workflow_is_read_only, workflow_tool_definition, BridgeToolDecision, BridgeToolKind,
    SqlTemplateDialect, BRIDGE_APPROVAL_TIMEOUT,
};
use datazen::ai::tool_loop::AiToolContext;
use datazen::ai::{ToolCall, ToolDefinition};
use datazen::mcp::approval::{
    approval_preview, redacted_approval_arguments, McpApprovalDecision, McpApprovalPort,
};
use datazen::workflow::application::sql_templates::{
    workflow_loop_variables, workflow_sql_templates, workflow_step_ids,
};
use datazen::workflow::executor::WorkflowExecutor;
use datazen::workflow::model::{WorkflowDefinition, WorkflowExecutionResult};
use serde_json::{Map, Value};
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

const MAX_BRIDGE_TOOLS: usize = 64;
const WORKFLOW_TEMPLATE_INVALID_REASON: &str = "workflowTemplateInvalid";

#[derive(Debug, Clone)]
struct PluginToolSpec {
    plugin_id: String,
    tool: datazen::workspace_apps::WappAiTool,
}

#[derive(Debug, Clone)]
enum BridgeSpec {
    Plugin(PluginToolSpec),
    Workflow(WorkflowDefinition),
}

impl BridgeSpec {
    fn connection_id(&self) -> Option<String> {
        match self {
            Self::Plugin(spec) => Some(spec.tool.connection.clone()),
            Self::Workflow(workflow) => workflow.connection.clone(),
        }
    }

    fn database(&self) -> Option<String> {
        match self {
            Self::Plugin(spec) => spec.tool.database.clone(),
            Self::Workflow(workflow) => workflow.database.clone(),
        }
    }

    fn input_schema(&self) -> Value {
        match self {
            Self::Plugin(spec) => spec.tool.input_schema.clone(),
            Self::Workflow(workflow) => {
                datazen::ai::application::tool_bridge::workflow_input_schema(&workflow.variables)
            }
        }
    }
}

/// Tool definitions for the current opt-in settings. Empty by default.
pub(crate) async fn bridge_tool_definitions(state: &AppState) -> Vec<ToolDefinition> {
    let settings = state.store.get_settings().await;
    let mut definitions = Vec::new();
    let mut seen = HashSet::new();

    for plugin_id in settings.ai_plugin_tool_ids.iter() {
        let Some(loaded) = state.wapps.get(plugin_id) else {
            continue;
        };
        if !loaded.enabled {
            continue;
        }
        let Some(ai) = loaded.manifest.ai.as_ref() else {
            continue;
        };
        for tool in &ai.tools {
            if definitions.len() >= MAX_BRIDGE_TOOLS {
                return definitions;
            }
            let name =
                datazen::ai::application::tool_bridge::plugin_tool_name(plugin_id, &tool.name);
            if !seen.insert(name) {
                continue;
            }
            definitions.push(plugin_tool_definition(
                plugin_id,
                &tool.name,
                &tool.description,
                &tool.input_schema,
            ));
        }
    }

    for workflow_id in settings.ai_workflow_tool_ids.iter() {
        if definitions.len() >= MAX_BRIDGE_TOOLS {
            break;
        }
        let Ok(scope) = state.organization_scope() else {
            break;
        };
        let Some(workflow) = state.workflow_registry.get(&scope, workflow_id).await else {
            continue;
        };
        let name = datazen::ai::application::tool_bridge::workflow_tool_name(&workflow.id);
        if !seen.insert(name) {
            continue;
        }
        definitions.push(workflow_tool_definition(
            &workflow.id,
            &workflow.name,
            &workflow.description,
            &workflow.variables,
        ));
    }

    definitions
}

async fn resolve_bridge_spec(
    state: &AppState,
    kind: &BridgeToolKind,
) -> Result<BridgeSpec, String> {
    let settings = state.store.get_settings().await;
    match kind {
        BridgeToolKind::Plugin {
            plugin_id,
            tool_name,
        } => {
            if !settings.ai_plugin_tool_ids.iter().any(|id| id == plugin_id) {
                return Err("plugin tool is not enabled".into());
            }
            let loaded = state
                .wapps
                .get(plugin_id)
                .ok_or_else(|| "plugin is unavailable".to_string())?;
            if !loaded.enabled {
                return Err("plugin is disabled".into());
            }
            if !loaded
                .manifest
                .permissions
                .contains(&datazen::workspace_apps::Permission::CommandInvoke)
            {
                return Err("plugin is missing command:invoke permission".into());
            }
            let tool = loaded
                .manifest
                .ai
                .as_ref()
                .and_then(|ai| ai.tools.iter().find(|tool| &tool.name == tool_name))
                .cloned()
                .ok_or_else(|| "plugin tool is not declared".to_string())?;
            Ok(BridgeSpec::Plugin(PluginToolSpec {
                plugin_id: plugin_id.clone(),
                tool,
            }))
        }
        BridgeToolKind::Workflow { workflow_id } => {
            if !settings
                .ai_workflow_tool_ids
                .iter()
                .any(|id| id == workflow_id)
            {
                return Err("workflow tool is not enabled".into());
            }
            let scope = state
                .organization_scope()
                .map_err(|error| error.to_string())?;
            let workflow = state
                .workflow_registry
                .get(&scope, workflow_id)
                .await
                .ok_or_else(|| "workflow is unavailable".to_string())?;
            Ok(BridgeSpec::Workflow(workflow))
        }
    }
}

fn spec_is_read_only(spec: &BridgeSpec) -> bool {
    match spec {
        BridgeSpec::Plugin(spec) => spec.tool.read_only,
        BridgeSpec::Workflow(workflow) => workflow_is_read_only(workflow),
    }
}

async fn plugin_sql_dialect(state: &AppState, connection_id: &str) -> SqlTemplateDialect {
    state.connection_sql_dialect(connection_id).await
}

/// Values available to Workflow SQL templates before execution.
///
/// `steps.*` and loop values do not exist yet; the resolver maps them to SQL
/// NULL for validation/preview. They are never rendered from an untyped raw
/// replacement.
fn workflow_template_values(
    workflow: &WorkflowDefinition,
    arguments: &Value,
    redact: bool,
) -> Map<String, Value> {
    let mut values = arguments.as_object().cloned().unwrap_or_default();
    for variable in &workflow.variables {
        if values.contains_key(&variable.name) {
            continue;
        }
        if let Some(default) = &variable.default {
            values.insert(variable.name.clone(), default.clone());
        }
    }
    let now = chrono::Local::now();
    for (name, value) in [
        ("current_month", now.format("%Y-%m").to_string()),
        ("current_date", now.format("%Y-%m-%d").to_string()),
        ("current_year", now.format("%Y").to_string()),
        ("current_time", now.format("%H:%M:%S").to_string()),
    ] {
        values
            .entry(name.to_string())
            .or_insert_with(|| Value::String(value));
    }
    if redact {
        redacted_approval_arguments(&values)
            .as_object()
            .cloned()
            .unwrap_or_default()
    } else {
        values
    }
}

fn workflow_template_value(
    values: &Map<String, Value>,
    loop_vars: &HashSet<String>,
    step_ids: &HashSet<String>,
    expression: &str,
) -> Result<Value, String> {
    if let Some(value) = values.get(expression) {
        return Ok(value.clone());
    }
    if let Some(rest) = expression.strip_prefix("steps.") {
        let Some((step_id, path)) = rest.split_once('.') else {
            return Err("unresolvable workflow step path".into());
        };
        if step_id.is_empty() || !step_ids.contains(step_id) {
            return Err(format!("unknown workflow step '{step_id}'"));
        }
        validate_workflow_step_path(path)?;
        return Ok(Value::Null);
    }
    if let Some(separator) = expression.find(|ch| ch == '.' || ch == '[') {
        let var_name = &expression[..separator];
        if loop_vars.contains(var_name) {
            let rest = &expression[separator..];
            let path = rest.strip_prefix('.').unwrap_or(rest);
            if path.is_empty() || path.starts_with('.') {
                return Err("unresolvable workflow template path".into());
            }
            validate_workflow_step_path(path)?;
            return Ok(Value::Null);
        }
    }
    if loop_vars.contains(expression) {
        return Ok(Value::Null);
    }
    Err(format!("unknown workflow template value '{expression}'"))
}

/// Validate the static shape of a `steps.<id>.<path>` or loop path.
///
/// Runtime values do not exist during authorization, but the path grammar is
/// static. Missing step ids, empty/double segments, unterminated brackets,
/// non-numeric indices and characters that the resolver cannot address are
/// rejected before approval or execution.
fn validate_workflow_step_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("unresolvable workflow step path".into());
    }
    for part in path.split('.') {
        if part.is_empty() {
            return Err("unresolvable workflow step path".into());
        }
        if let Some(bracket) = part.find('[') {
            if !part.ends_with(']') {
                return Err("unresolvable workflow step path".into());
            }
            let field = &part[..bracket];
            let index = &part[bracket + 1..part.len() - 1];
            if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("unresolvable workflow step path".into());
            }
            if index.parse::<usize>().is_err() {
                return Err("unresolvable workflow step path".into());
            }
            if !field.is_empty() {
                validate_workflow_path_segment(field)?;
            }
        } else {
            validate_workflow_path_segment(part)?;
        }
    }
    Ok(())
}

fn validate_workflow_path_segment(segment: &str) -> Result<(), String> {
    if segment == "*" {
        return Ok(());
    }
    if !segment.is_empty()
        && segment
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
    {
        return Ok(());
    }
    Err("unresolvable workflow step path".into())
}

async fn validate_workflow_sql_templates(
    state: &AppState,
    workflow: &WorkflowDefinition,
    values: &Map<String, Value>,
    loop_vars: &HashSet<String>,
    step_ids: &HashSet<String>,
) -> Result<(), String> {
    let templates = workflow_sql_templates(workflow).map_err(|error| error.to_string())?;
    for template in templates {
        let dialect = match template.connection {
            Some(connection) => state.connection_sql_dialect(connection).await,
            None => SqlTemplateDialect::default(),
        };
        render_sql_template_with_resolver(template.sql, dialect, |expression| {
            workflow_template_value(values, loop_vars, step_ids, expression)
        })
        .map_err(|error| format!("step '{}': {error}", template.step_id))?;
    }
    Ok(())
}

async fn render_workflow_preview_sql(
    state: &AppState,
    workflow: &WorkflowDefinition,
    values: &Map<String, Value>,
    loop_vars: &HashSet<String>,
    step_ids: &HashSet<String>,
) -> Option<String> {
    let templates = workflow_sql_templates(workflow).ok()?;
    if templates.is_empty() {
        return None;
    }
    let mut rendered = Vec::with_capacity(templates.len());
    for template in templates {
        let dialect = match template.connection {
            Some(connection) => state.connection_sql_dialect(connection).await,
            None => SqlTemplateDialect::default(),
        };
        let sql = render_sql_template_with_resolver(template.sql, dialect, |expression| {
            workflow_template_value(values, loop_vars, step_ids, expression)
        })
        .ok()?;
        rendered.push(sql);
    }
    Some(rendered.join("\n\n"))
}

/// Authorize one bridge call. Read-only calls return Allow without prompting
/// once schema/template checks pass; a non-read-only call pauses on the T19
/// broker for at most five minutes.
pub(crate) async fn authorize_bridge_tool(
    state: &AppState,
    request_id: &str,
    tool: &str,
    arguments: &str,
    cancel_token: Option<&CancellationToken>,
) -> BridgeToolDecision {
    let broker = crate::mcp::approval_broker();
    authorize_bridge_tool_with_port(
        state,
        broker.as_ref(),
        request_id,
        tool,
        arguments,
        cancel_token,
    )
    .await
}

pub(crate) async fn authorize_bridge_tool_with_port(
    state: &AppState,
    port: &dyn McpApprovalPort,
    request_id: &str,
    tool: &str,
    arguments: &str,
    cancel_token: Option<&CancellationToken>,
) -> BridgeToolDecision {
    let Some(kind) = parse_bridge_tool(tool) else {
        return BridgeToolDecision::Deny {
            reason: "unknown tool".into(),
        };
    };
    let spec = match resolve_bridge_spec(state, &kind).await {
        Ok(spec) => spec,
        Err(reason) => {
            audit_bridge_decision(
                state,
                tool,
                None,
                None,
                "deny",
                None,
                Some(request_id.to_string()),
                Some(reason.clone()),
            )
            .await;
            return BridgeToolDecision::Deny { reason };
        }
    };
    let parsed_arguments = match parse_and_validate_arguments(arguments, &spec.input_schema()) {
        Ok(value) => value,
        Err(reason) => {
            let reason = format!("invalid arguments: {reason}");
            audit_bridge_decision(
                state,
                tool,
                spec.connection_id(),
                spec.database(),
                "deny",
                None,
                Some(request_id.to_string()),
                Some("invalid arguments".into()),
            )
            .await;
            return BridgeToolDecision::Deny { reason };
        }
    };

    let dialect = match &spec {
        BridgeSpec::Plugin(plugin) => plugin_sql_dialect(state, &plugin.tool.connection).await,
        BridgeSpec::Workflow(_) => SqlTemplateDialect::default(),
    };
    // A template that cannot be rendered safely is rejected before the user
    // sees an approval prompt. This also keeps read-only calls from reaching
    // execution with an ambiguous placeholder context.
    if let BridgeSpec::Plugin(plugin) = &spec {
        if let Err(error) =
            render_input_template_with_dialect(&plugin.tool.input, &parsed_arguments, dialect)
        {
            let reason = format!("invalid plugin SQL template: {error}");
            audit_bridge_decision(
                state,
                tool,
                Some(plugin.tool.connection.clone()),
                plugin.tool.database.clone(),
                "deny",
                None,
                Some(request_id.to_string()),
                Some("invalid template".into()),
            )
            .await;
            return BridgeToolDecision::Deny { reason };
        }
    }
    let workflow_preview_values = if let BridgeSpec::Workflow(workflow) = &spec {
        let values = workflow_template_values(workflow, &parsed_arguments, false);
        let loop_vars: HashSet<String> = workflow_loop_variables(workflow)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let step_ids: HashSet<String> = workflow_step_ids(workflow)
            .into_iter()
            .map(str::to_owned)
            .collect();
        if let Err(error) =
            validate_workflow_sql_templates(state, workflow, &values, &loop_vars, &step_ids).await
        {
            let reason = format!("invalid workflow SQL template: {error}");
            audit_bridge_decision(
                state,
                tool,
                workflow.connection.clone(),
                workflow.database.clone(),
                "deny",
                None,
                Some(request_id.to_string()),
                Some(WORKFLOW_TEMPLATE_INVALID_REASON.into()),
            )
            .await;
            return BridgeToolDecision::Deny { reason };
        }
        Some((
            workflow_template_values(workflow, &parsed_arguments, true),
            loop_vars,
            step_ids,
        ))
    } else {
        None
    };
    if spec_is_read_only(&spec) {
        return BridgeToolDecision::Allow;
    }

    let preview_args = parsed_arguments.as_object().cloned();
    // The approval SQL is a rendering of the template, so it must be rendered
    // from the same redacted arguments the approval payload exposes. Rendering
    // from the raw map would put a password back into `sql` even though
    // `arguments` is already `[REDACTED]`.
    let sql = match (&spec, workflow_preview_values.as_ref()) {
        (BridgeSpec::Plugin(_), _) => preview_args
            .as_ref()
            .map(redacted_approval_arguments)
            .and_then(|args| {
                render_input_template_with_dialect(&spec_input_template(&spec), &args, dialect).ok()
            })
            .and_then(|input| input.get("sql").and_then(Value::as_str).map(str::to_owned)),
        (BridgeSpec::Workflow(workflow), Some((values, loop_vars, step_ids))) => {
            render_workflow_preview_sql(state, workflow, values, loop_vars, step_ids).await
        }
        (BridgeSpec::Workflow(_), None) => None,
    };
    let created_at = chrono::Utc::now();
    let expires_at =
        created_at + chrono::Duration::from_std(BRIDGE_APPROVAL_TIMEOUT).unwrap_or_default();
    let approval_id = format!("ai:{request_id}:{}", uuid::Uuid::new_v4());
    let connection_id = spec.connection_id();
    let database = spec.database();
    let request = approval_preview(
        &approval_id,
        tool,
        connection_id.as_deref(),
        database.as_deref(),
        None,
        sql.as_deref(),
        preview_args.as_ref(),
        &created_at.to_rfc3339(),
        &expires_at.to_rfc3339(),
        BRIDGE_APPROVAL_TIMEOUT.as_millis() as u64,
    );

    // The T19 broker already enforces the five-minute window; wrapping the
    // port here keeps that guarantee for every host implementation and lets a
    // Stop win over a slow presenter without waiting for its timeout.
    let decision = if let Some(token) = cancel_token {
        tokio::select! {
            decision = tokio::time::timeout(BRIDGE_APPROVAL_TIMEOUT, port.decide(&request)) => {
                decision.unwrap_or(McpApprovalDecision::Timeout)
            }
            _ = token.cancelled() => McpApprovalDecision::Cancel,
        }
    } else {
        tokio::time::timeout(BRIDGE_APPROVAL_TIMEOUT, port.decide(&request))
            .await
            .unwrap_or(McpApprovalDecision::Timeout)
    };

    let mapped = match decision {
        McpApprovalDecision::AllowOnce => BridgeToolDecision::Allow,
        McpApprovalDecision::Deny => BridgeToolDecision::Deny {
            reason: "denied by the user".into(),
        },
        McpApprovalDecision::Timeout => BridgeToolDecision::Timeout,
        McpApprovalDecision::Cancel => BridgeToolDecision::Cancel,
    };
    if !mapped.is_allowed() {
        audit_bridge_decision(
            state,
            tool,
            spec.connection_id(),
            spec.database(),
            mapped.audit_decision(),
            None,
            Some(request_id.to_string()),
            Some(match mapped {
                BridgeToolDecision::Deny { .. } => "denied by the user".into(),
                BridgeToolDecision::Timeout => "approval timed out (5 minutes)".into(),
                BridgeToolDecision::Cancel => "cancelled by the user".into(),
                BridgeToolDecision::Allow => String::new(),
            }),
        )
        .await;
    }
    mapped
}

fn spec_input_template(spec: &BridgeSpec) -> Value {
    match spec {
        BridgeSpec::Plugin(spec) => spec.tool.input.clone(),
        BridgeSpec::Workflow(_) => Value::Object(Default::default()),
    }
}

pub(crate) async fn execute_plugin_tool(
    state: &AppState,
    request_id: &str,
    ctx: &AiToolContext,
    call: &ToolCall,
) -> String {
    if ctx
        .cancel_token
        .as_ref()
        .is_some_and(|token| token.is_cancelled())
    {
        audit_bridge_decision(
            state,
            &call.name,
            None,
            None,
            "cancel",
            None,
            Some(request_id.to_string()),
            Some("cancelled by the user".into()),
        )
        .await;
        return format!("Error: plugin tool `{}` was cancelled", call.name);
    }
    let Some(BridgeToolKind::Plugin {
        plugin_id,
        tool_name,
    }) = parse_bridge_tool(&call.name)
    else {
        return format!("Error: invalid plugin tool name `{}`", call.name);
    };
    let kind = BridgeToolKind::Plugin {
        plugin_id,
        tool_name,
    };
    let spec = match resolve_bridge_spec(state, &kind).await {
        Ok(BridgeSpec::Plugin(spec)) => spec,
        Ok(_) => return "Error: plugin tool resolved to the wrong kind".into(),
        Err(error) => {
            audit_bridge_decision(
                state,
                &call.name,
                None,
                None,
                "deny",
                None,
                Some(request_id.to_string()),
                Some("tool is not enabled".into()),
            )
            .await;
            return format!("Error: plugin tool was not executed: {error}");
        }
    };
    let arguments = match parse_and_validate_arguments(&call.arguments, &spec.tool.input_schema) {
        Ok(value) => value,
        Err(error) => {
            audit_bridge_decision(
                state,
                &call.name,
                Some(spec.tool.connection.clone()),
                spec.tool.database.clone(),
                "deny",
                None,
                Some(request_id.to_string()),
                Some("invalid arguments".into()),
            )
            .await;
            return format!("Error: plugin tool arguments rejected: {error}");
        }
    };
    let sql_dialect = plugin_sql_dialect(state, &spec.tool.connection).await;
    let mut input =
        match render_input_template_with_dialect(&spec.tool.input, &arguments, sql_dialect) {
            Ok(input) => input,
            Err(error) => {
                audit_bridge_decision(
                    state,
                    &call.name,
                    Some(spec.tool.connection.clone()),
                    spec.tool.database.clone(),
                    "deny",
                    None,
                    Some(request_id.to_string()),
                    Some("invalid arguments".into()),
                )
                .await;
                return format!("Error: plugin tool template rejected: {error}");
            }
        };
    if let Some(database) = spec.tool.database.as_deref() {
        if let Some(object) = input.as_object_mut() {
            // The manifest owns the target database; a model-supplied extra
            // field must never retarget the call.
            object.insert("database".into(), Value::String(database.to_string()));
        }
    }

    let connection = spec.tool.connection.clone();
    let command = spec.tool.command.clone();
    let execution = async {
        if spec.tool.read_only {
            datazen::workflow::runtime_reads::read_target_command(
                state,
                &connection,
                &command,
                input,
            )
            .await
            .map_err(|error| format!("plugin tool failed: {error}"))
        } else {
            datazen::ai::application::ports::DatabaseToolHost::execute_target(
                state,
                &connection,
                &command,
                input,
            )
            .await
            .map(|result| result.data)
            .map_err(|error| format!("plugin tool failed: {error}"))
        }
    };
    let result = if let Some(token) = ctx.cancel_token.as_ref() {
        tokio::select! {
            result = execution => Some(result),
            _ = token.cancelled() => None,
        }
    } else {
        Some(execution.await)
    };
    let Some(result) = result else {
        audit_bridge_decision(
            state,
            &call.name,
            Some(spec.tool.connection.clone()),
            spec.tool.database.clone(),
            "cancel",
            None,
            Some(request_id.to_string()),
            Some("cancelled by the user".into()),
        )
        .await;
        return format!("Error: plugin tool `{}` was cancelled", call.name);
    };

    match result {
        Ok(value) => {
            let rows = count_result_rows(&value);
            let text = serde_json::to_string(&value)
                .unwrap_or_else(|_| "{\"error\":\"result could not be serialized\"}".into());
            audit_bridge_decision(
                state,
                &call.name,
                Some(spec.tool.connection.clone()),
                spec.tool.database.clone(),
                "allow",
                rows,
                Some(request_id.to_string()),
                None,
            )
            .await;
            truncate_bridge_result(&text)
        }
        Err(error) => {
            audit_bridge_decision(
                state,
                &call.name,
                Some(spec.tool.connection.clone()),
                spec.tool.database.clone(),
                "allow",
                None,
                Some(request_id.to_string()),
                None,
            )
            .await;
            format!("Error: {error}")
        }
    }
}

pub(crate) async fn execute_workflow_tool(
    state: &AppState,
    request_id: &str,
    ctx: &AiToolContext,
    call: &ToolCall,
) -> String {
    if ctx
        .cancel_token
        .as_ref()
        .is_some_and(|token| token.is_cancelled())
    {
        audit_bridge_decision(
            state,
            &call.name,
            None,
            None,
            "cancel",
            None,
            Some(request_id.to_string()),
            Some("cancelled by the user".into()),
        )
        .await;
        return format!("Error: workflow `{}` was cancelled", call.name);
    }
    let Some(BridgeToolKind::Workflow { workflow_id }) = parse_bridge_tool(&call.name) else {
        return format!("Error: invalid workflow tool name `{}`", call.name);
    };
    let workflow = match resolve_bridge_spec(
        state,
        &BridgeToolKind::Workflow {
            workflow_id: workflow_id.clone(),
        },
    )
    .await
    {
        Ok(BridgeSpec::Workflow(workflow)) => workflow,
        Ok(_) => return "Error: workflow tool resolved to the wrong kind".into(),
        Err(error) => {
            audit_bridge_decision(
                state,
                &call.name,
                None,
                None,
                "deny",
                None,
                Some(request_id.to_string()),
                Some("tool is not enabled".into()),
            )
            .await;
            return format!("Error: workflow was not executed: {error}");
        }
    };
    let variables = match parse_and_validate_arguments(
        &call.arguments,
        &datazen::ai::application::tool_bridge::workflow_input_schema(&workflow.variables),
    ) {
        Ok(value) => value,
        Err(error) => {
            audit_bridge_decision(
                state,
                &call.name,
                workflow.connection.clone(),
                workflow.database.clone(),
                "deny",
                None,
                Some(request_id.to_string()),
                Some("invalid arguments".into()),
            )
            .await;
            return format!("Error: workflow arguments rejected: {error}");
        }
    };

    // Revalidate against the definition just re-read above. Approval and
    // execution are separate registry reads; a workflow edited in between must
    // not reach the executor with a template that only passed prevalidation
    // for the earlier revision.
    let values = workflow_template_values(&workflow, &variables, false);
    let loop_vars: HashSet<String> = workflow_loop_variables(&workflow)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let step_ids: HashSet<String> = workflow_step_ids(&workflow)
        .into_iter()
        .map(str::to_owned)
        .collect();
    if let Err(error) =
        validate_workflow_sql_templates(state, &workflow, &values, &loop_vars, &step_ids).await
    {
        tracing::warn!(
            workflow = %workflow.id,
            error = %error,
            "workflow SQL template rejected at execution"
        );
        audit_bridge_decision(
            state,
            &call.name,
            workflow.connection.clone(),
            workflow.database.clone(),
            "deny",
            None,
            Some(request_id.to_string()),
            Some(WORKFLOW_TEMPLATE_INVALID_REASON.into()),
        )
        .await;
        return format!("Error: workflow template rejected: {WORKFLOW_TEMPLATE_INVALID_REASON}");
    }

    let execution = WorkflowExecutor::execute(&workflow, state, None, &variables);
    let result = if let Some(token) = ctx.cancel_token.as_ref() {
        tokio::select! {
            result = execution => Some(result),
            _ = token.cancelled() => None,
        }
    } else {
        Some(execution.await)
    };
    let Some(result) = result else {
        audit_bridge_decision(
            state,
            &call.name,
            workflow.connection.clone(),
            workflow.database.clone(),
            "cancel",
            None,
            Some(request_id.to_string()),
            Some("cancelled by the user".into()),
        )
        .await;
        return format!("Error: workflow `{workflow_id}` was cancelled");
    };

    match result {
        Ok(result) => {
            let rows = workflow_result_rows(&result);
            let mut value = serde_json::to_value(&result).unwrap_or(Value::Null);
            if let Some(object) = value.as_object_mut() {
                object.insert("workflowId".into(), Value::String(workflow.id.clone()));
            }
            audit_bridge_decision(
                state,
                &call.name,
                workflow.connection.clone(),
                workflow.database.clone(),
                "allow",
                rows,
                Some(request_id.to_string()),
                None,
            )
            .await;
            truncate_bridge_result(&value.to_string())
        }
        Err(error) => {
            audit_bridge_decision(
                state,
                &call.name,
                workflow.connection.clone(),
                workflow.database.clone(),
                "allow",
                None,
                Some(request_id.to_string()),
                None,
            )
            .await;
            format!("Error: workflow execution failed: {error}")
        }
    }
}

fn workflow_result_rows(result: &WorkflowExecutionResult) -> Option<i64> {
    result
        .steps
        .iter()
        .rev()
        .find_map(|step| step.result.as_ref().and_then(count_result_rows))
}

#[allow(clippy::too_many_arguments)]
async fn audit_bridge_decision(
    state: &AppState,
    tool: &str,
    connection_id: Option<String>,
    database: Option<String>,
    decision: &str,
    row_count: Option<i64>,
    request_id: Option<String>,
    reason: Option<String>,
) {
    let (organization_id, principal_id) = audit_identity(state);
    let mut record = crate::store::new_audit_record(
        organization_id,
        principal_id,
        connection_id,
        database,
        None,
        tool,
        decision,
        row_count,
        request_id,
    );
    record.reason = reason;
    if state.store.mcp_audit_log().append(&record).await.is_err() {
        tracing::warn!(tool, "AI bridge audit append failed");
        emit_audit_error(state);
    }
}

fn audit_identity(state: &AppState) -> (String, String) {
    if let Ok(adapter) = state.platform_require() {
        return (
            adapter.identity().organization_id().to_string(),
            adapter.identity().principal_id().to_string(),
        );
    }
    (
        state.organization_scope.organization_id().to_string(),
        "unknown".to_string(),
    )
}

fn emit_audit_error(state: &AppState) {
    if let Some(handle) = state.app_handle.get().cloned() {
        use tauri::Emitter;
        let _ = handle.emit(
            crate::mcp::MCP_AUDIT_ERROR_EVENT,
            "AI tool audit write failed",
        );
    }
}
