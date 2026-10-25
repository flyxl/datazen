//! Explicit editor-session grants for AI tool execution (P6 §14.4 / CM-53).
//!
//! AI runs independently by default. A user may grant the current editor session;
//! tool calls then execute in that editor's live runtime session so temporary
//! objects stay visible. Grants are owner-bound, expiring and revocable, and a
//! rejected grant is an explicit error -- never a silent fallback.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen_application::sessions::ConnectionUseCases;
pub use datazen_feature_core::ai::ownership::AiGrantService;
use datazen_feature_core::ai::ownership::{
    verify_session_ownership, AiGrantIssue, AiGrantReceipt, AiGrantRejection, AiTargetClaim,
};
use datazen_feature_core::ai::tool_loop::AiToolLoopHost;
use datazen_feature_core::services::ports::DatabaseToolHost;
use datazen_feature_core::workflow::command::WorkflowCommandStep;
use datazen_platform_api::dto::session::SessionHandle;
use datazen_platform_api::id::{ClientInstanceId, EditorSessionId};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Named rejection message for the IPC / tool boundary.
pub(crate) fn rejection_message(rejection: AiGrantRejection) -> String {
    format!("{}: grant refused", rejection.code())
}

/// Grant the current editor session to AI. The claim is admitted through
/// OwnerIntent::Editor, so a client instance other than the desktop process
/// cannot mint a grant for someone else's tab.
#[tauri::command]
pub async fn ai_grant_editor_session(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    editor_session_id: String,
    claimed_client_instance_id: String,
    session_handle: SessionHandle,
) -> Result<AiGrantReceipt, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(|error| CommandError::Validation(error.message))?;
    let claimed_client_instance_id = ClientInstanceId::new(claimed_client_instance_id);
    let intent = crate::platform::identity::OwnerIntent::Editor {
        claimed_client_instance_id: claimed_client_instance_id.clone(),
        editor_session_id: EditorSessionId::new(editor_session_id.clone()),
    };
    let (call, owner) = adapter.begin_owned_call(&intent).map_err(|error| {
        CommandError::Validation(format!("{:?}: {}", error.code, error.message))
    })?;
    let runtime = adapter
        .runtime()
        .map_err(|error| CommandError::Validation(error.message))?;
    // The handle must name a live session, otherwise the grant would outlive it.
    let view = runtime
        .get_session(call.context(), session_handle.clone())
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "{}: session unavailable ({error})",
                AiGrantRejection::SessionGone.code()
            ))
        })?;

    let (config, metadata) = state
        .store
        .platform_profile(&connection_id)
        .await
        .ok_or_else(|| {
            CommandError::Validation(format!(
                "{}: unknown connection {connection_id}",
                AiGrantRejection::SessionGone.code()
            ))
        })?;
    if !metadata.enabled {
        return Err(CommandError::Validation(format!(
            "{}: connection disabled",
            AiGrantRejection::SessionGone.code()
        )));
    }
    // A live session is not enough: it has to be *this* editor's session on this
    // connection at the current profile revision (CM-53 forged-session rejection).
    verify_session_ownership(
        &view,
        claimed_client_instance_id.as_str(),
        &editor_session_id,
        &connection_id,
        metadata.config_revision,
    )
    .map_err(|rejection| CommandError::Validation(rejection_message(rejection)))?;

    let owner_fingerprint = match &owner {
        datazen_platform_api::context::OwnerRef::Editor {
            client_instance_id,
            editor_session_id,
        } => format!(
            "editor:{}:{}",
            client_instance_id.as_str(),
            editor_session_id.as_str()
        ),
        other => format!("owner:{other:?}"),
    };
    let record = AiGrantIssue {
        grant_id: uuid::Uuid::new_v4().to_string(),
        // Take the admitted identity directly; deriving it from the fingerprint
        // string would silently mis-bind if the fingerprint format changed.
        client_instance_id: claimed_client_instance_id.as_str().to_string(),
        editor_session_id,
        owner_fingerprint,
        connection_id: connection_id.clone(),
        database: database.or(config.database),
        config_revision: metadata.config_revision,
        handle: Some(session_handle),
    };
    Ok(state.ai_grants.issue(record, now_ms()).await)
}

