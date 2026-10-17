//! Explicit editor-session grants for AI tool execution (P6 §14.4 / CM-53).
//!
//! AI runs independently by default. A user may grant the current editor session;
//! tool calls then execute in that editor's live runtime session so temporary
//! objects stay visible. Grants are owner-bound, expiring and revocable, and a
//! rejected grant is an explicit error -- never a silent fallback.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen_application::sessions::ConnectionUseCases;
use datazen_feature_core::ai::ownership::{
    AiGrantIssue, AiGrantReceipt, AiGrantRegistry, AiGrantRejection, AiTargetClaim,
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

/// Verify that a requested editor handle really is the caller's editor session
/// on the requested connection.
///
/// The runtime only filters sessions by organization/principal/client instance,
/// so two panes of the same desktop process are indistinguishable there. The
/// grant needs the stricter question answered: is this handle *that* editor, on
/// *that* connection, at the revision the profile still has?
pub(crate) fn verify_session_ownership(
    view: &datazen_platform_api::dto::session::SessionView,
    claimed_client_instance_id: &str,
    editor_session_id: &str,
    connection_id: &str,
    profile_revision: u64,
) -> Result<(), AiGrantRejection> {
    match &view.owner {
        datazen_platform_api::context::OwnerRef::Editor {
            client_instance_id,
            editor_session_id: owner_editor,
        } => {
            if client_instance_id.as_str() != claimed_client_instance_id {
                return Err(AiGrantRejection::ClientMismatch);
            }
            if owner_editor.as_str() != editor_session_id {
                return Err(AiGrantRejection::SessionGone);
            }
        }
        // A session owned by a job/block/opaque client is not an editor session,
        // so it must never back an editor grant.
        _ => return Err(AiGrantRejection::SessionGone),
    }
    if view.connection_id.as_str() != connection_id {
        return Err(AiGrantRejection::SessionGone);
    }
    if view.config_revision.0 != profile_revision {
        // The session was opened against an older profile revision; granting it
        // would execute on stale configuration.
        return Err(AiGrantRejection::SessionGone);
    }
    Ok(())
}

/// Desktop facade around the portable grant registry.
#[derive(Clone, Default)]
pub struct AiGrantService {
    registry: Arc<AiGrantRegistry>,
}

impl AiGrantService {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(AiGrantRegistry::default()),
        }
    }

    pub async fn issue(&self, issue: AiGrantIssue, now_ms: i64) -> AiGrantReceipt {
        self.registry.purge_expired(now_ms).await;
        self.registry.issue(issue, now_ms).await
    }

    pub async fn verify_claim(
        &self,
        claim: &AiTargetClaim,
        now_ms: i64,
    ) -> Result<Option<datazen_feature_core::ai::ownership::AiGrantRecord>, AiGrantRejection> {
        // Cheap opportunistic reclamation: every dispatch already pays a lock.
        self.registry.purge_expired(now_ms).await;
        self.registry.verify_claim(claim, now_ms).await
    }

    pub async fn touch(&self, grant_id: &str, now_ms: i64) -> bool {
        self.registry.touch(grant_id, now_ms).await
    }

    pub async fn revoke(&self, grant_id: &str) -> bool {
        self.registry.revoke(grant_id).await
    }

    /// Reclaim expired records. Called on the same paths that verify claims so
    /// a desktop left running for days does not accumulate dead grants.
    pub async fn purge_expired(&self, now_ms: i64) -> usize {
        self.registry.purge_expired(now_ms).await
    }

    pub async fn revoke_for_connection(&self, connection_id: &str) -> usize {
        self.registry.revoke_for_connection(connection_id).await
    }

    pub async fn revoke_for_session_handle(&self, db_session_id: &str) -> usize {
        self.registry.revoke_for_session_handle(db_session_id).await
    }

    pub async fn revoke_for_editor_session(&self, editor_session_id: &str) -> usize {
        self.registry
            .revoke_for_editor_session(editor_session_id)
            .await
    }

    pub async fn len(&self) -> usize {
        self.registry.len().await
    }
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

    fn with_attribution(&self, raw: String) -> String {
        let attribution = match &self.claim {
            AiTargetClaim::Independent => serde_json::json!({ "kind": "independent" }),
            AiTargetClaim::EditorGrant {
                grant_id,
                editor_session_id,
                ..
            } => serde_json::json!({
                "kind": "editorGrant",
                "grantId": grant_id,
                "editorSessionId": editor_session_id,
            }),
        };
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(serde_json::Value::Object(mut object)) => {
                object.insert("attribution".into(), attribution);
                serde_json::Value::Object(object).to_string()
            }
            _ => raw,
        }
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
        // filled from the grant so a tool cannot silently retarget itself; a
        // database the user typed is kept only when it is the granted one.
        let mut input = input;
        if let Some(object) = input.as_object_mut() {
            let requested = object
                .get("database")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            match (requested, record.database.as_deref()) {
                (Some(requested), Some(granted)) if requested != granted => {
                    return Err(format!(
                        "{}: tool target {requested} is outside the granted database {granted}",
                        AiGrantRejection::SessionGone.code()
                    ));
                }
                (None, Some(granted)) => {
                    object.insert(
                        "database".into(),
                        serde_json::Value::String(granted.to_owned()),
                    );
                }
                _ => {}
            }
        }
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
    async fn resolve_session_for_connection(
        &self,
        id: &str,
    ) -> Result<
        (
            String,
            Arc<dyn datazen_driver_api::DatabaseDriver>,
            datazen_driver_api::ConnectionHandle,
        ),
        String,
    > {
        DatabaseToolHost::resolve_session_for_connection(self.state, id).await
    }

    async fn get_session_config(
        &self,
        id: &str,
    ) -> Result<datazen_driver_api::ConnectionConfig, String> {
        DatabaseToolHost::get_session_config(self.state, id).await
    }

    async fn execution_settings(&self) -> datazen_platform_api::settings::ExecutionSettings {
        DatabaseToolHost::execution_settings(self.state).await
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
        self.with_attribution(raw)
    }

    async fn execute_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> String {
        AiToolLoopHost::execute_mcp_tool(self.state, server, tool, arguments).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::context::OwnerRef;
    use datazen_platform_api::dto::session::SessionView;
    use datazen_platform_api::id::{
        ClientInstanceId, ConnectionId, Counter, DbSessionId, EditorSessionId, RuntimeEpoch,
    };

    fn view(owner: OwnerRef, connection_id: &str, config_revision: u64) -> SessionView {
        let namespace = datazen_platform_api::target::NamespaceTarget::new(
            Some("app".into()),
            None,
            None,
            vec![],
        );
        SessionView {
            handle: datazen_platform_api::dto::session::SessionHandle::new(
                DbSessionId::new("dbs-1"),
                RuntimeEpoch::new("1"),
            ),
            connection_id: ConnectionId::new(connection_id),
            config_revision: Counter::new(config_revision),
            owner,
            initial_target: datazen_platform_api::target::ExecutionTarget::new(
                ConnectionId::new(connection_id),
                namespace.clone(),
            ),
            observed_context: datazen_platform_api::dto::session::SessionContext {
                namespace,
                search_path: None,
                effective_identity: None,
                transaction_state: datazen_platform_api::dto::session::TransactionState::None,
                autocommit: None,
                confidence: datazen_platform_api::dto::session::ContextConfidence::Confirmed,
            },
            context_revision: Counter::new(1),
            state: datazen_platform_api::dto::session::SessionState::Ready,
            attachment_state: datazen_platform_api::dto::session::AttachmentState::Attached,
            active_execution_id: None,
            expires_at: None,
        }
    }

    fn editor_owner(client: &str, editor: &str) -> OwnerRef {
        OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new(client),
            editor_session_id: EditorSessionId::new(editor),
        }
    }

    #[test]
    fn matching_editor_session_is_accepted() {
        let session = view(editor_owner("client-a", "pane-1"), "conn-1", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Ok(())
        );
    }

    #[test]
    fn another_pane_of_the_same_client_is_rejected() {
        // The runtime only filters by client instance, so this is exactly the
        // forgery the grant has to catch by itself (CM-53).
        let session = view(editor_owner("client-a", "pane-2"), "conn-1", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::SessionGone)
        );
    }

    #[test]
    fn another_client_is_rejected() {
        let session = view(editor_owner("client-b", "pane-1"), "conn-1", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::ClientMismatch)
        );
    }

    #[test]
    fn session_on_another_connection_is_rejected() {
        let session = view(editor_owner("client-a", "pane-1"), "conn-2", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::SessionGone)
        );
    }

    #[test]
    fn stale_config_revision_is_rejected() {
        let session = view(editor_owner("client-a", "pane-1"), "conn-1", 6);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::SessionGone)
        );
    }

    #[test]
    fn non_editor_owned_session_is_rejected() {
        let session = view(
            OwnerRef::ClientSession {
                client_instance_id: ClientInstanceId::new("client-a"),
                purpose: "mcp".into(),
            },
            "conn-1",
            7,
        );
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::SessionGone)
        );
    }
}
