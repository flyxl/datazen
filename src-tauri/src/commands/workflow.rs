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
    workflow_execute_bound(state, workflow_id, variables, connection_id, false).await
}
async fn workflow_execute_bound(
    state: &AppState,
    workflow_id: String,
    variables: serde_json::Value,
    connection_id: Option<String>,
    desktop_user: bool,
) -> Result<crate::workflow::WorkflowExecutionResult, CommandError> {
    let workflow = state
        .workflow_registry
        .get(&workflow_id)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Workflow '{workflow_id}' not found")))?;

    let result = if desktop_user
        && workflow.version.as_deref() == Some("2")
        && !workflow.blocks.is_empty()
    {
        crate::workflow::job::execute(state, &workflow, &variables).await
    } else {
        crate::workflow::WorkflowExecutor::execute(
            &workflow,
            state,
            connection_id.as_deref(),
            &variables,
        )
        .await
    }
    .cmd_err("workflow_execute")?;

    // Dashboard-owned hidden workflows must not pollute the user-facing history list.
    if workflow.visibility != crate::workflow::WorkflowVisibility::DashboardHidden {
        let history_variables =
            crate::workflow::migration::sanitize_variables(&workflow, &variables);
        if let Err(e) = state
            .workflow_history
            .record(&workflow.id, &workflow.name, &history_variables, &result)
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
    workflow_execute_bound(&state, workflow_id, variables, connection_id, true).await
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

/// User-facing Job controls use the desktop RequestContext, never caller identity.
#[tauri::command]
pub async fn workflow_list_jobs(
    state: State<'_, AppState>,
) -> Result<Vec<datazen_platform_api::dto::job::JobView>, CommandError> {
    let context = workflow_job_context(&state).await?;
    state
        .desktop_job_host
        .list(
            &context,
            datazen_platform_api::dto::job::JobFilter::default(),
        )
        .await
        .map(|jobs| {
            jobs.into_iter()
                .filter(|job| job.view.kind == "workflowRun")
                .map(|job| job.view)
                .collect()
        })
        .map_err(|_| CommandError::Validation("Workflow Job lookup unavailable".into()))
}
#[tauri::command]
pub async fn workflow_cancel_job(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<datazen_platform_api::dto::job::JobView, CommandError> {
    let context = workflow_job_context(&state).await?;
    let id = datazen_platform_api::id::JobId::new(job_id);
    let job = state
        .desktop_job_host
        .get(&context, id.clone())
        .await
        .map_err(|_| CommandError::Validation("Workflow Job unavailable".into()))?;
    if job.view.kind != "workflowRun" {
        return Err(CommandError::Validation("Workflow Job required".into()));
    }
    state
        .desktop_job_host
        .cancel(&context, &id)
        .await
        .map(|job| job.view)
        .map_err(|_| CommandError::Validation("Workflow Job cancellation unavailable".into()))
}
async fn workflow_job_context(
    state: &AppState,
) -> Result<datazen_platform_api::context::RequestContext, CommandError> {
    let platform = state
        .platform_require()
        .map_err(|_| CommandError::Validation("platform unavailable".into()))?;
    let context = platform.begin_call().context().clone();
    platform
        .runtime()
        .map_err(|_| CommandError::Validation("runtime unavailable".into()))?
        .authorize_workflow_job(&context)
        .await
        .map_err(|_| CommandError::Validation("Workflow Job authorization denied".into()))?;
    Ok(context)
}
