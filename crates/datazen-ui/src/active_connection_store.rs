//! Active-connection store ported from React
//! `packages/frontend/src/stores/activeConnectionStore.ts` (T039).
//!
//! The one map of live connections, keyed by persistent connection id: each
//! entry tracks the runtime session, the connection status, the server info
//! and driver capabilities, the current database and the last error. The
//! React store is a zustand store whose actions are `async` (IPC promises,
//! `setTimeout`, cross-window broadcasts). Rune is immediate-mode and
//! single-threaded, so this port is a **synchronous state machine**: every
//! React action is one or more pure transitions, and every step that needs
//! the outside world (an IPC call, a cross-window broadcast, a metadata
//! binding) is returned to the host as a [`ConnectionEffect`] instead of
//! being awaited here.
//!
//! # Host driving contract
//!
//! * `connect` — [`ActiveConnectionStore::connect`] claims the attempt
//!   (`ConnectStart::Started`) or joins the one already in flight
//!   (`ConnectStart::JoinedInFlight`). The claiming caller drives the IPC
//!   sequence `connect` → `testConnection` → `getConnectionInfo`, then feeds
//!   the answers back through [`ActiveConnectionStore::complete_connect`]
//!   (or [`ActiveConnectionStore::fail_connect`] on any failure). The
//!   `Started` caller is the only one that runs IPC; a joining caller does
//!   nothing and reads the entry the first attempt lands.
//! * `markConnected` — the cross-window entry point. The returned effects
//!   bind the session identity and ask the host for the capabilities IPC;
//!   the host feeds the answer back through
//!   [`ActiveConnectionStore::apply_capabilities`].
//! * `disconnect` — [`ActiveConnectionStore::disconnect`] returns a
//!   [`DisconnectPlan`]. `Removed` needs nothing more. `DisconnectSession`
//!   means the host runs the `disconnect` IPC, emits the plan's
//!   `on_success` effect, and then calls
//!   [`ActiveConnectionStore::finish_disconnect`] — on IPC failure too
//!   (React `finally`).
//! * `removeByDbSessionId` — the returned effects always start with
//!   [`ConnectionEffect::DropColumnReadsForSession`]. The host applies
//!   effects in order, synchronously, before any further transition, so the
//!   drop still precedes anything that could observe the removal.
//! * The 30-second `markConnecting` timeout is a due-at timestamp, not a
//!   timer: [`ActiveConnectionStore::mark_connecting`] records
//!   `connecting_since_ms`, and the host calls
//!   [`ActiveConnectionStore::tick`] with [`now_ms`] on its frame tick
//!   (the `metadata_cache` precedent). Only entries armed by
//!   `mark_connecting` expire — a `connect` attempt has no timeout in
//!   React, so [`ActiveConnectionStore::connect`] leaves the stamp empty.
//!
//! # Host hook points (no Rune implementation exists yet)
//!
//! * [`ConnectionEffect::BindSessionMetadataIdentity`] — React
//!   `bindSessionMetadataIdentity(dbSessionId, connectionId)` from
//!   `stores/metadataIdentityBinding`: announces that a runtime session now
//!   belongs to a persistent connection config, so the schema store drops
//!   relation columns cached under a previous binding. The host must run it
//!   after a successful connect **and** after `markConnected`, before any
//!   schema read for the session. Do not invent the call: until the binding
//!   lands as a Rune API, the host performs the equivalent invalidation.
//! * [`ConnectionEffect::DropColumnReadsForSession`] — React
//!   `dropColumnReadsForSession(dbSessionId)` from
//!   `stores/schemaColumnBatcher`: drops queued column reads for a session
//!   that is going away, so they are dropped here rather than left to fail
//!   later. Same rule: the host performs the equivalent until a Rune API
//!   exists.
//! * `Broadcast*` effects — React `emitCrossWindow` on the cross-window
//!   bus: `datazen:connection-ready` / `datazen:connection-failed` /
//!   `datazen:disconnect-requested`. No Rune cross-window emit exists yet;
//!   the host delivers them through whatever bus the windows share.
//!
//! # Deliberate differences from React
//!
//! * DEV `console.log` calls (`[connect] starting`, `[connect] pool
//!   created`, …) are dropped: native Rune has no console channel, and no
//!   ported state module keeps one. The transitions the logs bracketed are
//!   all covered by the state machine and its effects; failures still
//!   surface as the entry's `error` plus the `connection-failed` broadcast.
//!   `console.error`/`console.warn` on the failure paths are likewise
//!   dropped for the same reason — the error is in the entry, and a
//!   capability-discovery failure is advisory by design (it never fails
//!   the connection; the host passes `None` and warns on its own side).
//! * `extractError` takes `Option<&str>` — a message string, or nothing —
//!   instead of React's `unknown`: Rust call sites already hold either a
//!   message or no message. A `Some` message passes through verbatim; `None`
//!   resolves through `crate::i18n::t("backend.unknownError")` (`未知错误`).
//! * `connections` is a `HashMap`, not an insertion-ordered record. React
//!   only iterates the keys to delete by session id, where order is
//!   unobservable, so nothing is lost.
//! * `reset` clears the connections only, exactly like React: the
//!   in-flight claim set is untouched, so a connect already in flight keeps
//!   its claim across a reset.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds a `markConnecting` entry may stay `connecting` before the
/// store fails it with `Connection timeout` (React `setTimeout(…, 30_000)`).
pub(crate) const CONNECT_TIMEOUT_MS: u64 = 30_000;

