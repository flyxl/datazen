#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

pub const GRID_ERROR_NAMESPACE: &str = "grid.";

pub const GRID_ERROR_CODES: [GridErrorCode; 52] = [
    GridErrorCode::CommitStalePlan,
    GridErrorCode::CommitAffectedMismatch,
    GridErrorCode::CommitConflictingIntents,
    GridErrorCode::CellNoRowIdentity,
    GridErrorCode::CellAmbiguousIdentity,
    GridErrorCode::CellReadOnly,
    GridErrorCode::CellInvalidValue,
    GridErrorCode::InsertUnknownColumn,
    GridErrorCode::InsertReturningUnavailable,
    GridErrorCode::FkShapeMismatch,
    GridErrorCode::FkUnknownConstraint,
    GridErrorCode::FkValueNull,
    GridErrorCode::FkTargetUnresolved,
    GridErrorCode::FkProbeFailed,
    GridErrorCode::FkNoReferences,
    GridErrorCode::FilterUnsupportedOperator,
    GridErrorCode::FilterIncomplete,
    GridErrorCode::ViewInvalid,
    GridErrorCode::ViewNameExists,
    GridErrorCode::ResultOriginUnavailable,
    GridErrorCode::ResultOriginAmbiguous,
    GridErrorCode::ResultRowNotAddressable,
    GridErrorCode::ResultMultiRowNotAllowed,
    GridErrorCode::ResultWriteDenied,
    GridErrorCode::WriteExpressionUnsupported,
    GridErrorCode::PagingKeysetUnavailable,
    GridErrorCode::PagingCursorInvalidated,
    GridErrorCode::PagingCursorTooDeep,
    GridErrorCode::PasteEmpty,
    GridErrorCode::PasteTooLarge,
    GridErrorCode::PasteRaggedRow,
    GridErrorCode::PasteUnclosedQuote,
    GridErrorCode::PasteOutOfBounds,
    GridErrorCode::PasteNoWritableCell,
    GridErrorCode::PasteNotNullEmpty,
    GridErrorCode::PasteIdentityChanged,
    GridErrorCode::PasteCrossTable,
    GridErrorCode::PasteColumnMismatch,
    GridErrorCode::ClipboardWriteFailed,
    GridErrorCode::ClipboardReadFailed,
    GridErrorCode::PasteRowsNotRowData,
    GridErrorCode::PasteRowsNoWritableColumn,
    GridErrorCode::FilterRawWhereEmpty,
    GridErrorCode::FilterRawWhereTooLong,
    GridErrorCode::FilterRawWhereUnterminatedString,
    GridErrorCode::FilterRawWhereUnterminatedIdentifier,
    GridErrorCode::FilterRawWhereComment,
    GridErrorCode::FilterRawWhereMultiStatement,
    GridErrorCode::FilterRawWhereUnbalancedParenthesis,
    GridErrorCode::FilterRawWhereForbiddenKeyword,
    GridErrorCode::FilterRawWhereDollarSign,
    GridErrorCode::FilterRawWhereUnsupported,
];

