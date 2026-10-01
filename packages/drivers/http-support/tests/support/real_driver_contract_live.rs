//! Live tier of the real-driver contract template: one named `#[tokio::test]` per
//! required CM dimension. `#[path]`-included by `real_driver_contract.rs`; it is not a
//! Cargo target on its own. Each test either proves a dimension against a real server
//! or reports that exact dimension as unverified — it never turns a skip into a pass
//! (`fake-runtime-fixtures.md` §10.4).

#![allow(dead_code, unused_imports)]

use std::time::{SystemTime, UNIX_EPOCH};

use datazen_driver_api::{DatabaseDriver, DriverError, MultiQueryResult, QueryExecutionId};

// The whole template surface, so a live test can reach any helper the free tier
// also uses. `#[path]`-included, so nothing here is a public API of the crate.
use super::*;

/// Open the live tier for one dimension, or return `None` after saying which
/// dimension went unverified and why.
async fn open<D: DatabaseDriver>(driver: D, dimension: &'static str) -> Option<Live<D>> {
    super::open_live(driver, dimension).await
}

// ---------------------------------------------------------------------------
// Live tier — one named test per dimension
// ---------------------------------------------------------------------------

/// CM-08 — two independent sessions each stay on their own database; the same
/// named relation resolves to that session's own catalog.
#[tokio::test]
async fn cm08_two_sessions_keep_independent_databases() {
    let Some(live) = open(crate::contract_driver(), "CM-08").await else {
        return;
    };
    let dialect = live.dialect;
    let mut fixture = Fixture::new("cm08");

    let outcome = async {
        let target_a = live.profile.a.clone();
        let target_b = live.profile.b.clone();
        assert_ne!(target_a, target_b, "CM-08 needs two distinct targets");

        let other = live.second_session("dz-contract-cm08-a").await?;
        // §10.2 rule 2: identically named marker tables, different marker values.
        seed_marker(&live.driver, &other, dialect, "dz_marker_a").await?;
        seed_marker(&live.driver, &live.handle, dialect, "dz_marker_b").await?;
        fixture.on_drop(dialect.drop_marker.to_string());

        let namespace_b = live.namespace().await?;
        let namespace_a = live
            .driver
            .query(&other, &format!("SELECT {}", dialect.current_namespace))
            .await
            .map(|result| first_cell(&result))?;
        assert_eq!(namespace_b, target_b, "session B drifted off its own target");
        assert_eq!(namespace_a, target_a, "session A drifted off its own target");
        assert_ne!(namespace_a, namespace_b, "the two sessions must stay isolated");

        // The same named relation resolves inside each session's own catalog:
        // an unqualified read never crosses over to the other session's target.
        let read_b = live.query(dialect.select_marker).await?;
        assert!(
            rows_contain(&read_b, "dz_marker_b") && !rows_contain(&read_b, "dz_marker_a"),
            "an unqualified read must stay inside its own session: {:?}",
            read_b.rows
        );
        let read_a = live.driver.query(&other, dialect.select_marker).await?;
        assert!(
            rows_contain(&read_a, "dz_marker_a") && !rows_contain(&read_a, "dz_marker_b"),
            "an unqualified read must stay inside its own session: {:?}",
            read_a.rows
        );

        live.driver.disconnect(other).await
    }
    .await;

    let teardown = fixture.finish(&live.driver, &live.handle).await;
    assert!(teardown.is_ok(), "fixture teardown must succeed");
    outcome.expect("CM-08 two independent sessions");
}

/// CM-09 — session-scoped temporary state stays inside the session that made it.
#[tokio::test]
async fn cm09_session_scoped_state_stays_in_its_own_session() {
    let Some(live) = open(crate::contract_driver(), "CM-09").await else {
        return;
    };
    let dialect = live.dialect;
    let mut fixture = Fixture::new("cm09");

    let outcome = async {
        let temp = fixture.name("tmp");
        let create = render(dialect.create_temp, &temp);
        match crate::CONTRACT.session_scoped_state {
            Capability::Supported => {
                live.execute(&create).await?;
                live.execute(&render(dialect.insert_temp, &temp)).await?;
                let here = live.query(&render(dialect.select_temp, &temp)).await?;
                assert!(!here.rows.is_empty(), "the owning session must see its own row");
                let other = live.second_session("dz-contract-cm09-a").await?;
                let leaked = live
                    .driver
                    .query(&other, &render(dialect.select_temp, &temp))
                    .await
                    .expect_err("a session-scoped object must not be visible to another session");
                assert_eq!(err_kind(&leaked), "QueryFailed");
                live.driver.disconnect(other).await
            }
            _ => {
                // Missing capability: the contract requires an explicit refusal, not
                // a skip that goes on to claim the dimension was verified.
                let err = live
                    .execute(&create)
                    .await
                    .expect_err("a driver without session-scoped state must refuse, not accept");
                assert!(
                    matches!(err, DriverError::Unsupported(_) | DriverError::NotSupported(_)),
                    "expected an explicit Unsupported refusal, got {}",
                    err_kind(&err)
                );
                Ok(())
            }
        }
    }
    .await;

    let teardown = fixture.finish(&live.driver, &live.handle).await;
    assert!(teardown.is_ok(), "fixture teardown must succeed");
    outcome.expect("CM-09 session-scoped state");
}

