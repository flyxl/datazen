//! Workflow execution engine.
//!
//! Data-operation steps are normalized to Driver Commands and dispatched
//! through `command_runtime`, while AI and control-flow steps remain generic.

use crate::commands::AppState;
use crate::mcp::permission::{self, McpPermissionMode};
use datazen_ai_api::{ChatMessage, CompletionRequest, MessageRole};
use datazen_driver_api::MultiQueryResult;
use std::time::Instant;
use uuid::Uuid;

use super::command::WorkflowCommandStep;
use super::command_runtime;
use super::conditions::evaluate_condition;
use super::context::WorkflowContext;
use crate::workflow::model::{
    ErrorStrategy, StepExecutionResult, StepStatus, WorkflowDefinition, WorkflowExecutionResult,
    WorkflowStep,
};

pub const WORKFLOW_QUERY_ROW_LIMIT: u32 = 1000;

#[derive(Debug, Clone)]
pub struct WorkflowExecuteOptions {
    pub permission_mode: Option<McpPermissionMode>,
    pub query_row_limit: Option<u32>,
}

impl Default for WorkflowExecuteOptions {
    fn default() -> Self {
        Self {
            permission_mode: None,
            query_row_limit: Some(WORKFLOW_QUERY_ROW_LIMIT),
        }
    }
}

pub fn enforce_workflow_query_guards(
    sql: &str,
    permission_mode: Option<McpPermissionMode>,
) -> Result<(), String> {
    if let Some(mode) = permission_mode {
        permission::check_sql_allowed(sql, mode)?;
    }
    Ok(())
}

pub struct WorkflowExecutor;

impl WorkflowExecutor {
    pub async fn execute(
        workflow: &WorkflowDefinition,
        app_state: &AppState,
        connection_id: Option<&str>,
        variables: &serde_json::Value,
    ) -> Result<WorkflowExecutionResult, String> {
        Self::execute_with_options(
            workflow,
            app_state,
            connection_id,
            variables,
            WorkflowExecuteOptions::default(),
        )
        .await
    }

