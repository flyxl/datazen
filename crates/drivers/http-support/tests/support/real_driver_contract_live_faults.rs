//! Live tier, second half: the CM dimensions whose subject is what a driver does
//! **after something goes wrong** — a hand-written rollback (CM-17), result typing
//! (CM-18), multiple result sets (CM-22), cancelling a query the driver never
//! registered (CM-24), a failed statement (CM-26), streaming (CM-48) and a
//! deliberately closed database (CM-69).
//!
//! Split from `real_driver_contract_live.rs` because that file had reached the
//! 800-line ceiling, and because the halves are different work: this one starts
//! from a failure or a teardown and has to leave the session in a state the next
//! test can trust. Keeping that visible beats listing fourteen tests in one file
//! where the ones that clean up after something are indistinguishable from the
//! ones that do not.
//!
//! Same contract as the rest of the live tier: either the dimension is proved
//! against a real server, or it is reported as unverified — never upgraded
//! (`fake-runtime-fixtures.md` §10.4). `#[path]`-included, never compiled alone.

#![allow(dead_code, unused_imports)]

use std::time::{SystemTime, UNIX_EPOCH};

use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, DriverError, MultiQueryResult, QueryExecutionId,
};

use super::live::open;
use super::scope::{partial_obligation, report_partial_obligation};
use super::*;

// ---------------------------------------------------------------------------
// Live tier, failure half — one named test per dimension
// ---------------------------------------------------------------------------

/// CM-17 — a hand-written transaction rolls back, and the row it wrote is gone.
#[tokio::test]
async fn cm17_hand_written_transaction_rolls_back_cleanly() {
    let Some(live) = open(crate::contract_driver(), "CM-17").await else {
        return;
    };
    let dialect = live.dialect;
    let mut fixture = Fixture::new("cm17");
    // §10.2 rule 3/4: the name is decided up front so teardown can always target it,
    // whether the assertions passed, failed, or were never reached.
    let table = fixture.name("tx");
    fixture.on_drop(render(dialect.drop_named, &table));

    let outcome = async {
        let create = render(dialect.create_named, &table);
        let insert = render(dialect.insert_named, &table);
        let select = render(dialect.select_named, &table);
        match crate::CONTRACT.transactions {
            Capability::Supported => {
                live.execute(&create).await?;
                let other = live.second_session("dz-contract-cm17-a").await?;
                let transaction = live.driver.begin_transaction(&live.handle).await?;
                live.execute(&insert).await?;
                let uncommitted = live
                    .driver
                    .query(&other, &select)
                    .await
                    .expect_err("an open transaction must stay invisible to other sessions");
                assert_eq!(err_kind(&uncommitted), "QueryFailed");
                live.driver.rollback(transaction).await?;
                // The data is gone, and the session is usable with no active handle.
                let after = live.query(&select).await;
                assert!(
                    after.map(|r| r.rows.is_empty()).unwrap_or(true),
                    "rolled-back data must not be readable"
                );
                live.namespace().await?;
                live.driver.disconnect(other).await
            }
            _ => {
                let err = live
                    .driver
                    .begin_transaction(&live.handle)
                    .await
                    .expect_err("a driver without transactions must refuse, not silently succeed");
                assert!(
                    matches!(
                        err,
                        DriverError::TransactionError(_) | DriverError::Unsupported(_)
                    ),
                    "expected an explicit transaction refusal, got {}",
                    err_kind(&err)
                );
                Ok(())
            }
        }
    }
    .await;

    let teardown = fixture.finish(&live.driver, &live.handle).await;
    assert!(teardown.is_ok(), "fixture teardown must succeed");
    outcome.expect("CM-17 hand-written transaction journey");
}