pub const REGISTERED_PREFIXES: [GridErrorCode; 50] = [
    GridErrorCode::CommitStalePlan,
    GridErrorCode::CommitAffectedMismatch,
    GridErrorCode::CommitConflictingIntents,
    GridErrorCode::CellNoRowIdentity,
    GridErrorCode::CellAmbiguousIdentity,
    GridErrorCode::CellReadOnly,
    GridErrorCode::CellInvalidValue,
    GridErrorCode::InsertUnknownColumn,
    GridErrorCode::InsertReturningUnavailable,
    GridErrorCode::FkShapeMismatch,
    GridErrorCode::FkUnknownConstraint,
    GridErrorCode::FkValueNull,
    GridErrorCode::FkTargetUnresolved,
    GridErrorCode::FkProbeFailed,
    GridErrorCode::FkNoReferences,
    GridErrorCode::FilterUnsupportedOperator,
    GridErrorCode::FilterIncomplete,
    GridErrorCode::ViewInvalid,
    GridErrorCode::ViewNameExists,
    GridErrorCode::ResultOriginUnavailable,
    GridErrorCode::ResultOriginAmbiguous,
    GridErrorCode::ResultRowNotAddressable,
    GridErrorCode::ResultMultiRowNotAllowed,
    GridErrorCode::ResultWriteDenied,
    GridErrorCode::WriteExpressionUnsupported,
    GridErrorCode::PagingKeysetUnavailable,
    GridErrorCode::PagingCursorInvalidated,
    GridErrorCode::PagingCursorTooDeep,
    GridErrorCode::FilterRawWhereEmpty,
    GridErrorCode::FilterRawWhereTooLong,
    GridErrorCode::FilterRawWhereUnterminatedString,
    GridErrorCode::FilterRawWhereUnterminatedIdentifier,
    GridErrorCode::FilterRawWhereComment,
    GridErrorCode::FilterRawWhereMultiStatement,
    GridErrorCode::FilterRawWhereUnbalancedParenthesis,
    GridErrorCode::FilterRawWhereForbiddenKeyword,
    GridErrorCode::FilterRawWhereDollarSign,
    GridErrorCode::FilterRawWhereUnsupported,
    GridErrorCode::PasteEmpty,
    GridErrorCode::PasteTooLarge,
    GridErrorCode::PasteRaggedRow,
    GridErrorCode::PasteUnclosedQuote,
    GridErrorCode::PasteOutOfBounds,
    GridErrorCode::PasteNoWritableCell,
    GridErrorCode::PasteNotNullEmpty,
    GridErrorCode::PasteIdentityChanged,
    GridErrorCode::PasteCrossTable,
    GridErrorCode::PasteColumnMismatch,
    GridErrorCode::ClipboardWriteFailed,
    GridErrorCode::ClipboardReadFailed,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteProblem {
    pub code: GridErrorCode,
    pub row: Option<i64>,
    pub column: Option<String>,
    pub detail: Option<String>,
}

pub fn classify_grid_error(message: &str) -> GridErrorCode {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return GridErrorCode::Unknown;
    }
    let mut prefixes = REGISTERED_PREFIXES.to_vec();
    prefixes.sort_by(|left, right| right.as_str().len().cmp(&left.as_str().len()));
    for code in prefixes {
        let text = code.as_str();
        if trimmed == text || trimmed.starts_with(&format!("{text}:")) {
            return code;
        }
    }
    GridErrorCode::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_is_pinned() {
        assert_eq!(GRID_ERROR_NAMESPACE, "grid.");
    }

    #[test]
    fn all_wire_spellings_are_pinned_and_unique() {
        assert_eq!(GRID_ERROR_CODES.len(), 52);
        for code in GRID_ERROR_CODES {
            assert!(code.as_str().starts_with("grid."));
        }
        let mut strings = GRID_ERROR_CODES.iter().map(|code| code.as_str()).collect::<Vec<_>>();
        strings.sort_unstable();
        strings.dedup();
        assert_eq!(strings.len(), 52);
    }

    #[test]
    fn registry_is_the_50_reachable_codes() {
        assert_eq!(REGISTERED_PREFIXES.len(), 50);
        assert!(!REGISTERED_PREFIXES.contains(&GridErrorCode::PasteRowsNotRowData));
        assert!(!REGISTERED_PREFIXES.contains(&GridErrorCode::PasteRowsNoWritableColumn));
        assert!(GRID_ERROR_CODES.contains(&GridErrorCode::PasteRowsNotRowData));
        assert!(GRID_ERROR_CODES.contains(&GridErrorCode::PasteRowsNoWritableColumn));
        for code in REGISTERED_PREFIXES {
            assert!(GRID_ERROR_CODES.contains(&code));
        }
    }

    #[test]
    fn empty_and_unknown_messages_do_not_match() {
        assert_eq!(classify_grid_error(""), GridErrorCode::Unknown);
        assert_eq!(classify_grid_error("   "), GridErrorCode::Unknown);
        assert_eq!(classify_grid_error("plain error"), GridErrorCode::Unknown);
        assert_eq!(classify_grid_error("grid.cell.readOnlyBecause: x"), GridErrorCode::Unknown);
        assert_eq!(classify_grid_error("error: grid.cell.readOnly happened"), GridErrorCode::Unknown);
        assert_eq!(classify_grid_error("GRID.CELL.READONLY: x"), GridErrorCode::Unknown);
    }

    #[test]
    fn exact_code_and_code_with_separator_match() {
        assert_eq!(classify_grid_error("grid.cell.readOnly"), GridErrorCode::CellReadOnly);
        assert_eq!(classify_grid_error("  grid.cell.readOnly: nope  "), GridErrorCode::CellReadOnly);
        assert_eq!(classify_grid_error("grid.fk.noReferences: none"), GridErrorCode::FkNoReferences);
        assert_eq!(
            classify_grid_error("grid.filter.rawWhereUnterminatedString: bad"),
            GridErrorCode::FilterRawWhereUnterminatedString
        );
        assert_eq!(classify_grid_error("grid.pasteRows.notRowData: x"), GridErrorCode::Unknown);
    }

    #[test]
    fn every_registered_prefix_is_recognized() {
        for code in REGISTERED_PREFIXES {
            assert_eq!(classify_grid_error(code.as_str()), code);
            let with_message = format!("{}: explanation", code.as_str());
            assert_eq!(classify_grid_error(&with_message), code);
        }
    }

    #[test]
    fn paste_problem_carries_structured_fields() {
        let problem = PasteProblem {
            code: GridErrorCode::PasteRaggedRow,
            row: Some(2),
            column: Some("name".to_owned()),
            detail: None,
        };
        assert_eq!(problem.code, GridErrorCode::PasteRaggedRow);
        assert_eq!(problem.row, Some(2));
        assert_eq!(problem.detail, None);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::classify_grid_error;
        assert_eq!(crate::GRID_ERROR_NAMESPACE, "grid.");
        let _ = crate::GRID_ERROR_CODES;
        let _ = crate::REGISTERED_PREFIXES;
        let _: Option<crate::GridErrorCode> = None;
        let _: Option<crate::PasteProblem> = None;
    }
}
