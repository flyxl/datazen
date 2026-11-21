use crate::SyncOptions;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSyncTableMapping {
    pub source_table: String,
    pub target_table: String,
    pub enabled: bool,
    pub source_filter: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SyncProfile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub source_connection_id: String,
    pub target_connection_id: String,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    pub tables: Vec<DataSyncTableMapping>,
    pub options: SyncOptions,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappingRowLike {
    pub source_table: String,
    pub target_table: String,
    pub source_filter: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MigrationRunRecordLike {
    pub operation: String,
    pub outcome: String,
    pub profile_id: Option<String>,
    pub profile_revision: Option<String>,
    pub source_connection_id: Option<String>,
    pub target_connection_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileConnection {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProfileEffect {
    SetError(String),
    SetErrorOpen(bool),
    SetStatus(String),
    SetProfiles(Vec<SyncProfile>),
    SetProfileName(String),
    SetSelectedProfileId(String),
    SetSourceId(String),
    SetTargetId(String),
    SetSourceDatabase(String),
    SetTargetDatabase(String),
    SetSourceSchema(String),
    SetTargetSchema(String),
    SetSyncOptions(SyncOptions),
    ResetCompareState,
    ClearSelectedRows,
    ClearTableSelections,
    ClearPageIndex,
    ClearPageCursors,
    SetStep(String),
    SetSyncState(String),
    ClearLoadedPage,
    SetLastExecutionResultNull,
    SetWriteOutcomeUncertain(bool),
    SetScopeReconfirmationRequired(bool),
    BumpGeneration,
    ClearJobRefs,
    SetPendingMappings(Option<Vec<DataSyncTableMapping>>),
    SetDatabases {
        source: Vec<String>,
        target: Vec<String>,
    },
    ClearSchemas,
    SetConnections(Vec<ProfileConnection>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfilesState {
    pub sync_profiles: Vec<SyncProfile>,
    pub profile_name: String,
    pub selected_profile_id: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileEndpointState {
    pub source_id: String,
    pub target_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: String,
    pub target_schema: String,
    pub connections: Vec<ProfileConnection>,
}

pub trait ProfilesHost {
    fn translate(&self, key: &str) -> String;

    fn get_profiles(&self) -> Result<Vec<SyncProfile>, String>;

    fn save_profile(&self, profile: &SyncProfile) -> Result<(), String>;

    fn delete_profile(&self, id: &str) -> Result<(), String>;

    fn get_connections(&self) -> Result<Vec<ProfileConnection>, String>;

    fn list_databases(&self, connection_id: &str) -> Result<Vec<String>, String>;

    fn new_id(&self) -> String;

    fn now_iso(&self) -> String;
}
