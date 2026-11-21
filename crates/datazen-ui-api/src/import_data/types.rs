use std::fmt;

use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedData {
    pub columns: Vec<String>,
    pub rows: Vec<Map<String, Value>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    Csv,
    Json,
    Xlsx,
}

impl ImportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Xlsx => "xlsx",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportParseErrorKind {
    UnterminatedQuote,
    InvalidJson,
    JsonShape,
    EmptyFile,
    UnsupportedSheet,
    UnsupportedEncoding,
    InvalidBase64,
}

impl ImportParseErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnterminatedQuote => "unterminated-quote",
            Self::InvalidJson => "invalid-json",
            Self::JsonShape => "json-shape",
            Self::EmptyFile => "empty-file",
            Self::UnsupportedSheet => "unsupported-sheet",
            Self::UnsupportedEncoding => "unsupported-encoding",
            Self::InvalidBase64 => "invalid-base64",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportParseError {
    pub kind: ImportParseErrorKind,
    pub message: String,
    pub line: Option<usize>,
}

impl ImportParseError {
    pub fn new(kind: ImportParseErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            line: None,
        }
    }

    pub fn at_line(kind: ImportParseErrorKind, message: impl Into<String>, line: usize) -> Self {
        Self {
            kind,
            message: message.into(),
            line: Some(line),
        }
    }
}

impl fmt::Display for ImportParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ImportParseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportDiagnostic {
    DuplicateColumn {
        column: String,
    },
    RaggedRow {
        line: usize,
        expected: usize,
        got: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportParseResult {
    pub columns: Vec<String>,
    pub rows: Vec<Map<String, Value>>,
    pub row_origins: Vec<usize>,
    pub diagnostics: Vec<ImportDiagnostic>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImportEmptyFieldPolicy {
    #[default]
    Null,
    Empty,
}

impl ImportEmptyFieldPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Empty => "empty",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvParseOptions {
    pub delimiter: String,
    pub quote_char: String,
    pub first_row_is_header: bool,
    pub empty_unquoted_as: ImportEmptyFieldPolicy,
}

impl Default for CsvParseOptions {
    fn default() -> Self {
        Self {
            delimiter: "auto".to_owned(),
            quote_char: "\"".to_owned(),
            first_row_is_header: true,
            empty_unquoted_as: ImportEmptyFieldPolicy::Null,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JsonParseOptions {
    pub mode: Option<()>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XlsxParseOptions {
    pub sheet_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XlsxParseResult {
    pub columns: Vec<String>,
    pub rows: Vec<Map<String, Value>>,
    pub row_origins: Vec<usize>,
    pub diagnostics: Vec<ImportDiagnostic>,
    pub sheet_names: Vec<String>,
    pub chosen_sheet: Option<String>,
}

pub const IMPORT_ENCODINGS: &[&str] = &["utf-8", "gbk", "gb18030", "big5", "shift_jis", "latin1"];

pub fn is_supported_encoding(encoding: &str) -> bool {
    IMPORT_ENCODINGS.contains(&encoding)
}

pub fn detect_import_format(file_name: &str) -> Option<ImportFormat> {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".json") {
        return Some(ImportFormat::Json);
    }
    if lower.ends_with(".csv") {
        return Some(ImportFormat::Csv);
    }
    if lower.ends_with(".xlsx") {
        return Some(ImportFormat::Xlsx);
    }
    None
}
