use serde_json::Value;

pub const SQL_INSERT_BATCH_SIZE: usize = 500;

pub const XLSX_MIME_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportColumn {
    pub name: String,
    pub is_primary_key: bool,
}

impl ExportColumn {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            is_primary_key: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatRowsOptions {
    pub table_name: String,
    pub columns: Vec<ExportColumn>,
    pub rows: Vec<Vec<Value>>,
    pub database_type: Option<String>,
    pub primary_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextExportResult {
    pub content: String,
    pub extension: String,
    pub mime_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryExportResult {
    pub data_base64: String,
    pub extension: String,
    pub mime_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportFormatError {
    NoColumns,
    SqlUpdateNeedsNonKeyColumn,
    BinaryFormatNotText(crate::ExportDataFormat),
    XlsxFormatRejected(crate::ExportDataFormat),
    XlsxWriterUnavailable,
}

impl ExportFormatError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoColumns => "export needs at least one column",
            Self::SqlUpdateNeedsNonKeyColumn => {
                "sql_update export needs at least one non-key column to SET"
            }
            Self::BinaryFormatNotText(_) => "binary format cannot be formatted as text",
            Self::XlsxFormatRejected(_) => "formatRowsAsXlsx only handles xlsx",
            Self::XlsxWriterUnavailable => "xlsx writer unavailable in ui-api",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum XlsxCellValue {
    Blank,
    Bool(bool),
    Number(String),
    Text(String),
    BytesHex(String),
    ObjectJson(String),
}
