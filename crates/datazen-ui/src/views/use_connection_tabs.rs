//! `windows/connection/useConnectionTabs.ts` — the connection-tab state
//! machine, ported as a pure decision module for T610.
//!
//! React keeps a list of `ConnectionTab`s, an active index and a one-shot
//! pending action. The decisions worth porting are the transitions around
//! that list: bootstrapping from a consumed pending payload, discovering
//! tabs that need a connect, reusing an already-live session, applying a
//! successful/failed connect, the three cross-window events, and the set of
//! sessions a heartbeat would ping.
//!
//! The crate's existing [`ConnectionTab`] / [`ConnectionStatus`] /
//! [`ConsumedConnection`] contracts from `datazen-ui-api` are reused verbatim.
//! The host owns the store, the cross-window bus, the in-flight set and the
//! timer: this module never calls an IPC, opens a socket or schedules work —
//! it returns the tab list / intents the host should apply.
//!
//! ## Consumer / removal stage
//!
//! No production connection host mounts these decisions yet. Every reusable
//! item carries a per-item `#[allow(dead_code)]` with this stage as its removal
//! note. The only translation key is React's own `backend.unknownError`.

use std::collections::BTreeSet;

use datazen_ui_api::{ConnectionStatus, ConnectionTab, ConsumedConnection};

use crate::i18n::t;

/// Host-owned projection of the active-connection store entry for one id.
#[allow(dead_code)] // VM mirror; supplied by the active-connection store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveConnectionSnapshot {
    pub status: String,
    pub db_session_id: Option<String>,
}

/// The hook's full host-owned state. `active_idx` is stored raw, matching
/// React's `setActiveIdx` (an out-of-range index simply yields no active tab).
#[allow(dead_code)] // VM surface; owned by the connection host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionTabsVm {
    pub tabs: Vec<ConnectionTab>,
    pub active_idx: usize,
    pub pending_action: Option<String>,
    /// Connection ids whose connect is already in flight (React's ref set).
    pub connecting: BTreeSet<String>,
}

/// `activeTab = tabs[activeIdx] ?? null` — no clamping, no fallback to 0.
#[allow(dead_code)] // pure projection; consumed by the host and tests.
pub fn active_tab(vm: &ConnectionTabsVm) -> Option<&ConnectionTab> {
    vm.tabs.get(vm.active_idx)
}

/// React's lazy `useState` initializer plus the effect that consumes the
/// pending action once. A missing/invalid payload leaves an empty tab list.
#[allow(dead_code)] // pure projection; consumed by the host and tests.
pub fn bootstrap(consumed: Option<&ConsumedConnection>) -> ConnectionTabsVm {
    match consumed {
        Some(consumed) => ConnectionTabsVm {
            tabs: vec![consumed.tab.clone()],
            active_idx: 0,
            pending_action: consumed.action.clone().filter(|action| !action.is_empty()),
            connecting: BTreeSet::new(),
        },
        None => ConnectionTabsVm::default(),
    }
}

/// React's pending-tab effect: a tab needs connecting when it is still
/// `connecting`, has no session, and this connection id is not already in
/// flight. Returns indices in tab order.
#[allow(dead_code)] // pure selection; consumed by the host and tests.
pub fn pending_connect_indices(vm: &ConnectionTabsVm) -> Vec<usize> {
    vm.tabs
        .iter()
        .enumerate()
        .filter(|(_, tab)| {
            tab.status == ConnectionStatus::Connecting
                && tab.db_session_id.is_empty()
                && !vm.connecting.contains(&tab.connection_id)
        })
        .map(|(index, _)| index)
        .collect()
}

/// React `connectTab`: an existing connected entry with a session is reused;
/// otherwise the host must mark it connecting and run the connect command.
#[allow(dead_code)] // decision; remove when the connection host wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectDecision {
    Reuse { db_session_id: String },
    Connect { connection_id: String, initial_database: Option<String> },
}

#[allow(dead_code)] // pure decision; consumed by the host and tests.
pub fn connect_decision(
    existing: Option<&ActiveConnectionSnapshot>,
    tab: &ConnectionTab,
) -> ConnectDecision {
    if let Some(existing) = existing {
        if existing.status == "connected" {
            if let Some(session) = &existing.db_session_id {
                if !session.is_empty() {
                    return ConnectDecision::Reuse { db_session_id: session.clone() };
                }
            }
        }
    }
    ConnectDecision::Connect {
        connection_id: tab.connection_id.clone(),
        initial_database: tab.initial_database.clone(),
    }
}

