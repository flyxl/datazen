//! Workflow IPC commands.

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use datazen::workflow::executor::WorkflowExecutor;
use datazen::workflow::history::{WorkflowRunListItem, WorkflowRunRecord};
use datazen::workflow::model::{
    WorkflowDefinition, WorkflowExecutionResult, WorkflowListItem, WorkflowVisibility,
};
use tauri::State;

// ─── Workflow IPC commands ───

pub(crate) async fn workflow_list_impl(
    state: &AppState,
) -> Result<Vec<WorkflowListItem>, CommandError> {
    let scope = state.organization_scope()?;
    Ok(state.workflow_registry.list(&scope).await)
}

#[tauri::command]
pub async fn workflow_list(
    state: State<'_, AppState>,
) -> Result<Vec<WorkflowListItem>, CommandError> {
    workflow_list_impl(&state).await
}

pub(crate) async fn workflow_execute_impl(
    state: &AppState,
    workflow_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<WorkflowExecutionResult, CommandError> {
    let scope = state.organization_scope()?;
    let workflow = state
        .workflow_registry
        .get(&scope, &workflow_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Workflow '{workflow_id}' not found")))?;

    let result = WorkflowExecutor::execute(&workflow, state, connection_id.as_deref(), &variables)
        .await
        .cmd_err("workflow_execute")?;

    // Dashboard-owned hidden workflows must not pollute the user-facing history list.
    if workflow.visibility != WorkflowVisibility::DashboardHidden {
        if let Err(e) = state
            .workflow_history
            .record(&workflow.id, &workflow.name, &variables, &result)
            .await
        {
            tracing::warn!("Failed to record workflow history: {e}");
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn workflow_execute(
    state: State<'_, AppState>,
    workflow_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<WorkflowExecutionResult, CommandError> {
    workflow_execute_impl(&state, workflow_id, variables, connection_id).await
}

#[tauri::command]
pub async fn workflow_save(
    state: State<'_, AppState>,
    workflow: WorkflowDefinition,
) -> Result<(), CommandError> {
    let scope = state.organization_scope()?;
    state
        .workflow_registry
        .save_workflow(&scope, &workflow)
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn workflow_save_yaml(
    state: State<'_, AppState>,
    yaml: String,
) -> Result<WorkflowDefinition, CommandError> {
    let scope = state.organization_scope()?;
    state
        .workflow_registry
        .save_workflow_yaml(&scope, &yaml)
        .await
        .map_err(CommandError::Validation)
}

#[tauri::command]
pub async fn workflow_get_yaml(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<String, CommandError> {
    let scope = state.organization_scope()?;
    // 桌面是单租户：别的组织的工作流对本组织**不存在**（CM-05）。
    if !crate::platform::scope::is_local_scope(&scope) {
        return Err(CommandError::NotFound(format!(
            "Workflow '{workflow_id}' not found"
        )));
    }
    let record = state
        .store
        .app_db()
        .get_workflow(&workflow_id)
        .map_err(|e| match e {
            crate::store::AppDbError::NotFound(id) => {
                CommandError::NotFound(format!("Workflow '{id}' not found"))
            }
            other => CommandError::Internal(other.to_string()),
        })?;
    Ok(record.definition_yaml)
}

#[tauri::command]
pub async fn workflow_delete(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<(), CommandError> {
    let scope = state.organization_scope()?;
    state
        .workflow_registry
        .delete_workflow(&scope, &workflow_id)
        .await
        .map_err(|msg| CommandError::Validation(msg))
}

#[tauri::command]
pub async fn workflow_reload(state: State<'_, AppState>) -> Result<(), CommandError> {
    let scope = state.organization_scope()?;
    state
        .workflow_registry
        .load_all(&scope)
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn workflow_get_dir(state: State<'_, AppState>) -> Result<String, CommandError> {
    Ok(state
        .workflow_registry
        .workflows_dir()
        .display()
        .to_string())
}

#[tauri::command]
pub async fn workflow_get(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<WorkflowDefinition, CommandError> {
    let scope = state.organization_scope()?;
    state
        .workflow_registry
        .get(&scope, &workflow_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Workflow '{workflow_id}' not found")))
}

// ─── Workflow History ───

#[tauri::command]
pub async fn workflow_history_list(
    state: State<'_, AppState>,
    workflow_id: Option<String>,
) -> Result<Vec<WorkflowRunListItem>, CommandError> {
    Ok(state.workflow_history.list(workflow_id.as_deref()).await)
}

#[tauri::command]
pub async fn workflow_history_get(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<WorkflowRunRecord, CommandError> {
    state
        .workflow_history
        .get(&history_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("History '{history_id}' not found")))
}

#[tauri::command]
pub async fn workflow_history_clear(
    state: State<'_, AppState>,
    workflow_id: Option<String>,
) -> Result<usize, CommandError> {
    state
        .workflow_history
        .clear(workflow_id.as_deref())
        .await
        .cmd_err("workflow_history_clear")
}
