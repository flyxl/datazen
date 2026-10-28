use crate::commands::AppState;
use datazen::application::sessions::ConnectionUseCases;
use datazen::execution::application::requests::*;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::{
    connection::domain::ProfileRepository,
    connection::dto::{ProfileDraft, ProfilePatch, ProfileView},
    execution::application::events::ConnectionEventEnvelope,
    execution::application::idempotency::IdempotentOperation,
    shared::context::OwnerRef,
    shared::id::*,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use tauri::{ipc::Channel, State};

fn adapter(state: &AppState) -> Result<&Arc<super::PlatformAdapter>, ApiError> {
    state.platform_require()
}
fn encode<T: Serialize>(value: T) -> Result<serde_json::Value, ApiError> {
    serde_json::to_value(value).map_err(|_| {
        ApiError::new(
            ApiErrorCode::OutcomeUnknown,
            "response serialization failed",
        )
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandleRequest {
    pub handle: datazen::execution::SessionHandle,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRequest {
    pub execution_id: ExecutionId,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelExecutionRequest {
    pub execution_id: ExecutionId,
    pub submission_token: datazen::execution::application::idempotency::SubmissionToken,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRequest {
    pub operation: IdempotentOperation,
    pub handle: Option<datazen::execution::SessionHandle>,
    pub scope: Option<TokenScope>,
    pub connection_id: Option<ConnectionId>,
    pub owner: Option<OwnerRef>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenScope {
    pub handle: Option<datazen::execution::SessionHandle>,
}
#[tauri::command]
pub async fn get_platform_identity(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, ApiError> {
    let call = adapter(&state)?.begin_call();
    let ctx = call.context();
    Ok(
        serde_json::json!({"clientInstanceId":ctx.client_instance_id,"organizationId":ctx.organization_id,"principalId":ctx.principal_id}),
    )
}
#[tauri::command]
pub async fn issue_submission_token(
    state: State<'_, AppState>,
    request: TokenRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let _ = (&request.connection_id, &request.owner);
    let handle = request
        .handle
        .or_else(|| request.scope.and_then(|scope| scope.handle));
    encode(
        adapter
            .runtime()?
            .issue_submission_token(call.context(), request.operation, handle.as_ref())
            .await?,
    )
}
macro_rules! session_command {
    ($name:ident,$request:ty) => {
        #[tauri::command]
        pub async fn $name(
            state: State<'_, AppState>,
            request: $request,
        ) -> Result<serde_json::Value, ApiError> {
            let adapter = adapter(&state)?;
            let call = adapter.begin_call();
            let result = adapter.runtime()?.$name(call.context(), request).await;
            if let Err(error) = &result {
                // 会话命令曾经完全静默：被拒的请求在日志里不留一行，一次「执行失败」
                // 只能靠猜是哪个命令、哪一类原因。`code` 是分类而非细节（消息本身
                // 已脱敏），可以安全落日志，也是报告问题时唯一的锚点。
                tracing::warn!(
                    command = stringify!($name),
                    code = error.code.as_str(),
                    "session command rejected"
                );
            }
            encode(result?)
        }
    };
}
session_command!(open_session, OpenSessionRequest);
session_command!(execute_in_session, ExecuteInSessionRequest);
#[tauri::command]
pub async fn execute_at_target(
    state: State<'_, AppState>,
    mut request: ExecuteAtTargetRequest,
) -> Result<serde_json::Value, ApiError> {
    request.validate()?;
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let canonical = adapter
        .resolve_execution_target(
            call.context(),
            request.expected_revision,
            &request.call.command,
            &request.target,
        )
        .await?;
    request.target = super::into_execution_target(canonical);
    encode(
        adapter
            .runtime()?
            .execute_at_target(call.context(), request)
            .await?,
    )
}
session_command!(set_session_context, SetSessionContextRequest);
/// Close a session and drop the AI grants that borrowed it.
///
/// Not the generic macro: closing an editor session is exactly the moment a
/// shared AI grant must stop being usable. Reclaim runs even when the close is
/// rejected, because a failed close leaves the handle just as unusable, and a
/// grant must never outlive the session it points at.
#[tauri::command]
pub async fn close_session(
    state: State<'_, AppState>,
    request: CloseSessionRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let db_session_id = request.handle.db_session_id.as_str().to_owned();
    let result = adapter
        .runtime()?
        .close_session(call.context(), request)
        .await;
    if let Err(error) = &result {
        tracing::warn!(
            code = error.code.as_str(),
            "session close rejected; reclaiming AI grants anyway"
        );
    }
    let revoked = state
        .ai_grants
        .revoke_for_session_handle(&db_session_id)
        .await;
    if revoked > 0 {
        tracing::info!(revoked, "AI grants revoked with their editor session");
    }
    encode(result?)
}
session_command!(attach_session, AttachmentRequest);
session_command!(detach_session, AttachmentRequest);
#[tauri::command]
pub async fn get_session(
    state: State<'_, AppState>,
    request: HandleRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    encode(
        adapter
            .runtime()?
            .get_session(call.context(), request.handle)
            .await?,
    )
}
#[tauri::command]
pub async fn get_execution(
    state: State<'_, AppState>,
    request: ExecutionRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    encode(
        adapter
            .runtime()?
            .get_execution(call.context(), request.execution_id)
            .await?,
    )
}
#[tauri::command]
pub async fn cancel_execution(
    state: State<'_, AppState>,
    request: CancelExecutionRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    adapter.runtime()?.validate_cancel_submission(
        call.context(),
        &request.submission_token.idempotency_key,
        &request.execution_id,
    )?;
    encode(
        adapter
            .runtime()?
            .cancel_execution(call.context(), request.execution_id)
            .await?,
    )
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, AppState>,
    request: serde_json::Value,
) -> Result<serde_json::Value, ApiError> {
    let _ = request;
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    encode(adapter.runtime()?.list_connections(call.context()).await?)
}
/// Kernel-wire create request (form-split F4): the facade folds the draft flat
/// and carries the kernel's own submission token — the same wire
/// `BackendClient.createConnection` sends, 1:1.
#[derive(Deserialize)]
pub struct CreateProfileRequest {
    #[serde(flatten)]
    pub draft: ProfileDraft,
    pub submission_token: datazen::execution::application::idempotency::SubmissionToken,
}
/// Kernel-wire update request (form-split F4): `expectedRevision` is the
/// facade's name for the config-revision CAS guard.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequest {
    pub connection_id: ConnectionId,
    pub expected_revision: ConfigRevision,
    pub patch: ProfilePatch,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRequest {
    pub connection_id: ConnectionId,
}
fn port_error(error: datazen::shared::error::PortError) -> ApiError {
    match error {
        datazen::shared::error::PortError::CasConflict { .. } => ApiError::new(
            ApiErrorCode::ConfigRevisionMismatch,
            "profile revision changed",
        ),
        datazen::shared::error::PortError::NotFound(_) => {
            ApiError::new(ApiErrorCode::NotFound, "profile unavailable")
        }
        _ => ApiError::new(ApiErrorCode::ServiceUnavailable, "profile operation failed"),
    }
}
#[tauri::command]
pub async fn create_connection(
    state: State<'_, AppState>,
    request: CreateProfileRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    encode(ProfileView::from_record(
        &adapter
            .profiles()?
            .create(
                call.context(),
                request.draft,
                &request.submission_token.idempotency_key,
            )
            .await
            .map_err(port_error)?,
    ))
}
#[tauri::command]
pub async fn update_connection(
    state: State<'_, AppState>,
    request: UpdateProfileRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let connection_id = request.connection_id.clone();
    let view = ProfileView::from_record(
        &adapter
            .profiles()?
            .compare_and_set(
                call.context(),
                request.connection_id,
                request.expected_revision,
                request.patch,
            )
            .await
            .map_err(port_error)?,
    );
    // A profile edit bumps the config revision, which invalidates every session
    // on that connection; a grant pinned to the old revision must not survive it.
    let revoked = state
        .ai_grants
        .revoke_for_connection(connection_id.as_str())
        .await;
    if revoked > 0 {
        tracing::info!(revoked, "AI grants revoked by a profile revision change");
    }
    encode(view)
}
#[tauri::command]
pub async fn disable_connection(
    state: State<'_, AppState>,
    request: ProfileRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let connection_id = request.connection_id.clone();
    let view = ProfileView::from_record(
        &adapter
            .profiles()?
            .disable(call.context(), request.connection_id)
            .await
            .map_err(port_error)?,
    );
    let revoked = state
        .ai_grants
        .revoke_for_connection(connection_id.as_str())
        .await;
    if revoked > 0 {
        tracing::info!(
            revoked,
            "AI grants revoked because the profile was disabled"
        );
    }
    encode(view)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRequest {
    pub stream_id: StreamId,
    pub after_sequence: Option<Counter>,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DesktopEventMessage {
    Event { event: ConnectionEventEnvelope },
    Closed,
    Error { error: ApiError },
}
struct Subscription {
    client: ClientInstanceId,
    cancel: Arc<AtomicBool>,
}
fn subscriptions() -> &'static Mutex<HashMap<String, Subscription>> {
    static SUBSCRIPTIONS: OnceLock<Mutex<HashMap<String, Subscription>>> = OnceLock::new();
    SUBSCRIPTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
#[tauri::command]
pub async fn subscribe_events(
    state: State<'_, AppState>,
    request: SubscribeRequest,
    subscription_id: String,
    on_event: Channel<DesktopEventMessage>,
) -> Result<(), ApiError> {
    if uuid::Uuid::parse_str(&subscription_id).is_err() {
        return Err(ApiError::invalid_argument("invalid subscription id"));
    }
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let cancel = Arc::new(AtomicBool::new(false));
    let events = adapter
        .runtime()?
        .subscribe_events_cancellable(
            call.context(),
            request.stream_id,
            request.after_sequence,
            cancel.clone(),
        )
        .await?;
    {
        let mut subscriptions = subscriptions().lock().map_err(|_| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "subscriptions unavailable",
            )
        })?;
        if subscriptions.contains_key(&subscription_id) {
            return Err(ApiError::new(
                ApiErrorCode::ContextConflict,
                "subscription already exists",
            ));
        }
        subscriptions.insert(
            subscription_id.clone(),
            Subscription {
                client: call.context().client_instance_id.clone(),
                cancel: cancel.clone(),
            },
        );
    }
    tauri::async_runtime::spawn_blocking(move || {
        for event in events {
            if cancel.load(Ordering::Acquire)
                || on_event.send(DesktopEventMessage::Event { event }).is_err()
            {
                cancel.store(true, Ordering::Release);
                break;
            }
        }
        let _ = on_event.send(DesktopEventMessage::Closed);
        if let Ok(mut subscriptions) = subscriptions().lock() {
            subscriptions.remove(&subscription_id);
        }
    });
    Ok(())
}
#[tauri::command]
pub async fn stop_event_subscription(
    state: State<'_, AppState>,
    subscription_id: String,
) -> Result<(), ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let mut subscriptions = subscriptions().lock().map_err(|_| {
        ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            "subscriptions unavailable",
        )
    })?;
    if let Some(subscription) = subscriptions.get(&subscription_id) {
        if subscription.client != call.context().client_instance_id {
            return Err(ApiError::permission_denied("subscription not visible"));
        }
        subscription.cancel.store(true, Ordering::Release);
    }
    subscriptions.remove(&subscription_id);
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRequest {
    pub artifact_id: ArtifactId,
    pub chunk_index: Counter,
}
#[tauri::command]
pub async fn read_artifact(
    state: State<'_, AppState>,
    request: ArtifactRequest,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    let call = adapter.begin_call();
    let chunk = adapter
        .runtime()?
        .read_artifact_chunk(call.context(), request.artifact_id, request.chunk_index)
        .await?;
    let bytes=serde_json::to_vec(&serde_json::json!({"columns":chunk.columns,"rows":chunk.rows,"output":chunk.output,"source":chunk.source})).map_err(|_|ApiError::new(ApiErrorCode::OutcomeUnknown,"artifact encoding failed"))?;
    Ok(
        serde_json::json!({"artifactId":chunk.artifact_id,"chunkIndex":chunk.chunk_index,"totalChunks":if chunk.complete {Some(chunk.published_chunk_count)} else {None},"offset":"0","bytes":bytes,"publishedChunkCount":chunk.published_chunk_count,"publishedByteLength":chunk.published_byte_length,"resultCompleteness":if chunk.complete {"complete"}else{"pending"}}),
    )
}

/// 只读的池 / 预算观测快照。
///
/// 存在的理由不是顺便看看：CM-33 的 F 断言要在真实 UI 上证明「表格并发刷新不为每个
/// tab 建连接、并发资源不越预算」，而 pg_stat_activity 无法把宿主的旧会话与 runtime
/// 的会话分开。所以计数必须由 runtime 自己回答，走它自己的台账。
///
/// 载荷里没有凭据、没有连接串、没有原始库名：池键只是摘要（见 resource::observability）。
/// 未实现该端口后端返回 null，不是全零。
#[tauri::command]
pub async fn runtime_observability_snapshot(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, ApiError> {
    let adapter = adapter(&state)?;
    encode(adapter.runtime()?.observability_snapshot().await)
}
