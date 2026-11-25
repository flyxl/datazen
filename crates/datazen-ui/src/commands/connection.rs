//! Port of `packages/frontend/src/commands/connection.ts` (T190) — the
//! connection command family's typed IPC shell, plus the in-flight
//! connect de-duplication that guarantees one backend session per
//! connection.
//!
//! ## What maps where
//!
//! - `legacyInvoke(cmd, args)` becomes [`ConnectionRequest`] (command +
//!   payload, the `ai::wire`/`backup` shaper pattern); the host
//!   executes it against the transport and decodes the typed receipts.
//! - `toIpcConnectionConfig` (validate, then a fresh plain clone) is
//!   `lib/connectionConfig.ts` — T375's, a seam here: the shapers take
//!   an api-crate [`ConnectionConfig`] and serialize it (serde's
//!   camelCase wire), which is the clone half; the cycle-check half
//!   (`assertPlainConnectionConfig`) is moot for an owned Rust value —
//!   an ownership tree cannot be cyclic by construction, exactly what
//!   the assert defends against on the JS side.
//! - `waitForE2EConnectDelay` is the webdriver harness's injectable
//!   delay (`VITE_E2E` + a window global): E2E-only machinery, not
//!   ported (the `override_path` ruling's twin).
//! - [`ConnectJoiner`] is `inFlightConnects`: the main window reaches a
//!   connection's session through two independent paths (the tab's
//!   `connectTab` and the store's `connect`), both funnel through
//!   `connect`, so joining concurrent callers to the first in-flight
//!   invoke HERE is the single guarantee of one backend session —
//!   without it the backend retires the first session and every reader
//!   fails with `sessionNotFound`. A settled attempt frees the slot
//!   (`.finally`); the shared answer/rejection reaches every joiner
//!   through the attempt they joined. `connectDedicated` deliberately
//!   bypasses the joiner: sub-windows open additional, isolated
//!   sessions.
//!
//! Known deltas: none behavioural — every command name and payload key
//! is the React source's verbatim.

use std::collections::BTreeSet;

use datazen_ui_api::ConnectionConfig;
use serde::{Deserialize, Serialize};

/// One shaped legacy command: the `legacyInvoke` name plus its payload.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionRequest {
    pub command: &'static str,
    pub value: serde_json::Value,
}

impl ConnectionRequest {
    fn native(command: &'static str, value: serde_json::Value) -> Self {
        Self { command, value }
    }
}

/// React `ConnectionSessionRef` — the self-describing result of
/// `connect` / `connect_dedicated`. `connectionId` is the persistent
/// connection-config id (survives restart); `dbSessionId` is the runtime
/// database session id (in-memory only, never persisted).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSessionRef {
    pub connection_id: String,
    pub db_session_id: String,
}

/// React `importConnections*`'s receipt (the merged-IPC import answers).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportConnectionsReceipt {
    pub imported: u32,
    pub overwritten: u32,
    pub groups_added: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_format: Option<String>,
}

/// React `getConnectionInfo`'s receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfoReceipt {
    pub database_type: String,
    pub driver_category: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    /// Null/absent means the backend or legacy driver did not advertise it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<datazen_ui_api::DriverCapabilities>,
}

/// React `detectConnectionImportPath`'s receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectImportPathReceipt {
    pub path: String,
    pub found: bool,
}

/// The import pickers' modes (React `'file' | 'folder'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportPathMode {
    File,
    Folder,
}

impl ImportPathMode {
    fn wire(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
        }
    }
}

// ---- the shapers (method for method) ---------------------------------------

/// `getConnections`.
pub fn get_connections_request() -> ConnectionRequest {
    ConnectionRequest::native("get_connections", serde_json::json!({}))
}

/// `saveConnection(config, clearMissingCredentials = false)`.
pub fn save_connection_request(
    config: &ConnectionConfig,
    clear_missing_credentials: bool,
) -> ConnectionRequest {
    ConnectionRequest::native(
        "save_connection",
        serde_json::json!({
            "config": config,
            "clearMissingCredentials": clear_missing_credentials,
        }),
    )
}

