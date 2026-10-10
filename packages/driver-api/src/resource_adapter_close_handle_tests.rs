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
