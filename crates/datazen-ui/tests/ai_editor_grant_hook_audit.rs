//! T086 audit: the `useAiEditorGrant` hook port, verified against
//! `packages/frontend/src/hooks/useAiEditorGrant.ts` re-read line by line,
//! plus the `ai/targetClaim.ts` seam (the claim builders it reads).
//!
//! Tester-written: the coder's docs are ignored; every expectation below
//! comes from the React sources. Time is injected, so the touch/expiry
//! arithmetic is deterministic.

use std::cell::RefCell;
use std::rc::Rc;

use datazen_ui::ai::grant_state::{
    AiGrantReceipt, GrantConfig, GrantEffect, GrantInput, GrantPhase,
};
use datazen_ui::ai::target_claim::{AiTargetClaim, independent};
use datazen_ui::hooks::use_ai_editor_grant::{AiEditorGrant, UseAiEditorGrantParams};

fn params(connection: Option<&str>, database: Option<&str>, editor: Option<&str>) -> UseAiEditorGrantParams {
    UseAiEditorGrantParams {
        connection_id: connection.map(str::to_owned),
        database: database.map(str::to_owned),
        editor_session_id: editor.map(str::to_owned),
    }
}

/// A hook over an injectable clock (the host seam `with_config` exposes).
fn hook(now_ms: i64, params: UseAiEditorGrantParams) -> (AiEditorGrant, Rc<RefCell<i64>>) {
    let clock = Rc::new(RefCell::new(now_ms));
    let reader = clock.clone();
    let config = GrantConfig {
        touch_interval_ratio: 0.5,
        now: Box::new(move || *reader.borrow()),
    };
    (AiEditorGrant::with_config(params, config), clock)
}

fn receipt(grant_id: &str, expires_at_ms: i64) -> AiGrantReceipt {
    AiGrantReceipt {
        grant_id: grant_id.to_owned(),
        connection_id: "conn-1".to_owned(),
        database: Some("db".to_owned()),
        config_revision: 7,
        expires_at_ms,
    }
}

#[test]
fn without_both_ids_the_hook_is_inert() {
    // canShare = Boolean(connectionId && editorSessionId); the machine is
    // null so setSharing does nothing and the claim is independent.
    for p in [
        params(None, None, None),
        params(Some("conn-1"), None, None),
        params(None, None, Some("editor-1")),
    ] {
        let (mut h, _clock) = hook(1_000, p.clone());
        assert!(!h.can_share(), "{p:?}");
        assert!(!h.sharing());
        assert_eq!(h.phase(), GrantPhase::Independent);
        assert_eq!(h.error(), None);
        assert!(h.set_sharing(true).is_empty(), "grant with no machine: no-op");
        assert!(h.set_sharing(false).is_empty(), "revoke with no machine: no-op");
        assert_eq!(h.claim_for_send(), independent());
    }
}

#[test]
fn granting_flows_the_hook_params_into_the_call() {
    let (mut h, _clock) = hook(1_000, params(Some("conn-1"), Some("db"), Some("editor-1")));
    assert!(h.can_share());
    assert_eq!(h.phase(), GrantPhase::Independent);

    let effects = h.set_sharing(true);
    // `database ?? null` keeps the Some; the connection/editor ids are the
    // non-null casts.
    assert_eq!(
        effects,
        vec![GrantEffect::CallGrant {
            input: GrantInput {
                connection_id: "conn-1".to_owned(),
                database: Some("db".to_owned()),
                editor_session_id: "editor-1".to_owned(),
            },
            token: 1,
        }]
    );
    // The grant input keeps a null database as None (React's `?? null`).
    let (mut h2, _c2) = hook(1_000, params(Some("conn-1"), None, Some("editor-1")));
    let effects2 = h2.set_sharing(true);
    assert_eq!(
        effects2,
        vec![GrantEffect::CallGrant {
            input: GrantInput {
                connection_id: "conn-1".to_owned(),
                database: None,
                editor_session_id: "editor-1".to_owned(),
            },
            token: 1,
        }]
    );
}

#[test]
fn a_resolved_grant_shares_and_claims_with_the_client_instance() {
    let (mut h, clock) = hook(1_000, params(Some("conn-1"), Some("db"), Some("editor-1")));
    h.set_sharing(true);
    assert!(h.sharing(), "granting counts as sharing");

    let effects = h.on_grant_result(1, Ok((receipt("g-9", 3_000), "client-7".to_owned())));
    // ttl = 2000 from now=1000: touch at 2000, expiry at 3000.
    assert_eq!(
        effects,
        vec![
            GrantEffect::SetTouchTimer { handle: 1, delay_ms: 1_000 },
            GrantEffect::SetExpiryTimer { handle: 2, delay_ms: 2_000 },
        ]
    );
    assert!(h.sharing());
    assert_eq!(h.phase(), GrantPhase::Granted);
    assert_eq!(h.error(), None);
    assert_eq!(
        h.claim_for_send(),
        AiTargetClaim::EditorGrant {
            grant_id: "g-9".to_owned(),
            claimed_client_instance_id: "client-7".to_owned(),
            editor_session_id: "editor-1".to_owned(),
        }
    );

    // The touch cycle: timer fires -> CallTouch; a successful touch
    // extends the expiry (now + ttl).
    *clock.borrow_mut() = 2_000;
    let touch_effects = h.fire_touch_timer(1);
    assert_eq!(touch_effects, vec![GrantEffect::CallTouch { grant_id: "g-9".to_owned() }]);
    *clock.borrow_mut() = 2_050;
    let rearm = h.on_touch_result("g-9", Ok(true));
    // The touch timer already fired (consumed); the still-armed expiry
    // timer is cleared before both timers re-arm.
    assert_eq!(
        rearm,
        vec![
            GrantEffect::ClearTimer { handle: 2 },
            GrantEffect::SetTouchTimer { handle: 3, delay_ms: 1_000 },
            GrantEffect::SetExpiryTimer { handle: 4, delay_ms: 2_000 },
        ]
    );
}