/// `duplicateConnection(sourceId, newId, newName)`.
pub fn duplicate_connection_request(
    source_id: &str,
    new_id: &str,
    new_name: &str,
) -> ConnectionRequest {
    ConnectionRequest::native(
        "duplicate_connection",
        serde_json::json!({ "sourceId": source_id, "newId": new_id, "newName": new_name }),
    )
}

/// `deleteConnection(id)`.
pub fn delete_connection_request(id: &str) -> ConnectionRequest {
    ConnectionRequest::native("delete_connection", serde_json::json!({ "id": id }))
}

/// `reorderConnections(orderedIds)`.
pub fn reorder_connections_request(ordered_ids: &[&str]) -> ConnectionRequest {
    ConnectionRequest::native(
        "reorder_connections",
        serde_json::json!({ "orderedIds": ordered_ids }),
    )
}

/// `testConnection(config)`.
pub fn test_connection_request(config: &ConnectionConfig) -> ConnectionRequest {
    ConnectionRequest::native("test_connection", serde_json::json!({ "config": config }))
}

/// `connect(connectionId)` — the wire half; the join decision is
/// [`ConnectJoiner`] below.
pub fn connect_request(connection_id: &str) -> ConnectionRequest {
    ConnectionRequest::native("connect", serde_json::json!({ "connectionId": connection_id }))
}

/// `connectDedicated(connectionId, database?)` — sub-window only, never
/// de-duplicated; `database ?? null` on the wire.
pub fn connect_dedicated_request(
    connection_id: &str,
    database: Option<&str>,
) -> ConnectionRequest {
    ConnectionRequest::native(
        "connect_dedicated",
        serde_json::json!({ "connectionId": connection_id, "database": database }),
    )
}

/// `pingConnection(dbSessionId)`.
pub fn ping_connection_request(db_session_id: &str) -> ConnectionRequest {
    ConnectionRequest::native("ping_connection", serde_json::json!({ "dbSessionId": db_session_id }))
}

/// `releaseConnection(dbSessionId)`.
pub fn release_connection_request(db_session_id: &str) -> ConnectionRequest {
    ConnectionRequest::native(
        "release_connection",
        serde_json::json!({ "dbSessionId": db_session_id }),
    )
}

/// `disconnect(dbSessionId)`.
pub fn disconnect_request(db_session_id: &str) -> ConnectionRequest {
    ConnectionRequest::native("disconnect", serde_json::json!({ "dbSessionId": db_session_id }))
}

/// `closeDatabase(dbSessionId, database)` — releases one database's
/// backend resources without closing the session.
pub fn close_database_request(db_session_id: &str, database: &str) -> ConnectionRequest {
    ConnectionRequest::native(
        "close_database",
        serde_json::json!({ "dbSessionId": db_session_id, "database": database }),
    )
}

/// `getOpenDatabases(dbSessionId)` — drives the navigator's "open" marker.
pub fn get_open_databases_request(db_session_id: &str) -> ConnectionRequest {
    ConnectionRequest::native(
        "get_open_databases",
        serde_json::json!({ "dbSessionId": db_session_id }),
    )
}

/// `getConnectionInfo(dbSessionId)`.
pub fn get_connection_info_request(db_session_id: &str) -> ConnectionRequest {
    ConnectionRequest::native(
        "get_connection_info",
        serde_json::json!({ "dbSessionId": db_session_id }),
    )
}

/// `getAvailableDrivers()`.
pub fn get_available_drivers_request() -> ConnectionRequest {
    ConnectionRequest::native("get_available_drivers", serde_json::json!({}))
}

/// `getGroups()`.
pub fn get_groups_request() -> ConnectionRequest {
    ConnectionRequest::native("get_groups", serde_json::json!({}))
}

/// `saveGroups(groups)`.
pub fn save_groups_request(groups: &[String]) -> ConnectionRequest {
    ConnectionRequest::native("save_groups", serde_json::json!({ "groups": groups }))
}

/// `exportConnections(password, defaultFileName)` — merged IPC (native
/// save dialog + encrypted export); the `override_path` hatch is
/// E2E-only. Answers the connection count, or null when dismissed.
pub fn export_connections_request(
    password: &str,
    default_file_name: &str,
) -> ConnectionRequest {
    ConnectionRequest::native(
        "export_connections",
        serde_json::json!({ "password": password, "defaultFileName": default_file_name }),
    )
}

