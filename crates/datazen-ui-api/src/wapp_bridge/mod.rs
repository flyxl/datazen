mod host;
mod machine;
mod protocol;
mod routes;
mod state;
mod validation;

#[cfg(test)]
mod tests;

pub use host::WappBridgeHost;
pub use machine::WappBridge;
pub use protocol::{
    err_envelope, host_ready_envelope, is_request_envelope, ok_envelope, parse_request,
    response_type_of, theme_apply_envelope, theme_snapshot_value, to_public_connection,
    BridgeConnectionSummary, BridgeErrorCode, BridgeErrorPayload, BridgeTarget,
    ConnectionConfigLike, WappErrEnvelope, WappOkEnvelope, WappRequestEnvelope,
    WappResponseEnvelope, BRIDGE_CHANNEL, BRIDGE_ERROR_CODES, ERROR_MESSAGE_LIMIT_CHARS,
    MAX_INFLIGHT_REQUESTS, MAX_REQUEST_ID_CHARS, NOTIFY_MIN_INTERVAL_MS, REQUEST_TIMEOUT_MS,
    SESSION_REQUEST_LIMIT,
};
pub use routes::{is_wapp_command_allowed, route_for, API_ROUTES, WAPP_COMMAND_DENYLIST};
pub use state::{
    timeout_disposition, AttachBridgeOptions, BridgeEffect, BridgeState, TimeoutDisposition,
};
pub use validation::{
    as_nonempty_string, looks_like_not_found, parse_command_invoke, parse_i18n_key, parse_notify,
    parse_storage, BridgeApiError, CommandInvokeInput, NotifyInput, StorageInput,
};

pub const WAPP_BRIDGE_SURFACE: [&str; 9] = [
    "BRIDGE_CHANNEL",
    "attachBridge",
    "isWappCommandAllowed",
    "routeFor",
    "WappRequestEnvelope",
    "WappResponseEnvelope",
    "BridgeApiError",
    "WappBridgeHost",
    "WappBridge",
];

pub const WAPP_BRIDGE_DOM_GAP: [&str; 5] = [
    "iframe.contentWindow",
    "iframe.addEventListener('load', onFrameLoad)",
    "window.addEventListener('message', onMessage)",
    "contentWindow.postMessage(payload, '*')",
    "window.removeEventListener('message', onMessage)",
];

pub const WAPP_BRIDGE_IPC_GAP: [&str; 7] = [
    "legacyInvoke('wapp_session', register)",
    "legacyInvoke('wapp_session', end)",
    "legacyInvoke('wapp_session', request)",
    "wappCommands.executeDriverCommand",
    "wappCommands.wappStorageGet/Set/Remove",
    "wappCommands.auditLog",
    "requirePlatformServices().showNotification",
];

pub const WAPP_BRIDGE_EVENT_GAP: [&str; 4] = [
    "message event.source === iframe.contentWindow",
    "iframe self-navigation 'load' event",
    "setTimeout/clearTimeout per-request deadline",
    "crypto.randomUUID() instance nonce",
];

pub const WAPP_BRIDGE_HANDLER_GAP: [&str; 4] = [
    "useConnectionStore.getState()",
    "useActiveConnectionStore.getState()",
    "resolveWappString(wappId, key, locale)",
    "buildThemeSnapshot()",
];
