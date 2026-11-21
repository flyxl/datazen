use std::collections::HashSet;

use serde_json::Value;

use crate::{
    ClipboardMatrix, GridClipboardAnchor, GridClipboardRange, PasteProblem, TableChangeContext,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardSource {
    pub table_key: String,
    pub column_names: Vec<String>,
    pub column_types: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowCopyPayload {
    pub table_key: String,
    pub column_names: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteOverflow {
    Expand,
    Clip,
}

impl PasteOverflow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Expand => "expand",
            Self::Clip => "clip",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteAlignment {
    ByName,
    ByPosition,
    Unknown,
}

impl PasteAlignment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByName => "byName",
            Self::ByPosition => "byPosition",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnWritabilityReason {
    AutoIncrement,
    UnknownColumn,
}

impl ColumnWritabilityReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoIncrement => "auto-increment",
            Self::UnknownColumn => "unknown-column",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnWritability {
    pub writable: bool,
    pub reason: Option<ColumnWritabilityReason>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellPasteWrite {
    pub row_index: i64,
    pub column_name: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastePlanOptions {
    pub matrix: ClipboardMatrix,
    pub anchor: GridClipboardAnchor,
    pub column_order: Vec<String>,
    pub columns: Vec<crate::types::ColumnSchema>,
    pub region: Option<GridClipboardRange>,
    pub page_row_count: i64,
    pub overflow: PasteOverflow,
    pub null_literal: String,
    pub pending_cell_keys: HashSet<String>,
    pub source: Option<ClipboardSource>,
    pub context: Option<TableChangeContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastePlan {
    pub writes: Vec<CellPasteWrite>,
    pub bounds: Option<GridClipboardRange>,
    pub problems: Vec<PasteProblem>,
    pub skipped: usize,
    pub overwritten_pending: usize,
    pub skipped_columns: Vec<String>,
    pub cross_table: Option<bool>,
    pub alignment: PasteAlignment,
    pub dropped_columns: Vec<String>,
    pub requires_confirmation: bool,
    pub matched_columns: usize,
    pub total_columns: usize,
}