/// `importConnectionsPreview(password)` (the password-then-dialog legacy
/// flow).
pub fn import_connections_preview_request(password: &str) -> ConnectionRequest {
    ConnectionRequest::native(
        "import_connections_preview",
        serde_json::json!({ "password": password }),
    )
}

/// `importConnections(password)` — legacy merged IPC; prefer
/// [`pick_connections_import_file_request`] +
/// [`import_connections_at_path_request`] for the file-first flow.
pub fn import_connections_request(password: &str) -> ConnectionRequest {
    ConnectionRequest::native(
        "import_connections_with_dialog",
        serde_json::json!({ "password": password }),
    )
}

/// `pickConnectionsImportFile()` — form-split F4c: the three import
/// pickers merged into one host command; the webview only names the
/// kind. Answers the picked path or null.
pub fn pick_connections_import_file_request() -> ConnectionRequest {
    ConnectionRequest::native("pick_import_file", serde_json::json!({ "kind": "connections" }))
}

/// `importConnectionsAtPath(password, path)` — the import after the file
/// is already chosen.
pub fn import_connections_at_path_request(
    password: &str,
    path: &str,
) -> ConnectionRequest {
    ConnectionRequest::native(
        "import_connections_at_path",
        serde_json::json!({ "password": password, "path": path }),
    )
}

/// `detectConnectionImportPath(source)`.
pub fn detect_connection_import_path_request(source: &str) -> ConnectionRequest {
    ConnectionRequest::native(
        "detect_connection_import_path",
        serde_json::json!({ "source": source }),
    )
}

/// `pickConnectionImportPathWithDialog(mode, source)` — the
/// competitor-path picker (F4c's third kind).
pub fn pick_connection_import_path_with_dialog_request(
    mode: ImportPathMode,
    source: &str,
) -> ConnectionRequest {
    ConnectionRequest::native(
        "pick_import_file",
        serde_json::json!({ "kind": "competitor-path", "mode": mode.wire(), "source": source }),
    )
}

/// `importConnectionsFromApp(source, password, dataPath)`.
pub fn import_connections_from_app_request(
    source: &str,
    password: &str,
    data_path: &str,
) -> ConnectionRequest {
    ConnectionRequest::native(
        "import_connections_from_app",
        serde_json::json!({ "source": source, "password": password, "dataPath": data_path }),
    )
}

// ---- the in-flight connect de-duplication ----------------------------------

/// What [`ConnectJoiner::begin`] decided for one `connect` caller.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectDecision {
    /// The first caller for this connection: issue this invoke.
    Issue(ConnectionRequest),
    /// An attempt is already in flight: join it — send nothing; the
    /// host delivers the attempt's settled answer (or rejection) to
    /// every joiner.
    JoinInFlight,
}

/// `inFlightConnects` — connects in flight, keyed by connectionId. Both
/// front-end paths (the tab's `connectTab` and the store's `connect`)
/// funnel through `connect`, so this shared boundary is the single
/// guarantee of one backend session per connection: without it the two
/// paths each issue `invoke('connect')`, the backend retires the first
/// session, and every reader of it fails with `sessionNotFound` (empty
/// schema tree, no visible error).
#[derive(Default)]
pub struct ConnectJoiner {
    in_flight: BTreeSet<String>,
}

impl ConnectJoiner {
    pub fn new() -> Self {
        Self::default()
    }

    /// `connect`'s decision half: the first caller for a connection
    /// issues the invoke; concurrent callers join it.
    pub fn begin(&mut self, connection_id: &str) -> ConnectDecision {
        if self.in_flight.insert(connection_id.to_owned()) {
            ConnectDecision::Issue(connect_request(connection_id))
        } else {
            ConnectDecision::JoinInFlight
        }
    }

    /// The attempt settled (answered or rejected): free the slot so the
    /// next `connect` issues a fresh invoke. The settled value/rejection
    /// rides the attempt the joiners already hold — the host delivers
    /// it to them; this only re-arms the boundary.
    pub fn finish(&mut self, connection_id: &str) {
        self.in_flight.remove(connection_id);
    }

    /// Whether an attempt for `connection_id` is still in flight.
    pub fn is_in_flight(&self, connection_id: &str) -> bool {
        self.in_flight.contains(connection_id)
    }
}
