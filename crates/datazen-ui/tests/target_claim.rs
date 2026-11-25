//! T164: `packages/frontend/src/ai/targetClaim.ts` port — pure contract tests.
//!
//! Pinned values come from the React source and its consumers' vitest cases:
//! `grant-1` / `client-99` / `panel-1` in `AiChatPanel.share.test.tsx`, the
//! `{ kind: 'independent' }` fallback, and the three `CancelReceipt` answers.
//! No rendering, no async, no weakened assertions.

use datazen_ui::ai::grant_state::AiGrantReceipt;
use datazen_ui::ai::session_cancel;
use datazen_ui::ai::target_claim::{
    attribution_of, editor_grant, independent, is_editor_grant, AiGrantRequest, AiSessionHandle,
    AiTargetClaim, CancelReceipt,
};

fn receipt(id: &str) -> AiGrantReceipt {
    AiGrantReceipt {
        grant_id: id.to_owned(),
        connection_id: "conn-1".to_owned(),
        database: Some("db".to_owned()),
        config_revision: 7,
        expires_at_ms: 3_000,
    }
}

#[test]
fn independent_is_the_default_unshared_claim() {
    let claim = independent();
    assert_eq!(claim, AiTargetClaim::Independent);
    assert_eq!(claim.kind(), "independent", "exact React discriminant");
}

#[test]
fn editor_grant_copies_only_the_grant_id_and_the_exact_client_editor_pair() {
    let claim = editor_grant(&receipt("grant-1"), "client-99", "panel-1");
    assert_eq!(
        claim,
        AiTargetClaim::EditorGrant {
            grant_id: "grant-1".to_owned(),
            claimed_client_instance_id: "client-99".to_owned(),
            editor_session_id: "panel-1".to_owned(),
        },
        "claimForSend payload from AiChatPanel.share.test.tsx"
    );
    assert_eq!(claim.kind(), "editorGrant", "exact React discriminant");

    // Pick<AiGrantReceipt, 'grantId'>: no other receipt field leaks in.
    let AiTargetClaim::EditorGrant { grant_id, .. } = &claim else {
        panic!("editor_grant must return the editorGrant arm");
    };
    assert_eq!(grant_id, "grant-1");
    assert_ne!(grant_id, "conn-1", "connectionId is not the grant id");
}

#[test]
fn is_editor_grant_narrows_only_the_editor_arm_and_fails_closed_on_absent() {
    assert!(!is_editor_grant(None), "null/undefined claim is not a grant");
    assert!(
        !is_editor_grant(Some(&independent())),
        "an independent claim is not a grant"
    );
    let grant = editor_grant(&receipt("grant-1"), "client-99", "panel-1");
    assert!(is_editor_grant(Some(&grant)), "editorGrant narrows to true");
}

#[test]
fn attribution_of_keeps_only_grant_id_and_editor_session_id() {
    let claim = editor_grant(&receipt("grant-1"), "client-99", "panel-1");
    let attribution = attribution_of(Some(&claim));
    assert_eq!(
        attribution,
        datazen_ui::ai::target_claim::AiRunAttribution::EditorGrant {
            grant_id: "grant-1".to_owned(),
            editor_session_id: "panel-1".to_owned(),
        },
        "the claimed client instance id must not enter the run list"
    );
    assert_eq!(attribution.kind(), "editorGrant");
}

#[test]
fn attribution_of_absent_or_independent_claims_is_independent() {
    use datazen_ui::ai::target_claim::AiRunAttribution;

    assert_eq!(attribution_of(None), AiRunAttribution::Independent);
    assert_eq!(
        attribution_of(Some(&independent())),
        AiRunAttribution::Independent
    );
    assert_eq!(attribution_of(None).kind(), "independent");
}

#[test]
fn cancel_receipt_is_reused_not_duplicated() {
    // The re-export must be the same type the T097 state machine consumes.
    fn same_type(receipt: &CancelReceipt) -> &session_cancel::CancelReceipt {
        receipt
    }
    let marked = CancelReceipt::Marked {
        request_id: "req-1".to_owned(),
    };
    let finished = CancelReceipt::AlreadyFinished {
        request_id: "req-2".to_owned(),
    };
    let forbidden = CancelReceipt::Forbidden {
        request_id: "req-3".to_owned(),
    };
    assert_eq!(same_type(&marked).kind(), "marked");
    assert_eq!(same_type(&finished).kind(), "alreadyFinished");
    assert_eq!(same_type(&forbidden).kind(), "forbidden");
    assert_eq!(marked.request_id(), "req-1");
    assert_eq!(finished.request_id(), "req-2");
    assert_eq!(forbidden.request_id(), "req-3");
}

#[test]
fn grant_request_and_session_handle_are_passed_through_verbatim() {
    let handle = AiSessionHandle {
        db_session_id: "db-sess".to_owned(),
        runtime_epoch: "epoch-1".to_owned(),
    };
    let request = AiGrantRequest {
        connection_id: "conn-1".to_owned(),
        database: Some("db".to_owned()),
        editor_session_id: "panel-1".to_owned(),
        claimed_client_instance_id: "client-99".to_owned(),
        session_handle: handle.clone(),
    };
    assert_eq!(handle.db_session_id, "db-sess");
    assert_eq!(handle.runtime_epoch, "epoch-1");
    assert_eq!(request.connection_id, "conn-1");
    assert_eq!(request.database.as_deref(), Some("db"));
    assert_eq!(request.editor_session_id, "panel-1");
    assert_eq!(request.claimed_client_instance_id, "client-99");
    assert_eq!(request.session_handle, handle, "opaque handle kept verbatim");

    let unscoped = AiGrantRequest {
        connection_id: "conn-2".to_owned(),
        database: None,
        editor_session_id: "panel-2".to_owned(),
        claimed_client_instance_id: "client-1".to_owned(),
        session_handle: handle,
    };
    assert_eq!(unscoped.database, None, "null database stays absent");
}