/// CM-10 — a raw script keeps its order and leaves the session on its final target.
#[tokio::test]
async fn cm10_script_order_is_preserved_and_final_target_is_last() {
    let Some(live) = open(crate::contract_driver(), "CM-10").await else {
        return;
    };
    let dialect = live.dialect;
    let mut fixture = Fixture::new("cm10");

    let outcome = async {
        // §10.2 rule 3: a uniquely named table, dropped again in teardown.
        let table = fixture.name("ordered");
        let drop_table = render(dialect.drop_named, &table);
        fixture.on_drop(drop_table);
        live.execute(&render(dialect.create_named, &table)).await?;
        let script = format!(
            "{}; {}; SELECT {}",
            render(dialect.insert_named, &table),
            render(dialect.select_named, &table),
            dialect.current_namespace
        );
        let MultiQueryResult { results, .. } = live.driver.query_multi(&live.handle, &script, None).await?;
        assert!(
            results.len() >= 3,
            "every statement in a raw script must produce its own result, got {}",
            results.len()
        );
        // Order is part of the contract: the select that runs after the insert must
        // already see the row it wrote.
        let reading = results
            .iter()
            .find(|result| result.sql.contains(&table))
            .expect("the script's own select must be reported");
        assert!(
            statement_rows_contain(reading, "42"),
            "statements must execute in order: the select must already see the insert"
        );
        let last = results.last().expect("a non-empty script");
        assert_eq!(
            statement_first_cell(last),
            live.profile.b,
            "the session must end on the script's final target"
        );
        Ok::<(), DriverError>(())
    }
    .await;

    let teardown = fixture.finish(&live.driver, &live.handle).await;
    assert!(teardown.is_ok(), "fixture teardown must succeed");
    outcome.expect("CM-10 raw script order");
}

/// CM-13 / CM-15 — an explicitly targeted statement reads the marker of the
/// requested target, not of the session's own catalog, and never switches.
#[tokio::test]
async fn cm13_cross_target_resolution_reads_the_requested_target() {
    let Some(live) = open(crate::contract_driver(), "CM-13").await else {
        return;
    };
    let dialect = live.dialect;
    let mut fixture = Fixture::new("cm13");

    let outcome = async {
        let marker_a = fixture.name("a");
        let marker_b = fixture.name("b");
        let target_a = live.profile.a.clone();
        let other = live.second_session("dz-contract-cm13-a").await?;
        seed_marker(&live.driver, &other, dialect, &marker_a).await?;
        seed_marker(&live.driver, &live.handle, dialect, &marker_b).await?;
        fixture.on_drop(dialect.drop_marker.to_string());

        // Requesting A must yield A's marker, even from a session opened on B.
        let requested = live
            .driver
            .query_at(&live.handle, dialect.select_marker, target_for(&target_a))
            .await?;
        assert!(
            rows_contain(&requested, &marker_a) && !rows_contain(&requested, &marker_b),
            "a targeted read must resolve the requested target's marker, got {:?}",
            requested.rows
        );
        // And the session must still be on B afterwards: qualification is not a switch.
        assert_eq!(live.namespace().await?, live.profile.b);
        live.driver.disconnect(other).await
    }
    .await;

    let teardown = fixture.finish(&live.driver, &live.handle).await;
    assert!(teardown.is_ok(), "fixture teardown must succeed");
    outcome.expect("CM-13 cross-target resolution");
}

/// CM-14 — the switch keyword inside a string literal must not move the session.
/// Text that merely looks like a switch is not a switch.
#[tokio::test]
async fn cm14_text_containing_the_switch_keyword_never_switches() {
    let Some(live) = open(crate::contract_driver(), "CM-14").await else {
        return;
    };
    let dialect = live.dialect;

    let outcome = async {
        let before = live.namespace().await?;
        let result = live.query(dialect.decoy_text_select).await?;
        assert!(
            rows_contain(&result, dialect.switch_keyword.trim()),
            "the decoy statement must round-trip its own text"
        );
        assert_eq!(
            before,
            live.namespace().await?,
            "text that merely contains the switch keyword must not change context"
        );
        Ok::<(), DriverError>(())
    }
    .await;

    outcome.expect("CM-14 keyword-shaped text must not switch database");
}

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
                    matches!(err, DriverError::TransactionError(_) | DriverError::Unsupported(_)),
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
                live.driver.prepare_query_execution(&live.handle, &execution).await?;
                // Cleanup is idempotent and makes the handle stale by design.
                live.driver.cleanup_query_execution(&live.handle, &execution).await?;
                live.driver.cleanup_query_execution(&live.handle, &execution).await?;
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
                assert!(live.driver.cancel_query_with_execution(&live.handle, &forged).await.is_err());
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
            matches!(err, DriverError::Unsupported(_) | DriverError::NotSupported(_)),
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

    if !crate::CONTRACT.reset_for_reuse.is_present() {
        // Fail closed: without a proven reset, nothing may claim the resource is
        // clean. There is no `resetResource` on today's trait surface to assert,
        // so the obligation is recorded here and the API gap is reported.
        eprintln!(
            "note: {} declares reset_for_reuse = {:?}; the trait has no resetResource yet, \
             so only the non-blind-reuse half of CM-26 is asserted.",
            crate::CONTRACT.label,
            crate::CONTRACT.reset_for_reuse
        );
    }
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
                matches!(error, DriverError::Unsupported(_) | DriverError::TransactionError(_)),
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
            let closed = live
                .driver
                .close_database(&live.handle, &target_a)
                .await?;
            assert!(
                closed,
                "a driver holding a per-database resource must report the close"
            );
        } else {
            let closed = live
                .driver
                .close_database(&live.handle, &target_a)
                .await?;
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
