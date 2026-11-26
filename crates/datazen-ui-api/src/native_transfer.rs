//! Typed vocabulary for live SQLite migration tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferSide {
    Source,
    Target,
}

impl TransferSide {
    pub fn key(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferChoice {
    pub connection_id: String,
    pub table: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferCatalog {
    pub choice: TransferChoice,
    pub tables: Vec<String>,
    pub columns: Vec<String>,
    /// Opaque physical identity; the worker checks it again before executing.
    pub identity: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferEndpoint {
    pub connection_id: String,
    pub table: String,
    pub identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTransferRequest {
    pub source: TransferEndpoint,
    pub target: TransferEndpoint,
    pub columns: Vec<(String, String)>,
}

/// A read-only request. Unlike transfer, it carries no mappings or write mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqliteReadComparison {
    pub source: TransferEndpoint,
    pub target: TransferEndpoint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSchemaDiff {
    pub missing_on_target: Vec<String>,
    pub extra_on_target: Vec<String>,
    /// Column name plus the attributes that differ (not executable SQL).
    pub changed: Vec<(String, Vec<String>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSyncPreview {
    pub source_only: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub target_only: usize,
}

pub const NATIVE_SYNC_ROW_LIMIT: u32 = 1000;