/// Wall-clock milliseconds since the Unix epoch, for the host to feed
/// [`ActiveConnectionStore::tick`]. Falls back to `0` when the clock is
/// unavailable (the `metadata_cache` / `ai_write_proposal_state` precedent).
#[allow(dead_code)] // foundation API; consumers land with the connection views
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// The connection lifecycle, mirroring React `ConnectionStatus`
/// (`idle` / `connecting` / `connected` / `error`). `idle` is the value a
/// fresh store implies for a connection it has never seen; entries the
/// store actually holds are never `idle` — they start `connecting`.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ConnectionStatus {
    Idle,
    Connecting,
    Connected,
    Error,
}

#[allow(dead_code)] // wire spelling of the reusable status
impl ConnectionStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }
}

/// What the server reports about itself (React `ServerInfo`): the version
/// string and the server type, both straight from `testConnection`.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServerInfo {
    pub server_version: String,
    pub server_type: String,
}

/// Capabilities reported by the concrete driver for a live session (React
/// `DriverCapabilities`). `None` on an entry means the session info is
/// unknown — the UI treats cancellation as unknown until discovery lands.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DriverCapabilities {
    pub supports_cancel_query: bool,
    /// True only when the driver implements opaque execution-handle cancel.
    pub supports_query_execution_cancel: bool,
    pub supports_explain: bool,
    pub supports_streaming_results: bool,
}

/// One tracked connection (React `ConnectionEntry`), keyed in the store by
/// `connection_id` — the persistent config id, not the runtime session id.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectionEntry {
    /// Runtime database session id (empty while connecting/errored).
    pub db_session_id: String,
    /// Persistent connection id this entry was opened from (the map key).
    pub connection_id: String,
    pub status: ConnectionStatus,
    pub server_info: Option<ServerInfo>,
    /// Runtime driver capabilities; `None` while the session info is unknown.
    pub capabilities: Option<DriverCapabilities>,
    pub current_database: Option<String>,
    pub error: Option<String>,
    /// Wall-clock ms when `mark_connecting` armed this entry, or `None`
    /// when no timeout is armed (a `connect` attempt never arms one).
    /// Internal bookkeeping for [`ActiveConnectionStore::tick`]; not part of
    /// the React entry.
    pub connecting_since_ms: Option<u64>,
}

