//! T039: React `stores/activeConnectionStore.ts` parity. Every assertion
//! below pins one behavior of the store — the connect dedup, the
//! open-connection sequence (including capability-discovery failure staying
//! connected), the `markConnecting` 30-second timeout, the `markConnected`
//! capabilities write-back guard, the disconnect `finally`, the
//! remove-by-session drop-first order, `reset`, and `extractError`.

use datazen_ui::active_connection_store::{
    extract_error, now_ms, ActiveConnectionStore, ConnectRequest, ConnectStart,
    ConnectionEffect, ConnectionEntry, ConnectionStatus, DisconnectPlan, DriverCapabilities,
    ServerInfo, CONNECT_TIMEOUT_MS,
};

fn caps() -> DriverCapabilities {
    DriverCapabilities {
        supports_cancel_query: true,
        supports_query_execution_cancel: false,
        supports_explain: true,
        supports_streaming_results: true,
        supports_offset: true,
        has_schema_level: true,
    }
}

fn server() -> ServerInfo {
    ServerInfo {
        server_version: "15.2".to_owned(),
        server_type: "PostgreSQL".to_owned(),
    }
}

fn request(connection_id: &str) -> ConnectRequest {
    ConnectRequest {
        connection_id: connection_id.to_owned(),
        database: Some("app_db".to_owned()),
    }
}

fn connected_entry(
    connection_id: &str,
    db_session_id: &str,
    capabilities: Option<DriverCapabilities>,
) -> ConnectionEntry {
    ConnectionEntry {
        db_session_id: db_session_id.to_owned(),
        connection_id: connection_id.to_owned(),
        status: ConnectionStatus::Connected,
        server_info: Some(server()),
        capabilities,
        current_database: Some("app_db".to_owned()),
        error: None,
        connecting_since_ms: None,
    }
}

#[test]
fn status_spellings_match_the_react_union() {
    assert_eq!(ConnectionStatus::Idle.as_str(), "idle");
    assert_eq!(ConnectionStatus::Connecting.as_str(), "connecting");
    assert_eq!(ConnectionStatus::Connected.as_str(), "connected");
    assert_eq!(ConnectionStatus::Error.as_str(), "error");
}

#[test]
fn connect_starts_a_connecting_entry_and_claims_the_attempt() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    assert!(store.is_connect_in_flight("c1"));

    let entry = store.entry("c1").expect("connect seeds the entry");
    assert_eq!(entry.status, ConnectionStatus::Connecting);
    assert_eq!(entry.db_session_id, "");
    assert_eq!(entry.server_info, None);
    assert_eq!(entry.capabilities, None);
    assert_eq!(entry.current_database, Some("app_db".to_owned()));
    assert_eq!(entry.error, None);
}

#[test]
fn a_second_connect_joins_the_first_instead_of_replacing_its_session() {
    // The dedup's whole reason: without it the second `connect` replaces
    // the first session and every reader of the old one fails with
    // `sessionNotFound` while the schema tree stays empty, silently.
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    assert_eq!(store.connect(request("c1")), ConnectStart::JoinedInFlight);
    assert!(store.is_connect_in_flight("c1"));

    // The join changes nothing: the first attempt still owns the entry.
    let entry = store.entry("c1").expect("the first attempt's entry");
    assert_eq!(entry.status, ConnectionStatus::Connecting);
    assert_eq!(entry.current_database, Some("app_db".to_owned()));

    // Different connections are independent claims.
    assert_eq!(store.connect(request("c2")), ConnectStart::Started);
}

#[test]
fn complete_connect_lands_the_session_and_releases_the_claim() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);

    let effects = store.complete_connect("c1", "s-1", server(), Some(caps()));
    assert_eq!(
        effects,
        vec![
            ConnectionEffect::BindSessionMetadataIdentity {
                db_session_id: "s-1".to_owned(),
                connection_id: "c1".to_owned(),
            },
            ConnectionEffect::BroadcastConnectionReady {
                connection_id: "c1".to_owned(),
                db_session_id: "s-1".to_owned(),
            },
        ],
        "bind first, then the ready broadcast"
    );
    assert!(!store.is_connect_in_flight("c1"));
    assert_eq!(
        store.entry("c1"),
        Some(&connected_entry("c1", "s-1", Some(caps())))
    );

    // A settled attempt frees the slot: the next connect starts fresh.
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    assert_eq!(
        store.entry("c1").expect("re-connect seeds again").status,
        ConnectionStatus::Connecting
    );
}

#[test]
fn capability_discovery_failure_keeps_a_successful_connection_connected() {
    // Older/headless backends may not return the optional field; that must
    // not turn a successful database connection into a failed one.
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);

    let effects = store.complete_connect("c1", "s-1", server(), None);
    assert_eq!(effects.len(), 2, "bind + ready broadcast, no failure");
    assert_eq!(
        store.entry("c1"),
        Some(&connected_entry("c1", "s-1", None))
    );
}

