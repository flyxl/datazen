//! Resource-reclamation facts the Host is responsible for.
//!
//! These run on the in-memory fixture, so they observe the Host's lease and
//! budget accounting rather than a database server's own connection count.

use crate::commands::driver_command::{execute_driver_command_impl, ExecuteDriverCommandRequest};
use crate::commands::release_connection_impl;
use crate::testing::app_state::{rich_mock_options, TestAppState};

fn query(session: &str) -> ExecuteDriverCommandRequest {
    ExecuteDriverCommandRequest {
        db_session_id: Some(session.to_owned()),
        driver_type: None,
        command: "query".into(),
        input: serde_json::json!({ "sql": "SELECT 1" }),
        database: None,
        schema: None,
    }
}

/// Repeating statements on one session must not accumulate physical resources.
///
/// This is the host-side half of the leak question: the fixture opens exactly
/// one resource for the session, executes many statements through the live
/// connection, and every snapshot (ledger + budget) stays flat until the
/// session is released.
#[tokio::test]
async fn repeated_statements_do_not_accumulate_physical_resources() {
    let test = TestAppState::with_resource_provider(rich_mock_options()).await;
    let (_config, session) = test.save_and_connect_runtime("resource-flat").await;

    let adapter = test.state.platform_require().expect("platform ready");
    let runtime = adapter.runtime().expect("runtime ready");
    let before = runtime
        .observability_snapshot()
        .await
        .expect("observability snapshot");
    assert_eq!(before.physical_total, 1, "one resource for one session");
    assert_eq!(before.budget_in_use, 1, "one physical permit in use");
    assert_eq!(
        before.physical_limit, 16,
        "desktop budget limit is unchanged"
    );
    let connects = test.mock.connect_calls();

    for _ in 0..32 {
        execute_driver_command_impl(&test.state, query(&session))
            .await
            .expect("statement succeeds on the live session");
    }

    let after = runtime
        .observability_snapshot()
        .await
        .expect("observability snapshot");
    assert_eq!(
        after.physical_total, before.physical_total,
        "no new resource"
    );
    assert_eq!(after.budget_in_use, before.budget_in_use, "no new permit");
    assert_eq!(after.physical_limit, before.physical_limit);
    assert_eq!(
        test.mock.connect_calls(),
        connects,
        "no extra driver connection was opened"
    );

    release_connection_impl(&test.state, session)
        .await
        .expect("release succeeds");
    let closed = runtime
        .observability_snapshot()
        .await
        .expect("observability snapshot");
    assert_eq!(closed.physical_total, 0, "release returns the resource");
    assert_eq!(closed.budget_in_use, 0, "release returns the permit");
}

/// Saving a profile bumps its revision. Sessions opened on the old revision are
/// unusable from then on, so the save must also return their quota instead of
/// leaving orphan sessions that pin physical connections until tab close.
#[tokio::test]
async fn saving_a_profile_retires_its_sessions_and_reconnect_succeeds() {
    let test = TestAppState::with_resource_provider(rich_mock_options()).await;
    test.save_connection("profile-retire").await;
    let first = test.open_runtime_session("profile-retire").await;
    let second = test.open_runtime_session("profile-retire").await;

    let adapter = test.state.platform_require().expect("platform ready");
    let runtime = adapter.runtime().expect("runtime ready");
    let before = runtime
        .observability_snapshot()
        .await
        .expect("observability snapshot");
    assert_eq!(before.physical_total, 2);
    assert_eq!(before.budget_in_use, 2);
    assert_eq!(runtime.registered_session_ids().len(), 2);

    let mut config = test
        .store
        .get_connection("profile-retire")
        .await
        .expect("profile exists");
    config.name = "Renamed profile".into();
    crate::commands::save_connection_impl(&test.state, config, false)
        .await
        .expect("save succeeds");

    assert!(
        runtime.registered_session_ids().is_empty(),
        "saving a profile closes every session on it"
    );
    let after = runtime
        .observability_snapshot()
        .await
        .expect("observability snapshot");
    assert_eq!(after.physical_total, 0, "physical resources returned");
    assert_eq!(after.budget_in_use, 0, "budget permits returned");
    for old in [&first, &second] {
        let error = execute_driver_command_impl(&test.state, query(old))
            .await
            .expect_err("a retired session is not reusable");
        assert!(
            matches!(error, crate::commands::CommandError::NotFound(_)),
            "retired session must be reported as gone, got {error:?}"
        );
    }

    let reconnected = test.open_runtime_session("profile-retire").await;
    let reopened = runtime
        .observability_snapshot()
        .await
        .expect("observability snapshot");
    assert_eq!(reopened.physical_total, 1, "reconnect opens one resource");
    assert_eq!(reopened.budget_in_use, 1);
    execute_driver_command_impl(&test.state, query(&reconnected))
        .await
        .expect("reconnected session executes");
}
