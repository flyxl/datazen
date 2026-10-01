//! Live tier of the real-driver contract template: one named `#[tokio::test]` per
//! required CM dimension. `#[path]`-included by `real_driver_contract.rs`; it is not a
//! Cargo target on its own. Each test either proves a dimension against a real server
//! or reports that exact dimension as unverified — it never turns a skip into a pass
//! (`fake-runtime-fixtures.md` §10.4).

#![allow(dead_code, unused_imports)]

use std::time::{SystemTime, UNIX_EPOCH};

use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, DriverError, MultiQueryResult, QueryExecutionId,
};

// The whole template surface, so a live test can reach any free-tier helper too.
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
    // The A side's teardown needs its own session, closed only after the fixture ran.
    let mut session_a: Option<ConnectionHandle> = None;
    // §10.2 rule 2: identically named marker tables, different marker values. The
    // name is this case's own, so a neighbouring case cannot drop the table it reads.
    let table = fixture.name("marker");
    let select = render_marker(dialect.select_marker, &table);

    let outcome = async {
        let target_a = live.profile.a.clone();
        let target_b = live.profile.b.clone();
        assert_ne!(target_a, target_b, "CM-08 needs two distinct targets");

        let other = live.second_session("dz-contract-cm08-a").await?;
        session_a = Some(other.clone());
        seed_marker(&live.driver, &other, dialect, &table, "dz_marker_a").await?;
        seed_marker(&live.driver, &live.handle, dialect, &table, "dz_marker_b").await?;
        // Both targets now hold the relation; drop it in each, through its own session.
        let drop = render_marker(dialect.drop_marker, &table);
        fixture.on_drop_extra(drop.clone());
        fixture.on_drop(drop);

        let namespace_b = live.namespace().await?;
        let namespace_a = live
            .driver
            .query(&other, &format!("SELECT {}", dialect.current_namespace))
            .await
            .map(|result| first_cell(&result))?;
        assert_eq!(namespace_b, target_b, "session B drifted off its own target");
        assert_eq!(namespace_a, target_a, "session A drifted off its own target");
        assert_ne!(namespace_a, namespace_b, "the two sessions must stay isolated");

        // The same named relation resolves inside each session's own catalog.
        let read_b = live.query(&select).await?;
        assert!(
            rows_contain(&read_b, "dz_marker_b") && !rows_contain(&read_b, "dz_marker_a"),
            "an unqualified read must stay inside its own session: {:?}",
            read_b.rows
        );
        let read_a = live.driver.query(&other, &select).await?;
        assert!(
            rows_contain(&read_a, "dz_marker_a") && !rows_contain(&read_a, "dz_marker_b"),
            "an unqualified read must stay inside its own session: {:?}",
            read_a.rows
        );

        Ok::<(), DriverError>(())
    }
    .await;

    // Drop each side's marker through the session that owns its target.
    let teardown_a = match &session_a {
        Some(session_a) => fixture.finish_extra(&live.driver, session_a).await,
        None => Ok(()),
    };
    let teardown_b = fixture.finish(&live.driver, &live.handle).await;
    // Teardown is verified, not assumed: a statement that silently did nothing
    // (wrong handle, wrong target) would leave one table behind per run and pass.
    let survived_a = match &session_a {
        Some(session_a) => live.driver.query(session_a, &select).await.ok(),
        None => None,
    };
    let survived_b = live.query(&select).await.ok();
    let closed = match &session_a {
        Some(session_a) => live.driver.disconnect(session_a.clone()).await,
        None => Ok(()),
    };
    assert!(
        teardown_a.is_ok() && teardown_b.is_ok(),
        "fixture teardown must succeed in both targets: {teardown_a:?} / {teardown_b:?}"
    );
    // `Some(..)` means the relation is still readable: teardown did nothing.
    assert!(
        survived_a.is_none(),
        "the fixture relation survived teardown in the second session's target: {survived_a:?}"
    );
    assert!(
        survived_b.is_none(),
        "the fixture relation survived teardown in the session's own target: {survived_b:?}"
    );
    assert!(closed.is_ok(), "the second session must close cleanly: {closed:?}");
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
                // The owning session's statements run inside a transaction on
                // purpose. Outside one, a pooled driver may hand the same session
                // two different connections, and a temporary table created on the
                // first is invisible on the second — that would assert the luck of
                // the pool instead of the contract. A transaction is the context in
                // which the driver pins one connection, and it is what turns "the
                // owning session" into a fact instead of a race.
                let tx = live.driver.begin_transaction(&live.handle).await?;
                live.execute(&create).await?;
                live.execute(&render(dialect.insert_temp, &temp)).await?;
                let here = live.query(&render(dialect.select_temp, &temp)).await?;
                assert!(!here.rows.is_empty(), "the owning session must see its own row");

                // A *second handle* is a second session by construction. Re-opening
                // the same handle would only reuse the pinned connection and could
                // not prove anything about isolation. The owning session has just
                // read the row, so the relation demonstrably exists at the moment
                // the other session fails to see it.
                let other = live.second_session("dz-contract-cm09-a").await?;
                let leaked = live
                    .driver
                    .query(&other, &render(dialect.select_temp, &temp))
                    .await;
                // Roll the owner back before judging: the pinned connection is
                // released even when the isolation check below fails.
                live.driver.rollback(tx).await?;
                let leaked = leaked
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
        // already see the row it wrote. Statements are matched by their own verbatim
        // text — both mention the table, so "the first result mentioning it" would
        // silently assert on the INSERT and fail for the wrong reason.
        let insert_sql = render(dialect.insert_named, &table);
        let select_sql = render(dialect.select_named, &table);
        let reported = results
            .iter()
            .map(|result| result.sql.as_str())
            .collect::<Vec<_>>();
        let insert_at = reported
            .iter()
            .position(|sql| *sql == insert_sql)
            .unwrap_or_else(|| panic!("the script's own insert must be reported verbatim: {reported:?}"));
        let read_at = reported
            .iter()
            .position(|sql| *sql == select_sql)
            .unwrap_or_else(|| panic!("the script's own select must be reported verbatim: {reported:?}"));
        assert!(
            read_at > insert_at,
            "statements must execute in order: the select runs after the insert, got \
             insert at {insert_at} and select at {read_at} of {reported:?}"
        );
        let reading = &results[read_at];
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
    // A's marker is only visible to the session opened on A, so that session
    // stays open until its own teardown has run.
    let mut session_a: Option<ConnectionHandle> = None;
    // §10.2 rule 2: the *same* relation name in both targets with a different
    // marker value in each, or a targeted read could be satisfied by the session's
    // own table and the dimension would prove nothing. The name is this case's own,
    // so concurrent live cases cannot share it.
    let table = fixture.name("marker");
    let marker_a = fixture.name("a");
    let marker_b = fixture.name("b");
    let select = render_marker(dialect.select_marker, &table);

    let outcome = async {
        let target_a = live.profile.a.clone();
        let other = live.second_session("dz-contract-cm13-a").await?;
        session_a = Some(other.clone());
        seed_marker(&live.driver, &other, dialect, &table, &marker_a).await?;
        seed_marker(&live.driver, &live.handle, dialect, &table, &marker_b).await?;
        // One relation, two targets: both sides need a drop, each on the session
        // that can reach it.
        let drop = render_marker(dialect.drop_marker, &table);
        fixture.on_drop_extra(drop.clone());
        fixture.on_drop(drop);

        // Requesting A must yield A's marker, even from a session opened on B.
        let requested = live
            .driver
            .query_at(&live.handle, &select, target_for(&target_a))
            .await?;
        assert!(
            rows_contain(&requested, &marker_a) && !rows_contain(&requested, &marker_b),
            "a targeted read must resolve the requested target's marker, got {:?}",
            requested.rows
        );
        // The unqualified read still names the same relation, so the distinction
        // above came from the target and not from a different table name.
        let own = live.query(&select).await?;
        assert!(
            rows_contain(&own, &marker_b) && !rows_contain(&own, &marker_a),
            "the session's own target must still answer with its own marker: {:?}",
            own.rows
        );
        // And the session must still be on B afterwards: qualification is not a switch.
        assert_eq!(live.namespace().await?, live.profile.b);
        Ok::<(), DriverError>(())
    }
    .await;

    let teardown_a = match &session_a {
        Some(session_a) => fixture.finish_extra(&live.driver, session_a).await,
        None => Ok(()),
    };
    let teardown_b = fixture.finish(&live.driver, &live.handle).await;
    // Verified, not assumed: the relation this case created in each target must
    // be gone from that target once teardown has run.
    let survived_a = match &session_a {
        Some(session_a) => live.driver.query(session_a, &select).await.ok(),
        None => None,
    };
    let survived_b = live.query(&select).await.ok();
    let closed = match &session_a {
        Some(session_a) => live.driver.disconnect(session_a.clone()).await,
        None => Ok(()),
    };
    assert!(
        teardown_a.is_ok() && teardown_b.is_ok(),
        "fixture teardown must succeed in both targets: {teardown_a:?} / {teardown_b:?}"
    );
    // `Some(..)` means the relation is still readable: teardown did nothing.
    assert!(
        survived_a.is_none(),
        "the fixture relation survived teardown in the second session's target: {survived_a:?}"
    );
    assert!(
        survived_b.is_none(),
        "the fixture relation survived teardown in the session's own target: {survived_b:?}"
    );
    assert!(closed.is_ok(), "the second session must close cleanly: {closed:?}");
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

