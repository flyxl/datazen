//! Independent audit for T164: `ai/targetClaim.ts` contract port.
//!
//! The coder's suite already hits every branch of the four pure functions.
//! These two tests strengthen invariants the React source pins but the coder
//! only checked indirectly: `editorGrant` is typed
//! `Pick<AiGrantReceipt, 'grantId'>` (every other receipt field is irrelevant),
//! and the strings it carries are copied verbatim through `attributionOf`
//! with no normalisation or secret field leaking into the run projection.

use datazen_ui::ai::grant_state::AiGrantReceipt;
use datazen_ui::ai::target_claim::{
    attribution_of, editor_grant, is_editor_grant, AiRunAttribution, AiTargetClaim,
};

fn receipt(id: &str, connection: &str, database: Option<&str>, revision: i64, expiry: i64) -> AiGrantReceipt {
    AiGrantReceipt {
        grant_id: id.to_owned(),
        connection_id: connection.to_owned(),
        database: database.map(str::to_owned),
        config_revision: revision,
        expires_at_ms: expiry,
    }
}

/// React's parameter is `Pick<AiGrantReceipt, 'grantId'>`. Two receipts that
/// differ in every other field but share a grant id must produce the *same*
/// claim; changing only the grant id must change the claim.
#[test]
fn editor_grant_ignores_every_receipt_field_except_grant_id() {
    let a = editor_grant(&receipt("grant-1", "conn-1", Some("db"), 7, 3_000), "client-99", "panel-1");
    let b = editor_grant(&receipt("grant-1", "other-conn", None, -1, 9_999_999), "client-99", "panel-1");
    assert_eq!(a, b, "connectionId/database/configRevision/expiresAtMs cannot affect the claim");

    let c = editor_grant(&receipt("grant-2", "conn-1", Some("db"), 7, 3_000), "client-99", "panel-1");
    assert_ne!(a, c, "the grant id is the only receipt field that changes the claim");

    let AiTargetClaim::EditorGrant { grant_id, .. } = &a else {
        panic!("editor_grant must return the editorGrant arm");
    };
    assert_eq!(grant_id, "grant-1");
    assert_ne!(grant_id, "conn-1");
}

/// Strings flow through `editorGrant` → `attributionOf` unchanged. In
/// particular an empty editor session id must survive as a value (React copies
/// it, it is not treated as absent), and the claimed client instance id must
/// never appear in the run-list attribution.
#[test]
fn claim_strings_flow_verbatim_into_attribution_without_the_client_id() {
    const CLIENT_ID: &str = "SECRET-CLIENT-99";
    let claim = editor_grant(&receipt("授权-🔑", "conn-1", None, 0, 0), CLIENT_ID, "会话/一");
    assert!(is_editor_grant(Some(&claim)), "the claim is still an editor grant");

    let attribution = attribution_of(Some(&claim));
    assert_eq!(
        attribution,
        AiRunAttribution::EditorGrant {
            grant_id: "授权-🔑".to_owned(),
            editor_session_id: "会话/一".to_owned(),
        },
        "grant id and editor session id are copied verbatim, including an empty string"
    );
    assert_eq!(attribution.kind(), "editorGrant");
    assert!(
        !format!("{attribution:?}").contains(CLIENT_ID),
        "the claimed client instance id must stay out of the run projection"
    );
}