/// React's success path: mark the tab connected and clear its error. Every
/// tab of the same connection id is updated, not just the active one.
#[allow(dead_code)] // pure transition; consumed by the host and tests.
pub fn apply_connected(
    tabs: &[ConnectionTab],
    connection_id: &str,
    db_session_id: &str,
) -> Vec<ConnectionTab> {
    tabs.iter()
        .map(|tab| {
            if tab.connection_id == connection_id {
                ConnectionTab {
                    db_session_id: db_session_id.to_owned(),
                    status: ConnectionStatus::Connected,
                    error: None,
                    ..tab.clone()
                }
            } else {
                tab.clone()
            }
        })
        .collect()
}

/// React's failure message derivation: a raw string, an Error's message, or
/// the exact `backend.unknownError` key for an unknown thrown value.
#[allow(dead_code)] // VM mirror; supplied by the connect command host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectError {
    Message(String),
    Unknown,
}

#[allow(dead_code)] // pure helper; consumed by the failure transition.
pub fn connect_error_message(error: &ConnectError) -> String {
    match error {
        ConnectError::Message(message) => message.clone(),
        ConnectError::Unknown => t("backend.unknownError").to_owned(),
    }
}

/// React's failure path: mark every tab of that connection as `error`.
#[allow(dead_code)] // pure transition; consumed by the host and tests.
pub fn apply_failed(
    tabs: &[ConnectionTab],
    connection_id: &str,
    message: &str,
) -> Vec<ConnectionTab> {
    tabs.iter()
        .map(|tab| {
            if tab.connection_id == connection_id {
                ConnectionTab {
                    status: ConnectionStatus::Error,
                    error: Some(message.to_owned()),
                    ..tab.clone()
                }
            } else {
                tab.clone()
            }
        })
        .collect()
}

// ── cross-window events ───────────────────────────────────────────────────

/// `datazen:connection-ready`: both ids are required; a partial payload is
/// ignored (returns `None` — the host keeps the current list).
#[allow(dead_code)] // pure transition; consumed by the host and tests.
pub fn apply_cross_ready(
    tabs: &[ConnectionTab],
    connection_id: Option<&str>,
    db_session_id: Option<&str>,
) -> Option<Vec<ConnectionTab>> {
    let (connection_id, session) = (connection_id?, db_session_id?);
    if connection_id.is_empty() || session.is_empty() {
        return None;
    }
    Some(apply_connected(tabs, connection_id, session))
}

/// `datazen:connection-failed`: only a connection id is required; a missing
/// error text uses React's literal `'Unknown error'` (not an i18n key).
#[allow(dead_code)] // pure transition; consumed by the host and tests.
pub fn apply_cross_failed(
    tabs: &[ConnectionTab],
    connection_id: Option<&str>,
    error: Option<&str>,
) -> Option<Vec<ConnectionTab>> {
    let connection_id = connection_id?;
    if connection_id.is_empty() {
        return None;
    }
    let message = error.unwrap_or("Unknown error");
    Some(apply_failed(tabs, connection_id, message))
}

/// `datazen:disconnect-requested`: a session id is required; every tab bound
/// to that session is dropped (tabs of other sessions stay untouched).
#[allow(dead_code)] // pure transition; consumed by the host and tests.
pub fn apply_disconnect_requested(
    tabs: &[ConnectionTab],
    db_session_id: Option<&str>,
) -> Option<Vec<ConnectionTab>> {
    let session = db_session_id?;
    if session.is_empty() {
        return None;
    }
    Some(
        tabs.iter()
            .filter(|tab| tab.db_session_id != session)
            .cloned()
            .collect(),
    )
}

// ── selection + heartbeat ─────────────────────────────────────────────────

/// React's `setActiveIdx` is a raw store write; the observed value is the
/// index itself even when it points past the current tab list.
#[allow(dead_code)] // pure transition; consumed by the host and tests.
pub fn set_active_idx(vm: &mut ConnectionTabsVm, index: usize) {
    vm.active_idx = index;
}

/// The host's heartbeat tick selects exactly the connected tabs that hold a
/// session. The 5-minute cadence and the ping command stay host concerns.
#[allow(dead_code)] // pure selection; consumed by the host and tests.
pub fn heartbeat_sessions(tabs: &[ConnectionTab]) -> Vec<String> {
    tabs.iter()
        .filter(|tab| tab.status == ConnectionStatus::Connected && !tab.db_session_id.is_empty())
        .map(|tab| tab.db_session_id.clone())
        .collect()
}

/// React's ready-event emission payload, built from the same id pair the
/// success transition uses.
#[allow(dead_code)] // host port; remove when the connection host wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionReadyEvent {
    pub connection_id: String,
    pub db_session_id: String,
}

#[allow(dead_code)] // pure helper; consumed by the host and tests.
pub fn ready_event(connection_id: &str, db_session_id: &str) -> ConnectionReadyEvent {
    ConnectionReadyEvent {
        connection_id: connection_id.to_owned(),
        db_session_id: db_session_id.to_owned(),
    }
}
