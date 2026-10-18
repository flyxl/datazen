//! AI invocation ownership: explicit editor-session grants for AI tool execution.
//!
//! P6 §14.4: AI operates **independently** by default. Only when the user
//! explicitly grants access to the current editor session may an AI tool run on
//! that editor's fixed runtime session (same queue, temporary objects visible).
//!
//! Grants are opaque, owner-bound, expiring and revocable. Every failure mode is
//! a named rejection instead of a silent fallback to a different resource: a
//! forged, expired or revoked grant never degrades into "independent" execution.

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::session::{SessionHandle, SessionView};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Default grant lifetime. A live conversation refreshes the grant through
/// [AiGrantRegistry::touch]; an idle one lets it expire.
pub const DEFAULT_GRANT_TTL_MS: i64 = 15 * 60 * 1000;

/// How one AI request claims database access.
///
/// Serialized shape is the IPC boundary shared with the frontend; field names
/// are frozen ("kind" tag, camelCase).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum AiTargetClaim {
    /// Default: run on a one-shot resource through the shared execution gateway.
    Independent,
    /// Explicit user grant: run on the editor's fixed runtime session.
    EditorGrant {
        grant_id: String,
        claimed_client_instance_id: String,
        editor_session_id: String,
    },
}

/// What the host hands back to the frontend after a successful grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiGrantReceipt {
    pub grant_id: String,
    pub connection_id: String,
    pub database: Option<String>,
    pub config_revision: u64,
    pub expires_at_ms: i64,
}

/// Why a claim was refused. Callers map this to an explicit error; none of the
/// variants is allowed to fall back to another execution path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiGrantRejection {
    /// No such grant was ever issued (or it was purged after expiry).
    Unknown,
    /// The claim comes from a client instance that does not own the grant.
    ClientMismatch,
    /// The grant names a different editor session than the claim.
    SessionGone,
    /// The grant outlived its deadline without being refreshed.
    Expired,
    /// The grant was explicitly revoked (user toggle, editor close, config change).
    Revoked,
}

/// Confirm that a runtime handle belongs to the editor requesting a grant,
/// against the current profile revision.
///
/// Runtime session lookup is scoped to the client instance, so it cannot
/// distinguish two panes in the same desktop process. This policy checks the
/// editor-session identity as well as the connection and config revision before
/// the host records a grant.
pub fn verify_session_ownership(
    view: &SessionView,
    claimed_client_instance_id: &str,
    editor_session_id: &str,
    connection_id: &str,
    profile_revision: u64,
) -> Result<(), AiGrantRejection> {
    match &view.owner {
        OwnerRef::Editor {
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

impl AiGrantRejection {
    /// Stable machine-readable code for the IPC error payload.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unknown => "aiGrantUnknown",
            Self::ClientMismatch => "aiGrantClientMismatch",
            Self::SessionGone => "aiGrantSessionGone",
            Self::Expired => "aiGrantExpired",
            Self::Revoked => "aiGrantRevoked",
        }
    }
}

/// Inputs required to mint a grant. The host resolves these from an admitted
/// OwnerIntent::Editor, never from request-body fields.
#[derive(Debug, Clone)]
pub struct AiGrantIssue {
    pub grant_id: String,
    pub client_instance_id: String,
    pub editor_session_id: String,
    pub owner_fingerprint: String,
    pub connection_id: String,
    pub database: Option<String>,
    pub config_revision: u64,
    /// The editor's live runtime session. Tool calls under this grant run in it
    /// so temporary objects created by the editor stay visible (CM-53).
    pub handle: Option<SessionHandle>,
}

/// Verified grant contents handed to the execution path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiGrantRecord {
    pub grant_id: String,
    pub client_instance_id: String,
    pub editor_session_id: String,
    pub owner_fingerprint: String,
    pub connection_id: String,
    pub database: Option<String>,
    pub config_revision: u64,
    pub deadline_ms: i64,
    pub handle: Option<SessionHandle>,
}

#[derive(Debug, Clone)]
struct GrantState {
    record: AiGrantRecord,
    revoked: bool,
}

/// Transport-neutral grant registry. Time is injected so tests do not depend on
/// the wall clock, and the type carries no host handles.
#[derive(Clone)]
pub struct AiGrantRegistry {
    ttl_ms: i64,
    inner: Arc<Mutex<HashMap<String, GrantState>>>,
}

