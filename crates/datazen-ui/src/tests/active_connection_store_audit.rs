//! T039 independent audit of the `stores/activeConnectionStore.ts` port.
//!
//! Fresh fixtures, independent angles from the coder tests: the
//! `markConnected` spread semantics (server info survives, timeout disarms),
//! re-arming the `markConnecting` deadline, per-entry tick isolation,
//! disconnect `finally` ordering, `removeByDbSessionId("")` parity, the
//! `reset` + in-flight interaction, and `extractError` against the live
//! zh-CN table.
//!
//! `DriverCapabilities` here carries all 6 React fields
//! (`packages/frontend/src/types`, `supportsOffset` / `hasSchemaLevel`
//! included — fixed by 70ee0e7a6, re-test round), and the round-trip
//! tests below pin all 6 end to end through `complete_connect` and
//! `apply_capabilities`. The backend registry payload carries a 7th
//! field (`hasMultiDatabase`, not in the React type and not consumed by
//! this store), which is out of scope for this store's capabilities.

use crate::active_connection_store::{
    extract_error, now_ms, ActiveConnectionStore, ConnectRequest, ConnectStart, ConnectionEffect,
    ConnectionStatus, DisconnectPlan, DriverCapabilities, ServerInfo, CONNECT_TIMEOUT_MS,
};

fn caps() -> DriverCapabilities {
    DriverCapabilities {
        supports_cancel_query: true,
        supports_query_execution_cancel: true,
        supports_explain: false,
        supports_streaming_results: true,
        // Distinct values: true/false proves each flag travelled, not a default.
        supports_offset: true,
        has_schema_level: false,
    }
}

fn server() -> ServerInfo {
    ServerInfo {
        server_version: "16.1".to_owned(),
        server_type: "PostgreSQL".to_owned(),
    }
}

fn request(connection_id: &str, database: Option<&str>) -> ConnectRequest {
    ConnectRequest {
        connection_id: connection_id.to_owned(),
        database: database.map(str::to_owned),
    }
}

fn connect_and_complete(
    store: &mut ActiveConnectionStore,
    connection_id: &str,
    db_session_id: &str,
) {
    assert_eq!(
        store.connect(request(connection_id, Some("app_db"))),
        ConnectStart::Started
    );
    store.complete_connect(connection_id, db_session_id, server(), Some(caps()));
}

#[test]
fn capabilities_round_trip_field_by_field_through_complete_connect() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(
        store.connect(request("c1", Some("app_db"))),
        ConnectStart::Started
    );
    store.complete_connect("c1", "s-1", server(), Some(caps()));
    let entry = store.entry("c1").expect("connected entry");
    let got = entry.capabilities.as_ref().expect("capabilities landed");
    assert!(got.supports_cancel_query);
    assert!(got.supports_query_execution_cancel);
    assert!(!got.supports_explain);
    assert!(got.supports_streaming_results);
    assert!(got.supports_offset);
    assert!(!got.has_schema_level);
    assert_eq!(entry.server_info, Some(server()));
}

#[test]
fn capabilities_round_trip_field_by_field_through_apply_capabilities() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connected("c1", "s-1");
    assert!(store.apply_capabilities("c1", "s-1", Some(caps())));
    let got = store
        .entry("c1")
        .expect("entry")
        .capabilities
        .as_ref()
        .expect("capabilities landed");
    assert!(got.supports_cancel_query);
    assert!(got.supports_query_execution_cancel);
    assert!(!got.supports_explain);
    assert!(got.supports_streaming_results);
    assert!(got.supports_offset);
    assert!(!got.has_schema_level);
}

#[test]
fn mark_connected_creates_a_connected_entry_from_scratch() {
    let mut store = ActiveConnectionStore::new();
    let effects = store.mark_connected("ghost", "s-7");
    assert_eq!(
        effects,
        vec![
            ConnectionEffect::BindSessionMetadataIdentity {
                db_session_id: "s-7".to_owned(),
                connection_id: "ghost".to_owned(),
            },
            ConnectionEffect::RequestCapabilities {
                connection_id: "ghost".to_owned(),
                db_session_id: "s-7".to_owned(),
            },
        ],
        "bind first, then the capabilities request"
    );
    let entry = store.entry("ghost").expect("created entry");
    assert_eq!(entry.status, ConnectionStatus::Connected);
    assert_eq!(entry.db_session_id, "s-7");
    assert_eq!(entry.server_info, None);
    assert_eq!(entry.capabilities, None);
    assert_eq!(entry.current_database, None);
    assert_eq!(entry.error, None);
}

