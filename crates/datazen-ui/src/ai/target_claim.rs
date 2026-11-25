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
pub use super::session_cancel::CancelReceipt;

/// `AiTargetClaim` — what a chat run is allowed to observe.
///
/// `independent` owns its own runtime context. `editorGrant` binds the run to
/// an editor session the user explicitly shared, carrying the server-issued
/// grant id and the exact client/editor pair that claimed it.
#[allow(dead_code)] // consumer: the AI chat panel's claimForSend; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiTargetClaim {
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
    pub fn kind(&self) -> &'static str {
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
pub struct AiSessionHandle {
    pub db_session_id: String,
    pub runtime_epoch: String,
}

/// Arguments for `ai_grant_editor_session` (camelCase in React; the host maps
/// them to snake_case).
#[allow(dead_code)] // consumer: ai_grant_editor_session request builder; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiGrantRequest {
    pub connection_id: String,
    pub database: Option<String>,
    pub editor_session_id: String,
    pub claimed_client_instance_id: String,
    pub session_handle: AiSessionHandle,
}

/// Minimal attribution metadata retained for a run/message list. The claimed
/// client instance id is deliberately **not** part of it.
#[allow(dead_code)] // consumer: the AI run/message list projection; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiRunAttribution {
    Independent,
    EditorGrant {
        grant_id: String,
        editor_session_id: String,
    },
}

impl AiRunAttribution {
    /// Exact React `kind` discriminant.
    #[allow(dead_code)] // consumer: the run/message list projection; delete then.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Independent => "independent",
            Self::EditorGrant { .. } => "editorGrant",
        }
    }
}

/// The default, unshared target: `{ kind: 'independent' }`.
#[allow(dead_code)] // consumer: the AI chat panel's claimForSend; delete then.
pub fn independent() -> AiTargetClaim {
    AiTargetClaim::Independent
}

/// Build a claim from an issued grant receipt. Only the receipt's `grantId` is
/// copied, exactly like React's `Pick<AiGrantReceipt, 'grantId'>`.
#[allow(dead_code)] // consumer: the AI chat panel's claimForSend; delete then.
pub fn editor_grant(
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
pub fn is_editor_grant(claim: Option<&AiTargetClaim>) -> bool {
    claim.is_some_and(|claim| matches!(claim, AiTargetClaim::EditorGrant { .. }))
}

/// Project a claim into the non-secret attribution metadata kept by the UI.
/// An editor grant keeps only `grantId` and `editorSessionId`; everything else
/// (notably the claimed client instance id) is dropped, and any absent/other
/// claim becomes `independent`.
#[allow(dead_code)] // consumer: the AI run/message list projection; delete then.
pub fn attribution_of(claim: Option<&AiTargetClaim>) -> AiRunAttribution {
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

// segment 4's T086 inline suite: the wire contract against the real receipt
// produced by grant_state; `impl Into<String>` above accepts its `&str` args.
#[cfg(test)]
pub mod tests {
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