impl ConnectionEntry {
    /// A fresh `connecting` entry with an empty session, as
    /// `openConnection` / `markConnecting` seed it.
    fn connecting(connection_id: &str, database: Option<String>) -> Self {
        Self {
            db_session_id: String::new(),
            connection_id: connection_id.to_owned(),
            status: ConnectionStatus::Connecting,
            server_info: None,
            capabilities: None,
            current_database: database,
            error: None,
            connecting_since_ms: None,
        }
    }
}

/// What the store needs from a `ConnectionConfig` to open a connection: the
/// persistent id (the map key and the IPC argument) and the database to seed
/// `currentDatabase` with. The rest of the config (host, credentials, …)
/// stays on the IPC side — the store never sees it.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectRequest {
    pub connection_id: String,
    pub database: Option<String>,
}

/// Whether [`ActiveConnectionStore::connect`] started the attempt or joined
/// one already in flight.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConnectStart {
    /// This caller owns the attempt: it runs the IPC sequence and feeds the
    /// outcome back through `complete_connect` / `fail_connect`.
    Started,
    /// Another caller already owns the attempt for this connection id. This
    /// caller runs no IPC and reads the entry the first attempt lands.
    JoinedInFlight,
}

/// One step the host must perform outside the store, in the order returned.
/// Effects are instructions, not actions: the store never touches IPC, the
/// cross-window bus, or the schema store itself.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionEffect {
    /// Announce that a runtime session now belongs to a persistent
    /// connection config (React `bindSessionMetadataIdentity`). See the
    /// module docs: no Rune implementation exists yet.
    BindSessionMetadataIdentity {
        db_session_id: String,
        connection_id: String,
    },
    /// Broadcast `datazen:connection-ready` with this session.
    BroadcastConnectionReady {
        connection_id: String,
        db_session_id: String,
    },
    /// Broadcast `datazen:connection-failed` with this error sentence.
    BroadcastConnectionFailed {
        connection_id: String,
        error: String,
    },
    /// Broadcast `datazen:disconnect-requested` for this session.
    BroadcastDisconnectRequested { db_session_id: String },
    /// Ask the host for the session info IPC (`getConnectionInfo`) and feed
    /// the answer back through `apply_capabilities`. A discovery failure is
    /// advisory: the host passes `None` and the connection stays connected.
    RequestCapabilities {
        connection_id: String,
        db_session_id: String,
    },
    /// Drop queued column reads for a session that is going away (React
    /// `dropColumnReadsForSession`). See the module docs: no Rune
    /// implementation exists yet. Always first in the returned order.
    DropColumnReadsForSession { db_session_id: String },
}

/// What [`ActiveConnectionStore::disconnect`] asks the host to do.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DisconnectPlan {
    /// No live session for this connection: the entry is already removed
    /// and the host has nothing to do.
    Removed,
    /// A live session: the host runs the `disconnect` IPC for this session,
    /// then emits `on_success` (`datazen:disconnect-requested`), then calls
    /// `finish_disconnect` — on IPC failure too (React `finally`).
    DisconnectSession {
        db_session_id: String,
        on_success: ConnectionEffect,
    },
}

/// The active-connection store: all tracked connections, keyed by
/// connection id, plus the in-flight connect claims.
#[allow(dead_code)] // foundation API; consumers land with the connection views
#[derive(Clone, Debug, Default)]
pub(crate) struct ActiveConnectionStore {
    /// All tracked connections, keyed by connection id (persistent config
    /// connection id).
    connections: HashMap<String, ConnectionEntry>,
    /// Connection ids with a connect attempt in flight. A second caller for
    /// the same id joins the first instead of replacing its session.
    in_flight: HashSet<String>,
}

