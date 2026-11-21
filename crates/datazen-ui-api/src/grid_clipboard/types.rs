pub const GRID_PASTE_MAX_BYTES: usize = 10 * 1024 * 1024;
pub const GRID_PASTE_MAX_CELLS: usize = 100_000;
pub const GRID_PASTE_CONFIRM_CELLS: usize = 2_000;
pub const GRID_PASTE_CHUNK_CELLS: usize = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardDelimiter {
    Tab,
    Comma,
    Semicolon,
}

impl ClipboardDelimiter {
    pub fn as_char(self) -> char {
        match self {
            Self::Tab => '\t',
            Self::Comma => ',',
            Self::Semicolon => ';',
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tab => "\t",
            Self::Comma => ",",
            Self::Semicolon => ";",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardCellCoord {
    pub row_index: i64,
    pub column_name: String,
}

pub type GridClipboardAnchor = ClipboardCellCoord;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridClipboardRange {
    pub top_row: i64,
    pub bottom_row: i64,
    pub left_column: String,
    pub right_column: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardMatrix {
    pub cells: Vec<Vec<String>>,
    pub quoted: Vec<Vec<bool>>,
    pub delimiter: ClipboardDelimiter,
    pub multiline: bool,
    pub byte_length: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteParseErrorCode {
    Empty,
    TooLarge,
    RaggedRow,
}

impl PasteParseErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "grid.paste.empty",
            Self::TooLarge => "grid.paste.tooLarge",
            Self::RaggedRow => "grid.paste.raggedRow",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteParseRefusal {
    pub code: PasteParseErrorCode,
    pub cells: usize,
    pub byte_length: usize,
    pub row: Option<usize>,
    pub expected: Option<usize>,
    pub actual: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardParseResult {
    Ok {
        matrix: ClipboardMatrix,
        problems: Vec<PasteProblem>,
    },
    Refused(PasteParseRefusal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteProblemCode {
    Empty,
    TooLarge,
    RaggedRow,
    UnclosedQuote,
    NoWritableCell,
    OutOfBounds,
    ReadOnly,
    NotNullEmpty,
    ColumnMismatch,
}

impl PasteProblemCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "grid.paste.empty",
            Self::TooLarge => "grid.paste.tooLarge",
            Self::RaggedRow => "grid.paste.raggedRow",
            Self::UnclosedQuote => "grid.paste.unclosedQuote",
            Self::NoWritableCell => "grid.paste.noWritableCell",
            Self::OutOfBounds => "grid.paste.outOfBounds",
            Self::ReadOnly => "grid.cell.readOnly",
            Self::NotNullEmpty => "grid.paste.notNullEmpty",
            Self::ColumnMismatch => "grid.paste.columnMismatch",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteProblem {
    pub code: PasteProblemCode,
    pub row: Option<i64>,
    pub column: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardFormat {
    Tsv,
    Csv,
    Json,
    Plain,
    Html,
    Markdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializeCellsOptions {
    pub format: ClipboardFormat,
    pub column_names: Vec<String>,
    pub include_header: Option<bool>,
    pub null_literal: String,
    pub json_objects: bool,
    pub json_indent: usize,
}

impl SerializeCellsOptions {
    pub fn new(format: ClipboardFormat, column_names: Vec<String>) -> Self {
        Self {
            format,
            column_names,
            include_header: None,
            null_literal: String::new(),
            json_objects: false,
            json_indent: 0,
        }
    }
}
