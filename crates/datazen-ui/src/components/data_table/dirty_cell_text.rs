//! How a dirty cell reads in words, ported from React
//! `packages/frontend/src/components/DataTable/dirtyCellText.ts` (tasklist T405).
//!
//! Two values need a word instead of the empty string, because an empty
//! tooltip is indistinguishable from "no value":
//!
//! - SQL `NULL` (the database's NULL reaches the UI as `null`);
//! - the empty string — which the inline editor cannot produce (it commits
//!   `null` instead), but a paste can.
//!
//! A JSON column is serialised rather than stringified with `String()`, so
//! the tooltip shows the value and not `[object Object]`.
//!
//! Cell values mirror as `serde_json::Value` (the convention
//! `cell_renderer.rs` set: `Value::Null` covers React's `null` and
//! `undefined`), so "the database's NULL" arrives as `Value::Null` and the
//! React `value === null` guard becomes the `Value::Null` arm. The React
//! `typeof value === 'object'` arm covers objects AND arrays, so both
//! serialise here; `String(value)` for everything else is the crate's
//! [`js_string`] — the same `String()` the cell renderer already mirrors,
//! single-sourced there.
//!
//! The standalone translation function is used on purpose (the React
//! file's own rationale): the label is built per cell, per render, and
//! handing a translate function down to every row would make each row
//! subscribe to the locale.

use serde_json::Value;

use crate::components::data_table::cell_renderer::js_string;
use crate::i18n::t;

/// `formatDirtyValue`: the word a value reads as inside a dirty-cell tooltip.
pub fn format_dirty_value(value: &Value) -> String {
    match value {
        Value::Null => t("tableData.dirtyValueNull").to_owned(),
        Value::String(text) if text.is_empty() => t("tableData.dirtyValueEmpty").to_owned(),
        Value::Object(_) | Value::Array(_) => value.to_string(),
        _ => js_string(value),
    }
}

/// `formatDirtyExpression` (DBR-05): the word a staged write expression
/// reads as in a dirty tooltip — the intent in the user's words, not the
/// placeholder value the wire invariant keeps in `currentValues`.
pub fn format_dirty_expression() -> String {
    t("tableData.dirtyValueExpression").to_owned()
}

/// The `expression === 'currentTimestamp'` marker's spelling, as the React
/// call sites compare it.
pub const CURRENT_TIMESTAMP_EXPRESSION: &str = "currentTimestamp";

/// `dirtyCellTooltip`: the tooltip of a dirty cell — the original value and
/// the value it becomes. `expression` rewrites the "to" side only.
pub fn dirty_cell_tooltip(
    original: &Value,
    next: &Value,
    expression: Option<&str>,
) -> String {
    let to = match expression {
        Some(CURRENT_TIMESTAMP_EXPRESSION) => format_dirty_expression(),
        _ => format_dirty_value(next),
    };
    format!("{} \u{2192} {}", format_dirty_value(original), to)
}

/// `dirtyCellAriaLabel`: the screen-reader label of a dirty cell — the
/// tooltip in words. The `{original}`/`{next}` placeholders the zh-CN pack
/// carries are formatted here, the caller-facing convention `i18n.rs`
/// documents.
pub fn dirty_cell_aria_label(
    original: &Value,
    next: &Value,
    expression: Option<&str>,
) -> String {
    let to = match expression {
        Some(CURRENT_TIMESTAMP_EXPRESSION) => format_dirty_expression(),
        _ => format_dirty_value(next),
    };
    t("tableData.dirtyCellAria")
        .replace("{original}", &format_dirty_value(original))
        .replace("{next}", &to)
}
