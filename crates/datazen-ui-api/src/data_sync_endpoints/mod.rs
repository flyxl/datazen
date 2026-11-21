mod catalog;
mod identity;
mod scope;
mod selection;

pub use catalog::{
    pick_default_schema, pick_prefill_database, pick_prefill_schema, resolve_default_database,
    schema_load_allowed, unique_schemas_from_tables, EndpointPrefill, PrefillSide,
};
pub use identity::{
    is_same_endpoint_identity, is_verified_migration_pair, EndpointIdentity,
    VERIFIED_MIGRATION_TYPES,
};
pub use scope::{
    apply_endpoint_change, can_refresh_sessions, clear_endpoint_selection, connect_failed_message,
    load_databases_failed_message, session_failed_message, swap_scope, ChangeEffect, EndpointField,
    EndpointScope, CONNECT_FAILED_KEY, LOAD_DATABASES_FAILED_KEY, SESSION_FAILED_KEY,
};
pub use selection::{
    build_conn_options, build_target_options, connection_label, is_experimental,
    is_target_read_only, ConnectionLike, ConnectionOption, TargetOption,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedicatedSideSession {
    pub db_session_id: String,
    pub connection_id: String,
    pub database: String,
}

pub const CONNECTIONS_CLOSED_EVENT: &str = "datazen:connection-closed";
pub const CONNECTIONS_CHANGED_EVENT: &str = "datazen:connections-changed";
pub const UNSUPPORTED_PAIR_KEY: &str = "common.unsupportedPair";
pub const EXPERIMENTAL_PAIR_KEY: &str = "common.experimentalPairHint";

pub const DATA_SYNC_ENDPOINTS_SURFACE: [&str; 10] = [
    "useDataSyncEndpoints",
    "refreshEndpointSessions",
    "isSameEndpoint",
    "handleSwap",
    "handleSourceChange",
    "handleTargetChange",
    "buildTargetOptions",
    "pickPrefillDatabase",
    "isVerifiedMigrationPair",
    "DATA_SYNC_ENDPOINTS_GAP",
];

pub const DATA_SYNC_ENDPOINTS_GAP: [&str; 8] = [
    "React hook state, effects and cancellation flags",
    "get_connections / listDatabasesDedicated / listTables IPC",
    "ensureDedicatedSession / releaseDedicatedSession lifecycle",
    "crossWindowBus listeners for connection-closed / connections-changed",
    "useSyncPairingState and activePairing results",
    "useMigrationEndpointPrefill URL prefill side effects",
    "crypto.randomUUID and async job/session timing",
    "i18n t() for labels, hints and error messages",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_and_surfaces_are_pinned() {
        assert_eq!(CONNECTIONS_CLOSED_EVENT, "datazen:connection-closed");
        assert_eq!(CONNECTIONS_CHANGED_EVENT, "datazen:connections-changed");
        assert_eq!(UNSUPPORTED_PAIR_KEY, "common.unsupportedPair");
        assert_eq!(EXPERIMENTAL_PAIR_KEY, "common.experimentalPairHint");
        assert_eq!(DATA_SYNC_ENDPOINTS_SURFACE.len(), 10);
        assert_eq!(DATA_SYNC_ENDPOINTS_GAP.len(), 8);
        assert_eq!(crate::DATA_SYNC_ENDPOINTS_SURFACE.len(), 10);
        assert_eq!(crate::DATA_SYNC_ENDPOINTS_GAP.len(), 8);
    }

    #[test]
    fn dedicated_session_shape_is_pinned() {
        let session = DedicatedSideSession {
            db_session_id: "s1".to_owned(),
            connection_id: "c1".to_owned(),
            database: "db".to_owned(),
        };
        assert_eq!(session.db_session_id, "s1");
    }
}