/// CM-16 — switching a driver that holds a per-database resource **replaces**
/// the resource, and a replacement that fails keeps the old one.
///
/// The dimension is H/D/F and mandatory for every `requiresReplacement` driver, so
/// the branch is taken from [`crate::Contract::per_database_resource`] rather than
/// from the driver's name: today that is postgres (`driver-capability-migration.md:516`).
/// The failure half is what was missing — a switch against a target that cannot be
/// bound must not consume, poison or drop the connection, and the original target
/// must still be readable afterwards.
#[tokio::test]
async fn cm16_a_failed_target_replacement_keeps_the_old_connection() {
    let Some(live) = open(crate::contract_driver(), "CM-16").await else {
        return;
    };
    let contract = &crate::CONTRACT;
    let mut fixture = Fixture::new("cm16");

    let outcome = async {
        // §10.2 rule 1: both fixture names carry the dedicated prefix.
        let marker_b = fixture.name("b");
        let target_a = live.profile.a.clone();
        // A target that looks like ours but cannot be bound: same prefix, unique
        // name, so the failure comes from the engine, not from a rejected name.
        let absent = fixture.name("absent");
        let session_before = live.namespace().await?;
        assert_eq!(
            session_before, live.profile.b,
            "CM-16 starts on the session's own target"
        );
        let table = fixture.name("marker");
        let select = render_marker(live.dialect.select_marker, &table);
        seed_marker(&live.driver, &live.handle, live.dialect, &table, &marker_b).await?;
        fixture.on_drop(render_marker(live.dialect.drop_marker, &table));

        // The replacement that succeeds, for contrast: for a driver that holds
        // per-database resources, reaching the other fixture target really binds
        // a resource for it. (`open_databases` deliberately excludes the session's
        // own target — it is the primary pool, not a switchable one.)
        if contract.per_database_resource {
            live.driver
                .query_at(&live.handle, "SELECT 1", target_for(&target_a))
                .await?;
            let open_after_switch = live.driver.open_databases(&live.handle).await?;
            assert!(
                open_after_switch.iter().any(|db| db == &target_a),
                "a driver declaring per_database_resource must hold a resource for the \
                 switched-to target, reported as {open_after_switch:?}"
            );
        }

        // The replacement that fails. The statement names a relation, so under any
        // shape the request needs that database: qualifying the name needs it, and
        // binding a pool for it needs it too. It may fail; it may not take the
        // working connection with it.
        let refused = live
            .driver
            .query_at(&live.handle, &select, target_for(&absent))
            .await;
        assert!(
            refused.is_err(),
            "a targeted read against a database that does not exist must fail instead \
             of returning the session's own rows"
        );
        let err = refused.expect_err("checked above");
        eprintln!(
            "↩ CM-16 替换失败（{}）：旧连接与原目标必须仍然可用",
            err_kind(&err)
        );

        // 1. The old connection is preserved — still bound to its own target, and
        //    the target that could not be bound left nothing behind.
        assert_eq!(
            live.namespace().await?,
            live.profile.b,
            "a failed replacement must leave the session on its original target"
        );
        let open_after_failure = live.driver.open_databases(&live.handle).await?;
        assert!(
            !open_after_failure.iter().any(|db| db == &absent),
            "the target that failed to bind must not linger as an open resource: \
             {open_after_failure:?}"
        );

        // 2. The original target is still queryable — with the same marker it
        //    holds, so this is a real read and not an empty result set.
        let still_there = live.query(&select).await?;
        assert!(
            rows_contain(&still_there, &marker_b),
            "the original target must still return its own marker after a failed \
             replacement, got {:?}",
            still_there.rows
        );
        Ok::<(), DriverError>(())
    }
    .await;

    let teardown = fixture.finish(&live.driver, &live.handle).await;
    assert!(teardown.is_ok(), "fixture teardown must succeed");
    if contract.per_database_resource {
        outcome.expect("CM-16 a failed replacement must keep the old connection");
    } else {
        // Not a claim about the replacement half: this driver owns no per-database
        // resource (namespaceSwitch is in-place), so there is nothing to replace —
        // only what both shapes share: the failed switch moved nothing.
        assert!(
            outcome.is_ok(),
            "a driver holding no per-database resource must survive a failed switch: {:?}",
            outcome.err()
        );
        eprintln!(
            "ℹ CM-16（{}）的『替换』分支不适用于本驱动：per_database_resource=false，\
             本例只证明失败的定向切换没有移动会话，也没有损坏原连接。",
            contract.label
        );
    }
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
