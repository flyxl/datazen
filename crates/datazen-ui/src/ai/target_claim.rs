//! Port of `packages/frontend/src/ai/targetClaim.ts` — the shared, pure
//! lifecycle contract for AI chat runs.
//!
//! React keeps these shapes in one dependency-free module so `streamRegistry`,
//! `grantState`, `sessionCancel` and the IPC wrapper share one vocabulary. The
//! Rune side already has two of those shapes from earlier ports:
//!
//! * `AiGrantReceipt` lives in `super::grant_state` (T049);
//! * `CancelReceipt` lives in `super::session_cancel` (T097).
//!
//! They are **reused, not duplicated**: `editorGrant` reads the receipt already
//! produced by the grant state machine, and `CancelReceipt` is re-exported here
//! so callers can import the whole contract from one module, as in React.
//! Removal stage: when a neutral contract crate (`datazen_ui_api`) exposes
//! these DTOs, replace the two sibling imports/re-exports with that single
//! source; every item in this file stays unchanged.
//!
//! This module owns no state machine of its own — claim acquisition/release,
//! ownership/epoch checks and terminal gating belong to the sibling ports named
//! above. Pure-logic, immediate mode: no async, no threads, no view, no IPC.
//! Every public item carries a per-item `#[allow(dead_code)]` with that removal
//! stage: drop it when the AI chat panel wires the contract.

use super::grant_state::AiGrantReceipt;

/// React's `CancelReceipt`, re-exported so this module is the single contract
/// surface for the three AI state machines and the host IPC wrapper.
// consumer: the AI chat panel's cancel wiring; delete this re-export (import
// directly from session_cancel) when that panel lands.
#[allow(dead_code)]
pub(crate) use super::session_cancel::CancelReceipt;

/// `AiTargetClaim` — what a chat run is allowed to observe.
///
/// `independent` owns its own runtime context. `editorGrant` binds the run to
/// an editor session the user explicitly shared, carrying the server-issued
/// grant id and the exact client/editor pair that claimed it.
#[allow(dead_code)] // consumer: the AI chat panel's claimForSend; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AiTargetClaim {
    Independent,
    EditorGrant {
        grant_id: String,
        claimed_client_instance_id: String,
        editor_session_id: String,
    },
}

impl AiTargetClaim {
    /// Exact React `kind` discriminant.
    #[allow(dead_code)] // consumer: the AI chat panel's IPC payload; delete then.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Independent => "independent",
            Self::EditorGrant { .. } => "editorGrant",
        }
    }
}

/// Opaque runtime handle the editor controller already holds. Passed through
/// verbatim; never reconstructed from parts.
#[allow(dead_code)] // consumer: ai_grant_editor_session request builder; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiSessionHandle {
    pub(crate) db_session_id: String,
    pub(crate) runtime_epoch: String,
}

/// Arguments for `ai_grant_editor_session` (camelCase in React; the host maps
/// them to snake_case).
#[allow(dead_code)] // consumer: ai_grant_editor_session request builder; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiGrantRequest {
    pub(crate) connection_id: String,
    pub(crate) database: Option<String>,
    pub(crate) editor_session_id: String,
    pub(crate) claimed_client_instance_id: String,
    pub(crate) session_handle: AiSessionHandle,
}

/// Minimal attribution metadata retained for a run/message list. The claimed
/// client instance id is deliberately **not** part of it.
#[allow(dead_code)] // consumer: the AI run/message list projection; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AiRunAttribution {
    Independent,
    EditorGrant {
        grant_id: String,
        editor_session_id: String,
    },
}

impl AiRunAttribution {
    /// Exact React `kind` discriminant.
    #[allow(dead_code)] // consumer: the run/message list projection; delete then.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Independent => "independent",
            Self::EditorGrant { .. } => "editorGrant",
        }
    }
}

/// The default, unshared target: `{ kind: 'independent' }`.
#[allow(dead_code)] // consumer: the AI chat panel's claimForSend; delete then.
pub(crate) fn independent() -> AiTargetClaim {
    AiTargetClaim::Independent
}

/// Build a claim from an issued grant receipt. Only the receipt's `grantId` is
/// copied, exactly like React's `Pick<AiGrantReceipt, 'grantId'>`.
#[allow(dead_code)] // consumer: the AI chat panel's claimForSend; delete then.
pub(crate) fn editor_grant(
    receipt: &AiGrantReceipt,
    claimed_client_instance_id: impl Into<String>,
    editor_session_id: impl Into<String>,
) -> AiTargetClaim {
    AiTargetClaim::EditorGrant {
        grant_id: receipt.grant_id.clone(),
        claimed_client_instance_id: claimed_client_instance_id.into(),
        editor_session_id: editor_session_id.into(),
    }
}

/// Narrow a claim to its editor-grant arm. React's `claim?.kind ===
/// 'editorGrant'` is `false` for both `null` and `undefined`, so `None` fails
/// closed here.
#[allow(dead_code)] // consumer: the run attribution projection; delete then.
pub(crate) fn is_editor_grant(claim: Option<&AiTargetClaim>) -> bool {
    claim.is_some_and(|claim| matches!(claim, AiTargetClaim::EditorGrant { .. }))
}

/// Project a claim into the non-secret attribution metadata kept by the UI.
/// An editor grant keeps only `grantId` and `editorSessionId`; everything else
/// (notably the claimed client instance id) is dropped, and any absent/other
/// claim becomes `independent`.
#[allow(dead_code)] // consumer: the AI run/message list projection; delete then.
pub(crate) fn attribution_of(claim: Option<&AiTargetClaim>) -> AiRunAttribution {
    match claim {
        Some(AiTargetClaim::EditorGrant {
            grant_id,
            editor_session_id,
            ..
        }) => AiRunAttribution::EditorGrant {
            grant_id: grant_id.clone(),
            editor_session_id: editor_session_id.clone(),
        },
        _ => AiRunAttribution::Independent,
    }
}