/// Revoke one grant. Revocation is what makes "no longer shared" enforceable.
#[tauri::command]
pub async fn ai_revoke_editor_grant(
    state: State<'_, AppState>,
    grant_id: String,
) -> Result<bool, CommandError> {
    Ok(state.ai_grants.revoke(&grant_id).await)
}

/// Refresh a live grant's deadline; false means it already expired or was revoked.
#[tauri::command]
pub async fn ai_touch_editor_grant(
    state: State<'_, AppState>,
    grant_id: String,
) -> Result<bool, CommandError> {
    Ok(state.ai_grants.touch(&grant_id, now_ms()).await)
}

/// Per-request tool host: the claim decides which resource an AI tool may touch.
pub(crate) struct AiRequestHost<'a> {
    pub state: &'a AppState,
    pub claim: AiTargetClaim,
}

impl AiRequestHost<'_> {
    fn reject(&self, rejection: AiGrantRejection) -> String {
        rejection_message(rejection)
    }

    async fn execute_in_granted_session(
        &self,
        claim: &AiTargetClaim,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        let record = self
            .state
            .ai_grants
            .verify_claim(claim, now_ms())
            .await
            .map_err(|rejection| self.reject(rejection))?
            .ok_or_else(|| {
                format!(
                    "{}: claim carries no grant",
                    AiGrantRejection::Unknown.code()
                )
            })?;
        let handle = record.handle.clone().ok_or_else(|| {
            format!(
                "{}: grant has no live session",
                AiGrantRejection::SessionGone.code()
            )
        })?;
        // Bind the tool to the session the grant recorded. Missing database is
        // filled from the grant and conflicting targets keep the existing refusal code.
        let mut input = input;
        datazen_feature_core::ai::ownership::bind_database_to_grant(
            &mut input,
            record.database.as_deref(),
        )
        .map_err(|mismatch| {
            format!(
                "{}: tool target {} is outside the granted database {}",
                AiGrantRejection::SessionGone.code(),
                mismatch.requested_database,
                mismatch.granted_database
            )
        })?;
        let step = WorkflowCommandStep::new(
            "ai-tool",
            command,
            Some(record.connection_id.clone()),
            input,
        );
        // Borrow the editor's session: execute_with_existing_session never
        // closes it, so a successful tool call cannot roll back the editor.
        datazen_feature_core::workflow::resource_execution::execute_with_existing_session(
            self.state,
            &step,
            &record.connection_id,
            None,
            None,
            false,
            handle,
        )
        .await
        .map_err(|error| format!("aiGrantExecutionFailed: {error}"))
    }
}

#[async_trait::async_trait]
impl DatabaseToolHost for AiRequestHost<'_> {
    async fn get_session_config(
        &self,
        id: &str,
    ) -> Result<datazen_driver_api::ConnectionConfig, String> {
        DatabaseToolHost::get_session_config(self.state, id).await
    }

    async fn execution_settings(&self) -> datazen_modules::settings::ExecutionSettings {
        DatabaseToolHost::execution_settings(self.state).await
    }

    async fn driver_for_connection(
        &self,
        id: &str,
    ) -> Result<Arc<dyn datazen_driver_api::DatabaseDriver>, String> {
        DatabaseToolHost::driver_for_connection(self.state, id).await
    }

    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        match &self.claim {
            AiTargetClaim::Independent => {
                DatabaseToolHost::execute_target(self.state, id, command, input).await
            }
            claim @ AiTargetClaim::EditorGrant { .. } => {
                self.execute_in_granted_session(claim, command, input).await
            }
        }
    }
}

#[async_trait::async_trait]
impl AiToolLoopHost for AiRequestHost<'_> {
    async fn egress_summary(&self) -> String {
        AiToolLoopHost::egress_summary(self.state).await
    }

    async fn execute_db_tool(&self, call: &datazen_ai_api::ToolCall) -> String {
        let raw =
            datazen_feature_core::ai::tools::execute_db_tool(self, self.state.store.as_ref(), call)
                .await;
        datazen_feature_core::ai::ownership::attach_tool_attribution(raw, &self.claim)
    }

    async fn execute_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> String {
        AiToolLoopHost::execute_mcp_tool(self.state, server, tool, arguments).await
    }
}
