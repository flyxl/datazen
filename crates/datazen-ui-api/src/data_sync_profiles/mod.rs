mod load;
mod reconcile;
mod save;
mod types;

#[cfg(test)]
mod tests;

pub use load::{delete_selected_profile, load_selected_profile, load_sync_profiles};
pub use reconcile::{parse_timestamp, reconcile_unknown_run, ReconcileInput};
pub use save::{current_profile_tables, save_current_profile};
pub use types::{
    DataSyncTableMapping, MappingRowLike, MigrationRunRecordLike, ProfileConnection, ProfileEffect,
    ProfileEndpointState, ProfilesHost, ProfilesState, SyncProfile,
};

pub const PROFILE_MISSING_FIELDS_KEY: &str = "sync.profile.missingFields";
pub const PROFILE_MISSING_CONNECTION_KEY: &str = "sync.profile.missingConnection";
pub const PROFILE_SAVED_KEY: &str = "sync.profile.saved";
pub const PROFILE_LOADED_KEY: &str = "sync.profile.loaded";
pub const PROFILE_DELETED_KEY: &str = "sync.profile.deleted";
pub const RECONCILE_BUSY_KEY: &str = "migrationHistory.syncReconcileBusy";
pub const RECONCILE_MISSING_CONNECTION_KEY: &str =
    "migrationHistory.syncReconcileMissingConnection";
pub const RECONCILE_PROFILE_READY_KEY: &str = "migrationHistory.syncReconcileProfileReady";
pub const RECONCILE_SCOPE_UNAVAILABLE_KEY: &str = "migrationHistory.syncReconcileScopeUnavailable";
pub const RECONCILE_PROFILE_CHANGED_KEY: &str = "migrationHistory.syncReconcileProfileChanged";
pub const RECONCILE_NEEDS_SCOPE_KEY: &str = "migrationHistory.syncReconcileNeedsScope";

pub const DATA_SYNC_PROFILES_SURFACE: [&str; 10] = [
    "loadSyncProfiles",
    "saveCurrentProfile",
    "loadSelectedProfile",
    "deleteSelectedProfile",
    "reconcileUnknownRun",
    "currentProfileTables",
    "SyncProfile",
    "ProfilesHost",
    "ProfileEffect",
    "DATA_SYNC_PROFILES_GAP",
];

pub const DATA_SYNC_PROFILES_GAP: [&str; 7] = [
    "React hook state and effect teardown",
    "syncCommands get/save/delete profile and get_connections IPC",
    "listDatabasesDedicated catalog IPC is injected as ProfilesHost",
    "crypto.randomUUID and Date.toISOString are injected seams",
    "the seven shared refs and the wizard/selection/endpoint setters become effects",
    "useI18n t() is injected via ProfilesHost::translate",
    "Promise.all timing is immediate-mode here",
];
