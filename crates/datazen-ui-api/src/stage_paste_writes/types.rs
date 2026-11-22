use std::collections::BTreeMap;

use crate::{PendingRowChange, ValueMap, Value};

pub const STAGE_PASTE_WRITES_GAP: [&str; 3] = [
    "the caller applies the proposal in one set()",
    "GridErrorCode is surfaced by the caller, never written to ts.error",
    "single-cell stageCellValue shares these identity primitives",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridErrorCode {
    Unknown,
    CommitStalePlan,
    CommitAffectedMismatch,
    CommitConflictingIntents,
    CellNoRowIdentity,
    CellAmbiguousIdentity,
    CellReadOnly,
    CellInvalidValue,
    InsertUnknownColumn,
    InsertReturningUnavailable,
    FkShapeMismatch,
    FkUnknownConstraint,
    FkValueNull,
    FkTargetUnresolved,
    FkProbeFailed,
    FkNoReferences,
    FilterUnsupportedOperator,
    FilterIncomplete,
    ViewInvalid,
    ViewNameExists,
    ResultOriginUnavailable,
    ResultOriginAmbiguous,
    ResultRowNotAddressable,
    ResultMultiRowNotAllowed,
    ResultWriteDenied,
    WriteExpressionUnsupported,
    PagingKeysetUnavailable,
    PagingCursorInvalidated,
    PagingCursorTooDeep,
    PasteEmpty,
    PasteTooLarge,
    PasteRaggedRow,
    PasteUnclosedQuote,
    PasteOutOfBounds,
    PasteNoWritableCell,
    PasteNotNullEmpty,
    PasteIdentityChanged,
    PasteCrossTable,
    PasteColumnMismatch,
    ClipboardWriteFailed,
    ClipboardReadFailed,
    PasteRowsNotRowData,
    PasteRowsNoWritableColumn,
    FilterRawWhereEmpty,
    FilterRawWhereTooLong,
    FilterRawWhereUnterminatedString,
    FilterRawWhereUnterminatedIdentifier,
    FilterRawWhereComment,
    FilterRawWhereMultiStatement,
    FilterRawWhereUnbalancedParenthesis,
    FilterRawWhereForbiddenKeyword,
    FilterRawWhereDollarSign,
    FilterRawWhereUnsupported,
}

impl GridErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            GridErrorCode::Unknown => "unknown",
            GridErrorCode::CommitStalePlan => "grid.commit.stalePlan",
            GridErrorCode::CommitAffectedMismatch => "grid.commit.affectedMismatch",
            GridErrorCode::CommitConflictingIntents => "grid.commit.conflictingIntents",
            GridErrorCode::CellNoRowIdentity => "grid.cell.noRowIdentity",
            GridErrorCode::CellAmbiguousIdentity => "grid.cell.ambiguousIdentity",
            GridErrorCode::CellReadOnly => "grid.cell.readOnly",
            GridErrorCode::CellInvalidValue => "grid.cell.invalidValue",
            GridErrorCode::InsertUnknownColumn => "grid.insert.unknownColumn",
            GridErrorCode::InsertReturningUnavailable => "grid.insert.returningUnavailable",
            GridErrorCode::FkShapeMismatch => "grid.fk.shapeMismatch",
            GridErrorCode::FkUnknownConstraint => "grid.fk.unknownConstraint",
            GridErrorCode::FkValueNull => "grid.fk.valueNull",
            GridErrorCode::FkTargetUnresolved => "grid.fk.targetUnresolved",
            GridErrorCode::FkProbeFailed => "grid.fk.probeFailed",
            GridErrorCode::FkNoReferences => "grid.fk.noReferences",
            GridErrorCode::FilterUnsupportedOperator => "grid.filter.unsupportedOperator",
            GridErrorCode::FilterIncomplete => "grid.filter.incomplete",
            GridErrorCode::ViewInvalid => "grid.view.invalid",
            GridErrorCode::ViewNameExists => "grid.view.nameExists",
            GridErrorCode::ResultOriginUnavailable => "grid.result.originUnavailable",
            GridErrorCode::ResultOriginAmbiguous => "grid.result.originAmbiguous",
            GridErrorCode::ResultRowNotAddressable => "grid.result.rowNotAddressable",
            GridErrorCode::ResultMultiRowNotAllowed => "grid.result.multiRowNotAllowed",
            GridErrorCode::ResultWriteDenied => "grid.result.writeDenied",
            GridErrorCode::WriteExpressionUnsupported => "grid.write.expressionUnsupported",
            GridErrorCode::PagingKeysetUnavailable => "grid.paging.keysetUnavailable",
            GridErrorCode::PagingCursorInvalidated => "grid.paging.cursorInvalidated",
            GridErrorCode::PagingCursorTooDeep => "grid.paging.cursorTooDeep",
            GridErrorCode::PasteEmpty => "grid.paste.empty",
            GridErrorCode::PasteTooLarge => "grid.paste.tooLarge",
            GridErrorCode::PasteRaggedRow => "grid.paste.raggedRow",
            GridErrorCode::PasteUnclosedQuote => "grid.paste.unclosedQuote",
            GridErrorCode::PasteOutOfBounds => "grid.paste.outOfBounds",
            GridErrorCode::PasteNoWritableCell => "grid.paste.noWritableCell",
            GridErrorCode::PasteNotNullEmpty => "grid.paste.notNullEmpty",
            GridErrorCode::PasteIdentityChanged => "grid.paste.identityChanged",
            GridErrorCode::PasteCrossTable => "grid.paste.crossTable",
            GridErrorCode::PasteColumnMismatch => "grid.paste.columnMismatch",
            GridErrorCode::ClipboardWriteFailed => "grid.clipboard.writeFailed",
            GridErrorCode::ClipboardReadFailed => "grid.clipboard.readFailed",
            GridErrorCode::PasteRowsNotRowData => "grid.pasteRows.notRowData",
            GridErrorCode::PasteRowsNoWritableColumn => "grid.pasteRows.noWritableColumn",
            GridErrorCode::FilterRawWhereEmpty => "grid.filter.rawWhereEmpty",
            GridErrorCode::FilterRawWhereTooLong => "grid.filter.rawWhereTooLong",
            GridErrorCode::FilterRawWhereUnterminatedString => "grid.filter.rawWhereUnterminatedString",
            GridErrorCode::FilterRawWhereUnterminatedIdentifier => "grid.filter.rawWhereUnterminatedIdentifier",
            GridErrorCode::FilterRawWhereComment => "grid.filter.rawWhereComment",
            GridErrorCode::FilterRawWhereMultiStatement => "grid.filter.rawWhereMultiStatement",
            GridErrorCode::FilterRawWhereUnbalancedParenthesis => "grid.filter.rawWhereUnbalancedParenthesis",
            GridErrorCode::FilterRawWhereForbiddenKeyword => "grid.filter.rawWhereForbiddenKeyword",
            GridErrorCode::FilterRawWhereDollarSign => "grid.filter.rawWhereDollarSign",
            GridErrorCode::FilterRawWhereUnsupported => "grid.filter.rawWhereUnsupported",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteCellProblem {
    pub row_index: u64,
    pub column_name: String,
    pub code: GridErrorCode,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellPasteWrite {
    pub row_index: u64,
    pub column_name: String,
    pub value: Value,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StagedTableState {
    pub rows: Vec<ValueMap>,
    pub pending_changes: BTreeMap<String, PendingRowChange>,
    pub row_identity_anchors: BTreeMap<u64, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StagePasteWritesOutcome {
    pub rows: Vec<ValueMap>,
    pub pending_changes: BTreeMap<String, PendingRowChange>,
    pub row_identity_anchors: BTreeMap<u64, String>,
    pub staged: usize,
    pub problems: Vec<PasteCellProblem>,
}
