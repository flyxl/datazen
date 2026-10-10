//! Offline tests for Redis's per-handle close (§6.5).
//!
//! Split out of [`tests`](super::tests) rather than appended to it: that file is
//! already at the size ceiling, and this is a single method's behaviour.
//!
//! Redis issues no session-scoped handle at all — no cursor, no server-side
//! prepared statement, no multi-statement transaction — so
//! `session_scoped_handles` stays at its fail-closed `Unknown` and every
//! `ExecutionCompletion` reports an empty `session_handles` list. The only
//! honest answer to `close_handle` is therefore a refusal. These tests pin that
//! down, because the tempting wrong answer — `Ok(())` — is not a harmless
//! no-op: the host would deregister the ref and judge the session clean while
//! nothing was ever closed.
//!
//! That refusal is scoped to a **live** resource, though. §6.5 carves out one
//! repeat that is not a refusal: a handle the resource handle already marks
//! closed. The tests below hold both halves, because the two failure modes are
//! opposite — dropping the short-circuit parks a lease forever, and letting the
//! refusal run on a closed resource is the same "closed" a live one is owed.

use std::sync::Arc;

use datazen_driver_api::resource::{ResourceError, ResourceHandle, ResourceProvider};
use datazen_driver_api::session::{SessionHandleKind, SessionHandleRef};

use super::capabilities::REDIS_PROVIDER_ID;
use super::tests::{provider, seed_under, BudgetLedger};

/// A `SessionHandleRef` as the host would hand it over: same resource, same
/// epoch, a name. Redis has no such handle to name, which is the point.
fn session_handle_for(handle: &ResourceHandle) -> SessionHandleRef {
    SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        handle.resource_key().to_string(),
        handle.runtime_epoch(),
    )
}

#[tokio::test]
async fn close_handle_refuses_rather_than_claiming_a_close_that_never_happened() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed_under(&provider, &ledger, "redis_close_handle");

    let error = provider
        .close_handle(&handle, &session_handle_for(&handle))
        .await
        .expect_err("a provider that issues no session-scoped handle must not report one closed");

    assert!(
        matches!(
            error,
            ResourceError::OperationNotSupported { ref operation, ref reason, .. }
                if operation == "close_handle"
                    && reason.contains("tx-1")
                    && reason.contains("Transaction")
        ),
        "the refusal must name the handle and its kind, got {error:?}"
    );
    // Nothing was closed, so nothing may claim to be: the resource handle is
    // still open, and the permit the seed registered is untouched — neither
    // acquired again nor released. `seed_under` hands the permit in directly,
    // so an untouched ledger is the whole accounting story.
    assert!(!handle.is_closed());
    assert!(
        ledger.acquired().is_empty(),
        "a refusal must not touch the budget at all"
    );
    assert!(
        ledger.released().is_empty(),
        "a refusal must not release the permit"
    );
}

#[tokio::test]
async fn close_handle_refuses_for_every_handle_kind() {
    // The refusal is a property of the provider, not of the kind it was handed.
    // Redis issues none of the three, so none of the three may come back `Ok`.
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed_under(&provider, &ledger, "redis_close_handle_kinds");

    for kind in [
        SessionHandleKind::Transaction,
        SessionHandleKind::Cursor,
        SessionHandleKind::ServerPrepared,
    ] {
        let session_handle = SessionHandleRef::new(
            format!("{kind:?}-1"),
            kind,
            handle.resource_key().to_string(),
            handle.runtime_epoch(),
        );
        assert!(
            provider
                .close_handle(&handle, &session_handle)
                .await
                .is_err(),
            "a {kind:?} handle must be refused too"
        );
    }
    assert!(!handle.is_closed());
    assert!(ledger.released().is_empty());
}

