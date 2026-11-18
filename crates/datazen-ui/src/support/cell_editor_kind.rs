//! Cell editor kind resolution, ported from the kind half of
//! `packages/frontend/src/lib/cellEditorKind.ts`.
//!
//! This is a projection of [`CellType`]: the grid's editor control must agree
//! with the colour it paints, so the decision is made once here rather than
//! inline in the grid.
#![allow(dead_code)] // consumers: CellRenderer/GridCell/DetailPanel (T004/T015/T009)

use crate::support::cell_types::CellType;

/// The editor control a cell needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellEditorKind {
    Text,
    Number,
    Boolean,
    Enum,
    Date,
    Json,
    Binary,
    LongText,
}

impl CellEditorKind {
    /// Stable identifier, handy for assertions and diagnostics.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Enum => "enum",
            Self::Date => "date",
            Self::Json => "json",
            Self::Binary => "binary",
            Self::LongText => "longText",
        }
    }
}

/// A value longer than this is edited as a long text, not inline.
pub(crate) const INLINE_TEXT_MAX_CODE_POINTS: usize = 200;

/// The inline editor never holds more than 64 KiB.
pub(crate) const CELL_EDIT_MAX_BYTES: usize = 64 * 1024;

/// Everything the kind resolution needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellEditorKindInput {
    pub cell_type: CellType,
    /// The value carries a time zone (a picker cannot hold an offset).
    pub zoned: bool,
    /// The column has a candidate list; currently unused by the dispatch but
    /// part of the React contract.
    pub has_candidates: bool,
    pub value_code_points: usize,
    pub has_line_break: bool,
}

/// Resolve the editor kind.
///
/// Zoned time and datetime cells fall back to text on purpose: silently
/// dropping the offset would move the instant. `uuid` is always text because
/// there is no dedicated editor.
pub(crate) fn resolve_cell_editor_kind(input: CellEditorKindInput) -> CellEditorKind {
    match input.cell_type {
        CellType::Bool => CellEditorKind::Boolean,
        CellType::Enum => CellEditorKind::Enum,
        CellType::Date => CellEditorKind::Date,
        CellType::Time => {
            if input.zoned {
                CellEditorKind::Text
            } else {
                CellEditorKind::Date
            }
        }
        CellType::DateTime => {
            if input.zoned {
                CellEditorKind::Text
            } else {
                CellEditorKind::Date
            }
        }
        CellType::Number => CellEditorKind::Number,
        CellType::Json => CellEditorKind::Json,
        CellType::Bytes => CellEditorKind::Binary,
        CellType::Text | CellType::Unknown => {
            if input.value_code_points > INLINE_TEXT_MAX_CODE_POINTS || input.has_line_break {
                CellEditorKind::LongText
            } else {
                CellEditorKind::Text
            }
        }
        CellType::Uuid => CellEditorKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(cell_type: CellType) -> CellEditorKindInput {
        CellEditorKindInput {
            cell_type,
            zoned: false,
            has_candidates: false,
            value_code_points: 0,
            has_line_break: false,
        }
    }

    #[test]
    fn maps_every_plain_cell_type() {
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Bool)),
            CellEditorKind::Boolean
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Enum)),
            CellEditorKind::Enum
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Date)),
            CellEditorKind::Date
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Time)),
            CellEditorKind::Date
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::DateTime)),
            CellEditorKind::Date
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Number)),
            CellEditorKind::Number
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Json)),
            CellEditorKind::Json
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Bytes)),
            CellEditorKind::Binary
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Text)),
            CellEditorKind::Text
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Unknown)),
            CellEditorKind::Text
        );
        assert_eq!(
            resolve_cell_editor_kind(input(CellType::Uuid)),
            CellEditorKind::Text
        );
    }

    #[test]
    fn zoned_time_and_datetime_fall_back_to_text() {
        let mut zoned = input(CellType::Time);
        zoned.zoned = true;
        assert_eq!(resolve_cell_editor_kind(zoned), CellEditorKind::Text);

        let mut zoned = input(CellType::DateTime);
        zoned.zoned = true;
        assert_eq!(resolve_cell_editor_kind(zoned), CellEditorKind::Text);
    }

    #[test]
    fn long_or_multiline_text_uses_the_long_editor() {
        let mut long = input(CellType::Text);
        long.value_code_points = INLINE_TEXT_MAX_CODE_POINTS + 1;
        assert_eq!(resolve_cell_editor_kind(long), CellEditorKind::LongText);

        let mut multiline = input(CellType::Unknown);
        multiline.has_line_break = true;
        assert_eq!(
            resolve_cell_editor_kind(multiline),
            CellEditorKind::LongText
        );

        let mut boundary = input(CellType::Text);
        boundary.value_code_points = INLINE_TEXT_MAX_CODE_POINTS;
        assert_eq!(resolve_cell_editor_kind(boundary), CellEditorKind::Text);
    }

    #[test]
    fn uuid_stays_text_even_when_long() {
        let mut uuid = input(CellType::Uuid);
        uuid.value_code_points = INLINE_TEXT_MAX_CODE_POINTS + 10;
        uuid.has_line_break = true;
        assert_eq!(resolve_cell_editor_kind(uuid), CellEditorKind::Text);
    }
}
