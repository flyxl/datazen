//! Unit tests for [`crate::resource_adapter::LegacyResourceAdapter`] §6.5.
//! Loaded via `#[path]` from `resource_adapter.rs` under `#[cfg(test)]`, the
//! same way its sibling `resource_adapter_tests.rs` is — split out because that
//! file is already past the size ceiling and this covers exactly one method.

use super::tests::{adapter, LegacyFake};
use super::*;
use crate::session::{SessionHandleKind, SessionHandleRef};

fn ref_for(handle: &ResourceHandle, handle_id: &str, kind: SessionHandleKind) -> SessionHandleRef {
    SessionHandleRef::new(
        handle_id.to_string(),
        kind,
        handle.resource_key().to_string(),
        7,
    )
}

#[tokio::test]
async fn the_adapter_refuses_a_handle_close_instead_of_pretending_to_have_closed_it() {
    let provider = adapter(LegacyFake::new());
    let handle = ResourceHandle::issue("legacy-fake", "app", 7);
    let session_handle = ref_for(&handle, "tx-1", SessionHandleKind::Transaction);

    let error = provider
        .close_handle(&handle, &session_handle)
        .await
        .expect_err("the legacy surface has nowhere to put a session-scoped handle");

    match error {
        ResourceError::OperationNotSupported {
            ref operation,
            ref driver,
            ref reason,
        } => {
            assert_eq!(operation, "close_handle");
            assert_eq!(driver, "legacy-fake");
            // The reason has to identify *which* handle went unclosed, or the
            // host has nothing to log and nothing to retry.
            assert!(reason.contains("tx-1"), "reason was: {reason}");
            assert!(reason.contains("Transaction"), "reason was: {reason}");
        }
        other => panic!("expected an explicit refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn the_adapter_checks_the_handle_ownership_before_it_refuses() {
    // A wrong handle and a missing capability are different faults. The host
    // can drop the first; it can only report the second.
    let provider = adapter(LegacyFake::new());
    let foreign = ResourceHandle::issue("someone-else", "app", 7);
    let session_handle = ref_for(&foreign, "tx-1", SessionHandleKind::Transaction);

    let error = provider
        .close_handle(&foreign, &session_handle)
        .await
        .expect_err("a foreign handle cannot be closed");
    assert!(
        matches!(error, ResourceError::ResourceOwnershipMismatch { .. }),
        "expected an ownership mismatch, got {error:?}"
    );
}

#[tokio::test]
async fn the_adapter_refuses_every_handle_kind_and_leaves_the_handle_open() {
    // Nothing in the refusal path may mark the handle closed: the host keys
    // "this resource is done" off that flag, so a refusal that flipped it would
    // be indistinguishable from a successful close.
    let provider = adapter(LegacyFake::new());
    let handle = ResourceHandle::issue("legacy-fake", "app", 7);

    for kind in [
        SessionHandleKind::Transaction,
        SessionHandleKind::Cursor,
        SessionHandleKind::ServerPrepared,
    ] {
        let session_handle = ref_for(&handle, "handle-1", kind);
        let error = provider
            .close_handle(&handle, &session_handle)
            .await
            .expect_err("a kind the legacy surface cannot hold cannot be closed");
        assert!(
            matches!(
                error,
                ResourceError::OperationNotSupported {
                    ref operation,
                    ref reason,
                    ..
                } if operation == "close_handle"
                    && reason.contains("handle-1")
                    && reason.contains(&format!("{kind:?}"))
            ),
            "the refusal must name the handle and its kind, got {error:?}"
        );
        assert!(!handle.is_closed());
    }
}

/// §6.5's one legitimate repeat, and the only piece of the trait doc the legacy
/// surface was not honouring.
///
/// `close_resource` has already run, so every handle issued on the resource went
/// with it and repeating the close carries no information — the adapter must say
/// so rather than refuse. This is operational, not cosmetic: the host is
/// fail-closed on `Err` (`LeaseManager::release` keeps the lease exactly as it
/// is, un-pooled), so a refusal on this path parks the lease forever over a close
/// that already happened. The legacy driver never mints a session-scoped handle,
/// which is what makes the `Ok` honest: nothing is being claimed closed that was
/// not already gone.
#[tokio::test]
async fn an_already_closed_resource_makes_the_handle_close_a_success() {
    let fake = LegacyFake::new();
    let disconnect_calls = fake.disconnect_calls.clone();
    let provider = adapter(fake);
    let handle = provider
        .acquire_resource(
            &super::tests::acquire_request(),
            &super::tests::port_for_test(),
        )
        .await
        .expect("a legacy driver acquires through the adapter");

    // `close_resource` is what actually ends the resource — and now records the
    // fact on the handle, which is what makes the branch below reachable at all
    // in production rather than only in tests.
    provider
        .close_resource(&handle)
        .await
        .expect("the resource closes");
    assert!(handle.is_closed());
    let disconnects_after_close = disconnect_calls.load(std::sync::atomic::Ordering::SeqCst);

    provider
        .close_handle(
            &handle,
            &ref_for(&handle, "tx-1", SessionHandleKind::Transaction),
        )
        .await
        .expect("the resource is already closed, so this close already happened");

    // Neither side of the ledger moved: no connection was dropped, and the
    // resource's own close remains the only thing that ever released it.
    assert!(handle.is_closed());
    // The wire side proves the `Ok` was a report, not a second close: the only
    // `disconnect` is the one `close_resource` already performed.
    assert_eq!(
        disconnect_calls.load(std::sync::atomic::Ordering::SeqCst),
        disconnects_after_close,
        "an idempotent close must not disconnect the driver a second time"
    );
}

/// Ordering, not just presence: `is_closed` is checked *after* ownership, so a
/// closed resource never excuses a ref belonging to another one. That is the
/// order postgres uses and the order §6.5 describes; sharing one order across
/// every provider is worth more than any convenience a shorter path would buy.
///
/// The premise has to land on the handle actually handed to `close_handle`,
/// not merely on some handle in the test: a *foreign* handle that was never
/// closed yields the same ownership refusal in both orders, so the test would
/// pass even with the short-circuit hoisted above `handle.check`. So the one
/// passed in is marked closed too — that is the only shape where the two
/// orders disagree and this test pins which one runs.
#[tokio::test]
async fn a_foreign_ref_is_still_refused_on_a_closed_resource() {
    let provider = adapter(LegacyFake::new());
    let foreign = ResourceHandle::issue("someone-else", "app", 7);
    foreign.mark_closed();

    let error = provider
        .close_handle(
            &foreign,
            &ref_for(&foreign, "tx-1", SessionHandleKind::Transaction),
        )
        .await
        .expect_err("a closed resource must not excuse a ref from another one");

    assert!(
        matches!(error, ResourceError::ResourceOwnershipMismatch { .. }),
        "expected an ownership mismatch, got {error:?}"
    );
}