#[test]
fn a_denied_grant_stays_independent_with_a_visible_error() {
    let (mut h, _clock) = hook(1_000, params(Some("conn-1"), None, Some("editor-1")));
    h.set_sharing(true);
    let effects = h.on_grant_result(1, Err("Editor session is not ready".to_owned()));
    assert!(effects.is_empty(), "no timers to arm");
    assert!(!h.sharing());
    assert_eq!(h.phase(), GrantPhase::Independent);
    assert_eq!(h.error(), Some("Editor session is not ready"));
    assert_eq!(h.claim_for_send(), independent(), "no silent unclaimed send");
}

#[test]
fn a_grant_without_a_claimed_client_instance_cannot_claim() {
    // The hook's guard: granted + receipt + claimedClientInstanceId +
    // editorSessionId, all four, or independent.
    let (mut h, _clock) = hook(1_000, params(Some("conn-1"), None, Some("editor-1")));
    h.set_sharing(true);
    h.on_grant_result(1, Ok((receipt("g-9", 3_000), "client-7".to_owned())));
    assert!(matches!(h.claim_for_send(), AiTargetClaim::EditorGrant { .. }));

    // Rekey to a pane with no editor id: the granted machine's dispose
    // releases the grant (timers cleared + the quiet revoke), then the
    // machine is gone and the claim is independent.
    let effects = h.update_params(params(Some("conn-1"), None, None));
    assert_eq!(
        effects,
        vec![
            GrantEffect::ClearTimer { handle: 1 },
            GrantEffect::ClearTimer { handle: 2 },
            GrantEffect::CallRevoke { grant_id: "g-9".to_owned() },
        ]
    );
    assert!(!h.can_share());
    assert_eq!(h.claim_for_send(), independent());
}

#[test]
fn revoking_releases_the_grant_and_rekeys_dispose_live_grants() {
    let (mut h, _clock) = hook(1_000, params(Some("conn-1"), None, Some("editor-1")));
    h.set_sharing(true);
    h.on_grant_result(1, Ok((receipt("g-9", 3_000), "client-7".to_owned())));

    // setSharing(false) -> revoke(): timers cleared + the quiet release.
    let revoke_effects = h.set_sharing(false);
    assert_eq!(
        revoke_effects,
        vec![
            GrantEffect::ClearTimer { handle: 1 },
            GrantEffect::ClearTimer { handle: 2 },
            GrantEffect::CallRevoke { grant_id: "g-9".to_owned() },
        ]
    );
    assert!(!h.sharing());
    assert_eq!(h.phase(), GrantPhase::Revoked);
    assert_eq!(h.claim_for_send(), independent());

    // A rekey while granted: the old machine's dispose releases the grant;
    // the new machine starts independent and the claimed id is cleared.
    let (mut h2, clock2) = hook(1_000, params(Some("conn-1"), None, Some("editor-1")));
    h2.set_sharing(true);
    h2.on_grant_result(1, Ok((receipt("g-1", 3_000), "client-1".to_owned())));
    let reader = clock2.clone();
    let config = GrantConfig { touch_interval_ratio: 0.5, now: Box::new(move || *reader.borrow()) };
    let rekey = h2.update_params_with(params(Some("conn-2"), None, Some("editor-2")), config);
    assert_eq!(
        rekey,
        vec![
            GrantEffect::ClearTimer { handle: 1 },
            GrantEffect::ClearTimer { handle: 2 },
            GrantEffect::CallRevoke { grant_id: "g-1".to_owned() },
        ]
    );
    assert_eq!(h2.phase(), GrantPhase::Independent);
    assert_eq!(h2.claim_for_send(), independent());
    assert_eq!(h2.params().editor_session_id.as_deref(), Some("editor-2"));
}

#[test]
fn a_database_only_change_keeps_the_machine() {
    let (mut h, _clock) = hook(1_000, params(Some("conn-1"), Some("db"), Some("editor-1")));
    h.set_sharing(true);
    h.on_grant_result(1, Ok((receipt("g-9", 3_000), "client-7".to_owned())));
    let effects = h.update_params(params(Some("conn-1"), Some("other-db"), Some("editor-1")));
    assert!(effects.is_empty(), "the memo key is (connectionId, editorSessionId)");
    assert_eq!(h.phase(), GrantPhase::Granted, "the grant survives");
    assert!(h.sharing());
}