#[test]
fn mark_connected_preserves_server_info_and_database() {
    // React spreads the previous entry: a cross-window markConnected on an
    // already-connected entry keeps its server info and database.
    let mut store = ActiveConnectionStore::new();
    connect_and_complete(&mut store, "c1", "s-1");
    store.mark_connected("c1", "s-2");
    let entry = store.entry("c1").expect("entry");
    assert_eq!(entry.status, ConnectionStatus::Connected);
    assert_eq!(entry.db_session_id, "s-2");
    assert_eq!(entry.server_info, Some(server()));
    assert_eq!(entry.current_database, Some("app_db".to_owned()));
    assert_eq!(entry.capabilities, None, "capabilities reset to unknown");
    assert_eq!(entry.error, None);
}

#[test]
fn mark_connected_disarms_the_mark_connecting_timeout() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", Some("app_db".to_owned()), 1_000);
    store.mark_connected("c1", "s-1");
    assert!(store.tick(1_000 + 10 * CONNECT_TIMEOUT_MS).is_empty());
    assert_eq!(
        store.entry("c1").expect("still connected").status,
        ConnectionStatus::Connected
    );
}

#[test]
fn mark_error_on_an_armed_entry_never_times_out() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", None, 1_000);
    store.mark_error("c1", "remote closed".to_owned());
    assert!(store.tick(1_000 + 10 * CONNECT_TIMEOUT_MS).is_empty());
    let entry = store.entry("c1").expect("entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.error, Some("remote closed".to_owned()));
}

#[test]
fn rearming_mark_connecting_moves_the_deadline() {
    // Rust keeps one stamp per entry, so a second markConnecting replaces
    // the deadline. (React stacks a second setTimeout whose earlier timer
    // can still fire; the difference is unobservable in practice because
    // markConnecting is armed once per cross-window event.)
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", None, 1_000);
    store.mark_connecting("c1", None, 50_000);
    assert!(store.tick(1_000 + CONNECT_TIMEOUT_MS).is_empty());
    assert_eq!(
        store.entry("c1").expect("still connecting").status,
        ConnectionStatus::Connecting
    );
    assert_eq!(
        store.tick(50_000 + CONNECT_TIMEOUT_MS),
        vec!["c1".to_owned()]
    );
}

#[test]
fn tick_times_out_only_the_entries_that_are_due() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("early", None, 0);
    store.mark_connecting("late", None, 20_000);
    assert_eq!(store.tick(30_000), vec!["early".to_owned()]);
    assert_eq!(
        store.entry("late").expect("not yet due").status,
        ConnectionStatus::Connecting
    );
    assert_eq!(store.tick(50_000), vec!["late".to_owned()]);
}

#[test]
fn tick_after_timeout_reports_each_entry_once() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", None, 0);
    store.mark_connecting("c2", None, 0);
    let mut timed_out = store.tick(CONNECT_TIMEOUT_MS);
    timed_out.sort();
    assert_eq!(timed_out, vec!["c1".to_owned(), "c2".to_owned()]);
    assert!(store.tick(2 * CONNECT_TIMEOUT_MS).is_empty());
}

#[test]
fn disconnect_on_an_error_entry_removes_it_at_once() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(
        store.connect(request("c1", Some("app_db"))),
        ConnectStart::Started
    );
    store.fail_connect("c1", "refused".to_owned());
    assert_eq!(store.disconnect("c1"), DisconnectPlan::Removed);
    assert_eq!(store.entry("c1"), None);
}

#[test]
fn disconnect_plan_holds_the_entry_until_finish_disconnect() {
    let mut store = ActiveConnectionStore::new();
    connect_and_complete(&mut store, "c1", "s-1");
    let plan = store.disconnect("c1");
    assert!(matches!(plan, DisconnectPlan::DisconnectSession { .. }));
    // The IPC and the broadcast happen first; the entry survives them.
    assert!(store.entry("c1").is_some());
    store.finish_disconnect("c1");
    assert_eq!(store.entry("c1"), None);
    // Finishing twice is a harmless no-op.
    store.finish_disconnect("c1");
    assert_eq!(store.entry("c1"), None);
}

