use std::{collections::HashMap, sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}}};
use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, State};
use datazen_application::{dto::requests::*, error::{ApiError, ApiErrorCode}, sessions::ConnectionUseCases};
use datazen_platform_api::{context::OwnerRef, dto::{event::ConnectionEventEnvelope, idempotency::IdempotentOperation, profile::{ProfileDraft, ProfilePatch}}, id::*, ports::profile::ProfileRepository};
use crate::commands::AppState;

fn adapter(state: &AppState) -> Result<&Arc<super::PlatformAdapter>,ApiError> { state.platform_require() }
fn encode<T:Serialize>(value:T) -> Result<serde_json::Value,ApiError> { serde_json::to_value(value).map_err(|_| ApiError::new(ApiErrorCode::OutcomeUnknown,"response serialization failed")) }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct HandleRequest { pub handle: datazen_platform_api::dto::session::SessionHandle }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ExecutionRequest { pub execution_id: ExecutionId }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct TokenRequest { pub operation: IdempotentOperation, pub handle: Option<datazen_platform_api::dto::session::SessionHandle>, pub scope: Option<TokenScope>, pub connection_id: Option<ConnectionId>, pub owner: Option<OwnerRef> }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct TokenScope { pub handle: Option<datazen_platform_api::dto::session::SessionHandle> }
#[tauri::command]
pub async fn get_platform_identity(state: State<'_,AppState>) -> Result<serde_json::Value,ApiError> {
    let call = adapter(&state)?.begin_call(); let ctx=call.context();
    Ok(serde_json::json!({"clientInstanceId":ctx.client_instance_id,"organizationId":ctx.organization_id,"principalId":ctx.principal_id}))
}
#[tauri::command]
pub async fn issue_submission_token(state: State<'_,AppState>, request: TokenRequest) -> Result<serde_json::Value,ApiError> {
    let adapter=adapter(&state)?; let call=adapter.begin_call();
    let _ = (&request.connection_id,&request.owner);
    let handle=request.handle.or_else(|| request.scope.and_then(|scope| scope.handle));
    encode(adapter.runtime()?.issue_submission_token(call.context(),request.operation,handle.as_ref()).await?)
}
macro_rules! session_command {
    ($name:ident,$request:ty) => {
        #[tauri::command]
        pub async fn $name(state:State<'_,AppState>,request:$request)->Result<serde_json::Value,ApiError> {
            let adapter=adapter(&state)?; let call=adapter.begin_call();
            encode(adapter.runtime()?.$name(call.context(),request).await?)
        }
    }
}
session_command!(open_session,OpenSessionRequest);
session_command!(execute_in_session,ExecuteInSessionRequest);
session_command!(execute_at_target,ExecuteAtTargetRequest);
session_command!(set_session_context,SetSessionContextRequest);
session_command!(close_session,CloseSessionRequest);
session_command!(attach_session,AttachmentRequest);
session_command!(detach_session,AttachmentRequest);
#[tauri::command]
pub async fn get_session(state:State<'_,AppState>,request:HandleRequest)->Result<serde_json::Value,ApiError> { let adapter=adapter(&state)?;let call=adapter.begin_call();encode(adapter.runtime()?.get_session(call.context(),request.handle).await?) }
#[tauri::command]
pub async fn get_execution(state:State<'_,AppState>,request:ExecutionRequest)->Result<serde_json::Value,ApiError> { let adapter=adapter(&state)?;let call=adapter.begin_call();encode(adapter.runtime()?.get_execution(call.context(),request.execution_id).await?) }
#[tauri::command]
pub async fn cancel_execution(state:State<'_,AppState>,request:ExecutionRequest)->Result<serde_json::Value,ApiError> { let adapter=adapter(&state)?;let call=adapter.begin_call();encode(adapter.runtime()?.cancel_execution(call.context(),request.execution_id).await?) }

#[tauri::command]
pub async fn platform_list_profiles(state:State<'_,AppState>,request:serde_json::Value)->Result<serde_json::Value,ApiError> { let _=request;let adapter=adapter(&state)?;let call=adapter.begin_call();encode(adapter.runtime()?.list_connections(call.context()).await?) }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct CreateProfileRequest { pub draft: ProfileDraft, pub idempotency_key: IdempotencyKey }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct UpdateProfileRequest { pub connection_id: ConnectionId, pub expected_config_revision: ConfigRevision, pub patch: ProfilePatch }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProfileRequest { pub connection_id: ConnectionId }
fn port_error(error:datazen_platform_api::error::PortError)->ApiError { match error {
    datazen_platform_api::error::PortError::CasConflict { .. } => ApiError::new(ApiErrorCode::ConfigRevisionMismatch,"profile revision changed"),
    datazen_platform_api::error::PortError::NotFound(_) => ApiError::new(ApiErrorCode::NotFound,"profile unavailable"),
    _ => ApiError::new(ApiErrorCode::ServiceUnavailable,"profile operation failed"),
} }
#[tauri::command]
pub async fn platform_create_profile(state:State<'_,AppState>,request:CreateProfileRequest)->Result<serde_json::Value,ApiError> { let adapter=adapter(&state)?;let call=adapter.begin_call();encode(adapter.profiles()?.create(call.context(),request.draft,&request.idempotency_key).await.map_err(port_error)?.to_view()) }
#[tauri::command]
pub async fn platform_update_profile(state:State<'_,AppState>,request:UpdateProfileRequest)->Result<serde_json::Value,ApiError> { let adapter=adapter(&state)?;let call=adapter.begin_call();encode(adapter.profiles()?.compare_and_set(call.context(),request.connection_id,request.expected_config_revision,request.patch).await.map_err(port_error)?.to_view()) }
#[tauri::command]
pub async fn platform_disable_profile(state:State<'_,AppState>,request:ProfileRequest)->Result<serde_json::Value,ApiError> { let adapter=adapter(&state)?;let call=adapter.begin_call();encode(adapter.profiles()?.disable(call.context(),request.connection_id).await.map_err(port_error)?.to_view()) }

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SubscribeRequest { pub stream_id: StreamId, pub after_sequence: Option<Counter> }
#[derive(Serialize)]
#[serde(tag="kind",rename_all="camelCase")]
pub enum DesktopEventMessage { Event {event:ConnectionEventEnvelope}, Closed, Error {error:ApiError} }
struct Subscription { client:ClientInstanceId, cancel:Arc<AtomicBool> }
fn subscriptions()->&'static Mutex<HashMap<String,Subscription>> { static SUBSCRIPTIONS:OnceLock<Mutex<HashMap<String,Subscription>>>=OnceLock::new();SUBSCRIPTIONS.get_or_init(||Mutex::new(HashMap::new())) }
#[tauri::command]
pub async fn subscribe_events(state:State<'_,AppState>,request:SubscribeRequest,subscription_id:String,on_event:Channel<DesktopEventMessage>)->Result<(),ApiError> {
    if uuid::Uuid::parse_str(&subscription_id).is_err() {return Err(ApiError::invalid_argument("invalid subscription id"));}
    let adapter=adapter(&state)?;let call=adapter.begin_call();let cancel=Arc::new(AtomicBool::new(false));
    let events=adapter.runtime()?.subscribe_events_cancellable(call.context(),request.stream_id,request.after_sequence,cancel.clone()).await?;
    { let mut subscriptions=subscriptions().lock().map_err(|_|ApiError::new(ApiErrorCode::ServiceUnavailable,"subscriptions unavailable"))?;
      if subscriptions.contains_key(&subscription_id) {return Err(ApiError::new(ApiErrorCode::ContextConflict,"subscription already exists"));}
      subscriptions.insert(subscription_id.clone(),Subscription{client:call.context().client_instance_id.clone(),cancel:cancel.clone()}); }
    tauri::async_runtime::spawn_blocking(move || {
        for event in events { if cancel.load(Ordering::Acquire)||on_event.send(DesktopEventMessage::Event{event}).is_err(){cancel.store(true,Ordering::Release);break;} }
        let _=on_event.send(DesktopEventMessage::Closed);
        if let Ok(mut subscriptions)=subscriptions().lock(){subscriptions.remove(&subscription_id);}
    });Ok(())
}
#[tauri::command]
pub async fn stop_event_subscription(state:State<'_,AppState>,subscription_id:String)->Result<(),ApiError> {
    let adapter=adapter(&state)?;let call=adapter.begin_call();
    let mut subscriptions=subscriptions().lock().map_err(|_|ApiError::new(ApiErrorCode::ServiceUnavailable,"subscriptions unavailable"))?;
    if let Some(subscription)=subscriptions.get(&subscription_id) {if subscription.client!=call.context().client_instance_id {return Err(ApiError::permission_denied("subscription not visible"));}subscription.cancel.store(true,Ordering::Release);}
    subscriptions.remove(&subscription_id);Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ArtifactRequest {pub artifact_id:ArtifactId,pub chunk_index:Counter}
#[tauri::command]
pub async fn read_artifact_chunk(state:State<'_,AppState>,request:ArtifactRequest)->Result<serde_json::Value,ApiError> {
    let adapter=adapter(&state)?;let call=adapter.begin_call();let chunk=adapter.runtime()?.read_artifact_chunk(call.context(),request.artifact_id,request.chunk_index).await?;
    let bytes=serde_json::to_vec(&serde_json::json!({"columns":chunk.columns,"rows":chunk.rows,"output":chunk.output,"source":chunk.source})).map_err(|_|ApiError::new(ApiErrorCode::OutcomeUnknown,"artifact encoding failed"))?;
    Ok(serde_json::json!({"artifactId":chunk.artifact_id,"chunkIndex":chunk.chunk_index,"totalChunks":if chunk.complete {Some(chunk.published_chunk_count)} else {None},"offset":"0","bytes":bytes,"publishedChunkCount":chunk.published_chunk_count,"publishedByteLength":chunk.published_byte_length,"resultCompleteness":if chunk.complete {"complete"}else{"pending"}}))
}