    pub async fn execute_with_options(
        workflow: &WorkflowDefinition,
        app_state: &AppState,
        connection_id: Option<&str>,
        variables: &serde_json::Value,
        options: WorkflowExecuteOptions,
    ) -> Result<WorkflowExecutionResult, String> {
        let start = Instant::now();
        let global_timeout = workflow.timeout_secs.unwrap_or(300);
        let default_strategy = workflow
            .error_handling
            .as_ref()
            .map(|e| e.to_strategy())
            .unwrap_or(ErrorStrategy::Abort);
        let mut context = WorkflowContext::new(variables);
        context.set_builtin_variables();

        for var in &workflow.variables {
            if !context.variables.contains_key(&var.name) {
                if let Some(default) = &var.default {
                    context.variables.insert(
                        var.name.clone(),
                        match default {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        },
                    );
                }
            }
            if var.required.unwrap_or(false)
                && context
                    .variables
                    .get(&var.name)
                    .map_or(true, |v| v.is_empty())
            {
                return Err(format!("Required variable '{}' is missing", var.name));
            }
        }

        let workflow_connection = workflow.connection.as_deref().or(connection_id);
        let mut step_results = Vec::new();
        let outcome = Self::execute_steps(
            &workflow.steps,
            app_state,
            workflow_connection,
            &mut context,
            &mut step_results,
            &default_strategy,
            global_timeout,
            &start,
            &options,
        )
        .await;
        let total_time_ms = start.elapsed().as_millis() as u64;

        match outcome {
            Ok(()) => {
                let final_output = workflow
                    .output
                    .as_ref()
                    .and_then(|o| o.template.as_deref())
                    .map(|t| context.resolve_template(t).unwrap_or_default())
                    .unwrap_or_else(|| context.get_last_result().unwrap_or_default());
                Ok(WorkflowExecutionResult {
                    success: true,
                    final_output,
                    steps: step_results,
                    total_time_ms,
                    error: None,
                })
            }
            Err(e) => Ok(WorkflowExecutionResult {
                success: false,
                final_output: context.get_last_result().unwrap_or_default(),
                steps: step_results,
                total_time_ms,
                error: Some(e),
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_steps<'a>(
        steps: &'a [WorkflowStep],
        app_state: &'a AppState,
        connection_id: Option<&'a str>,
        context: &'a mut WorkflowContext,
        step_results: &'a mut Vec<StepExecutionResult>,
        default_strategy: &'a ErrorStrategy,
        global_timeout_secs: u64,
        global_start: &'a Instant,
        options: &'a WorkflowExecuteOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            for step in steps {
                if global_start.elapsed().as_secs() >= global_timeout_secs {
                    return Err(format!("Global timeout ({global_timeout_secs}s) exceeded"));
                }

                match step {
                    WorkflowStep::Condition {
                        id,
                        expr,
                        then_steps,
                        else_steps,
                    } => {
                        let resolved = context.resolve_template(expr)?;
                        let matched = evaluate_condition(&resolved, context);
                        step_results.push(StepExecutionResult {
                            step_id: id.clone(),
                            step_type: "condition".into(),
                            status: StepStatus::Success,
                            result: Some(serde_json::json!({"condition": matched})),
                            execution_time_ms: 0,
                            error: None,
                            connection_name: None,
                            sql_executed: None,
                        });
                        let branch = if matched {
                            then_steps.as_slice()
                        } else {
                            else_steps.as_deref().unwrap_or(&[])
                        };
                        Self::execute_steps(
                            branch,
                            app_state,
                            connection_id,
                            context,
                            step_results,
                            default_strategy,
                            global_timeout_secs,
                            global_start,
                            options,
                        )
                        .await?;
                    }
                    WorkflowStep::ForEach {
                        id,
                        items,
                        as_var,
                        steps: loop_steps,
                        max_iterations,
                    } => {
                        let max = max_iterations.unwrap_or(100);
                        let resolved = context.resolve_template(items)?;
                        let value = serde_json::from_str(&resolved).unwrap_or_else(|_| {
                            context
                                .resolve_deep_path(&resolved)
                                .unwrap_or(serde_json::Value::Null)
                        });
                        let arr = match value.as_array() {
                            Some(a) => a.clone(),
                            None => {
                                step_results.push(StepExecutionResult {
                                    step_id: id.clone(),
                                    step_type: "foreach".into(),
                                    status: StepStatus::Skipped,
                                    result: None,
                                    execution_time_ms: 0,
                                    error: Some(
                                        "Items expression did not resolve to an array".into(),
                                    ),
                                    connection_name: None,
                                    sql_executed: None,
                                });
                                continue;
                            }
                        };
                        let mut iterations = Vec::new();
                        for (i, item) in arr.iter().take(max).enumerate() {
                            context.set_loop_var(as_var, item.clone());
                            let mut iteration_steps = Vec::new();
                            Self::execute_steps(
                                loop_steps,
                                app_state,
                                connection_id,
                                context,
                                &mut iteration_steps,
                                default_strategy,
                                global_timeout_secs,
                                global_start,
                                options,
                            )
                            .await?;
                            iterations
                                .push(serde_json::json!({"index": i, "steps": iteration_steps}));
                            step_results.extend(iteration_steps);
                        }
                        context.clear_loop_var(as_var);
                        let result = serde_json::json!({"iterations_completed": iterations.len(), "iterations": iterations});
                        context.set_step_result(id, result.clone());
                        step_results.push(StepExecutionResult {
                            step_id: id.clone(),
                            step_type: "foreach".into(),
                            status: StepStatus::Success,
                            result: Some(result),
                            execution_time_ms: 0,
                            error: None,
                            connection_name: None,
                            sql_executed: None,
                        });
                    }
                    _ => {
                        let timeout = step.timeout_secs().unwrap_or(30);
                        let started = Instant::now();
                        let result = tokio::time::timeout(
                            std::time::Duration::from_secs(timeout),
                            Self::execute_single_step(
                                step,
                                app_state,
                                connection_id,
                                context,
                                options,
                            ),
                        )
                        .await;
                        let elapsed = started.elapsed().as_millis() as u64;
                        match result {
                            Ok(Ok(mut sr)) => {
                                sr.execution_time_ms = elapsed;
                                step_results.push(sr);
                            }
                            Ok(Err(err)) => {
                                Self::handle_step_error(
                                    step,
                                    err,
                                    elapsed,
                                    app_state,
                                    connection_id,
                                    context,
                                    step_results,
                                    default_strategy,
                                    global_timeout_secs,
                                    global_start,
                                    options,
                                )
                                .await?
                            }
                            Err(_) => {
                                let err = format!(
                                    "Step '{}' timed out after {}s",
                                    step.step_id(),
                                    timeout
                                );
                                Self::handle_step_error(
                                    step,
                                    err,
                                    elapsed,
                                    app_state,
                                    connection_id,
                                    context,
                                    step_results,
                                    default_strategy,
                                    global_timeout_secs,
                                    global_start,
                                    options,
                                )
                                .await?;
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_step_error(
        step: &WorkflowStep,
        err: String,
        elapsed: u64,
        app_state: &AppState,
        connection_id: Option<&str>,
        context: &mut WorkflowContext,
        step_results: &mut Vec<StepExecutionResult>,
        default_strategy: &ErrorStrategy,
        global_timeout_secs: u64,
        global_start: &Instant,
        options: &WorkflowExecuteOptions,
    ) -> Result<(), String> {
        match step
            .on_error_strategy()
            .unwrap_or_else(|| default_strategy.clone())
        {
            ErrorStrategy::Abort => {
                step_results.push(StepExecutionResult {
                    step_id: step.step_id().into(),
                    step_type: step.step_type_str().into(),
                    status: StepStatus::Failed,
                    result: None,
                    execution_time_ms: elapsed,
                    error: Some(err.clone()),
                    connection_name: None,
                    sql_executed: None,
                });
                Err(err)
            }
            ErrorStrategy::Skip => {
                context.set_step_result(step.step_id(), serde_json::Value::Null);
                step_results.push(StepExecutionResult {
                    step_id: step.step_id().into(),
                    step_type: step.step_type_str().into(),
                    status: StepStatus::Skipped,
                    result: None,
                    execution_time_ms: elapsed,
                    error: Some(err),
                    connection_name: None,
                    sql_executed: None,
                });
                Ok(())
            }
            ErrorStrategy::Fallback { steps } => {
                Self::execute_steps(
                    &steps,
                    app_state,
                    connection_id,
                    context,
                    step_results,
                    default_strategy,
                    global_timeout_secs,
                    global_start,
                    options,
                )
                .await
            }
        }
    }

    async fn execute_single_step(
        step: &WorkflowStep,
        app_state: &AppState,
        workflow_connection: Option<&str>,
        context: &mut WorkflowContext,
        options: &WorkflowExecuteOptions,
    ) -> Result<StepExecutionResult, String> {
        match step {
            WorkflowStep::Query {
                id,
                sql,
                connection,
                database,
                timeout_secs,
                on_error,
            } => {
                let command = WorkflowCommandStep::from_legacy_query(
                    id.clone(),
                    context.resolve_template(sql)?,
                    connection
                        .as_deref()
                        .map(|s| context.resolve_template(s))
                        .transpose()?,
                    database
                        .as_deref()
                        .map(|s| context.resolve_template(s))
                        .transpose()?,
                    *timeout_secs,
                    on_error.clone(),
                );
                Self::execute_command_step(
                    command,
                    app_state,
                    workflow_connection,
                    context,
                    options,
                    Some(sql),
                )
                .await
            }
            WorkflowStep::Command {
                id,
                command,
                connection,
                input,
                timeout_secs,
                on_error,
            } => {
                let resolved_input = resolve_json_templates(input, context)?;
                let resolved_connection = connection
                    .as_deref()
                    .map(|s| context.resolve_template(s))
                    .transpose()?;
                let command_step = WorkflowCommandStep {
                    id: id.clone(),
                    command: command.clone(),
                    connection: resolved_connection,
                    input: resolved_input,
                    timeout_secs: *timeout_secs,
                    on_error: on_error.clone(),
                };
                Self::execute_command_step(
                    command_step,
                    app_state,
                    workflow_connection,
                    context,
                    options,
                    None,
                )
                .await
            }
            WorkflowStep::Ai { id, prompt, .. } => {
                let resolved_prompt = context.resolve_template(prompt)?;
                let ai_config = app_state
                    .store
                    .get_ai_config()
                    .await
                    .ok_or("AI not configured")?;
                let provider = app_state
                    .ai_registry
                    .get(&ai_config.provider_type)
                    .await
                    .ok_or("AI provider not available")?;
                let request = CompletionRequest {
                    request_id: Uuid::new_v4().to_string(),
                    model: ai_config.model.clone(),
                    messages: vec![ChatMessage {
                        role: MessageRole::User,
                        content: resolved_prompt,
                        reasoning: None,
                        tool_calls: None,
                        tool_call_id: None,
                    }],
                    temperature: Some(0.3),
                    stop: None,
                    tools: None,
                    previous_response_id: None,
                };
                let response = provider
                    .complete(&request)
                    .await
                    .map_err(|e| e.to_string())?;
                let structured = serde_json::json!({"result": response.content});
                context.set_step_result(id, structured.clone());
                Ok(StepExecutionResult {
                    step_id: id.clone(),
                    step_type: "ai".into(),
                    status: StepStatus::Success,
                    result: Some(structured),
                    execution_time_ms: 0,
                    error: None,
                    connection_name: None,
                    sql_executed: None,
                })
            }
            WorkflowStep::Condition { .. } | WorkflowStep::ForEach { .. } => {
                unreachable!("control flow is handled by execute_steps")
            }
        }
    }

    async fn execute_command_step(
        step: WorkflowCommandStep,
        app_state: &AppState,
        workflow_connection: Option<&str>,
        context: &mut WorkflowContext,
        options: &WorkflowExecuteOptions,
        legacy_sql_template: Option<&str>,
    ) -> Result<StepExecutionResult, String> {
        if step.command == "query" {
            if let Some(sql) = step.input.get("sql").and_then(|v| v.as_str()) {
                let clean = sql.trim_end_matches(';').trim().to_string();
                enforce_workflow_query_guards(&clean, options.permission_mode)?;
                let limit = options.query_row_limit.unwrap_or(WORKFLOW_QUERY_ROW_LIMIT);
                let mut input = step.input.clone();
                input["sql"] = serde_json::Value::String(clean.clone());
                input["limit"] = serde_json::Value::Number(limit.into());
                let step = WorkflowCommandStep { input, ..step };
                let conn_name =
                    effective_connection_name(app_state, &step, workflow_connection).await?;
                let result = command_runtime::execute_command_with_mode(
                    app_state,
                    &step,
                    workflow_connection,
                    options.permission_mode,
                )
                .await?;
                let structured = normalize_query_result(&result.data)?;
                context.set_step_result(&step.id, structured.clone());
                return Ok(StepExecutionResult {
                    step_id: step.id,
                    step_type: "query".into(),
                    status: StepStatus::Success,
                    result: Some(structured),
                    execution_time_ms: 0,
                    error: None,
                    connection_name: Some(conn_name),
                    sql_executed: legacy_sql_template.map(str::to_owned).or(Some(clean)),
                });
            }
        }

        let conn_name = effective_connection_name(app_state, &step, workflow_connection).await?;
        let result = command_runtime::execute_command_with_mode(
            app_state,
            &step,
            workflow_connection,
            options.permission_mode,
        )
        .await?;
        context.set_step_result(&step.id, result.data.clone());
        Ok(StepExecutionResult {
            step_id: step.id,
            step_type: "command".into(),
            status: StepStatus::Success,
            result: Some(result.data),
            execution_time_ms: 0,
            error: None,
            connection_name: Some(conn_name),
            sql_executed: None,
        })
    }
}

async fn effective_connection_name(
    app_state: &AppState,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
) -> Result<String, String> {
    let id = command_runtime::resolve_connection_id(step, workflow_connection)?;
    let (runtime_id, _, _) = app_state
        .connection_manager
        .resolve_session(id)
        .await
        .map_err(|e| e.to_string())?;
    app_state
        .connection_manager
        .get_connection_config(&runtime_id)
        .await
        .map(|c| c.name)
        .map_err(|e| e.to_string())
}

fn resolve_json_templates(
    value: &serde_json::Value,
    context: &WorkflowContext,
) -> Result<serde_json::Value, String> {
    match value {
        serde_json::Value::String(s) => Ok(serde_json::Value::String(context.resolve_template(s)?)),
        serde_json::Value::Array(items) => Ok(serde_json::Value::Array(
            items
                .iter()
                .map(|v| resolve_json_templates(v, context))
                .collect::<Result<_, _>>()?,
        )),
        serde_json::Value::Object(map) => Ok(serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), resolve_json_templates(v, context)?)))
                .collect::<Result<_, String>>()?,
        )),
        other => Ok(other.clone()),
    }
}

fn normalize_query_result(data: &serde_json::Value) -> Result<serde_json::Value, String> {
    let multi: MultiQueryResult = serde_json::from_value(data.clone())
        .map_err(|e| format!("failed to decode query command result: {e}"))?;
    let result = multi
        .results
        .into_iter()
        .next()
        .ok_or_else(|| "Query command returned no statement result".to_string())?;
    let columns = result.columns.clone();
    let col_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    let rows: Vec<serde_json::Value> = result
        .rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (i, name) in col_names.iter().enumerate() {
                let value = row.get(i).and_then(|v| v.as_ref());
                obj.insert(
                    name.clone(),
                    value
                        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    Ok(serde_json::json!({
        "rows": rows,
        "rows_count": result.rows.len(),
        "columns": columns,
        "execution_time_ms": result.execution_time_ms,
        "truncated": result.truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::WorkflowDefinition;

    #[tokio::test]
    async fn workflow_default_connection_is_inherited_by_legacy_query() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("wf-default").await;
        let workflow = WorkflowDefinition {
            id: "inherit".into(),
            name: "Inherit".into(),
            description: String::new(),
            version: None,
            author: None,
            variables: vec![],
            connection: Some("wf-default".into()),
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.steps[0].step_type, "query");
        assert_eq!(result.steps[0].status, StepStatus::Success);
    }

    #[tokio::test]
    async fn step_connection_override_executes_command() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("wf-default").await;
        test.save_connection("wf-override").await;
        let workflow = WorkflowDefinition {
            id: "override".into(),
            name: "Override".into(),
            description: String::new(),
            version: None,
            author: None,
            variables: vec![],
            connection: Some("wf-default".into()),
            steps: vec![WorkflowStep::Command {
                id: "c1".into(),
                command: "query".into(),
                connection: Some("wf-override".into()),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.steps[0].step_type, "query");
    }
}