#[test]
fn fail_connect_keeps_the_seeded_entry_and_releases_the_claim() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    store.complete_connect("c1", "s-1", server(), Some(caps()));

    // A retry re-seeds the connecting entry up front (server info and
    // capabilities wiped, database re-seeded from the request); the catch
    // then spreads *that* seeded entry, so the error entry has no server
    // info but keeps the database.
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    let effects = store.fail_connect("c1", "connection refused".to_owned());
    assert_eq!(
        effects,
        vec![ConnectionEffect::BroadcastConnectionFailed {
            connection_id: "c1".to_owned(),
            error: "connection refused".to_owned(),
        }]
    );
    assert!(!store.is_connect_in_flight("c1"));
    let entry = store.entry("c1").expect("the failed entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.db_session_id, "");
    assert_eq!(entry.error, Some("connection refused".to_owned()));
    assert_eq!(entry.server_info, None);
    assert_eq!(entry.capabilities, None);
    assert_eq!(entry.current_database, Some("app_db".to_owned()));

    // The slot is free again after the failure.
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
}

#[test]
fn fail_connect_spreads_whatever_entry_is_current() {
    // The transition itself spreads the entry it finds: called on a
    // connected entry (no re-seed in between), the server info,
    // capabilities and database survive and only the session clears.
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    store.complete_connect("c1", "s-1", server(), Some(caps()));

    store.fail_connect("c1", "late failure".to_owned());
    let entry = store.entry("c1").expect("the failed entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.db_session_id, "");
    assert_eq!(entry.error, Some("late failure".to_owned()));
    assert_eq!(entry.server_info, Some(server()));
    assert_eq!(entry.capabilities, Some(caps()));
    assert_eq!(entry.current_database, Some("app_db".to_owned()));
}

#[test]
fn fail_connect_without_a_prior_entry_creates_a_fresh_error_entry() {
    let mut store = ActiveConnectionStore::new();
    let effects = store.fail_connect("c1", "boom".to_owned());
    assert_eq!(effects.len(), 1);
    let entry = store.entry("c1").expect("a fresh error entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.db_session_id, "");
    assert_eq!(entry.error, Some("boom".to_owned()));
    assert_eq!(entry.server_info, None);
    assert_eq!(entry.current_database, None);
}

#[test]
fn mark_connecting_arms_the_thirty_second_timeout() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", Some("app_db".to_owned()), 1_000);
    let entry = store.entry("c1").expect("the connecting entry");
    assert_eq!(entry.status, ConnectionStatus::Connecting);
    assert_eq!(entry.connecting_since_ms, Some(1_000));

    // 29_999ms in: still connecting.
    assert!(store.tick(1_000 + CONNECT_TIMEOUT_MS - 1).is_empty());
    assert_eq!(
        store.entry("c1").expect("still tracked").status,
        ConnectionStatus::Connecting
    );

    // Exactly 30_000ms: the entry fails with the literal timeout sentence.
    assert_eq!(store.tick(1_000 + CONNECT_TIMEOUT_MS), vec!["c1".to_owned()]);
    let entry = store.entry("c1").expect("the timed-out entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.error, Some("Connection timeout".to_owned()));

    // A second tick does not re-report it.
    assert!(store.tick(1_000 + 2 * CONNECT_TIMEOUT_MS).is_empty());
}

#[test]
fn tick_ignores_connected_entries_and_a_backwards_clock() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", None, 1_000);
    store.mark_connected("c1", "s-1");
    // The entry left `connecting` before the deadline: no timeout.
    assert!(store.tick(1_000 + 2 * CONNECT_TIMEOUT_MS).is_empty());
    assert_eq!(
        store.entry("c1").expect("still connected").status,
        ConnectionStatus::Connected
    );

    // A clock that runs backwards never trips the deadline either.
    store.mark_connecting("c2", None, 1_000);
    assert!(store.tick(500).is_empty());
    assert_eq!(
        store.entry("c2").expect("still connecting").status,
        ConnectionStatus::Connecting
    );
}

#[test]
fn a_connect_attempt_has_no_timeout() {
    // Only `markConnecting` arms the 30s timer in React; a real `connect`
    // attempt must not be failed by the tick.
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    assert!(store.tick(60_000).is_empty());
    assert_eq!(
        store.entry("c1").expect("still connecting").status,
        ConnectionStatus::Connecting
    );
}

#[test]
fn mark_connected_binds_and_asks_for_capabilities() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", Some("app_db".to_owned()), 1_000);
    let effects = store.mark_connected("c1", "s-9");
    assert_eq!(
        effects,
        vec![
            ConnectionEffect::BindSessionMetadataIdentity {
                db_session_id: "s-9".to_owned(),
                connection_id: "c1".to_owned(),
            },
            ConnectionEffect::RequestCapabilities {
                connection_id: "c1".to_owned(),
                db_session_id: "s-9".to_owned(),
            },
        ]
    );
    let entry = store.entry("c1").expect("the connected entry");
    assert_eq!(entry.status, ConnectionStatus::Connected);
    assert_eq!(entry.db_session_id, "s-9");
    // Capabilities stay unknown until the session-info IPC resolves, and
    // the database seeded by `markConnecting` survives the spread.
    assert_eq!(entry.capabilities, None);
    assert_eq!(entry.current_database, Some("app_db".to_owned()));
    assert_eq!(entry.error, None);
}