#[test]
fn remove_by_empty_session_id_matches_connecting_entries_like_react() {
    // React matches on `dbSessionId === ""` too, so every entry still
    // connecting (empty session) is removed. Faithful parity, footgun
    // included — callers only ever pass a real session id.
    let mut store = ActiveConnectionStore::new();
    store.mark_connecting("c1", None, 0);
    store.mark_connecting("c2", None, 0);
    connect_and_complete(&mut store, "c3", "s-3");
    let effects = store.remove_by_db_session_id("");
    assert_eq!(
        effects,
        vec![ConnectionEffect::DropColumnReadsForSession {
            db_session_id: String::new(),
        }]
    );
    assert_eq!(store.entry("c1"), None);
    assert_eq!(store.entry("c2"), None);
    assert!(store.entry("c3").is_some());
}

#[test]
fn reset_then_complete_reinserts_without_the_seeded_database() {
    // The in-flight claim survives reset, but the entry is gone, so the
    // completion lands with no database to preserve.
    let mut store = ActiveConnectionStore::new();
    assert_eq!(
        store.connect(request("c1", Some("app_db"))),
        ConnectStart::Started
    );
    store.reset();
    assert!(store.connections().is_empty());
    assert!(store.is_connect_in_flight("c1"));
    store.complete_connect("c1", "s-1", server(), Some(caps()));
    let entry = store.entry("c1").expect("reinserted entry");
    assert_eq!(entry.status, ConnectionStatus::Connected);
    assert_eq!(entry.current_database, None);
    assert!(!store.is_connect_in_flight("c1"));
}

#[test]
fn second_connect_join_leaves_the_first_request_database() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(
        store.connect(request("c1", Some("first_db"))),
        ConnectStart::Started
    );
    assert_eq!(
        store.connect(request("c1", Some("second_db"))),
        ConnectStart::JoinedInFlight
    );
    assert_eq!(
        store.entry("c1").expect("entry").current_database,
        Some("first_db".to_owned())
    );
}

#[test]
fn connect_after_mark_connecting_arms_nothing_until_remarked() {
    // `connect` never arms a timeout; a later markConnecting on the same
    // id re-seeds the entry and arms it from that moment.
    let mut store = ActiveConnectionStore::new();
    assert_eq!(
        store.connect(request("c1", Some("app_db"))),
        ConnectStart::Started
    );
    assert!(store.tick(10 * CONNECT_TIMEOUT_MS).is_empty());
    store.mark_connecting("c1", Some("app_db".to_owned()), 100_000);
    assert_eq!(
        store.tick(100_000 + CONNECT_TIMEOUT_MS),
        vec!["c1".to_owned()]
    );
}

#[test]
fn fail_connect_twice_overwrites_the_message() {
    let mut store = ActiveConnectionStore::new();
    assert_eq!(
        store.connect(request("c1", Some("app_db"))),
        ConnectStart::Started
    );
    store.fail_connect("c1", "first".to_owned());
    store.fail_connect("c1", "second".to_owned());
    let entry = store.entry("c1").expect("entry");
    assert_eq!(entry.status, ConnectionStatus::Error);
    assert_eq!(entry.error, Some("second".to_owned()));
}

#[test]
fn apply_capabilities_refuses_a_disconnected_or_errored_session() {
    let mut store = ActiveConnectionStore::new();
    store.mark_connected("c1", "s-1");
    store.mark_error("c1", "gone".to_owned());
    // The session was cleared by markError, so the stale answer must not land.
    assert!(!store.apply_capabilities("c1", "s-1", Some(caps())));
    assert_eq!(store.entry("c1").expect("entry").capabilities, None);

    store.mark_connected("c2", "s-2");
    store.finish_disconnect("c2");
    assert!(!store.apply_capabilities("c2", "s-2", Some(caps())));
}

#[test]
fn extract_error_uses_the_live_zh_cn_table() {
    assert!(crate::i18n::has("backend.unknownError"));
    assert_eq!(extract_error(Some("")), "");
    assert_eq!(extract_error(Some("boom")), "boom");
    assert_eq!(extract_error(None), "未知错误");
}

#[test]
fn now_ms_is_monotonic_and_live() {
    let first = now_ms();
    assert!(first > 0);
    assert!(now_ms() >= first);
}
