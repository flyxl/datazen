//! User-defined AI workflow system.
//!
//! Workflows are YAML-defined reusable automations combining prompt templates,
//! database queries, conditional logic, loops, and variable substitution.
//! Supports cross-database workflows with per-step connection binding.

use crate::commands::AppState;
use crate::mcp::permission::{self, McpPermissionMode};
use datazen_ai_api::{ChatMessage, CompletionRequest, MessageRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

/// Documented workflow query row cap (`docs/architecture/security.md`).
pub const WORKFLOW_QUERY_ROW_LIMIT: u32 = 1000;

/// Options that differ between GUI and MCP invocation surfaces.
#[derive(Debug, Clone)]
pub struct WorkflowExecuteOptions {
    /// When set, each query step is checked with MCP SQL permission rules.
    pub permission_mode: Option<McpPermissionMode>,
    /// Max rows per query step (defaults to [`WORKFLOW_QUERY_ROW_LIMIT`]).
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

/// Shared guard used by query steps (also unit-tested in isolation).
pub fn enforce_workflow_query_guards(
    sql: &str,
    permission_mode: Option<McpPermissionMode>,
) -> Result<(), String> {
    if let Some(mode) = permission_mode {
        permission::check_sql_allowed(sql, mode)?;
    }
    Ok(())
}

// ─── Data Model (Phase 1) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    #[serde(default)]
    pub variables: Vec<WorkflowVariable>,
    pub steps: Vec<WorkflowStep>,
    pub output: Option<WorkflowOutput>,
    pub timeout_secs: Option<u64>,
    pub error_handling: Option<ErrorHandlingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    pub description: String,
    pub required: Option<bool>,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowStep {
    #[serde(rename = "query")]
    Query {
        id: String,
        sql: String,
        connection: Option<String>,
        database: Option<String>,
        timeout_secs: Option<u64>,
        on_error: Option<ErrorHandlingConfig>,
    },
    #[serde(rename = "ai")]
    Ai {
        id: String,
        prompt: String,
        timeout_secs: Option<u64>,
        on_error: Option<ErrorHandlingConfig>,
    },
    #[serde(rename = "condition")]
    Condition {
        id: String,
        #[serde(rename = "if")]
        expr: String,
        then_steps: Vec<WorkflowStep>,
        else_steps: Option<Vec<WorkflowStep>>,
    },
    #[serde(rename = "foreach")]
    ForEach {
        id: String,
        items: String,
        as_var: String,
        steps: Vec<WorkflowStep>,
        max_iterations: Option<usize>,
    },
}

impl WorkflowStep {
    pub fn step_id(&self) -> &str {
        match self {
            Self::Query { id, .. }
            | Self::Ai { id, .. }
            | Self::Condition { id, .. }
            | Self::ForEach { id, .. } => id,
        }
    }

    fn step_type_str(&self) -> &'static str {
        match self {
            Self::Query { .. } => "query",
            Self::Ai { .. } => "ai",
            Self::Condition { .. } => "condition",
            Self::ForEach { .. } => "foreach",
        }
    }

    fn on_error_strategy(&self) -> Option<ErrorStrategy> {
        match self {
            Self::Query { on_error, .. } | Self::Ai { on_error, .. } => {
                on_error.as_ref().map(|c| c.to_strategy())
            }
            _ => None,
        }
    }

    fn timeout_secs(&self) -> Option<u64> {
        match self {
            Self::Query { timeout_secs, .. } | Self::Ai { timeout_secs, .. } => *timeout_secs,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput {
    pub format: String,
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandlingConfig {
    pub strategy: ErrorStrategyKind,
    #[serde(default)]
    pub fallback_steps: Option<Vec<WorkflowStep>>,
}

impl ErrorHandlingConfig {
    pub fn to_strategy(&self) -> ErrorStrategy {
        match self.strategy {
            ErrorStrategyKind::Abort => ErrorStrategy::Abort,
            ErrorStrategyKind::Skip => ErrorStrategy::Skip,
            ErrorStrategyKind::Fallback => ErrorStrategy::Fallback {
                steps: self.fallback_steps.clone().unwrap_or_default(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorStrategyKind {
    Abort,
    Skip,
    Fallback,
}

#[derive(Debug, Clone)]
pub enum ErrorStrategy {
    Abort,
    Skip,
    Fallback { steps: Vec<WorkflowStep> },
}

// ─── Execution Result ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionResult {
    pub success: bool,
    pub final_output: String,
    pub steps: Vec<StepExecutionResult>,
    pub total_time_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepExecutionResult {
    pub step_id: String,
    pub step_type: String,
    pub status: StepStatus,
    pub result: Option<serde_json::Value>,
    pub execution_time_ms: u64,
    pub error: Option<String>,
    pub connection_name: Option<String>,
    pub sql_executed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
    TimedOut,
}

// ─── List Item (for frontend) ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowListItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variables: Vec<WorkflowVariable>,
}

// ─── Registry ───────────────────────────────────────────────────────────────

pub struct WorkflowRegistry {
    workflows: RwLock<HashMap<String, WorkflowDefinition>>,
    workflows_dir: PathBuf,
    /// Set after the first successful disk scan (or explicit reload).
    loaded: AtomicBool,
    /// Serializes first-load / reload so concurrent list/get don't race.
    load_lock: Mutex<()>,
}

impl WorkflowRegistry {
    pub fn new(workflows_dir: PathBuf) -> Self {
        Self {
            workflows: RwLock::new(HashMap::new()),
            workflows_dir,
            loaded: AtomicBool::new(false),
            load_lock: Mutex::new(()),
        }
    }

    pub fn workflows_dir(&self) -> &PathBuf {
        &self.workflows_dir
    }

    /// Load YAML workflows from disk if they have not been loaded yet.
    /// Called on first visit to the workflow UI (or MCP list/get).
    pub async fn ensure_loaded(&self) -> Result<(), String> {
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let _guard = self.load_lock.lock().await;
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        self.load_all_unlocked().await?;
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }

    /// Force re-scan the workflows directory (Refresh button / workflow_reload).
    pub async fn load_all(&self) -> Result<(), String> {
        let _guard = self.load_lock.lock().await;
        self.load_all_unlocked().await?;
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }

    async fn load_all_unlocked(&self) -> Result<(), String> {
        if !self.workflows_dir.exists() {
            std::fs::create_dir_all(&self.workflows_dir).map_err(|e| e.to_string())?;
            self.workflows.write().await.clear();
            return Ok(());
        }

        let mut workflows = self.workflows.write().await;
        workflows.clear();

        let entries = std::fs::read_dir(&self.workflows_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path
                .extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml")
            {
                match Self::load_workflow_file(&path) {
                    Ok(workflow) => {
                        tracing::info!("Loaded workflow: {} ({})", workflow.name, workflow.id);
                        workflows.insert(workflow.id.clone(), workflow);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load workflow {:?}: {}", path, e);
                    }
                }
            }
        }

        tracing::info!("Loaded {} workflows", workflows.len());
        Ok(())
    }

    fn load_workflow_file(path: &std::path::Path) -> Result<WorkflowDefinition, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;
        serde_yaml::from_str::<WorkflowDefinition>(&content)
            .map_err(|e| format!("Failed to parse {path:?}: {e}"))
    }

    pub async fn get(&self, id: &str) -> Option<WorkflowDefinition> {
        if let Err(e) = self.ensure_loaded().await {
            tracing::warn!("Failed to load workflows before get: {e}");
            return None;
        }
        self.workflows.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<WorkflowListItem> {
        if let Err(e) = self.ensure_loaded().await {
            tracing::warn!("Failed to load workflows before list: {e}");
            return Vec::new();
        }
        self.workflows
            .read()
            .await
            .values()
            .map(|s| WorkflowListItem {
                id: s.id.clone(),
                name: s.name.clone(),
                description: s.description.clone(),
                variables: s.variables.clone(),
            })
            .collect()
    }

    pub fn validate_id(id: &str) -> Result<(), String> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "Invalid workflow id: {id}. Only alphanumeric, dash, and underscore are allowed."
            ));
        }
        Ok(())
    }

    pub async fn save_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        Self::validate_id(&workflow.id)?;
        let yaml = serde_yaml::to_string(workflow).map_err(|e| e.to_string())?;
        let path = self.workflows_dir.join(format!("{}.yaml", workflow.id));
        std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
        self.workflows
            .write()
            .await
            .insert(workflow.id.clone(), workflow.clone());
        Ok(())
    }

    pub async fn delete_workflow(&self, id: &str) -> Result<(), String> {
        Self::validate_id(id)?;
        let path = self.workflows_dir.join(format!("{id}.yaml"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        self.workflows.write().await.remove(id);
        Ok(())
    }
}

// ─── Executor (Phase 2-6) ───────────────────────────────────────────────────

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
            .map(|eh| eh.to_strategy())
            .unwrap_or(ErrorStrategy::Abort);

        let mut context = WorkflowContext::new(variables);
        context.set_builtin_variables();

        for var in &workflow.variables {
            if !context.variables.contains_key(&var.name) {
                if let Some(default) = &var.default {
                    let val = match default {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    context.variables.insert(var.name.clone(), val);
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

        let mut step_results = Vec::new();
        let outcome = Self::execute_steps(
            &workflow.steps,
            app_state,
            connection_id,
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
                let final_output = if let Some(ref output) = workflow.output {
                    if let Some(ref template) = output.template {
                        context.resolve_template(template).unwrap_or_default()
                    } else {
                        context.get_last_result().unwrap_or_default()
                    }
                } else {
                    context.get_last_result().unwrap_or_default()
                };
                Ok(WorkflowExecutionResult {
                    success: true,
                    final_output,
                    steps: step_results,
                    total_time_ms,
                    error: None,
                })
            }
            Err(e) => {
                let final_output = context.get_last_result().unwrap_or_default();
                Ok(WorkflowExecutionResult {
                    success: false,
                    final_output,
                    steps: step_results,
                    total_time_ms,
                    error: Some(e),
                })
            }
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
                return Err(format!(
                    "Global timeout ({global_timeout_secs}s) exceeded"
                ));
            }

            match step {
                WorkflowStep::Condition {
                    id,
                    expr,
                    then_steps,
                    else_steps,
                } => {
                    let resolved_expr = context.resolve_template(expr)?;
                    let condition_met = evaluate_condition(&resolved_expr, context);
                    let branch = if condition_met {
                        then_steps.as_slice()
                    } else {
                        else_steps.as_deref().unwrap_or(&[])
                    };

                    step_results.push(StepExecutionResult {
                        step_id: id.clone(),
                        step_type: "condition".into(),
                        status: StepStatus::Success,
                        result: Some(serde_json::json!({ "condition": condition_met })),
                        execution_time_ms: 0,
                        error: None,
                        connection_name: None,
                        sql_executed: None,
                    });

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
                    let max_iter = max_iterations.unwrap_or(100);
                    let resolved_items = context.resolve_template(items)?;

                    let items_val: serde_json::Value =
                        serde_json::from_str(&resolved_items).unwrap_or_else(|_| {
                            context
                                .resolve_deep_path(&resolved_items)
                                .unwrap_or(serde_json::Value::Null)
                        });

                    let arr = match items_val.as_array() {
                        Some(a) => a.clone(),
                        None => {
                            step_results.push(StepExecutionResult {
                                step_id: id.clone(),
                                step_type: "foreach".into(),
                                status: StepStatus::Skipped,
                                result: None,
                                execution_time_ms: 0,
                                error: Some("Items expression did not resolve to an array".into()),
                                connection_name: None,
                                sql_executed: None,
                            });
                            continue;
                        }
                    };

                    let mut iterations_results = Vec::new();
                    let iter_count = arr.len().min(max_iter);

                    for (i, item) in arr.iter().take(max_iter).enumerate() {
                        context.set_loop_var(as_var, item.clone());

                        let mut iter_steps = Vec::new();
                        let outcome = Self::execute_steps(
                            loop_steps,
                            app_state,
                            connection_id,
                            context,
                            &mut iter_steps,
                            default_strategy,
                            global_timeout_secs,
                            global_start,
                            options,
                        )
                        .await;

                        iterations_results.push(serde_json::json!({
                            "index": i,
                            "steps": iter_steps,
                        }));
                        step_results.extend(iter_steps);

                        if let Err(e) = outcome {
                            step_results.push(StepExecutionResult {
                                step_id: id.clone(),
                                step_type: "foreach".into(),
                                status: StepStatus::Failed,
                                result: Some(serde_json::json!({
                                    "iterations_completed": i,
                                    "iterations": iterations_results,
                                })),
                                execution_time_ms: 0,
                                error: Some(e.clone()),
                                connection_name: None,
                                sql_executed: None,
                            });
                            return Err(e);
                        }
                    }

                    context.clear_loop_var(as_var);

                    let foreach_result = serde_json::json!({
                        "iterations_completed": iter_count,
                        "iterations": iterations_results,
                    });
                    context.set_step_result(id, foreach_result.clone());

                    step_results.push(StepExecutionResult {
                        step_id: id.clone(),
                        step_type: "foreach".into(),
                        status: StepStatus::Success,
                        result: Some(foreach_result),
                        execution_time_ms: 0,
                        error: None,
                        connection_name: None,
                        sql_executed: None,
                    });
                }

                _ => {
                    let step_timeout = step.timeout_secs().unwrap_or(30);
                    let step_start = Instant::now();

                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(step_timeout),
                        Self::execute_single_step(step, app_state, connection_id, context, options),
                    )
                    .await;

                    let elapsed = step_start.elapsed().as_millis() as u64;

                    match result {
                        Ok(Ok(step_result)) => {
                            let mut sr = step_result;
                            sr.execution_time_ms = elapsed;
                            step_results.push(sr);
                        }
                        Ok(Err(err)) => {
                            let strategy =
                                step.on_error_strategy().unwrap_or_else(|| default_strategy.clone());
                            match strategy {
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
                                    return Err(err);
                                }
                                ErrorStrategy::Skip => {
                                    context.set_step_result(
                                        step.step_id(),
                                        serde_json::Value::Null,
                                    );
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
                                }
                                ErrorStrategy::Fallback {
                                    steps: fallback_steps,
                                } => {
                                    Self::execute_steps(
                                        &fallback_steps,
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
                        Err(_elapsed) => {
                            let err = format!(
                                "Step '{}' timed out after {}s",
                                step.step_id(),
                                step_timeout
                            );
                            let strategy =
                                step.on_error_strategy().unwrap_or_else(|| default_strategy.clone());
                            match strategy {
                                ErrorStrategy::Abort => {
                                    step_results.push(StepExecutionResult {
                                        step_id: step.step_id().into(),
                                        step_type: step.step_type_str().into(),
                                        status: StepStatus::TimedOut,
                                        result: None,
                                        execution_time_ms: elapsed,
                                        error: Some(err.clone()),
                                        connection_name: None,
                                        sql_executed: None,
                                    });
                                    return Err(err);
                                }
                                ErrorStrategy::Skip => {
                                    context.set_step_result(
                                        step.step_id(),
                                        serde_json::Value::Null,
                                    );
                                    step_results.push(StepExecutionResult {
                                        step_id: step.step_id().into(),
                                        step_type: step.step_type_str().into(),
                                        status: StepStatus::TimedOut,
                                        result: None,
                                        execution_time_ms: elapsed,
                                        error: Some(err),
                                        connection_name: None,
                                        sql_executed: None,
                                    });
                                }
                                ErrorStrategy::Fallback {
                                    steps: fallback_steps,
                                } => {
                                    Self::execute_steps(
                                        &fallback_steps,
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
            }
        }
        Ok(())
        })
    }

    async fn execute_single_step(
        step: &WorkflowStep,
        app_state: &AppState,
        global_connection_id: Option<&str>,
        context: &mut WorkflowContext,
        options: &WorkflowExecuteOptions,
    ) -> Result<StepExecutionResult, String> {
        match step {
            WorkflowStep::Query {
                id,
                sql,
                connection,
                database,
                ..
            } => {
                let resolved_sql = context.resolve_template(sql)?;
                tracing::debug!("[workflow] step '{}' resolved sql: {}", id, resolved_sql);

                let conn_ref = if let Some(conn_tmpl) = connection {
                    Some(context.resolve_template(conn_tmpl)?)
                } else {
                    None
                };
                let conn_id_str = conn_ref.as_deref().or(global_connection_id);
                let conn_id =
                    conn_id_str.ok_or("Query step requires a database connection")?;

                let (runtime_id, driver, handle) = app_state
                    .connection_manager
                    .resolve_session(conn_id)
                    .await
                    .map_err(|e| format!("Failed to connect '{conn_id}': {e}"))?;
                let conn_name = app_state
                    .connection_manager
                    .get_connection_config(&runtime_id)
                    .await
                    .map(|c| c.name)
                    .unwrap_or_else(|_| conn_id.to_string());

                if let Some(db_tmpl) = database {
                    let resolved_db = context.resolve_template(db_tmpl)?;
                    if !resolved_db.is_empty() {
                        driver
                            .use_database(&handle, &resolved_db)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                }

                let clean_sql = resolved_sql.trim_end_matches(';').trim().to_string();
                enforce_workflow_query_guards(&clean_sql, options.permission_mode)?;

                let row_limit = options
                    .query_row_limit
                    .unwrap_or(WORKFLOW_QUERY_ROW_LIMIT);
                let multi = driver
                    .query_multi(&handle, &clean_sql, Some(row_limit))
                    .await
                    .map_err(|e| e.to_string())?;
                let result = multi
                    .results
                    .into_iter()
                    .next()
                    .ok_or_else(|| "Query step returned no statement result".to_string())?;

                let col_names: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
                let data: Vec<serde_json::Value> = result.rows.iter().map(|row| {
                    let mut obj = serde_json::Map::new();
                    for (i, col) in col_names.iter().enumerate() {
                        let val = row.get(i).and_then(|v| v.as_ref());
                        obj.insert(col.to_string(), match val {
                            None => serde_json::Value::Null,
                            Some(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
                        });
                    }
                    serde_json::Value::Object(obj)
                }).collect();

                let structured = serde_json::json!({
                    "rows": &data,
                    "rows_count": data.len(),
                    "columns": result.columns,
                    "execution_time_ms": result.execution_time_ms,
                    "truncated": result.truncated,
                });

                tracing::info!(
                    "[workflow] step '{}' result: columns={:?}, rows_count={}",
                    id, col_names, result.rows.len()
                );
                if let Some(first) = data.first() {
                    tracing::debug!("[workflow] step '{}' first_row: {}", id, first);
                }

                context.set_step_result(id, structured.clone());

                Ok(StepExecutionResult {
                    step_id: id.clone(),
                    step_type: "query".into(),
                    status: StepStatus::Success,
                    result: Some(structured),
                    execution_time_ms: 0,
                    error: None,
                    connection_name: Some(conn_name),
                    sql_executed: Some(resolved_sql),
                })
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
                        content: resolved_prompt.clone(),
                        reasoning: None,
                        tool_calls: None,
                        tool_call_id: None,
                    }],
                    temperature: Some(0.3),
                    stop: None,
                    tools: None,
                    previous_response_id: None,
                };

                let response = provider.complete(&request).await.map_err(|e| e.to_string())?;

                let structured = serde_json::json!({
                    "result": response.content,
                });
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

            _ => unreachable!("Condition/ForEach handled in execute_steps"),
        }
    }
}

// ─── Context (Phase 2: structured results + deep path resolution) ────────────

struct WorkflowContext {
    variables: HashMap<String, String>,
    step_results: HashMap<String, serde_json::Value>,
    loop_vars: HashMap<String, serde_json::Value>,
    last_step_id: Option<String>,
}

impl WorkflowContext {
    fn new(input: &serde_json::Value) -> Self {
        let mut variables = HashMap::new();
        if let Some(obj) = input.as_object() {
            for (k, v) in obj {
                variables.insert(
                    k.clone(),
                    match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    },
                );
            }
        }
        Self {
            variables,
            step_results: HashMap::new(),
            loop_vars: HashMap::new(),
            last_step_id: None,
        }
    }

    fn set_builtin_variables(&mut self) {
        let now = chrono::Local::now();
        self.variables
            .insert("current_month".into(), now.format("%Y-%m").to_string());
        self.variables
            .insert("current_date".into(), now.format("%Y-%m-%d").to_string());
        self.variables
            .insert("current_year".into(), now.format("%Y").to_string());
    }

    fn set_step_result(&mut self, step_id: &str, result: serde_json::Value) {
        self.step_results.insert(step_id.into(), result);
        self.last_step_id = Some(step_id.into());
    }

    fn set_loop_var(&mut self, name: &str, value: serde_json::Value) {
        self.loop_vars.insert(name.into(), value);
    }

    fn clear_loop_var(&mut self, name: &str) {
        self.loop_vars.remove(name);
    }

    fn get_last_result(&self) -> Option<String> {
        self.last_step_id.as_ref().and_then(|id| {
            self.step_results.get(id).map(|v| {
                v.get("result")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| serde_json::to_string_pretty(v).unwrap_or_default())
            })
        })
    }

    /// Resolve `{{...}}` template expressions.
    ///
    /// Supported patterns:
    /// - `{{variable_name}}` — input variables + built-ins
    /// - `{{steps.<id>.rows.0.field}}` — first row, specific column
    /// - `{{steps.<id>.rows.*.field}}` — all rows' field values, comma-separated for IN clauses
    /// - `{{steps.<id>.rows_count}}` — row count
    /// - `{{steps.<id>.result}}` — AI step output text
    /// - `{{item.field}}` — foreach loop variable
    fn resolve_template(&self, template: &str) -> Result<String, String> {
        let re = regex::Regex::new(r"\{\{([^}]+)\}\}").map_err(|e| e.to_string())?;

        let result = re.replace_all(template, |caps: &regex::Captures| {
            let expr = caps[1].trim();
            self.resolve_expression(expr)
        });

        Ok(result.to_string())
    }

    fn resolve_expression(&self, expr: &str) -> String {
        // steps.<step_id>.<path...>
        if let Some(rest) = expr.strip_prefix("steps.") {
            if let Some(dot_pos) = rest.find('.') {
                let step_id = &rest[..dot_pos];
                let path = &rest[dot_pos + 1..];
                if let Some(step_val) = self.step_results.get(step_id) {
                    return self.resolve_json_path(step_val, path);
                }
            }
            return String::new();
        }

        // Loop variable: <as_var>.<field>
        if let Some(dot_pos) = expr.find('.') {
            let var_name = &expr[..dot_pos];
            if let Some(loop_val) = self.loop_vars.get(var_name) {
                let path = &expr[dot_pos + 1..];
                return self.resolve_json_path(loop_val, path);
            }
        }

        // Loop variable (bare): {{item}} when item is a simple value
        if let Some(loop_val) = self.loop_vars.get(expr) {
            return json_value_to_string(loop_val);
        }

        // Simple variable
        self.variables.get(expr).cloned().unwrap_or_default()
    }

    fn resolve_json_path(&self, value: &serde_json::Value, path: &str) -> String {
        if path.contains(".*") {
            return self.resolve_wildcard_path(value, path);
        }

        let mut current = value.clone();

        for raw_part in path.split('.') {
            let part = raw_part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(bracket_pos) = part.find('[') {
                let field = &part[..bracket_pos];
                if !field.is_empty() {
                    let next = current.get(field).cloned().unwrap_or(serde_json::Value::Null);
                    current = if next.is_null() && (field == "data" || field == "result") {
                        current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
                    } else {
                        next
                    };
                }
                let idx_str = part[bracket_pos + 1..].trim_end_matches(']');
                if let Ok(idx) = idx_str.parse::<usize>() {
                    current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
                }
            } else if let Ok(idx) = part.parse::<usize>() {
                current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
            } else {
                let next = current.get(part).cloned().unwrap_or(serde_json::Value::Null);
                current = if next.is_null() && (part == "data" || part == "result") {
                    current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
                } else {
                    next
                };
            }
        }

        json_value_to_string(&current)
    }

    /// Resolve paths like `rows.*.order_id` → `'val1','val2','val3'`
    fn resolve_wildcard_path(&self, value: &serde_json::Value, path: &str) -> String {
        let parts: Vec<&str> = path.split('.').collect();
        let wildcard_pos = parts.iter().position(|p| *p == "*").unwrap_or(0);

        let mut current = value.clone();
        for part in &parts[..wildcard_pos] {
            if let Ok(idx) = part.parse::<usize>() {
                current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
            } else {
                let next = current.get(*part).cloned().unwrap_or(serde_json::Value::Null);
                current = if next.is_null() && (*part == "data" || *part == "result") {
                    current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
                } else {
                    next
                };
            }
        }

        let arr = match current.as_array() {
            Some(a) => a,
            None => return String::new(),
        };

        let remaining_path: Vec<&str> = parts[wildcard_pos + 1..].to_vec();

        let values: Vec<String> = arr
            .iter()
            .map(|item| {
                let mut val = item.clone();
                for part in &remaining_path {
                    val = val.get(*part).cloned().unwrap_or(serde_json::Value::Null);
                }
                let s = json_value_to_string(&val);
                format!("'{s}'")
            })
            .collect();

        values.join(",")
    }

    /// Resolve a deep path expression from step results.
    /// Used by ForEach to resolve the items source.
    fn resolve_deep_path(&self, expr: &str) -> Option<serde_json::Value> {
        if let Some(rest) = expr.strip_prefix("steps.") {
            if let Some(dot_pos) = rest.find('.') {
                let step_id = &rest[..dot_pos];
                let path = &rest[dot_pos + 1..];
                if let Some(step_val) = self.step_results.get(step_id) {
                    let parts: Vec<&str> = path.split('.').collect();
                    let mut current = step_val.clone();
                    for part in &parts {
                        if let Ok(idx) = part.parse::<usize>() {
                            current =
                                current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
                        } else {
                            let next =
                                current.get(*part).cloned().unwrap_or(serde_json::Value::Null);
                            current = if next.is_null() && (*part == "data" || *part == "result") {
                                current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
                            } else {
                                next
                            };
                        }
                    }
                    return Some(current);
                }
            }
        }
        None
    }
}

fn json_value_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

// ─── Condition Evaluator (Phase 4) ──────────────────────────────────────────

fn evaluate_condition(expr: &str, context: &WorkflowContext) -> bool {
    let expr = expr.trim();

    // is_empty / is_not_empty
    if let Some(rest) = expr.strip_suffix(".is_empty") {
        let val = context.resolve_expression(rest.trim());
        return val.is_empty() || val == "0" || val == "null" || val == "[]";
    }
    if let Some(rest) = expr.strip_suffix(".is_not_empty") {
        let val = context.resolve_expression(rest.trim());
        return !val.is_empty() && val != "0" && val != "null" && val != "[]";
    }

    // Binary comparisons
    for op in &[">=", "<=", "!=", "==", ">", "<"] {
        if let Some(pos) = expr.find(op) {
            let left_expr = expr[..pos].trim();
            let right_expr = expr[pos + op.len()..].trim();

            let left = context.resolve_expression(left_expr);
            let right = right_expr.trim_matches(|c: char| c == '\'' || c == '"').to_string();

            let left_num = left.parse::<f64>().ok();
            let right_num = right.parse::<f64>().ok();

            return match *op {
                "==" => left == right,
                "!=" => left != right,
                ">" => {
                    left_num
                        .zip(right_num)
                        .map_or(left > right, |(l, r)| l > r)
                }
                "<" => {
                    left_num
                        .zip(right_num)
                        .map_or(left < right, |(l, r)| l < r)
                }
                ">=" => {
                    left_num
                        .zip(right_num)
                        .map_or(left >= right, |(l, r)| l >= r)
                }
                "<=" => {
                    left_num
                        .zip(right_num)
                        .map_or(left <= right, |(l, r)| l <= r)
                }
                _ => false,
            };
        }
    }

    // Truthy check: non-empty, non-zero, non-null
    let val = context.resolve_expression(expr);
    !val.is_empty() && val != "0" && val != "false" && val != "null"
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_query_guards_block_drop_in_safe_write() {
        let err = enforce_workflow_query_guards(
            "DROP TABLE users",
            Some(McpPermissionMode::SafeWrite),
        )
        .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn workflow_query_guards_allow_select_in_safe_write() {
        assert!(enforce_workflow_query_guards(
            "SELECT 1",
            Some(McpPermissionMode::SafeWrite),
        )
        .is_ok());
    }

    #[test]
    fn workflow_query_guards_skip_when_mode_absent() {
        assert!(enforce_workflow_query_guards("DROP TABLE users", None).is_ok());
    }

    #[test]
    fn test_skill_definition_yaml_parsing() {
        let yaml = r#"
id: test-skill
name: Test Skill
description: A test skill
variables:
  - name: table_name
    type: string
    description: Table to query
    required: true
steps:
  - type: query
    id: get_data
    sql: "SELECT * FROM {{table_name}} LIMIT 10"
  - type: ai
    id: analyze
    prompt: "Analyze this data: {{steps.get_data.result}}"
output:
  format: markdown
"#;
        let skill: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.id, "test-skill");
        assert_eq!(skill.name, "Test Skill");
        assert_eq!(skill.variables.len(), 1);
        assert_eq!(skill.steps.len(), 2);
        assert!(skill.output.is_some());
    }

    #[test]
    fn test_skill_definition_yaml_with_new_fields() {
        let yaml = r#"
id: cross-db
name: Cross DB Skill
description: A cross-database skill
timeout_secs: 60
error_handling:
  strategy: abort
variables:
  - name: order_db
    type: connection
    description: Order database
    required: true
  - name: uid
    type: string
    description: User ID
    required: true
steps:
  - type: query
    id: get_order
    connection: "{{order_db}}"
    sql: "SELECT order_id FROM orders WHERE uid = '{{uid}}' LIMIT 1"
    timeout_secs: 10
  - type: condition
    id: check_order
    if: "steps.get_order.rows_count > 0"
    then_steps:
      - type: query
        id: get_logistics
        connection: "{{logistics_db}}"
        sql: "SELECT * FROM logistics WHERE order_id = '{{steps.get_order.rows.0.order_id}}'"
    else_steps:
      - type: ai
        id: no_order
        prompt: "No order found"
  - type: foreach
    id: batch
    items: "steps.get_order.rows"
    as_var: "order"
    max_iterations: 50
    steps:
      - type: query
        id: detail
        sql: "SELECT * FROM details WHERE id = '{{order.id}}'"
output:
  format: markdown
"#;
        let skill: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.id, "cross-db");
        assert_eq!(skill.timeout_secs, Some(60));
        assert!(skill.error_handling.is_some());
        assert_eq!(skill.steps.len(), 3);
        assert_eq!(skill.variables[0].var_type, "connection");

        match &skill.steps[0] {
            WorkflowStep::Query {
                connection,
                timeout_secs,
                ..
            } => {
                assert_eq!(connection.as_deref(), Some("{{order_db}}"));
                assert_eq!(*timeout_secs, Some(10));
            }
            _ => panic!("Expected Query step"),
        }

        match &skill.steps[1] {
            WorkflowStep::Condition {
                then_steps,
                else_steps,
                ..
            } => {
                assert_eq!(then_steps.len(), 1);
                assert!(else_steps.is_some());
            }
            _ => panic!("Expected Condition step"),
        }

        match &skill.steps[2] {
            WorkflowStep::ForEach {
                as_var,
                max_iterations,
                steps,
                ..
            } => {
                assert_eq!(as_var, "order");
                assert_eq!(*max_iterations, Some(50));
                assert_eq!(steps.len(), 1);
            }
            _ => panic!("Expected ForEach step"),
        }
    }

    #[test]
    fn test_error_strategy_yaml_parsing() {
        let yaml = r#"strategy: abort"#;
        let es: ErrorHandlingConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(es.strategy, ErrorStrategyKind::Abort);

        let yaml = r#"strategy: skip"#;
        let es: ErrorHandlingConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(es.strategy, ErrorStrategyKind::Skip);

        let yaml = r#"
strategy: fallback
fallback_steps:
  - type: ai
    id: fallback_msg
    prompt: "Generate fallback"
"#;
        let es: ErrorHandlingConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(es.strategy, ErrorStrategyKind::Fallback);
        let steps = es.fallback_steps.unwrap();
        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn test_skill_context_variable_resolution() {
        let input = serde_json::json!({"table_name": "users", "limit": 10});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_builtin_variables();

        let result = ctx
            .resolve_template("SELECT * FROM {{table_name}} LIMIT {{limit}}")
            .unwrap();
        assert_eq!(result, "SELECT * FROM users LIMIT 10");
    }

    #[test]
    fn test_skill_context_step_result_resolution_backward_compat() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "query1",
            serde_json::json!({
                "result": "[{\"id\": 1}]",
                "rows": [{"id": 1}],
                "rows_count": 1,
            }),
        );

        let result = ctx
            .resolve_template("Data: {{steps.query1.result}}")
            .unwrap();
        assert_eq!(result, "Data: [{\"id\": 1}]");
    }

    #[test]
    fn test_skill_context_deep_path_rows_0_field() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "get_order",
            serde_json::json!({
                "rows": [{"order_id": "ORD-001", "amount": 199.0}],
                "rows_count": 1,
            }),
        );

        let result = ctx
            .resolve_template("ID: {{steps.get_order.rows.0.order_id}}")
            .unwrap();
        assert_eq!(result, "ID: ORD-001");
    }

    #[test]
    fn test_skill_context_deep_path_rows_count() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "s1",
            serde_json::json!({
                "rows": [{"id": 1}, {"id": 2}],
                "rows_count": 2,
            }),
        );

        let result = ctx
            .resolve_template("Count: {{steps.s1.rows_count}}")
            .unwrap();
        assert_eq!(result, "Count: 2");
    }

    #[test]
    fn test_skill_context_wildcard_rows_star_field() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "s1",
            serde_json::json!({
                "rows": [
                    {"order_id": "ORD-001"},
                    {"order_id": "ORD-002"},
                    {"order_id": "ORD-003"},
                ],
                "rows_count": 3,
            }),
        );

        let result = ctx
            .resolve_template("SELECT * FROM t WHERE id IN ({{steps.s1.rows.*.order_id}})")
            .unwrap();
        assert_eq!(
            result,
            "SELECT * FROM t WHERE id IN ('ORD-001','ORD-002','ORD-003')"
        );
    }

    #[test]
    fn test_data_result_fallback_to_rows() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "s1",
            serde_json::json!({
                "rows": [
                    {"code": 42, "name": "hello"},
                ],
                "rows_count": 1,
            }),
        );

        // "data" and "result" fall back to "rows" when they don't exist
        let r1 = ctx.resolve_template("{{steps.s1.data[0].name}}").unwrap();
        assert_eq!(r1, "hello");

        let r2 = ctx.resolve_template("{{steps.s1.result[0].code}}").unwrap();
        assert_eq!(r2, "42");

        // "rows" still works directly
        let r3 = ctx.resolve_template("{{steps.s1.rows[0].name}}").unwrap();
        assert_eq!(r3, "hello");
    }

    #[test]
    fn test_skill_context_loop_variable() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_loop_var(
            "order",
            serde_json::json!({"order_id": "ORD-001", "amount": 100}),
        );

        let result = ctx
            .resolve_template("ID: {{order.order_id}}, Amount: {{order.amount}}")
            .unwrap();
        assert_eq!(result, "ID: ORD-001, Amount: 100");
    }

    #[test]
    fn test_skill_context_builtin_variables() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_builtin_variables();

        let result = ctx.resolve_template("Date: {{current_date}}").unwrap();
        assert!(result.starts_with("Date: 20"));
        assert!(result.len() > 10);
    }

    #[test]
    fn test_skill_context_hyphenated_step_id() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "get-data",
            serde_json::json!({"result": "result here"}),
        );

        let result = ctx
            .resolve_template("Output: {{steps.get-data.result}}")
            .unwrap();
        assert_eq!(result, "Output: result here");
    }

    #[test]
    fn test_skill_context_last_result_ordering() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result("step1", serde_json::json!({"result": "first"}));
        ctx.set_step_result("step2", serde_json::json!({"result": "second"}));
        ctx.set_step_result("step3", serde_json::json!({"result": "third"}));
        assert_eq!(ctx.get_last_result(), Some("third".to_string()));
    }

    #[test]
    fn test_skill_list_item_serialization() {
        let item = WorkflowListItem {
            id: "test".into(),
            name: "Test".into(),
            description: "A test".into(),
            variables: vec![],
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"id\":\"test\""));
    }

    #[test]
    fn test_skill_id_validation() {
        assert!(WorkflowRegistry::validate_id("valid-id_123").is_ok());
        assert!(WorkflowRegistry::validate_id("").is_err());
        assert!(WorkflowRegistry::validate_id("../../evil").is_err());
        assert!(WorkflowRegistry::validate_id("has space").is_err());
        assert!(WorkflowRegistry::validate_id("path/slash").is_err());
    }

    #[test]
    fn test_skill_step_enum_deserialization() {
        let yaml = r#"type: query
id: get_data
sql: "SELECT 1""#;
        let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
        match step {
            WorkflowStep::Query { id, sql, .. } => {
                assert_eq!(id, "get_data");
                assert_eq!(sql, "SELECT 1");
            }
            _ => panic!("Expected Query step"),
        }

        let yaml = r#"type: ai
id: analyze
prompt: "Analyze this""#;
        let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
        match step {
            WorkflowStep::Ai { id, prompt, .. } => {
                assert_eq!(id, "analyze");
                assert_eq!(prompt, "Analyze this");
            }
            _ => panic!("Expected Ai step"),
        }
    }

    #[test]
    fn test_condition_evaluator_numeric_comparison() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "s1",
            serde_json::json!({"rows_count": 3}),
        );

        assert!(evaluate_condition("steps.s1.rows_count > 0", &ctx));
        assert!(!evaluate_condition("steps.s1.rows_count > 5", &ctx));
        assert!(evaluate_condition("steps.s1.rows_count >= 3", &ctx));
        assert!(evaluate_condition("steps.s1.rows_count <= 3", &ctx));
        assert!(evaluate_condition("steps.s1.rows_count == 3", &ctx));
        assert!(evaluate_condition("steps.s1.rows_count != 0", &ctx));
    }

    #[test]
    fn test_condition_evaluator_is_empty() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "s1",
            serde_json::json!({"rows_count": 0, "rows": []}),
        );
        ctx.set_step_result(
            "s2",
            serde_json::json!({"rows_count": 1, "rows": [{"id": 1}]}),
        );

        assert!(evaluate_condition("steps.s1.rows_count.is_empty", &ctx));
        assert!(evaluate_condition("steps.s2.rows_count.is_not_empty", &ctx));
    }

    #[test]
    fn test_condition_evaluator_string_comparison() {
        let input = serde_json::json!({"status": "active"});
        let ctx = WorkflowContext::new(&input);

        assert!(evaluate_condition("status == 'active'", &ctx));
        assert!(!evaluate_condition("status == 'inactive'", &ctx));
        assert!(evaluate_condition("status != 'inactive'", &ctx));
    }

    #[test]
    fn test_condition_evaluator_truthy() {
        let input = serde_json::json!({"flag": "yes"});
        let ctx = WorkflowContext::new(&input);

        assert!(evaluate_condition("flag", &ctx));
        assert!(!evaluate_condition("missing_var", &ctx));
    }

    #[test]
    fn test_execution_result_serialization() {
        let result = WorkflowExecutionResult {
            success: true,
            final_output: "done".into(),
            steps: vec![StepExecutionResult {
                step_id: "s1".into(),
                step_type: "query".into(),
                status: StepStatus::Success,
                result: Some(serde_json::json!({"rows": []})),
                execution_time_ms: 42,
                error: None,
                connection_name: Some("My DB".into()),
                sql_executed: Some("SELECT 1".into()),
            }],
            total_time_ms: 100,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"finalOutput\""));
        assert!(json.contains("\"stepId\""));
        assert!(json.contains("\"connectionName\""));
        assert!(json.contains("\"sqlExecuted\""));
    }

    #[test]
    fn test_resolve_deep_path() {
        let input = serde_json::json!({});
        let mut ctx = WorkflowContext::new(&input);
        ctx.set_step_result(
            "get_orders",
            serde_json::json!({
                "rows": [
                    {"order_id": "ORD-001"},
                    {"order_id": "ORD-002"},
                ],
                "rows_count": 2,
            }),
        );

        let val = ctx.resolve_deep_path("steps.get_orders.rows");
        assert!(val.is_some());
        let arr = val.unwrap();
        assert!(arr.is_array());
        assert_eq!(arr.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_backward_compat_old_skill_yaml() {
        let yaml = r#"
id: old-skill
name: Old Skill
description: Uses old format without new fields
variables:
  - name: query
    type: string
    description: SQL query
    required: true
steps:
  - type: query
    id: run
    sql: "{{query}}"
  - type: ai
    id: analyze
    prompt: "Analyze: {{steps.run.result}}"
output:
  format: text
"#;
        let skill: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.timeout_secs, None);
        assert!(skill.error_handling.is_none());
        match &skill.steps[0] {
            WorkflowStep::Query {
                connection,
                timeout_secs,
                on_error,
                ..
            } => {
                assert!(connection.is_none());
                assert!(timeout_secs.is_none());
                assert!(on_error.is_none());
            }
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_on_error_skip_yaml() {
        let yaml = r#"
type: query
id: risky
sql: "SELECT 1"
on_error:
  strategy: skip
"#;
        let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
        match step {
            WorkflowStep::Query { on_error, .. } => {
                let cfg = on_error.unwrap();
                assert_eq!(cfg.strategy, ErrorStrategyKind::Skip);
            }
            _ => panic!("Expected Query"),
        }
    }

    
}
