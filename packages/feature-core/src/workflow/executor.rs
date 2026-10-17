//! Workflow execution engine.
//! Data-operation steps are normalized to Driver Commands and dispatched
//! through `command_runtime`, while AI and control-flow steps remain generic.

use super::host::WorkflowHost;
use crate::mcp::permission::{self, McpPermissionMode};
use std::time::Instant;

use super::command::WorkflowCommandStep;
use super::command_runtime;
use super::conditions::evaluate_condition;
use super::context::WorkflowContext;
use super::data_step::{run_merge, run_transform};
use super::results::{merge_template_resolve, normalize_query_result, resolve_json_templates};
use crate::workflow::model::{
    ErrorStrategy, MergeSource, StepExecutionResult, StepStatus, TransformColumn,
    WorkflowDefinition, WorkflowExecutionResult, WorkflowStep,
};
use crate::workflow::WorkflowError;
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
) -> Result<(), WorkflowError> {
    if let Some(mode) = permission_mode {
        permission::check_sql_allowed(sql, mode).map_err(WorkflowError::Validation)?;
    }
    Ok(())
}

/// Decide whether a SQL step must name a database before it can run.
///
/// True only when all three hold: the step (and workflow) resolved nothing,
/// the connection pins no default of its own, and the driver can address more
/// than one database. A single-database driver has nothing to disambiguate, so
/// omitting the value stays legal for it.
pub fn database_is_required(
    resolved_database: Option<&str>,
    connection_database: Option<&str>,
    driver_has_multi_database: bool,
) -> bool {
    let named = |v: Option<&str>| v.map(str::trim).filter(|s| !s.is_empty()).is_some();
    !named(resolved_database) && !named(connection_database) && driver_has_multi_database
}
pub struct WorkflowExecutor;
impl WorkflowExecutor {
    pub async fn execute(
        workflow: &WorkflowDefinition,
        app_state: &dyn WorkflowHost,
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
        app_state: &dyn WorkflowHost,
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
                return Err(WorkflowError::MissingVariable(var.name.clone()).into_ipc_string());
            }
        }

        let prepared_definition;
        let version = super::executable::WorkflowVersion::parse(workflow.version.as_deref())
            .map_err(WorkflowError::into_ipc_string)?;
        let workflow = if version == super::executable::WorkflowVersion::V2 {
            prepared_definition = super::session_block::prepare_v2(
                workflow,
                &context,
                app_state,
                tokio::time::Instant::from_std(start)
                    + std::time::Duration::from_secs(global_timeout),
            )
            .await
            .map_err(WorkflowError::into_ipc_string)?;
            &prepared_definition
        } else {
            workflow
        };
        let executable = workflow
            .clone()
            .into_executable()
            .map_err(WorkflowError::into_ipc_string)?;
        let blocks = super::session_block::plan_blocks(&executable, app_state)
            .map_err(WorkflowError::into_ipc_string)?;
        let workflow_connection = workflow.connection.as_deref().or(
            if executable.version == super::executable::WorkflowVersion::V1 {
                connection_id
            } else {
                None
            },
        );
        let workflow_database = if version == super::executable::WorkflowVersion::V1 {
            workflow.database.as_deref()
        } else {
            None
        };
        let mut step_results = Vec::new();
        let outcome = async {
            Self::execute_steps(
                &workflow.steps,
                app_state,
                workflow_connection,
                workflow_database,
                &mut context,
                &mut step_results,
                &default_strategy,
                global_timeout,
                &start,
                &options,
            )
            .await?;
            super::session_block::execute_blocks(
                blocks,
                app_state,
                &mut context,
                &mut step_results,
                &default_strategy,
                global_timeout,
                &start,
                &options,
            )
            .await?;
            Ok(())
        }
        .await
        .map_err(WorkflowError::into_ipc_string);
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
    pub(super) fn execute_steps<'a>(
        steps: &'a [WorkflowStep],
        app_state: &'a dyn WorkflowHost,
        connection_id: Option<&'a str>,
        workflow_database: Option<&'a str>,
        context: &'a mut WorkflowContext,
        step_results: &'a mut Vec<StepExecutionResult>,
        default_strategy: &'a ErrorStrategy,
        global_timeout_secs: u64,
        global_start: &'a Instant,
        options: &'a WorkflowExecuteOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WorkflowError>> + Send + 'a>>
    {
        Box::pin(async move {
            for step in steps {
                if global_start.elapsed().as_secs() >= global_timeout_secs {
                    return Err(WorkflowError::GlobalTimeout(global_timeout_secs));
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
                            workflow_database,
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
                                workflow_database,
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
                        let execution = Self::execute_single_step(
                            step,
                            app_state,
                            connection_id,
                            workflow_database,
                            context,
                            options,
                            tokio::time::Instant::from_std(*global_start)
                                + std::time::Duration::from_secs(global_timeout_secs),
                        );
                        // SQL owns its deadline so dropping a future cannot release
                        // Skip/Fallback before the database reaches a terminal state.
                        let is_sql = matches!(step, WorkflowStep::Query { .. })
                            || matches!(step, WorkflowStep::Command { command, .. } if matches!(command.as_str(), "query" | "execute"));
                        let result = if is_sql {
                            Ok(execution.await)
                        } else {
                            tokio::time::timeout_at(
                                (tokio::time::Instant::from_std(*global_start)
                                    + std::time::Duration::from_secs(global_timeout_secs))
                                .min(
                                    tokio::time::Instant::now()
                                        + std::time::Duration::from_secs(timeout),
                                ),
                                execution,
                            )
                            .await
                        };
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
                                    workflow_database,
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
                                let err = WorkflowError::Step(format!(
                                    "Step '{}' timed out after {}s",
                                    step.step_id(),
                                    timeout
                                ));
                                Self::handle_step_error(
                                    step,
                                    err,
                                    elapsed,
                                    app_state,
                                    connection_id,
                                    workflow_database,
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
            if global_start.elapsed().as_secs() >= global_timeout_secs {
                return Err(WorkflowError::GlobalTimeout(global_timeout_secs));
            }
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_step_error(
        step: &WorkflowStep,
        err: WorkflowError,
        elapsed: u64,
        app_state: &dyn WorkflowHost,
        connection_id: Option<&str>,
        workflow_database: Option<&str>,
        context: &mut WorkflowContext,
        step_results: &mut Vec<StepExecutionResult>,
        default_strategy: &ErrorStrategy,
        global_timeout_secs: u64,
        global_start: &Instant,
        options: &WorkflowExecuteOptions,
    ) -> Result<(), WorkflowError> {
        let global_expired = global_start.elapsed().as_secs() >= global_timeout_secs;
        let err = if global_expired
            && !matches!(&err, WorkflowError::SqlExecution { code, .. } if code.starts_with("GlobalTimeout:"))
        {
            WorkflowError::Step(format!("GlobalTimeout ({global_timeout_secs}s): {err}"))
        } else {
            err
        };
        let err_text = err.to_string();
        let strategy = if global_expired || matches!(&err, WorkflowError::SqlUnconfirmed { .. }) {
            ErrorStrategy::Abort
        } else {
            step.on_error_strategy()
                .unwrap_or_else(|| default_strategy.clone())
        };
        match strategy {
            ErrorStrategy::Abort => {
                step_results.push(StepExecutionResult {
                    step_id: step.step_id().into(),
                    step_type: step.step_type_str().into(),
                    status: StepStatus::Failed,
                    result: None,
                    execution_time_ms: elapsed,
                    error: Some(err_text.clone()),
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
                    error: Some(err_text),
                    connection_name: None,
                    sql_executed: None,
                });
                Ok(())
            }
            ErrorStrategy::Fallback { steps } => {
                // Retain the failed SQL execution evidence even when fallback succeeds.
                if matches!(
                    &err,
                    WorkflowError::SqlExecution { .. } | WorkflowError::SqlResultRead { .. }
                ) {
                    step_results.push(StepExecutionResult {
                        step_id: step.step_id().into(),
                        step_type: step.step_type_str().into(),
                        status: StepStatus::Failed,
                        result: None,
                        execution_time_ms: elapsed,
                        error: Some(err_text),
                        connection_name: None,
                        sql_executed: None,
                    });
                }
                Self::execute_steps(
                    &steps,
                    app_state,
                    connection_id,
                    workflow_database,
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
        app_state: &dyn WorkflowHost,
        workflow_connection: Option<&str>,
        workflow_database: Option<&str>,
        context: &mut WorkflowContext,
        options: &WorkflowExecuteOptions,
        workflow_deadline: tokio::time::Instant,
    ) -> Result<StepExecutionResult, WorkflowError> {
        match step {
            WorkflowStep::Query {
                id,
                sql,
                connection,
                database,
                timeout_secs,
                on_error,
            } => {
                let step_database = database
                    .as_deref()
                    .map(|s| context.resolve_template(s))
                    .transpose()?;
                let inherited_database = workflow_database
                    .map(|s| context.resolve_template(s))
                    .transpose()?;
                let resolved_database = step_database.or(inherited_database);
                let resolved_step_connection = connection
                    .as_deref()
                    .map(|s| context.resolve_template(s))
                    .transpose()?;
                let command = WorkflowCommandStep::from_legacy_query(
                    id.clone(),
                    context.resolve_template(sql)?,
                    resolved_step_connection,
                    resolved_database,
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
                    workflow_deadline,
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
                let mut resolved_input = resolve_json_templates(input, context)?;
                let resolved_connection = connection
                    .as_deref()
                    .map(|s| context.resolve_template(s))
                    .transpose()?;
                // Command steps carry their target in `input.database`, so the
                // workflow default must be injected explicitly — a query step
                // gets it for free through `resolved_database` above.
                if let Some(workflow_database) = workflow_database {
                    let database = context.resolve_template(workflow_database)?;
                    resolved_input = crate::workflow::command_runtime::inject_inherited_database(
                        resolved_input,
                        &database,
                    );
                }
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
                    workflow_deadline,
                )
                .await
            }
            WorkflowStep::Ai { id, prompt, .. } => {
                let resolved_prompt = context.resolve_template(prompt)?;
                let response = app_state.complete_ai(resolved_prompt).await?;
                let structured = serde_json::json!({"result": response});
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
            WorkflowStep::Merge {
                id,
                sources,
                columns,
                ..
            } => {
                let resolved: Vec<MergeSource> = sources
                    .iter()
                    .map(|s| merge_template_resolve(s, context))
                    .collect::<Result<_, String>>()?;
                let value = run_merge(&resolved, columns, context)?;
                context.set_step_result(id, value.clone());
                Ok(StepExecutionResult {
                    step_id: id.clone(),
                    step_type: "merge".into(),
                    status: StepStatus::Success,
                    result: Some(value),
                    execution_time_ms: 0,
                    error: None,
                    connection_name: None,
                    sql_executed: None,
                })
            }
            WorkflowStep::Transform {
                id,
                from,
                add_columns,
                filter,
                sort_by,
                offset,
                limit,
                ..
            } => {
                let resolved_from = context.resolve_template(from)?;
                let resolved_cols: Vec<TransformColumn> = add_columns
                    .iter()
                    .map(|c| TransformColumn {
                        name: c.name.clone(),
                        expr: context
                            .resolve_template(&c.expr)
                            .unwrap_or_else(|_| c.expr.clone()),
                    })
                    .collect();
                let resolved_filter = filter
                    .as_ref()
                    .map(|f| context.resolve_template(f).unwrap_or_else(|_| f.clone()));
                let value = run_transform(
                    &resolved_from,
                    &resolved_cols,
                    &resolved_filter,
                    sort_by,
                    *offset,
                    *limit,
                    context,
                )?;
                context.set_step_result(id, value.clone());
                Ok(StepExecutionResult {
                    step_id: id.clone(),
                    step_type: "transform".into(),
                    status: StepStatus::Success,
                    result: Some(value),
                    execution_time_ms: 0,
                    error: None,
                    connection_name: None,
                    sql_executed: None,
                })
            }
            WorkflowStep::Migration {
                id,
                operation,
                profile_id,
                profile_revision,
                destructive_policy,
                sql_file_token_variable,
                ..
            } => {
                let resolved_profile_id = context.resolve_template(profile_id)?;
                let resolved_revision = profile_revision
                    .as_deref()
                    .map(|revision| context.resolve_template(revision))
                    .transpose()?;
                let result = app_state
                    .execute_profile_step(
                        id,
                        *operation,
                        &resolved_profile_id,
                        resolved_revision.as_deref(),
                        *destructive_policy,
                        sql_file_token_variable.as_deref(),
                        context,
                    )
                    .await?;
                context
                    .set_step_result(id, result.result.clone().unwrap_or(serde_json::Value::Null));
                Ok(result)
            }
            WorkflowStep::Condition { .. } | WorkflowStep::ForEach { .. } => {
                unreachable!("control flow is handled by execute_steps")
            }
        }
    }

    async fn execute_command_step(
        step: WorkflowCommandStep,
        app_state: &dyn WorkflowHost,
        workflow_connection: Option<&str>,
        context: &mut WorkflowContext,
        options: &WorkflowExecuteOptions,
        legacy_sql_template: Option<&str>,
        workflow_deadline: tokio::time::Instant,
    ) -> Result<StepExecutionResult, WorkflowError> {
        if step.command == "query" {
            if let Some(sql) = step.input.get("sql").and_then(|v| v.as_str()) {
                let clean = sql.trim_end_matches(';').trim().to_string();
                enforce_workflow_query_guards(&clean, options.permission_mode)?;
                let limit = options.query_row_limit.unwrap_or(WORKFLOW_QUERY_ROW_LIMIT);
                let mut input = step.input.clone();
                input["sql"] = serde_json::Value::String(clean.clone());
                input["limit"] = serde_json::Value::Number(limit.into());
                let step = WorkflowCommandStep { input, ..step };
                let conn_name = tokio::time::timeout_at(
                    workflow_deadline,
                    effective_connection_name(app_state, &step, workflow_connection),
                )
                .await
                .map_err(|_| {
                    WorkflowError::Step("GlobalTimeout before dispatch; no SQL submitted".into())
                })??;
                let result = app_state
                    .execute_command(
                        &step,
                        workflow_connection,
                        options.permission_mode,
                        Some(workflow_deadline),
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

        let conn_name = tokio::time::timeout_at(
            workflow_deadline,
            effective_connection_name(app_state, &step, workflow_connection),
        )
        .await
        .map_err(|_| {
            WorkflowError::Step("GlobalTimeout before dispatch; no SQL submitted".into())
        })??;
        let result = app_state
            .execute_command(
                &step,
                workflow_connection,
                options.permission_mode,
                Some(workflow_deadline),
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
    app_state: &dyn WorkflowHost,
    step: &WorkflowCommandStep,
    workflow_connection: Option<&str>,
) -> Result<String, WorkflowError> {
    let id = command_runtime::resolve_connection_id(step, workflow_connection)?;
    app_state.connection_name(id).await
}
