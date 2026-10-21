//! Product entry for deployment-pinned version probes, not native DB export/restore.
use super::{error::CommandError, AppState};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::job::{JobDefinition, JobDetails, JobFilter, JobView},
    id::{IdempotencyKey, JobId, StageId, WorkerId},
    ports::native_tool::NativeToolSelection,
};
use datazen_runtime::native_tool::NativeToolJobHandler;
use futures_util::FutureExt;
use std::{panic::AssertUnwindSafe, sync::Arc};
use tauri::State;

const KIND: &str = "nativeToolProbe";
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeProbeRequest {
    selection: NativeToolSelection,
    idempotency_key: String,
}
fn denied() -> CommandError {
    CommandError::Validation(
        "CapabilityUnsupported: native operation unavailable or unauthorized".into(),
    )
}
async fn context(state: &AppState) -> Result<RequestContext, CommandError> {
    let adapter = state.platform_require().map_err(|_| denied())?;
    let context = adapter.begin_call().context().clone();
    adapter
        .runtime()
        .map_err(|_| denied())?
        .authorize_job(&context)
        .await
        .map_err(|_| denied())?;
    Ok(context)
}
#[tauri::command]
pub async fn native_tool_choices(
    state: State<'_, AppState>,
) -> Result<Vec<crate::platform::native_tool::deployment::ProbeChoice>, CommandError> {
    choices(&state).await
}
async fn choices(
    state: &AppState,
) -> Result<Vec<crate::platform::native_tool::deployment::ProbeChoice>, CommandError> {
    context(state).await?;
    Ok(state
        .platform_require()
        .map_err(|_| denied())?
        .native_tools()
        .map_err(|_| denied())?
        .choices())
}
#[tauri::command]
pub async fn native_tool_submit(
    state: State<'_, AppState>,
    request: NativeProbeRequest,
) -> Result<JobView, CommandError> {
    submit(&state, request).await
}
pub(crate) async fn submit(
    state: &AppState,
    request: NativeProbeRequest,
) -> Result<JobView, CommandError> {
    let ctx = context(state).await?;
    let native = state
        .platform_require()
        .map_err(|_| denied())?
        .native_tools()
        .map_err(|_| denied())?;
    if request.selection.tool_id.len() > 64
        || request.selection.version.len() > 128
        || request.idempotency_key.len() > 36
    {
        return Err(denied());
    }
    let digest = native.freeze(&request.selection).map_err(|_| denied())?;
    let key = uuid::Uuid::parse_str(&request.idempotency_key).map_err(|_| denied())?;
    let id = JobId::new(uuid::Uuid::new_v4().to_string());
    let mut payload = NativeToolJobHandler::plan_payload(&request.selection);
    payload["planDigest"] = digest.into();
    let record = state
        .desktop_job_host
        .accept(
            &ctx,
            JobDefinition {
                job_id: id.clone(),
                organization_id: ctx.organization_id.clone(),
                kind: KIND.into(),
                owner: OwnerRef::Job {
                    job_id: id.clone(),
                    stage_id: StageId::new("native-probe"),
                },
                payload,
                created_at: state.desktop_job_host.now(),
            },
            &IdempotencyKey::new(format!("native-probe:{key}")),
        )
        .await
        .map_err(|_| denied())?;
    if record.view.job_id == id {
        let host = state.desktop_job_host.clone();
        let handler = Arc::new(NativeToolJobHandler::new(
            id.clone(),
            request.selection,
            native.runtime.clone(),
        ));
        // Only newly committed admission dispatches. Retries/restart never
        // reconstruct a handler; caller cancellation cannot drop this worker.
        tokio::spawn(async move {
            let run = AssertUnwindSafe(host.dispatch(
                &ctx,
                &id,
                &WorkerId::new(format!("native-{}", id.as_str())),
                &[],
                handler,
            ))
            .catch_unwind()
            .await;
            if !matches!(run, Ok(Ok(_))) {
                let _ = host
                    .repository()
                    .mark_pending_verification(&ctx, &id, "nativeDispatchUnconfirmed")
                    .await;
            }
        });
    }
    Ok(record.view)
}
async fn require_native(
    state: &AppState,
    ctx: &RequestContext,
    id: &JobId,
) -> Result<(), CommandError> {
    let record = state
        .desktop_job_host
        .get(ctx, id.clone())
        .await
        .map_err(|_| denied())?;
    if record.view.kind != KIND {
        return Err(denied());
    }
    Ok(())
}
#[tauri::command]
pub async fn native_tool_get(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<JobDetails, CommandError> {
    get(&state, job_id).await
}
async fn get(state: &AppState, job_id: String) -> Result<JobDetails, CommandError> {
    let ctx = context(state).await?;
    let id = JobId::new(job_id);
    require_native(state, &ctx, &id).await?;
    state
        .desktop_job_host
        .details(&ctx, id)
        .await
        .map_err(|_| denied())
}
#[tauri::command]
pub async fn native_tool_list_jobs(
    state: State<'_, AppState>,
) -> Result<Vec<JobView>, CommandError> {
    list(&state).await
}
async fn list(state: &AppState) -> Result<Vec<JobView>, CommandError> {
    let ctx = context(state).await?;
    Ok(state
        .desktop_job_host
        .list(
            &ctx,
            JobFilter {
                organization_id: ctx.organization_id.clone(),
                states: Vec::new(),
                owner: None,
                after: None,
                limit: None,
            },
        )
        .await
        .map_err(|_| denied())?
        .into_iter()
        .filter(|job| job.view.kind == KIND)
        .take(100)
        .map(|job| job.view)
        .collect())
}
#[tauri::command]
pub async fn native_tool_cancel(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<JobView, CommandError> {
    cancel(&state, job_id).await
}
async fn cancel(state: &AppState, job_id: String) -> Result<JobView, CommandError> {
    let ctx = context(state).await?;
    let id = JobId::new(job_id);
    require_native(state, &ctx, &id).await?;
    state
        .desktop_job_host
        .cancel(&ctx, &id)
        .await
        .map(|record| record.view)
        .map_err(|_| denied())
}

#[cfg(all(test, unix))]
#[path = "native_tool_tests.rs"]
mod tests;