#[allow(dead_code)] // foundation API; consumers land with the connection views
impl ActiveConnectionStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// All tracked connections, keyed by connection id.
    pub(crate) fn connections(&self) -> &HashMap<String, ConnectionEntry> {
        &self.connections
    }

    /// The entry for one connection id, if the store tracks it.
    pub(crate) fn entry(&self, connection_id: &str) -> Option<&ConnectionEntry> {
        self.connections.get(connection_id)
    }

    /// Whether a connect attempt is in flight for this connection id.
    pub(crate) fn is_connect_in_flight(&self, connection_id: &str) -> bool {
        self.in_flight.contains(connection_id)
    }

    /// React `connect`: one connect per connection at a time. A double click
    /// and the tab it opens both ask for a session; without this guard the
    /// second `connect` replaces the first session and every reader of the
    /// old one fails with `sessionNotFound` — the schema tree then stays
    /// empty with no visible error. A joining caller gets
    /// [`ConnectStart::JoinedInFlight`] and runs no IPC; the starter gets
    /// [`ConnectStart::Started`] and owns the attempt until
    /// `complete_connect` / `fail_connect` releases the claim (React
    /// `.finally`, so a settled attempt frees the slot for the next one).
    pub(crate) fn connect(&mut self, request: ConnectRequest) -> ConnectStart {
        if self.in_flight.contains(&request.connection_id) {
            return ConnectStart::JoinedInFlight;
        }
        self.in_flight.insert(request.connection_id.clone());
        // `openConnection` seeds the connecting entry up front (empty
        // session, no server info, capabilities unknown).
        let entry = ConnectionEntry::connecting(&request.connection_id, request.database);
        self.connections.insert(request.connection_id, entry);
        ConnectStart::Started
    }

    /// React `openConnection` success: the `connect` IPC returned the
    /// session id, `testConnection` returned the server info, and capability
    /// discovery returned `capabilities` — or `None` when discovery failed,
    /// which must NOT turn a successful connection into a failed one (older
    /// / headless backends may not return the optional field; the UI then
    /// treats cancellation as unknown). Releases the in-flight claim and
    /// returns the bind + `connection-ready` broadcast, in that order.
    pub(crate) fn complete_connect(
        &mut self,
        connection_id: &str,
        db_session_id: &str,
        server_info: ServerInfo,
        capabilities: Option<DriverCapabilities>,
    ) -> Vec<ConnectionEffect> {
        self.in_flight.remove(connection_id);
        let current_database = self
            .connections
            .get(connection_id)
            .and_then(|entry| entry.current_database.clone());
        self.connections.insert(
            connection_id.to_owned(),
            ConnectionEntry {
                db_session_id: db_session_id.to_owned(),
                connection_id: connection_id.to_owned(),
                status: ConnectionStatus::Connected,
                server_info: Some(server_info),
                capabilities,
                current_database,
                error: None,
                connecting_since_ms: None,
            },
        );
        vec![
            ConnectionEffect::BindSessionMetadataIdentity {
                db_session_id: db_session_id.to_owned(),
                connection_id: connection_id.to_owned(),
            },
            ConnectionEffect::BroadcastConnectionReady {
                connection_id: connection_id.to_owned(),
                db_session_id: db_session_id.to_owned(),
            },
        ]
    }

    /// React `openConnection` failure: the entry keeps whatever the seeded
    /// connecting entry held — the current database re-seeded from the
    /// request (React spreads the entry current at catch time, which is the
    /// seed `openConnection` wrote up front, so server info and capabilities
    /// are already empty by then) — with an empty session, `error` status
    /// and the message. Releases the in-flight claim and returns the
    /// `connection-failed` broadcast.
    pub(crate) fn fail_connect(
        &mut self,
        connection_id: &str,
        error: String,
    ) -> Vec<ConnectionEffect> {
        self.in_flight.remove(connection_id);
        let previous = self.connections.get(connection_id);
        let mut entry = match previous {
            Some(entry) => entry.clone(),
            None => ConnectionEntry::connecting(connection_id, None),
        };
        entry.db_session_id = String::new();
        entry.connection_id = connection_id.to_owned();
        entry.status = ConnectionStatus::Error;
        entry.error = Some(error.clone());
        entry.connecting_since_ms = None;
        self.connections.insert(connection_id.to_owned(), entry);
        vec![ConnectionEffect::BroadcastConnectionFailed {
            connection_id: connection_id.to_owned(),
            error,
        }]
    }

    /// React `markConnecting`: mark a connection `connecting` without
    /// triggering IPC, and arm the 30-second timeout — the entry is failed
    /// with `Connection timeout` by [`ActiveConnectionStore::tick`] if it is
    /// still `connecting` then. (Only this path arms a timeout; `connect`
    /// does not.) A fresh entry replaces whatever was tracked before.
    pub(crate) fn mark_connecting(
        &mut self,
        connection_id: &str,
        database: Option<String>,
        now_ms: u64,
    ) {
        let mut entry = ConnectionEntry::connecting(connection_id, database);
        entry.connecting_since_ms = Some(now_ms);
        self.connections.insert(connection_id.to_owned(), entry);
    }

    /// The frame tick for the `markConnecting` timeout: every entry still
    /// `connecting` whose armed timestamp is at least [`CONNECT_TIMEOUT_MS`]
    /// old fails with the literal `Connection timeout` (React's timer checks
    /// `entry?.status === 'connecting'` at fire time — the same guard).
    /// Returns the timed-out connection ids, in no particular order.
    pub(crate) fn tick(&mut self, now_ms: u64) -> Vec<String> {
        let mut timed_out = Vec::new();
        for (connection_id, entry) in self.connections.iter_mut() {
            let armed_since = match entry.connecting_since_ms {
                Some(since) => since,
                None => continue,
            };
            if entry.status != ConnectionStatus::Connecting {
                continue;
            }
            if now_ms.saturating_sub(armed_since) < CONNECT_TIMEOUT_MS {
                continue;
            }
            entry.status = ConnectionStatus::Error;
            entry.error = Some("Connection timeout".to_owned());
            entry.connecting_since_ms = None;
            timed_out.push(connection_id.clone());
        }
        timed_out
    }

    /// React `markConnected`: the cross-window entry point. Marks the
    /// connection `connected` with the given session, capabilities unknown
    /// until the session-info IPC resolves (a direct ConnectionPage connect
    /// reaches this action without the store's `connect` flow, so discovery
    /// starts here too). Returns the bind effect plus the capabilities
    /// request; the host feeds the IPC answer back through
    /// `apply_capabilities`.
    pub(crate) fn mark_connected(
        &mut self,
        connection_id: &str,
        db_session_id: &str,
    ) -> Vec<ConnectionEffect> {
        let previous = self.connections.get(connection_id);
        let mut entry = match previous {
            Some(entry) => entry.clone(),
            None => ConnectionEntry::connecting(connection_id, None),
        };
        entry.db_session_id = db_session_id.to_owned();
        entry.connection_id = connection_id.to_owned();
        entry.status = ConnectionStatus::Connected;
        // Capabilities stay unknown until the session-info IPC resolves.
        entry.capabilities = None;
        entry.error = None;
        entry.connecting_since_ms = None;
        self.connections.insert(connection_id.to_owned(), entry);
        vec![
            ConnectionEffect::BindSessionMetadataIdentity {
                db_session_id: db_session_id.to_owned(),
                connection_id: connection_id.to_owned(),
            },
            ConnectionEffect::RequestCapabilities {
                connection_id: connection_id.to_owned(),
                db_session_id: db_session_id.to_owned(),
            },
        ]
    }

    /// The `getConnectionInfo` answer for a `markConnected` capabilities
    /// request. Applies only when the entry still exists **and** still holds
    /// this `db_session_id` — a session that was replaced or disconnected
    /// meanwhile must not receive the stale capabilities (React's
    /// `entry.dbSessionId !== dbSessionId` write-back guard). Returns
    /// whether the capabilities were written. `None` capabilities (a
    /// discovery failure, or a backend that advertises none) leave the
    /// entry connected with capabilities unknown.
    pub(crate) fn apply_capabilities(
        &mut self,
        connection_id: &str,
        db_session_id: &str,
        capabilities: Option<DriverCapabilities>,
    ) -> bool {
        let Some(entry) = self.connections.get_mut(connection_id) else {
            return false;
        };
        if entry.db_session_id != db_session_id {
            return false;
        }
        entry.capabilities = capabilities;
        true
    }

    /// React `markError`: the cross-window failure entry point. Marks the
    /// connection `error` with an empty session and the given message,
    /// keeping whatever else the entry held (server info, capabilities,
    /// current database — React spreads the old entry).
    pub(crate) fn mark_error(&mut self, connection_id: &str, error: String) {
        let previous = self.connections.get(connection_id);
        let mut entry = match previous {
            Some(entry) => entry.clone(),
            None => ConnectionEntry::connecting(connection_id, None),
        };
        entry.db_session_id = String::new();
        entry.connection_id = connection_id.to_owned();
        entry.status = ConnectionStatus::Error;
        entry.error = Some(error);
        entry.connecting_since_ms = None;
        self.connections.insert(connection_id.to_owned(), entry);
    }

    /// React `disconnect`: with no live session the entry is removed at
    /// once ([`DisconnectPlan::Removed`]); with a session the host must run
    /// the `disconnect` IPC and emit the plan's `on_success` broadcast, then
    /// call [`ActiveConnectionStore::finish_disconnect`] — on IPC failure
    /// too, so the entry is removed either way (React `finally`).
    pub(crate) fn disconnect(&mut self, connection_id: &str) -> DisconnectPlan {
        let db_session_id = self
            .connections
            .get(connection_id)
            .map(|entry| entry.db_session_id.clone())
            .unwrap_or_default();
        if db_session_id.is_empty() {
            self.connections.remove(connection_id);
            return DisconnectPlan::Removed;
        }
        DisconnectPlan::DisconnectSession {
            db_session_id: db_session_id.clone(),
            on_success: ConnectionEffect::BroadcastDisconnectRequested {
                db_session_id,
            },
        }
    }

    /// The `disconnect` tail: removes the entry unconditionally — the
    /// `finally` of the React flow, called after the IPC succeeds or fails.
    pub(crate) fn finish_disconnect(&mut self, connection_id: &str) {
        self.connections.remove(connection_id);
    }

    /// React `removeByDbSessionId`: removes every entry whose session is
    /// this one. Always returns the
    /// [`ConnectionEffect::DropColumnReadsForSession`] effect first — React
    /// drops the queued column reads before the removal, unconditionally,
    /// so a queued read for this session is dropped here rather than left
    /// to fail later. The host applies effects in order, synchronously,
    /// before any further transition.
    pub(crate) fn remove_by_db_session_id(&mut self, db_session_id: &str) -> Vec<ConnectionEffect> {
        self.connections
            .retain(|_, entry| entry.db_session_id != db_session_id);
        vec![ConnectionEffect::DropColumnReadsForSession {
            db_session_id: db_session_id.to_owned(),
        }]
    }

    /// React `reset`: clears the tracked connections. The in-flight claims
    /// are intentionally untouched — React's `reset` only sets
    /// `{ connections: {} }`.
    pub(crate) fn reset(&mut self) {
        self.connections.clear();
    }
}

/// React `extractError`: a message string passes through verbatim; anything
/// without a message resolves to the `backend.unknownError` text
/// (`未知错误`). Rust call sites hold either a message or none, so the
/// `unknown` input is `None` here.
#[allow(dead_code)] // foundation API; consumers land with the connection views
pub(crate) fn extract_error(message: Option<&str>) -> String {
    match message {
        Some(message) => message.to_owned(),
        None => crate::i18n::t("backend.unknownError").to_owned(),
    }
}
