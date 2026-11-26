//! Typed host requests for a bounded append-only CSV import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvImportChoice {
    pub source_path: String,
    pub connection_id: String,
    pub table: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvImportReview {
    pub choice: CsvImportChoice,
    pub file_name: String,
    pub target_name: String,
    /// Each exact CSV header is mapped to the identically named SQLite column.
    pub columns: Vec<String>,
    pub row_count: usize,
    pub file_digest: [u8; 32],
    pub table_digest: [u8; 32],
}
