//! Workflow execution orchestration.
//!
//! This module is intentionally introduced before the legacy `workflows.rs`
//! facade is rewritten. The next migration step will move the existing
//! executor implementation here without changing runtime behavior.

use crate::commands::AppState;
use crate::mcp::permission::{self, McpPermissionMode};
use datazen_ai_api::{ChatMessage, CompletionRequest, MessageRole};
use std::time::Instant;
use uuid::Uuid;

use super::context::WorkflowContext;
use super::model::{
    ErrorStrategy, StepExecutionResult, StepStatus, WorkflowDefinition, WorkflowExecutionResult,
    WorkflowStep,
};

/// Documented workflow query row cap (`docs/architecture/security.md`).
pub const WORKFLOW_QUERY_ROW_LIMIT: u32 = 1000;

/// Options that differ between GUI and MCP invocation surfaces.
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

/// Temporary execution facade. The legacy implementation remains in
/// `workflows.rs` until the facade migration is committed.
///
/// Keeping this type separate makes the next migration mechanical: move the
/// existing `WorkflowExecutor` implementation here and have `workflows.rs`
/// re-export it.
pub struct WorkflowExecutor;

impl WorkflowExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Resolve the common execution inputs used by the executor.
    ///
    /// This helper deliberately does not execute a Step yet; it establishes
    /// the module boundary while preserving the legacy runtime path.
    pub(crate) fn prepare_context(
        workflow: &WorkflowDefinition,
        variables: &serde_json::Value,
    ) -> Result<WorkflowContext, String> {
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
        Ok(context)
    }
}

// Keep these imports visible while the implementation is moved from the
// legacy module. They also document the dependencies of the eventual executor.
#[allow(dead_code)]
fn _execution_dependencies() {
    let _ = (
        Instant::now(),
        Uuid::new_v4(),
        ChatMessage {
            role: MessageRole::User,
            content: String::new(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        },
        CompletionRequest {
            request_id: String::new(),
            model: String::new(),
            messages: vec![],
            temperature: None,
            stop: None,
            tools: None,
            previous_response_id: None,
        },
    );
    let _ = std::mem::size_of::<(
        &AppState,
        ErrorStrategy,
        StepExecutionResult,
        StepStatus,
        WorkflowStep,
        WorkflowExecutionResult,
    )>();
}
