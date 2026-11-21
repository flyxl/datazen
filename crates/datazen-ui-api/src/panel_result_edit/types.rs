pub const PANEL_RESULT_EDIT_GAP: [&str; 4] = [
    "useEffect resolveForResult",
    "useEffect cleanup clearForResult",
    "queryCommands.resolveResultOrigin IPC",
    "t() translation of the message key",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridReadOnlyReason {
    UnknownOrigin,
    AmbiguousOrigin,
    MultiTable,
    Aggregate,
    Distinct,
    SetOperation,
    DerivedSource,
    View,
    NoPrimaryKey,
    KeyNotSelected,
    SessionReadOnly,
    KeyValueNull,
    DuplicateIdentity,
    RowNotFound,
}

impl GridReadOnlyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            GridReadOnlyReason::UnknownOrigin => "unknownOrigin",
            GridReadOnlyReason::AmbiguousOrigin => "ambiguousOrigin",
            GridReadOnlyReason::MultiTable => "multiTable",
            GridReadOnlyReason::Aggregate => "aggregate",
            GridReadOnlyReason::Distinct => "distinct",
            GridReadOnlyReason::SetOperation => "setOperation",
            GridReadOnlyReason::DerivedSource => "derivedSource",
            GridReadOnlyReason::View => "view",
            GridReadOnlyReason::NoPrimaryKey => "noPrimaryKey",
            GridReadOnlyReason::KeyNotSelected => "keyNotSelected",
            GridReadOnlyReason::SessionReadOnly => "sessionReadOnly",
            GridReadOnlyReason::KeyValueNull => "keyValueNull",
            GridReadOnlyReason::DuplicateIdentity => "duplicateIdentity",
            GridReadOnlyReason::RowNotFound => "rowNotFound",
        }
    }
}

pub fn grid_read_only_message_key(reason: GridReadOnlyReason) -> String {
    format!("query.gridReadOnly.{}", reason.as_str())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityColumn {
    pub column: String,
    pub source_column: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultTableRef {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    pub aliased: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerEditability {
    pub writable: bool,
    pub identity_columns: Vec<IdentityColumn>,
    pub editable_columns: Vec<IdentityColumn>,
    pub table: Option<ResultTableRef>,
    pub reason: Option<GridReadOnlyReason>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultOriginRequest {
    pub db_session_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub sql: String,
    pub result_columns: Vec<String>,
    pub table: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultEditabilityOptions {
    pub key: Option<String>,
    pub db_session_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub sql: String,
    pub result_columns: Vec<String>,
    pub table: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerdictView {
    pub verdict: Option<ServerEditability>,
    pub resolving: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolveTicket {
    pub key: String,
    pub generation: u64,
    pub request: ResultOriginRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultWritableError {
    pub message_key: String,
}

impl ResultWritableError {
    pub fn message_key(&self) -> &str {
        &self.message_key
    }
}

pub fn result_edit_key(db_session_id: &str, result_idx: usize) -> String {
    format!("{db_session_id}#{result_idx}")
}
