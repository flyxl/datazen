//! T610 audit for the pure connection-tab decisions: bootstrap, connect reuse,
//! success/failure transitions, cross-window events, active index and heartbeat.

use std::collections::BTreeSet;

use datazen_ui_api::{ConnectionStatus, ConnectionTab, ConsumedConnection};

use crate::i18n::t;
use crate::views::use_connection_tabs as tabs;
use crate::views::use_connection_tabs::{
    active_tab, apply_connected, apply_cross_failed,
    apply_cross_ready, apply_disconnect_requested, apply_failed, bootstrap, connect_decision,
    connect_error_message, heartbeat_sessions, pending_connect_indices, ready_event,
    set_active_idx, ActiveConnectionSnapshot, ConnectDecision, ConnectError, ConnectionTabsVm,
};

fn tab(id: &str, session: &str, status: ConnectionStatus) -> ConnectionTab {
    ConnectionTab {
        connection_id: id.to_owned(),
        db_session_id: session.to_owned(),
        connection_name: "PG".to_owned(),
        database_type: "postgresql".to_owned(),
        initial_database: None,
        status,
        error: None,
    }
}

// ── bootstrap / active tab ────────────────────────────────────────────────

#[test]
fn bootstrap_seeds_one_tab_and_consumes_the_pending_action_once() {
    let consumed = ConsumedConnection {
        tab: tab("cfg-1", "", ConnectionStatus::Connecting),
        action: Some("focus-query".to_owned()),
    };
    let vm = bootstrap(Some(&consumed));
    assert_eq!(vm.tabs.len(), 1);
    assert_eq!(vm.tabs[0].connection_id, "cfg-1");
    assert_eq!(vm.pending_action.as_deref(), Some("focus-query"));
    assert_eq!(vm.active_idx, 0);
    assert!(vm.connecting.is_empty());
    assert_eq!(bootstrap(None), ConnectionTabsVm::default());
}

#[test]
fn active_tab_uses_the_raw_index_without_clamping() {
    let mut vm = ConnectionTabsVm {
        tabs: vec![tab("a", "s1", ConnectionStatus::Connected), tab("b", "", ConnectionStatus::Connecting)],
        ..ConnectionTabsVm::default()
    };
    assert_eq!(active_tab(&vm).map(|tab| tab.connection_id.as_str()), Some("a"));
    set_active_idx(&mut vm, 1);
    assert_eq!(active_tab(&vm).map(|tab| tab.connection_id.as_str()), Some("b"));
    set_active_idx(&mut vm, 9);
    assert!(active_tab(&vm).is_none(), "React's tabs[activeIdx] ?? null");
}

// ── pending connects ──────────────────────────────────────────────────────

#[test]
fn pending_connect_indices_skips_connected_errored_and_inflight_tabs() {
    let vm = ConnectionTabsVm {
        tabs: vec![
            tab("a", "", ConnectionStatus::Connecting),
            tab("b", "s1", ConnectionStatus::Connected),
            tab("c", "", ConnectionStatus::Error),
            tab("d", "s2", ConnectionStatus::Connecting), // has a session
            tab("e", "", ConnectionStatus::Connecting),   // in flight
        ],
        connecting: BTreeSet::from(["e".to_owned()]),
        ..ConnectionTabsVm::default()
    };
    assert_eq!(pending_connect_indices(&vm), vec![0]);
}

#[test]
fn connect_decision_reuses_only_a_live_session() {
    let pending = tab("cfg-1", "", ConnectionStatus::Connecting);
    let live = ActiveConnectionSnapshot {
        status: "connected".to_owned(),
        db_session_id: Some("sess-1".to_owned()),
    };
    assert_eq!(
        connect_decision(Some(&live), &pending),
        ConnectDecision::Reuse { db_session_id: "sess-1".to_owned() }
    );
    let connecting = ActiveConnectionSnapshot { status: "connecting".to_owned(), ..live.clone() };
    let no_session = ActiveConnectionSnapshot { db_session_id: None, ..live.clone() };
    let empty_session = ActiveConnectionSnapshot { db_session_id: Some(String::new()), ..live };
    for existing in [&connecting, &no_session, &empty_session] {
        assert_eq!(
            connect_decision(Some(existing), &pending),
            ConnectDecision::Connect {
                connection_id: "cfg-1".to_owned(),
                initial_database: None,
            }
        );
    }
    assert_eq!(
        connect_decision(None, &pending),
        ConnectDecision::Connect { connection_id: "cfg-1".to_owned(), initial_database: None }
    );
    let mut with_db = pending;
    with_db.initial_database = Some("db3".to_owned());
    assert_eq!(
        connect_decision(None, &with_db),
        ConnectDecision::Connect {
            connection_id: "cfg-1".to_owned(),
            initial_database: Some("db3".to_owned()),
        }
    );
}

// ── success / failure ─────────────────────────────────────────────────────

#[test]
fn connect_success_updates_every_tab_of_that_connection_and_clears_error() {
    let mut errored = tab("cfg-1", "", ConnectionStatus::Error);
    errored.error = Some("old".to_owned());
    let tabs = vec![errored, tab("other", "", ConnectionStatus::Connecting), tab("cfg-1", "", ConnectionStatus::Connecting)];
    let next = apply_connected(&tabs, "cfg-1", "sess-9");
    assert_eq!(next[0].status, ConnectionStatus::Connected);
    assert_eq!(next[0].db_session_id, "sess-9");
    assert_eq!(next[0].error, None);
    assert_eq!(next[1].status, ConnectionStatus::Connecting, "other tabs untouched");
    assert_eq!(next[2].db_session_id, "sess-9");
    assert_eq!(ready_event("cfg-1", "sess-9"), tabs::ConnectionReadyEvent {
        connection_id: "cfg-1".to_owned(),
        db_session_id: "sess-9".to_owned(),
    });
}