#[tokio::test]
async fn close_handle_on_an_already_closed_resource_is_idempotent_success() {
    // §6.5's one legitimate repeat. `ResourceHandle::is_closed()` is the host's
    // own record that `close_resource` already ran, so every handle issued on
    // the resource went with it — and Redis issued none to begin with, so there
    // is nothing left to refuse. `Ok(())` is what the published trait doc
    // promises here, and the reason is operational rather than cosmetic:
    // `LeaseFacade::release` treats `Err` as fail-closed and keeps the lease
    // out of the pool, so a refusal on this path parks the lease forever over a
    // close that already happened.
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed_under(&provider, &ledger, "redis_close_handle_closed");
    handle.mark_closed();

    provider
        .close_handle(&handle, &session_handle_for(&handle))
        .await
        .expect("closing a handle on an already-closed resource is idempotent success");

    // The budget must be exactly where it was. `close_resource` is the only call
    // that releases the permit; this one must not release it again, and must not
    // charge anything either. `seed_under` hands the permit straight in, so an
    // untouched ledger is the whole accounting story.
    assert!(
        ledger.acquired().is_empty(),
        "an idempotent close must not re-charge the budget"
    );
    assert!(
        ledger.released().is_empty(),
        "an idempotent close must not release the permit a second time"
    );
    // And the flag is still the one `mark_closed` set: this call neither needs it
    // nor manufactures it, which is what makes the `Ok` above a report rather
    // than a side effect.
    assert!(handle.is_closed());
}

#[tokio::test]
async fn close_handle_refuses_a_foreign_ref_even_on_a_closed_resource() {
    // Ordering, not just presence: `is_closed` is checked *after* ownership, so
    // a closed resource never excuses a ref belonging to another one. That is
    // the order postgres uses and the order §6.5 describes — the closed flag
    // says "the close already happened", which is only true of refs this
    // provider really issued. Keeping all nine providers on one order is worth
    // more than any convenience a shorter path would buy.
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed_under(&provider, &ledger, "redis_close_handle_closed_foreign");
    handle.mark_closed();
    let foreign =
        ResourceHandle::issue("some-other-provider", "not-issued", handle.runtime_epoch());
    let session_handle = SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        "not-issued".to_string(),
        handle.runtime_epoch(),
    );

    let error = provider
        .close_handle(&foreign, &session_handle)
        .await
        .expect_err("a closed resource must not excuse a ref from another one");

    assert!(
        matches!(
            error,
            ResourceError::ResourceOwnershipMismatch { ref expected, ref actual }
                if expected == REDIS_PROVIDER_ID && actual == "some-other-provider"
        ),
        "expected an ownership mismatch, got {error:?}"
    );
}

#[tokio::test]
async fn close_handle_checks_ownership_before_it_refuses() {
    // A refusal must not also swallow a wrong-handle bug: the ownership check
    // runs first, so a handle from another provider is reported as exactly
    // that rather than as "this driver has no such handles".
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed_under(&provider, &ledger, "redis_close_handle_foreign");
    let foreign =
        ResourceHandle::issue("some-other-provider", "not-issued", handle.runtime_epoch());
    let session_handle = SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        "not-issued".to_string(),
        handle.runtime_epoch(),
    );

    let error = provider
        .close_handle(&foreign, &session_handle)
        .await
        .expect_err("a handle this provider never issued cannot be closed through it");

    assert!(
        matches!(
            error,
            ResourceError::ResourceOwnershipMismatch { ref expected, ref actual }
                if expected == REDIS_PROVIDER_ID && actual == "some-other-provider"
        ),
        "expected an ownership mismatch, got {error:?}"
    );
}

#[tokio::test]
async fn close_handle_leaves_the_resource_budget_exactly_where_it_was() {
    // The permit belongs to the *resource*, not to any session handle. A
    // refusal must leave it exactly where it was, or a caller that retries
    // would find the ledger short. `seed_under` registers under a distinct key
    // and hands the permit straight to the provider, so an untouched ledger is
    // exactly "the permit is still charged, unchanged".
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed_under(&provider, &ledger, "redis_close_handle_budget");

    let _ = provider
        .close_handle(&handle, &session_handle_for(&handle))
        .await;

    assert!(
        ledger.acquired().is_empty() && ledger.released().is_empty(),
        "a refusal must not acquire or release anything, got acquired {:?} released {:?}",
        ledger.acquired(),
        ledger.released()
    );
    // And the resource is still live, which is what makes the accounting above
    // meaningful rather than a vacuous truth about a ledger nobody charged.
    assert!(!handle.is_closed());
}