#[test]
fn apply_capabilities_only_writes_back_for_the_same_session() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connected("c1", "s-1");

    // A stale answer — the entry moved to another session meanwhile — must
    // not write back.
    store.mark_connected("c1", "s-2");
    assert!(!store.apply_capabilities("c1", "s-1", Some(caps())));
    assert_eq!(store.entry("c1").expect("entry").capabilities, None);

    // The current session's answer lands.
    assert!(store.apply_capabilities("c1", "s-2", Some(caps())));
    assert_eq!(
        store.entry("c1").expect("entry").capabilities,
        Some(caps())
    );

    // A discovery failure (None) leaves the connection connected with
    // capabilities unknown — advisory only.
    assert!(store.apply_capabilities("c1", "s-2", None));
    assert_eq!(store.entry("c1").expect("entry").capabilities, None);
    assert_eq!(
        store.entry("c1").expect("entry").status,
        ConnectionStatus::Connected
    );

    // No entry at all: nothing to write back to.
    assert!(!store.apply_capabilities("ghost", "s-1", Some(caps())));
}

#[test]
fn mark_error_clears_the_session_and_keeps_the_rest() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    store.complete_connect("c1", "s-1", server(), Some(caps()));

    store.mark_error("c1", "remote closed".to_owned());
    let entry = store.entry("c1").expect("the error entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.db_session_id, "");
    assert_eq!(entry.error, Some("remote closed".to_owned()));
    assert_eq!(entry.server_info, Some(server()));
    assert_eq!(entry.capabilities, Some(caps()));
    assert_eq!(entry.current_database, Some("app_db".to_owned()));

    // With no prior entry, a fresh error entry is created.
    store.mark_error("c2", "gone".to_owned());
    let entry = store.entry("c2").expect("a fresh error entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.error, Some("gone".to_owned()));
    assert_eq!(entry.server_info, None);
}

#[test]
fn disconnect_without_a_session_removes_the_entry_at_once() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", None, 0);
    assert_eq!(store.disconnect("c1"), DisconnectPlan::Removed);
    assert_eq!(store.entry("c1"), None);

    // An unknown connection is the same no-op.
    assert_eq!(store.disconnect("ghost"), DisconnectPlan::Removed);
}

#[test]
fn disconnect_with_a_session_plans_ipc_then_removal() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    store.complete_connect("c1", "s-1", server(), Some(caps()));

    let plan = store.disconnect("c1");
    assert_eq!(
        plan,
        DisconnectPlan::DisconnectSession {
            db_session_id: "s-1".to_owned(),
            on_success: ConnectionEffect::BroadcastDisconnectRequested {
                db_session_id: "s-1".to_owned(),
            },
        }
    );
    // The entry stays until the host finishes the disconnect — the IPC and
    // the broadcast happen first.
    assert!(store.entry("c1").is_some());

    store.finish_disconnect("c1");
    assert_eq!(store.entry("c1"), None);
}

#[test]
fn remove_by_db_session_id_removes_every_match_and_drops_reads_first() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    store.complete_connect("c1", "s-1", server(), Some(caps()));
    assert_eq!(store.connect(request("c2")), ConnectStart::Started);
    store.complete_connect("c2", "s-1", server(), None);
    assert_eq!(store.connect(request("c3")), ConnectStart::Started);
    store.complete_connect("c3", "s-2", server(), None);

    let effects = store.remove_by_db_session_id("s-1");
    assert_eq!(
        effects,
        vec![ConnectionEffect::DropColumnReadsForSession {
            db_session_id: "s-1".to_owned(),
        }],
        "the column-read drop is the first (only) effect, before the removal is observed"
    );
    assert_eq!(store.entry("c1"), None);
    assert_eq!(store.entry("c2"), None);
    assert!(store.entry("c3").is_some(), "another session is untouched");

    // React drops the reads unconditionally, even with no matching entry.
    let effects = store.remove_by_db_session_id("s-missing");
    assert_eq!(
        effects,
        vec![ConnectionEffect::DropColumnReadsForSession {
            db_session_id: "s-missing".to_owned(),
        }]
    );
}

#[test]
fn reset_clears_connections_but_not_in_flight_claims() {
    // React `reset` only sets `{ connections: {} }` — the module-level
    // in-flight map is untouched, so a connect already in flight keeps its
    // claim across a reset.
    let mut store = ActiveConnectionStore::new();
    assert_eq!(store.connect(request("c1")), ConnectStart::Started);
    store.reset();
    assert!(store.connections().is_empty());
    assert!(store.is_connect_in_flight("c1"));
    assert_eq!(store.connect(request("c1")), ConnectStart::JoinedInFlight);
}

#[test]
fn extract_error_passes_messages_through_and_falls_back() {
    assert_eq!(
        extract_error(Some("connection refused")),
        "connection refused"
    );
    assert_eq!(extract_error(Some("")), "");
    assert_eq!(extract_error(None), "未知错误");
}

#[test]
fn now_ms_reports_a_live_clock() {
    assert!(now_ms() > 0, "the wall clock is available");
}