/// CM-18 — a failed statement leaves the next operation usable; an aborted or
/// unknown state must never be reported as "nothing happened".
#[tokio::test]
async fn cm18_failed_statement_leaves_the_next_operation_working() {
    let Some(live) = open(crate::contract_driver(), "CM-18").await else {
        return;
    };
    let dialect = live.dialect;

    let outcome = async {
        let err = live
            .query(&format!("SELECT * FROM {}", dialect.missing_object))
            .await
            .expect_err("a statement against a missing relation must fail");
        assert_eq!(err_kind(&err), "QueryFailed");
        // Recovery: the very next statement must work on the same session.
        live.namespace().await?;
        Ok::<(), DriverError>(())
    }
    .await;

    outcome.expect("CM-18 recovery after a failed statement");
}

/// CM-22 / CM-23 — cancellation is addressed to one execution: an execution that
/// was never registered, or that has already finished, is rejected rather than
/// turned into a session-wide cancel.
#[tokio::test]
async fn cm22_precise_cancel_is_addressed_to_one_execution() {
    let Some(live) = open(crate::contract_driver(), "CM-22").await else {
        return;
    };

    let outcome = async {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let execution = QueryExecutionId::new(format!("{FIXTURE_PREFIX}unregistered_{stamp}"));
        match crate::CONTRACT.precise_cancel {
            Capability::Supported => {
                live.driver
                    .prepare_query_execution(&live.handle, &execution)
                    .await?;
                // Cleanup is idempotent and makes the handle stale by design.
                live.driver
                    .cleanup_query_execution(&live.handle, &execution)
                    .await?;
                live.driver
                    .cleanup_query_execution(&live.handle, &execution)
                    .await?;
                let err = live
                    .driver
                    .cancel_query_with_execution(&live.handle, &execution)
                    .await
                    .expect_err("a stale execution handle must be rejected, not honoured");
                assert!(
                    matches!(
                        err,
                        DriverError::QueryExecutionNotFound(_) | DriverError::Unsupported(_)
                    ),
                    "a finished/stale execution must be refused, got {}",
                    err_kind(&err)
                );
                // A forged id that was never prepared is refused the same way.
                let forged = QueryExecutionId::new(format!("{FIXTURE_PREFIX}forged_{stamp}"));
                assert!(live
                    .driver
                    .cancel_query_with_execution(&live.handle, &forged)
                    .await
                    .is_err());
            }
            _ => {
                let err = live
                    .driver
                    .cancel_query_with_execution(&live.handle, &execution)
                    .await
                    .expect_err("a driver without precise cancel must refuse explicitly");
                assert!(
                    matches!(err, DriverError::Unsupported(_)),
                    "expected Unsupported, got {}",
                    err_kind(&err)
                );
            }
        }
        // Either way the session is untouched: E2 was not cancelled by proxy.
        live.namespace().await?;
        Ok::<(), DriverError>(())
    }
    .await;

    outcome.expect("CM-22/CM-23 precise cancel is addressed to one execution");
}

/// CM-24 — with no session-wide fallback, the legacy cancel must not be used to
/// paper over a missing precise cancel: it must refuse and say why.
#[tokio::test]
async fn cm24_cancel_never_falls_back_to_session_wide() {
    let Some(live) = open(crate::contract_driver(), "CM-24").await else {
        return;
    };

    let outcome = async {
        let err = live
            .driver
            .cancel_query(&live.handle)
            .await
            .expect_err("a session-wide cancel must never be a silent success");
        assert!(
            matches!(
                err,
                DriverError::Unsupported(_) | DriverError::NotSupported(_)
            ),
            "session-wide cancel must be explicitly refused, got {}",
            err_kind(&err)
        );
        // Explicit close still works, which is what actually terminates work.
        live.namespace().await?;
        Ok::<(), DriverError>(())
    }
    .await;

    outcome.expect("CM-24 no session-wide cancel fallback");
}

