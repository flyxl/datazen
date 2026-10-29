//! Native-dialog export of the same immutable artifacts used by execution publication.
use crate::commands::{AppState, CommandError};
use datazen::{
    execution::domain::{ArtifactRepository, ExportSink},
    execution::ArtifactLifecycle,
    shared::id::{ArtifactId, LeaseId},
};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{AppHandle, State};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactExportRequest {
    pub artifact_id: ArtifactId,
}
#[derive(Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ArtifactExportResponse {
    Cancelled,
    Saved {
        artifact_id: ArtifactId,
        byte_length: u64,
    },
}
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn unavailable() -> CommandError {
    CommandError::Internal("artifact export unavailable".into())
}

/// Paths, identity, and partial-result confirmation are supplied by trusted Host code.
#[tauri::command]
pub async fn export_artifact(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ArtifactExportRequest,
) -> Result<ArtifactExportResponse, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(super::into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let store = adapter
        .artifact_store()
        .map_err(super::into_command_error)?;
    let metadata = store
        .describe(&context, request.artifact_id.clone())
        .await
        .map_err(|_| unavailable())?;
    let partial = match metadata.lifecycle {
        ArtifactLifecycle::Complete => false,
        ArtifactLifecycle::Truncated => {
            if !crate::commands::dialog::confirm_message(
                &app,
                "Export partial result",
                "This result is incomplete. Export its published data with a truncation marker?",
            )
            .await?
            {
                return Ok(ArtifactExportResponse::Cancelled);
            }
            true
        }
        _ => {
            return Err(CommandError::Validation(
                "only finalized results can be exported".into(),
            ))
        }
    };
    let Some(path) = crate::commands::dialog::save_file(
        &app,
        ("Result artifact (JSON Lines)".into(), vec!["jsonl".into()]),
        "result.jsonl".into(),
    )
    .await?
    else {
        return Ok(ArtifactExportResponse::Cancelled);
    };
    if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
        return Err(CommandError::Validation(
            "result export requires a .jsonl destination".into(),
        ));
    }
    // Existing files remain protected: this capability uses exclusive creation.
    let cancelled = Arc::new(AtomicBool::new(false));
    let guard = CancelOnDrop(cancelled.clone());
    let handle = LeaseId::new(uuid::Uuid::new_v4().to_string());
    store
        .authorize_dialog_export(
            &context,
            request.artifact_id.clone(),
            handle.clone(),
            path,
            partial,
            cancelled,
        )
        .map_err(|_| unavailable())?;
    let receipt = tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(store.export(
            &context,
            request.artifact_id,
            ExportSink::HostHandle(handle),
        ))
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    drop(guard);
    Ok(ArtifactExportResponse::Saved {
        artifact_id: receipt.artifact_id,
        byte_length: receipt.byte_length,
    })
}