impl Default for AiGrantRegistry {
    fn default() -> Self {
        Self::new(DEFAULT_GRANT_TTL_MS)
    }
}

impl AiGrantRegistry {
    pub fn new(ttl_ms: i64) -> Self {
        Self {
            ttl_ms: ttl_ms.max(1),
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Mint a grant. Re-issuing the same id replaces the old record and clears
    /// any revocation, which keeps a retried grant request idempotent.
    pub async fn issue(&self, issue: AiGrantIssue, now_ms: i64) -> AiGrantReceipt {
        let deadline_ms = now_ms.saturating_add(self.ttl_ms);
        let record = AiGrantRecord {
            grant_id: issue.grant_id,
            client_instance_id: issue.client_instance_id,
            editor_session_id: issue.editor_session_id,
            owner_fingerprint: issue.owner_fingerprint,
            connection_id: issue.connection_id,
            database: issue.database,
            config_revision: issue.config_revision,
            deadline_ms,
            handle: issue.handle,
        };
        let receipt = AiGrantReceipt {
            grant_id: record.grant_id.clone(),
            connection_id: record.connection_id.clone(),
            database: record.database.clone(),
            config_revision: record.config_revision,
            expires_at_ms: deadline_ms,
        };
        self.inner.lock().await.insert(
            record.grant_id.clone(),
            GrantState {
                record,
                revoked: false,
            },
        );
        receipt
    }

    /// Resolve a claim. Independent is not a grant and yields Ok(None);
    /// every EditorGrant failure is a named rejection.
    pub async fn verify_claim(
        &self,
        claim: &AiTargetClaim,
        now_ms: i64,
    ) -> Result<Option<AiGrantRecord>, AiGrantRejection> {
        match claim {
            AiTargetClaim::Independent => Ok(None),
            AiTargetClaim::EditorGrant {
                grant_id,
                claimed_client_instance_id,
                editor_session_id,
            } => self
                .verify(
                    grant_id,
                    claimed_client_instance_id,
                    editor_session_id,
                    now_ms,
                )
                .await
                .map(Some),
        }
    }

    /// Verify one grant against the claim's identity fields. Every field is
    /// checked by name so a mismatch names the violated field in the error code.
    pub async fn verify(
        &self,
        grant_id: &str,
        claimed_client_instance_id: &str,
        editor_session_id: &str,
        now_ms: i64,
    ) -> Result<AiGrantRecord, AiGrantRejection> {
        let guard = self.inner.lock().await;
        let state = guard.get(grant_id).ok_or(AiGrantRejection::Unknown)?;
        if state.revoked {
            return Err(AiGrantRejection::Revoked);
        }
        if state.record.client_instance_id != claimed_client_instance_id {
            return Err(AiGrantRejection::ClientMismatch);
        }
        if state.record.editor_session_id != editor_session_id {
            return Err(AiGrantRejection::SessionGone);
        }
        if now_ms >= state.record.deadline_ms {
            return Err(AiGrantRejection::Expired);
        }
        Ok(state.record.clone())
    }

    /// Extend a live grant. Returns false for unknown, revoked or expired
    /// grants so the caller can drop back to independent operation explicitly.
    pub async fn touch(&self, grant_id: &str, now_ms: i64) -> bool {
        let mut guard = self.inner.lock().await;
        let Some(state) = guard.get_mut(grant_id) else {
            return false;
        };
        if state.revoked || now_ms >= state.record.deadline_ms {
            return false;
        }
        state.record.deadline_ms = now_ms.saturating_add(self.ttl_ms);
        true
    }

    /// Revoke one grant. The record is kept so later claims report Revoked
    /// rather than Unknown; [AiGrantRegistry::purge_expired] reclaims it later.
    pub async fn revoke(&self, grant_id: &str) -> bool {
        let mut guard = self.inner.lock().await;
        match guard.get_mut(grant_id) {
            Some(state) if !state.revoked => {
                state.revoked = true;
                true
            }
            _ => false,
        }
    }

    /// Revoke every grant bound to an editor session. Called when the session
    /// reaches a terminal state (CM-53: the editor's own resources stay alive).
    pub async fn revoke_for_editor_session(&self, editor_session_id: &str) -> usize {
        let mut guard = self.inner.lock().await;
        let mut revoked = 0;
        for state in guard.values_mut() {
            if state.record.editor_session_id == editor_session_id && !state.revoked {
                state.revoked = true;
                revoked += 1;
            }
        }
        revoked
    }

    /// Revoke every grant that borrowed one database session. Closing a session
    /// must not leave a grant able to outlive the handle it points at.
    pub async fn revoke_for_session_handle(&self, db_session_id: &str) -> usize {
        let mut guard = self.inner.lock().await;
        let mut revoked = 0;
        for state in guard.values_mut() {
            let borrowed = state
                .record
                .handle
                .as_ref()
                .is_some_and(|handle| handle.db_session_id.as_str() == db_session_id);
            if borrowed && !state.revoked {
                state.revoked = true;
                revoked += 1;
            }
        }
        revoked
    }

    /// Revoke every grant bound to one connection. A profile edit or a session
    /// teardown on that connection must not leave a grant pointing at it.
    pub async fn revoke_for_connection(&self, connection_id: &str) -> usize {
        let mut guard = self.inner.lock().await;
        let mut revoked = 0;
        for state in guard.values_mut() {
            if state.record.connection_id == connection_id && !state.revoked {
                state.revoked = true;
                revoked += 1;
            }
        }
        revoked
    }

    /// Drop records whose deadline passed. Revoked records are retained until
    /// their original deadline so a late claim still reports Revoked.
    pub async fn purge_expired(&self, now_ms: i64) -> usize {
        let mut guard = self.inner.lock().await;
        let before = guard.len();
        guard.retain(|_, state| now_ms < state.record.deadline_ms);
        before - guard.len()
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn revoked_count(&self) -> usize {
        self.inner
            .lock()
            .await
            .values()
            .filter(|state| state.revoked)
            .count()
    }
}

/// Shared grant lifecycle policy used by host adapters.
///
/// The service reclaims expired records on issue and verification, while
/// preserving `Expired` for the request that discovers its own expiry.
#[derive(Clone, Default)]
pub struct AiGrantService {
    registry: AiGrantRegistry,
}

impl AiGrantService {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn issue(&self, issue: AiGrantIssue, now_ms: i64) -> AiGrantReceipt {
        self.registry.purge_expired(now_ms).await;
        self.registry.issue(issue, now_ms).await
    }

    pub async fn verify_claim(
        &self,
        claim: &AiTargetClaim,
        now_ms: i64,
    ) -> Result<Option<AiGrantRecord>, AiGrantRejection> {
        let result = self.registry.verify_claim(claim, now_ms).await;
        self.registry.purge_expired(now_ms).await;
        result
    }

    pub async fn touch(&self, grant_id: &str, now_ms: i64) -> bool {
        self.registry.touch(grant_id, now_ms).await
    }

    pub async fn revoke(&self, grant_id: &str) -> bool {
        self.registry.revoke(grant_id).await
    }

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

    pub async fn is_empty(&self) -> bool {
        self.registry.is_empty().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::context::OwnerRef;
    use datazen_platform_api::dto::session::{
        AttachmentState, ContextConfidence, SessionContext, SessionState, TransactionState,
    };
    use datazen_platform_api::id::{
        ClientInstanceId, ConnectionId, Counter, DbSessionId, EditorSessionId, RuntimeEpoch,
    };
    use datazen_platform_api::target::{ExecutionTarget, NamespaceTarget};

    const TTL: i64 = 60_000;

    fn issue(grant_id: &str, client: &str, editor: &str) -> AiGrantIssue {
        AiGrantIssue {
            grant_id: grant_id.into(),
            client_instance_id: client.into(),
            editor_session_id: editor.into(),
            owner_fingerprint: format!("editor:{client}:{editor}"),
            connection_id: "conn-1".into(),
            database: Some("app".into()),
            config_revision: 7,
            handle: None,
        }
    }

    fn session_view(owner: OwnerRef, connection_id: &str, config_revision: u64) -> SessionView {
        let namespace = NamespaceTarget::new(Some("app".into()), None, None, vec![]);
        SessionView {
            handle: SessionHandle::new(DbSessionId::new("dbs-1"), RuntimeEpoch::new("1")),
            connection_id: ConnectionId::new(connection_id),
            config_revision: Counter::new(config_revision),
            owner,
            initial_target: ExecutionTarget::new(
                ConnectionId::new(connection_id),
                namespace.clone(),
            ),
            observed_context: SessionContext {
                namespace,
                search_path: None,
                effective_identity: None,
                transaction_state: TransactionState::None,
                autocommit: None,
                confidence: ContextConfidence::Confirmed,
            },
            context_revision: Counter::new(1),
            state: SessionState::Ready,
            attachment_state: AttachmentState::Attached,
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

    #[tokio::test]
    async fn issue_returns_matching_receipt_and_verifies() {
        let registry = AiGrantRegistry::new(TTL);
        let receipt = registry
            .issue(issue("g1", "client-a", "pane-1"), 1_000)
            .await;
        assert_eq!(receipt.grant_id, "g1");
        assert_eq!(receipt.connection_id, "conn-1");
        assert_eq!(receipt.database.as_deref(), Some("app"));
        assert_eq!(receipt.config_revision, 7);
        assert_eq!(receipt.expires_at_ms, 1_000 + TTL);

        let record = registry
            .verify("g1", "client-a", "pane-1", 2_000)
            .await
            .expect("live grant verifies");
        assert_eq!(record.owner_fingerprint, "editor:client-a:pane-1");
        assert_eq!(record.config_revision, 7);
    }

    #[tokio::test]
    async fn independent_claim_is_not_a_grant() {
        let registry = AiGrantRegistry::new(TTL);
        let resolved = registry
            .verify_claim(&AiTargetClaim::Independent, 0)
            .await
            .expect("independent is always admissible");
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn verify_rejects_unknown_client_mismatch_and_session_mismatch() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("g1", "client-a", "pane-1"), 0).await;

        assert_eq!(
            registry.verify("nope", "client-a", "pane-1", 0).await,
            Err(AiGrantRejection::Unknown)
        );
        assert_eq!(
            registry.verify("g1", "client-b", "pane-1", 0).await,
            Err(AiGrantRejection::ClientMismatch)
        );
        assert_eq!(
            registry.verify("g1", "client-a", "pane-2", 0).await,
            Err(AiGrantRejection::SessionGone)
        );
    }

    #[tokio::test]
    async fn verify_rejects_after_deadline_without_touch() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("g1", "client-a", "pane-1"), 0).await;
        assert!(registry
            .verify("g1", "client-a", "pane-1", TTL - 1)
            .await
            .is_ok());
        assert_eq!(
            registry.verify("g1", "client-a", "pane-1", TTL).await,
            Err(AiGrantRejection::Expired)
        );
    }

    #[tokio::test]
    async fn touch_extends_the_deadline_and_rejects_unknown() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("g1", "client-a", "pane-1"), 0).await;
        assert!(registry.touch("g1", 10_000).await);
        assert!(registry
            .verify("g1", "client-a", "pane-1", 10_000 + TTL - 1)
            .await
            .is_ok());
        assert_eq!(
            registry
                .verify("g1", "client-a", "pane-1", 10_000 + TTL)
                .await,
            Err(AiGrantRejection::Expired)
        );
        assert!(!registry.touch("missing", 10_000).await);
    }

    #[tokio::test]
    async fn revoked_grants_report_revoked_not_unknown() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("g1", "client-a", "pane-1"), 0).await;
        assert!(registry.revoke("g1").await);
        assert!(!registry.revoke("g1").await, "double revoke is a no-op");
        assert_eq!(
            registry.verify("g1", "client-a", "pane-1", 1).await,
            Err(AiGrantRejection::Revoked)
        );
    }

    #[tokio::test]
    async fn revoke_for_editor_session_only_hits_that_session() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("g1", "client-a", "pane-1"), 0).await;
        registry.issue(issue("g2", "client-a", "pane-1"), 0).await;
        registry.issue(issue("g3", "client-a", "pane-2"), 0).await;

        assert_eq!(registry.revoke_for_editor_session("pane-1").await, 2);
        assert_eq!(
            registry.verify("g1", "client-a", "pane-1", 1).await,
            Err(AiGrantRejection::Revoked)
        );
        assert_eq!(
            registry.verify("g2", "client-a", "pane-1", 1).await,
            Err(AiGrantRejection::Revoked)
        );
        assert!(registry.verify("g3", "client-a", "pane-2", 1).await.is_ok());
        assert_eq!(registry.revoked_count().await, 2);
    }

    #[tokio::test]
    async fn issue_is_idempotent_for_the_same_id() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("g1", "client-a", "pane-1"), 0).await;
        registry.revoke("g1").await;
        registry.issue(issue("g1", "client-a", "pane-1"), 5).await;
        assert!(registry.verify("g1", "client-a", "pane-1", 6).await.is_ok());
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn revoke_for_session_handle_only_hits_grants_that_borrowed_it() {
        use datazen_platform_api::id::{DbSessionId, RuntimeEpoch};
        let registry = AiGrantRegistry::new(TTL);
        let mut with_handle = issue("g1", "client-a", "pane-1");
        with_handle.handle = Some(SessionHandle::new(
            DbSessionId::new("dbs-1"),
            RuntimeEpoch::new("1"),
        ));
        registry.issue(with_handle, 0).await;
        registry.issue(issue("g2", "client-a", "pane-2"), 0).await;

        assert_eq!(registry.revoke_for_session_handle("dbs-1").await, 1);
        assert_eq!(
            registry.verify("g1", "client-a", "pane-1", 1).await,
            Err(AiGrantRejection::Revoked)
        );
        assert!(registry.verify("g2", "client-a", "pane-2", 1).await.is_ok());
    }

    #[tokio::test]
    async fn revoke_for_connection_only_hits_that_connection() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("g1", "client-a", "pane-1"), 0).await;
        let mut other = issue("g2", "client-a", "pane-2");
        other.connection_id = "conn-2".into();
        registry.issue(other, 0).await;

        assert_eq!(registry.revoke_for_connection("conn-1").await, 1);
        assert_eq!(
            registry.verify("g1", "client-a", "pane-1", 1).await,
            Err(AiGrantRejection::Revoked)
        );
        assert!(registry.verify("g2", "client-a", "pane-2", 1).await.is_ok());
    }

    #[tokio::test]
    async fn purge_drops_only_expired_records() {
        let registry = AiGrantRegistry::new(TTL);
        registry.issue(issue("old", "client-a", "pane-1"), 0).await;
        registry
            .issue(issue("live", "client-a", "pane-1"), TTL + 1)
            .await;
        assert_eq!(registry.purge_expired(TTL + 2).await, 1);
        assert_eq!(registry.len().await, 1);
        assert_eq!(
            registry.verify("old", "client-a", "pane-1", TTL + 2).await,
            Err(AiGrantRejection::Unknown)
        );
    }

    #[tokio::test]
    async fn service_preserves_expired_rejection_then_reclaims_the_record() {
        let service = AiGrantService::new();
        let receipt = service.issue(issue("g1", "client-a", "pane-1"), 0).await;
        let claim = AiTargetClaim::EditorGrant {
            grant_id: receipt.grant_id,
            claimed_client_instance_id: "client-a".into(),
            editor_session_id: "pane-1".into(),
        };

        assert_eq!(
            service.verify_claim(&claim, DEFAULT_GRANT_TTL_MS).await,
            Err(AiGrantRejection::Expired)
        );
        assert!(service.is_empty().await);
    }

    #[tokio::test]
    async fn service_reclaims_expired_records_when_issuing_a_grant() {
        let service = AiGrantService::new();
        service.issue(issue("old", "client-a", "pane-1"), 0).await;
        service
            .issue(issue("new", "client-a", "pane-1"), DEFAULT_GRANT_TTL_MS)
            .await;

        assert_eq!(service.len().await, 1);
    }

    #[test]
    fn matching_editor_session_is_accepted() {
        let session = session_view(editor_owner("client-a", "pane-1"), "conn-1", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Ok(())
        );
    }

    #[test]
    fn another_pane_of_the_same_client_is_rejected() {
        // Runtime lookup is client-scoped, so the grant policy must distinguish panes.
        let session = session_view(editor_owner("client-a", "pane-2"), "conn-1", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::SessionGone)
        );
    }

    #[test]
    fn another_client_is_rejected() {
        let session = session_view(editor_owner("client-b", "pane-1"), "conn-1", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::ClientMismatch)
        );
    }

    #[test]
    fn session_on_another_connection_is_rejected() {
        let session = session_view(editor_owner("client-a", "pane-1"), "conn-2", 7);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::SessionGone)
        );
    }

    #[test]
    fn stale_config_revision_is_rejected() {
        let session = session_view(editor_owner("client-a", "pane-1"), "conn-1", 6);
        assert_eq!(
            verify_session_ownership(&session, "client-a", "pane-1", "conn-1", 7),
            Err(AiGrantRejection::SessionGone)
        );
    }

    #[test]
    fn non_editor_owned_session_is_rejected() {
        let session = session_view(
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