/// CM-26 — a failure must not leave a resource that is blindly reused: the open
/// resource set stays coherent and the next operation works without repair.
#[tokio::test]
async fn cm26_failed_statement_resource_is_not_reused_blindly() {
    let Some(live) = open(crate::contract_driver(), "CM-26").await else {
        return;
    };
    let dialect = live.dialect;

    let err = live
        .query(&format!("SELECT * FROM {}", dialect.missing_object))
        .await
        .expect_err("the probe statement is expected to fail");
    assert_eq!(err_kind(&err), "QueryFailed");

    let open = live
        .driver
        .open_databases(&live.handle)
        .await
        .expect("the resource set must still be reportable after a failure");
    assert!(
        !open.iter().any(|db| db == &live.profile.a),
        "a target the driver never opened must not be reported as open: {open:?}"
    );
    live.namespace()
        .await
        .expect("the session must still be usable after a failure — no blind reuse");

    // The honest half, reported unconditionally.
    //
    // This used to sit behind `if !reset_for_reuse.is_present()`. Every driver
    // under test declares `reset_for_reuse = Supported` and `is_present()` reads
    // the declaration only, so the condition was constantly false, this note never
    // printed — two real live runs recorded zero occurrences — and the scope
    // report went on listing CM-26 among the dimensions that really executed and
    // passed. The branch is gone, not inverted: a capability declared present with
    // nothing testable behind it is not a reason to stay quiet, it is the reason
    // to say so out loud. The fact lives in one table row, so the live note and
    // the scope report cannot drift apart.
    report_partial_obligation(partial_obligation("CM-26").expect(
        "CM-26 must keep its partial-obligation row; without it the missing half is \
                     silently reported as a pass",
    ));
}

/// CM-48 — readers share one proven snapshot, or the driver refuses parallel
/// strict-consistency reads outright. Refusal is a pass; an unverified shared
/// read is not.
#[tokio::test]
async fn cm48_read_snapshots_are_shared_or_explicitly_refused() {
    let Some(live) = open(crate::contract_driver(), "CM-48").await else {
        return;
    };

    match live.driver.begin_read_snapshot(&live.handle).await {
        Ok(snapshot) => {
            let view = live
                .namespace()
                .await
                .expect("every reader must see the proven snapshot");
            assert_eq!(view, live.profile.b);
            live.driver
                .rollback(snapshot)
                .await
                .expect("a proven snapshot must be releasable");
        }
        Err(error) => {
            // An absent snapshot capability is an explicit refusal, not a skip.
            assert!(
                crate::CONTRACT.read_snapshots != Capability::Supported,
                "{} declares stable read snapshots but refused to start one ({})",
                crate::CONTRACT.label,
                err_kind(&error)
            );
            assert!(
                matches!(
                    error,
                    DriverError::Unsupported(_) | DriverError::TransactionError(_)
                ),
                "snapshot refusal must be explicit, got {}",
                err_kind(&error)
            );
        }
    }
}

/// CM-69 — a database that was closed stops being reported as open. A driver
/// that holds no per-database resource must instead report nothing at all, so
/// the tree never marks a database the user never opened.
#[tokio::test]
async fn cm69_closed_database_stops_being_reported_open() {
    let Some(live) = open(crate::contract_driver(), "CM-69").await else {
        return;
    };
    let contract = &crate::CONTRACT;

    let outcome = async {
        let target_a = live.profile.a.clone();
        if contract.per_database_resource {
            // Open the other target for real, then release it.
            let _ = live
                .driver
                .query_at(&live.handle, "SELECT 1", target_for(&target_a))
                .await;
            let closed = live.driver.close_database(&live.handle, &target_a).await?;
            assert!(
                closed,
                "a driver holding a per-database resource must report the close"
            );
        } else {
            let closed = live.driver.close_database(&live.handle, &target_a).await?;
            assert!(
                !closed,
                "a driver that holds no per-database resource must report nothing to close"
            );
        }
        let open = live.driver.open_databases(&live.handle).await?;
        assert!(
            !open.iter().any(|db| db == &target_a),
            "a closed database must stop being reported as open: {open:?}"
        );
        // Reopening on demand must still work.
        live.namespace().await?;
        Ok::<(), DriverError>(())
    }
    .await;

    outcome.expect("CM-69 closed database reporting");
}
