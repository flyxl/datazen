//! Cell colour projection, ported from `packages/frontend/src/lib/dataTypeColors.ts`.
//!
//! The React version reads CSS custom properties (`--dt-*`). This port maps
//! the same seven families onto the kit's semantic tokens instead, so a cell
//! keeps the same "look" without introducing a second colour source.
#![allow(dead_code)] // consumers: T004 CellRenderer

use rune::Color;
use serde_json::Value;

use crate::support::cell_types::{cell_type_to_data_type_family, CellType, CellTypeHints};
use crate::ui::kit::kit;

/// The render family of a value. `Null` belongs to the value, not the type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DataTypeFamily {
    Null,
    Bool,
    Number,
    Datetime,
    Json,
    Binary,
    Text,
}

impl DataTypeFamily {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Number => "number",
            Self::Datetime => "datetime",
            Self::Json => "json",
            Self::Binary => "binary",
            Self::Text => "text",
        }
    }
}

/// The colour of one family.
///
/// Mapping: `null` → faint text, `bool` → info, `number` → warning,
/// `datetime` → success, `json` → accent, `text` → base text, `binary` →
/// danger.
pub(crate) fn family_color(family: DataTypeFamily, dark: bool) -> Color {
    let k = kit(dark);
    match family {
        DataTypeFamily::Null => k.c.text_faint,
        DataTypeFamily::Bool => k.c.info.fg,
        DataTypeFamily::Number => k.c.warning.fg,
        DataTypeFamily::Datetime => k.c.success.fg,
        DataTypeFamily::Json => k.c.text_accent,
        DataTypeFamily::Binary => k.c.danger.fg,
        DataTypeFamily::Text => k.c.text_base,
    }
}

/// The colour of a cell value: a NULL value always paints as the null family,
/// whatever the column type says.
pub(crate) fn cell_value_color(cell_type: CellType, value: &Value, dark: bool) -> Color {
    if value.is_null() {
        return family_color(DataTypeFamily::Null, dark);
    }
    family_color(cell_type_to_data_type_family(cell_type), dark)
}

/// The colour of a column type. An absent or blank type resolves to
/// `CellType::Unknown`, and React normalizes `unknown` to the text family in
/// `resolveCellType`/`cellTypeToDataTypeFamily`; the null family belongs to a
/// NULL *value*, never to a missing type.
pub(crate) fn data_type_color(
    data_type: Option<&str>,
    hints: Option<&CellTypeHints>,
    dark: bool,
) -> Color {
    let cell_type = crate::support::cell_types::resolve_cell_type(data_type, hints);
    family_color(cell_type_to_data_type_family(cell_type), dark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_family_has_a_distinct_token() {
        let families = [
            DataTypeFamily::Null,
            DataTypeFamily::Bool,
            DataTypeFamily::Number,
            DataTypeFamily::Datetime,
            DataTypeFamily::Json,
            DataTypeFamily::Binary,
            DataTypeFamily::Text,
        ];
        let mut seen = Vec::new();
        for family in families {
            let color = family_color(family, true);
            assert!(
                !seen.contains(&color),
                "family {} reuses another family's token",
                family.as_str()
            );
            seen.push(color);
        }
    }

    #[test]
    fn blank_column_types_paint_as_text_not_null() {
        let text = family_color(DataTypeFamily::Text, true);
        assert_eq!(data_type_color(None, None, true), text);
        assert_eq!(data_type_color(Some(""), None, true), text);
        assert_eq!(data_type_color(Some("   "), None, true), text);
        assert_ne!(
            data_type_color(None, None, true),
            family_color(DataTypeFamily::Null, true)
        );
    }

    #[test]
    fn null_values_paint_as_the_null_family() {
        let null_color = family_color(DataTypeFamily::Null, true);
        assert_eq!(
            cell_value_color(CellType::Text, &Value::Null, true),
            null_color
        );
        assert_eq!(
            cell_value_color(CellType::Number, &Value::Null, true),
            null_color
        );
    }

    #[test]
    fn values_borrow_their_column_family() {
        assert_eq!(
            cell_value_color(CellType::Number, &json!(42), true),
            family_color(DataTypeFamily::Number, true)
        );
        assert_eq!(
            cell_value_color(CellType::Json, &json!({"a": 1}), true),
            family_color(DataTypeFamily::Json, true)
        );
        assert_eq!(
            cell_value_color(CellType::Bytes, &json!("AA=="), true),
            family_color(DataTypeFamily::Binary, true)
        );
    }

    #[test]
    fn blank_types_paint_as_text_after_the_audit_fix() {
        assert_eq!(
            data_type_color(None, None, true),
            family_color(DataTypeFamily::Text, true)
        );
        assert_eq!(
            data_type_color(Some("  "), None, true),
            family_color(DataTypeFamily::Text, true)
        );
        assert_eq!(
            data_type_color(Some("timestamptz"), None, true),
            family_color(DataTypeFamily::Datetime, true)
        );
    }

    #[test]
    fn dark_and_light_tokens_are_different() {
        assert_ne!(
            family_color(DataTypeFamily::Text, true),
            family_color(DataTypeFamily::Text, false)
        );
    }
}
