//! Workflow IPC commands.

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use tauri::State;

// ─── Workflow IPC commands ───

pub(crate) async fn workflow_list_impl(
    state: &AppState,
) -> Result<Vec<crate::workflow::WorkflowListItem>, CommandError> {
    Ok(state.workflow_registry.list().await)
}

#[tauri::command]
pub async fn workflow_list(
    state: State<'_, AppState>,
) -> Result<Vec<crate::workflow::WorkflowListItem>, CommandError> {
    workflow_list_impl(&state).await
}

pub(crate) async fn workflow_execute_impl(
    state: &AppState,
    workflow_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
) -> Result<crate::workflow::WorkflowExecutionResult, CommandError> {
    let workflow = state
        .workflow_registry
        .get(&workflow_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Workflow '{workflow_id}' not found")))?;

    let result = crate::workflow::WorkflowExecutor::execute(
        &workflow,
        state,
        connection_id.as_deref(),
        &variables,
    )
    .await
    .cmd_err("workflow_execute")?;

    // Dashboard-owned hidden workflows must not pollute the user-facing history list.
    if workflow.visibility != crate::workflow::WorkflowVisibility::DashboardHidden {
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
) -> Result<crate::workflow::WorkflowExecutionResult, CommandError> {
    workflow_execute_impl(&state, workflow_id, variables, connection_id).await
}

#[tauri::command]
pub async fn workflow_save(
    state: State<'_, AppState>,
    workflow: crate::workflow::WorkflowDefinition,
) -> Result<(), CommandError> {
    state
        .workflow_registry
        .save_workflow(&workflow)
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn workflow_save_yaml(
    state: State<'_, AppState>,
    yaml: String,
) -> Result<crate::workflow::WorkflowDefinition, CommandError> {
    state
        .workflow_registry
        .save_workflow_yaml(&yaml)
        .await
        .map_err(CommandError::Validation)
}

#[tauri::command]
pub async fn workflow_get_yaml(
    state: State<'_, AppState>,
    workflow_id: String,
) -> Result<String, CommandError> {
    let record = state
        .workflow_registry
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
    state
        .workflow_registry
        .delete_workflow(&workflow_id)
        .await
        .map_err(|msg| CommandError::Validation(msg))
}

#[tauri::command]
pub async fn workflow_reload(state: State<'_, AppState>) -> Result<(), CommandError> {
    state
        .workflow_registry
        .load_all()
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
) -> Result<crate::workflow::WorkflowDefinition, CommandError> {
    state
        .workflow_registry
        .get(&workflow_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Workflow '{workflow_id}' not found")))
}

// ─── Workflow History ───

#[tauri::command]
pub async fn workflow_history_list(
    state: State<'_, AppState>,
    workflow_id: Option<String>,
) -> Result<Vec<crate::workflow::HistoryListItem>, CommandError> {
    Ok(state.workflow_history.list(workflow_id.as_deref()).await)
}

#[tauri::command]
pub async fn workflow_history_get(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<crate::workflow::HistoryEntry, CommandError> {
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
