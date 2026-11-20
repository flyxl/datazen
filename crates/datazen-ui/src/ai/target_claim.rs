//! Port of `packages/frontend/src/ai/targetClaim.ts`.
//!
//! AI runtime lifecycle wire contracts (P6 §14.4): the shapes the frontend
//! and the host agree on for the target a chat run claims
//! ([`AiTargetClaim`]), the receipt of a granted editor session
//! ([`AiGrantReceipt`], re-exported from `grant_state` where the machine
//! lives), and the three-state answer to `ai_cancel` ([`CancelReceipt`]).
//!
//! Dependency-free in React so the state machines (`streamRegistry`,
//! `grantState`, `sessionCancel`) and the IPC wrapper in `commands/ai.ts`
//! can share the contract without importing Tauri; same here — plain
//! data, no host types.
//!
//! **Ledger note**: `ai/targetClaim.ts` is segment 1's T162; this port
//! lands as the seam `hooks/useAiEditorGrant.ts` (segment 4's T086)
//! needs, following the `grant_state.rs` precedent (T111). The
//! coordinator reconciles when segment 1 lands its own pass.
//!
//! Known deltas: none behavioural — every arm, field and helper is the
//! React file's verbatim.

/// React `CancelReceipt`: the answer to `ai_cancel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CancelReceipt {
    /// The backend accepted the cancel and will stop the run.
    Marked { request_id: String },
    /// The run reached its terminal state before the cancel.
    AlreadyFinished { request_id: String },
    /// This caller/target may not cancel the run.
    Forbidden { request_id: String },
}

/// React `AiTargetClaim`: what a chat run is allowed to observe.
/// `Independent` means the run owns its own runtime context; `EditorGrant`
/// binds it to an editor session the user explicitly shared, identified
/// by the server-issued grant id and the exact client/editor pair that
/// claimed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AiTargetClaim {
    Independent,
    EditorGrant {
        grant_id: String,
        claimed_client_instance_id: String,
        editor_session_id: String,
    },
}

/// React `AiSessionHandle`: the opaque runtime handle the editor
/// controller already holds. Passed through verbatim; never reconstructed
/// from parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiSessionHandle {
    pub db_session_id: String,
    pub runtime_epoch: String,
}

/// React `AiGrantRequest`: arguments for `ai_grant_editor_session`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiGrantRequest {
    pub connection_id: String,
    pub database: Option<String>,
    pub editor_session_id: String,
    pub claimed_client_instance_id: String,
    pub session_handle: AiSessionHandle,
}

/// React `AiRunAttribution`: the minimal, non-secret attribution metadata
/// retained for a run/message list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AiRunAttribution {
    Independent,
    EditorGrant { grant_id: String, editor_session_id: String },
}

/// React `independent()`: the default, unshared target.
pub(crate) fn independent() -> AiTargetClaim {
    AiTargetClaim::Independent
}

/// React `editorGrant(receipt, claimedClientInstanceId, editorSessionId)`:
/// build a claim from an issued grant receipt (only the `grantId` is
/// read).
pub(crate) fn editor_grant(
    receipt: &crate::ai::grant_state::AiGrantReceipt,
    claimed_client_instance_id: &str,
    editor_session_id: &str,
) -> AiTargetClaim {
    AiTargetClaim::EditorGrant {
        grant_id: receipt.grant_id.clone(),
        claimed_client_instance_id: claimed_client_instance_id.to_owned(),
        editor_session_id: editor_session_id.to_owned(),
    }
}

/// React `isEditorGrant(claim)`: narrow a claim to its editor-grant arm.
pub(crate) fn is_editor_grant(claim: Option<&AiTargetClaim>) -> bool {
    matches!(claim, Some(AiTargetClaim::EditorGrant { .. }))
}

/// React `attributionOf(claim)`: project a claim into the non-secret
/// attribution metadata the UI keeps.
pub(crate) fn attribution_of(claim: Option<&AiTargetClaim>) -> AiRunAttribution {
    match claim {
        Some(AiTargetClaim::EditorGrant { grant_id, editor_session_id, .. }) => {
            AiRunAttribution::EditorGrant {
                grant_id: grant_id.clone(),
                editor_session_id: editor_session_id.clone(),
            }
        }
        _ => AiRunAttribution::Independent,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ai::grant_state::AiGrantReceipt;

    fn receipt(grant_id: &str) -> AiGrantReceipt {
        AiGrantReceipt {
            grant_id: grant_id.to_owned(),
            connection_id: "conn".to_owned(),
            database: None,
            config_revision: 3,
            expires_at_ms: 9_000,
        }
    }

    #[test]
    fn the_default_claim_is_independent() {
        assert_eq!(independent(), AiTargetClaim::Independent);
        assert!(!is_editor_grant(Some(&independent())));
        assert!(!is_editor_grant(None));
    }

    #[test]
    fn editor_grant_reads_only_the_grant_id() {
        let claim = editor_grant(&receipt("g-1"), "client-7", "editor-9");
        assert_eq!(
            claim,
            AiTargetClaim::EditorGrant {
                grant_id: "g-1".to_owned(),
                claimed_client_instance_id: "client-7".to_owned(),
                editor_session_id: "editor-9".to_owned(),
            }
        );
        assert!(is_editor_grant(Some(&claim)));
        // Only the grant id is read: every other receipt field may differ.
        let other = receipt("g-1");
        assert_eq!(editor_grant(&other, "client-7", "editor-9"), claim);
    }

    #[test]
    fn attribution_projects_the_non_secret_fields() {
        let claim = editor_grant(&receipt("g-2"), "client-7", "editor-9");
        assert_eq!(
            attribution_of(Some(&claim)),
            AiRunAttribution::EditorGrant {
                grant_id: "g-2".to_owned(),
                editor_session_id: "editor-9".to_owned(),
            }
        );
        // The claimed client instance never crosses into the attribution.
        assert!(!format!("{:?}", attribution_of(Some(&claim))).contains("client-7"));
        assert_eq!(attribution_of(None), AiRunAttribution::Independent);
        assert_eq!(attribution_of(Some(&independent())), AiRunAttribution::Independent);
    }
}
