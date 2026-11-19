//! The wiring pins for [`AiStreaming`]: the operations that are pure
//! routing/forwarding over the seams rather than stream lifecycle arms —
//! React's module-level `requestIdToKey` map, the `getStreamRegistry`
//! singleton accessor, `attributionFor`'s forward, `cancelChat`'s
//! resolution and delegation, the documented canceller-command
//! composition, and the crate-root re-export witness. The stream lifecycle
//! arms (chunk/error/timeout/persist) live in `tests`; see `test_support`
//! for the harness.

use std::rc::Rc;

use crate::ai_types::AiChatAttribution;

use super::seams::{AiStoreView, AiTargetClaim, ApplyChunkGate, CancelReceipt, SessionCancellerOps};
use super::test_support::{ScriptedCancelCommand, SharedStore, WiredCanceller, env};

#[test]
fn register_and_unregister_route_request_ids() {
    // React's module-level requestIdToKey map: register overwrites,
    // unregister hands the key back exactly once.
    let mut test = env();
    test.streaming.register_request_id("r1", "chat::a");
    test.streaming.register_request_id("r1", "chat::b");
    assert_eq!(test.streaming.unregister_request_id("r1").as_deref(), Some("chat::b"));
    // The second unregister reads absent (React returns undefined).
    assert_eq!(test.streaming.unregister_request_id("r1"), None);
    // An unknown id never had a route.
    assert_eq!(test.streaming.unregister_request_id("r9"), None);
}

#[test]
fn registry_accessor_exposes_the_same_instance_the_handlers_gate_through() {
    // React `getStreamRegistry()`: one registry shared by every caller.
    let mut test = env();
    test.registry.begin("r1");
    // Route through the accessor, exactly like the send path would.
    let settled = test
        .streaming
        .registry()
        .apply_chunk("r1", &ApplyChunkGate { done: true, cancelled: None });
    assert!(settled.accepted);
    // The settle is visible through the same instance: a late chunk now
    // drops (the same double the handler-side gating reads).
    let late = test
        .streaming
        .registry()
        .apply_chunk("r1", &ApplyChunkGate { done: false, cancelled: None });
    assert!(!late.accepted);
    assert_eq!(test.registry.chunk_calls().len(), 2);
}

#[test]
fn attribution_for_forwards_both_claim_arms() {
    // React `attributionFor` is the pure forward to `attributionOf`; the
    // stub implements the real projection, so the forward must pass both
    // arms through unchanged (the client instance id is dropped by the
    // projection, never by the forward).
    let test = env();
    let editor = AiTargetClaim::EditorGrant {
        grant_id: "g1".to_owned(),
        claimed_client_instance_id: "client-7".to_owned(),
        editor_session_id: "ed-1".to_owned(),
    };
    assert_eq!(
        test.streaming.attribution_for(Some(&editor)),
        AiChatAttribution::EditorGrant {
            grant_id: "g1".to_owned(),
            editor_session_id: "ed-1".to_owned(),
        }
    );
    assert_eq!(
        test.streaming.attribution_for(Some(&AiTargetClaim::Independent)),
        AiChatAttribution::Independent
    );
    assert_eq!(test.streaming.attribution_for(None), AiChatAttribution::Independent);
}

#[test]
fn cancel_chat_resolves_the_request_id_and_delegates_to_the_canceller() {
    let mut test = env();
    test.store.seed_chat(super::test_support::fixture_session("s1", "r-run"));
    let receipt = CancelReceipt::Marked { request_id: "r-run".to_owned() };
    test.canceller.program_receipt(receipt.clone());
    // Resolves from the visible session when the caller passes none.
    let returned = test.streaming.cancel_chat(&SharedStore(test.store.clone()), None);
    assert_eq!(returned, Some(receipt));
    assert_eq!(test.canceller.calls(), vec!["r-run".to_owned()]);
    // An explicit id wins over the visible session's.
    test.canceller
        .program_receipt(CancelReceipt::AlreadyFinished { request_id: "r-explicit".to_owned() });
    let returned = test
        .streaming
        .cancel_chat(&SharedStore(test.store.clone()), Some("r-explicit"));
    assert!(matches!(
        returned,
        Some(CancelReceipt::AlreadyFinished { request_id }) if request_id == "r-explicit"
    ));
    assert_eq!(test.canceller.calls(), vec!["r-run".to_owned(), "r-explicit".to_owned()]);
}

#[test]
fn cancel_chat_without_any_request_id_returns_none() {
    let mut test = env();
    let view = SharedStore(test.store.clone());
    // No visible session: nothing to resolve.
    assert_eq!(test.streaming.cancel_chat(&view, None), None);
    assert!(test.canceller.calls().is_empty());
    // A visible session without an in-flight request: still nothing.
    let mut idle = super::test_support::fixture_session("s1", "r-done");
    idle.request_id = None;
    test.store.seed_chat(idle);
    assert_eq!(test.streaming.cancel_chat(&view, None), None);
    // An explicit empty string is falsy in React (`!rid`) and cancels
    // nothing — it does NOT fall back to the visible session's id.
    assert_eq!(test.streaming.cancel_chat(&view, Some("")), None);
    assert!(test.canceller.calls().is_empty());
}

#[test]
fn the_canceller_wiring_folds_a_command_error_to_a_null_receipt() {
    // The composition documented on SessionCancellerOps: the adapter sits
    // on the aiCommands.cancel seam; a transport failure leaves the
    // receipt null (no proof the host observed the cancel) and the run
    // keeps streaming.
    let command = Rc::new(ScriptedCancelCommand::default());
    command.program_receipt(CancelReceipt::Marked { request_id: "r1".to_owned() });
    let mut wired = WiredCanceller { command: command.clone() };
    assert!(matches!(wired.cancel("r1"), Some(CancelReceipt::Marked { .. })));
    command.program_error("ipc down");
    assert_eq!(wired.cancel("r1"), None);
    assert_eq!(command.calls(), vec!["r1".to_owned(), "r1".to_owned()]);
}

#[test]
fn the_crate_root_re_exports_the_streaming_vocabulary() {
    // Pins the lib.rs re-export surface (the T234 audit's lesson: tests
    // speaking only `super::` cannot notice a root re-export shrink).
    let mut test = env();
    let streaming: &mut crate::AiStreaming = &mut test.streaming;
    streaming.register_request_id("r-root", "k");
    let receipt: Option<crate::CancelReceipt> =
        streaming.cancel_chat(&SharedStore(test.store.clone()), Some("r-root"));
    assert_eq!(receipt, None);
    let gate = crate::ApplyChunkGate { done: false, cancelled: None };
    let applied: crate::ApplyGateResult = streaming.registry().apply_chunk("r-root", &gate);
    assert!(!applied.accepted);
    let slice: crate::Nl2sqlStreamingSlice = SharedStore(test.store.clone()).nl2sql();
    assert_eq!(slice.request_id, None);
    let claim = crate::AiTargetClaim::Independent;
    assert_eq!(
        streaming.attribution_for(Some(&claim)),
        crate::AiChatAttribution::Independent
    );
    assert_eq!(streaming.unregister_request_id("r-root").as_deref(), Some("k"));
}