#[test]
fn failure_message_uses_the_exact_locale_key_only_for_unknown_values() {
    assert_eq!(connect_error_message(&ConnectError::Message("boom".to_owned())), "boom");
    assert_eq!(connect_error_message(&ConnectError::Unknown), t("backend.unknownError"));
}

#[test]
fn connect_failure_marks_the_error_on_all_matching_tabs() {
    let tabs = vec![tab("cfg-1", "", ConnectionStatus::Connecting), tab("other", "", ConnectionStatus::Connecting)];
    let next = apply_failed(&tabs, "cfg-1", "connect failed");
    assert_eq!(next[0].status, ConnectionStatus::Error);
    assert_eq!(next[0].error.as_deref(), Some("connect failed"));
    assert_eq!(next[1].status, ConnectionStatus::Connecting);
}

// ── cross-window events ───────────────────────────────────────────────────

#[test]
fn cross_ready_requires_both_ids() {
    let tabs = vec![tab("cfg-1", "", ConnectionStatus::Connecting)];
    assert!(apply_cross_ready(&tabs, Some("cfg-1"), None).is_none());
    assert!(apply_cross_ready(&tabs, None, Some("s1")).is_none());
    assert!(apply_cross_ready(&tabs, Some(""), Some("s1")).is_none());
    assert!(apply_cross_ready(&tabs, Some("cfg-1"), Some("")).is_none());
    let next = apply_cross_ready(&tabs, Some("cfg-1"), Some("sess-remote")).expect("ready");
    assert_eq!(next[0].db_session_id, "sess-remote");
    assert_eq!(next[0].status, ConnectionStatus::Connected);
    assert_eq!(next[0].error, None);
}

#[test]
fn cross_failed_defaults_to_the_literal_unknown_error() {
    let tabs = vec![tab("cfg-1", "", ConnectionStatus::Connecting)];
    assert!(apply_cross_failed(&tabs, None, Some("x")).is_none());
    assert!(apply_cross_failed(&tabs, Some(""), Some("x")).is_none());
    let next = apply_cross_failed(&tabs, Some("cfg-1"), None).expect("failed");
    assert_eq!(next[0].status, ConnectionStatus::Error);
    assert_eq!(next[0].error.as_deref(), Some("Unknown error"));
    let with_error = apply_cross_failed(&tabs, Some("cfg-1"), Some("remote error")).expect("failed");
    assert_eq!(with_error[0].error.as_deref(), Some("remote error"));
}

#[test]
fn disconnect_requested_drops_only_tabs_on_that_session() {
    let tabs = vec![
        tab("a", "s1", ConnectionStatus::Connected),
        tab("b", "s2", ConnectionStatus::Connected),
        tab("c", "", ConnectionStatus::Connecting),
    ];
    assert!(apply_disconnect_requested(&tabs, None).is_none());
    assert!(apply_disconnect_requested(&tabs, Some("")).is_none());
    let next = apply_disconnect_requested(&tabs, Some("s1")).expect("disconnect");
    assert_eq!(next.iter().map(|tab| tab.connection_id.as_str()).collect::<Vec<_>>(), ["b", "c"]);
}

// ── heartbeat ─────────────────────────────────────────────────────────────

#[test]
fn heartbeat_pings_only_connected_tabs_with_a_session() {
    let tabs = vec![
        tab("a", "s1", ConnectionStatus::Connected),
        tab("b", "", ConnectionStatus::Connected),
        tab("c", "s3", ConnectionStatus::Connecting),
        tab("d", "s4", ConnectionStatus::Error),
        tab("e", "s5", ConnectionStatus::Connected),
    ];
    assert_eq!(heartbeat_sessions(&tabs), vec!["s1".to_owned(), "s5".to_owned()]);
    assert!(heartbeat_sessions(&[]).is_empty());
}

#[test]
fn bootstrap_without_an_action_has_no_pending_intent() {
    let consumed = ConsumedConnection {
        tab: tab("cfg-1", "", ConnectionStatus::Connecting),
        action: None,
    };
    let vm = bootstrap(Some(&consumed));
    assert_eq!(vm.tabs.len(), 1);
    assert_eq!(vm.pending_action, None);
}

#[test]
fn cross_failed_keeps_an_empty_error_string() {
    let tabs = vec![tab("cfg-1", "", ConnectionStatus::Connecting)];
    let next = apply_cross_failed(&tabs, Some("cfg-1"), Some("")).expect("failed");
    assert_eq!(next[0].error.as_deref(), Some(""), "React's ?? keeps an empty error string");
}

// Independent lane-C repro probe (original D1), retained as a regression guard.
#[test]
fn probe_empty_pending_action_is_not_retained() {
    let consumed = ConsumedConnection {
        tab: tab("cfg-1", "", ConnectionStatus::Connecting),
        action: Some(String::new()),
    };
    let vm = bootstrap(Some(&consumed));
    assert_eq!(
        vm.pending_action, None,
        "React's `if (initialPendingRef.current?.action)` treats an empty string as absent"
    );
}
