//! T236 opens the Quick Look file with the three helpers the detail drawer
//! imports; T378 owns the rest of `QuickLookDialog.tsx`.
//!
//! React source: `packages/frontend/src/components/DataTable/QuickLookDialog.tsx`
//! (the file exists for the quick viewer of one cell's content — DBR-05 —
//! and exports the three type/value judgements the detail drawer reuses).
//! The drawer (`DetailPanel.tsx`) imports `isBlobType`, `isJsonValue` and
//! `prettyJson` from it, so the Rust module starts with exactly those:
//! the same slice-of-the-dependency-file boundary T233 used for the keyset
//! half (the current task ports what it consumes; the owning task grows the
//! file). The dialog itself — the `Dialog` panel, the binary/json viewers,
//! the save-to-file flow — lands with T378 and will register itself beside
//! these helpers.
//!
//! | React export | here |
//! |---|---|
//! | `BLOB_TYPE_KEYWORDS` | [`BLOB_TYPE_KEYWORDS`] |
//! | `isBlobType(type)` | [`is_blob_type`] |
//! | `isJsonValue(v)` | [`is_json_value`] |
//! | `prettyJson(v)` | [`pretty_json`] |
//!
//! Semantics are verbatim from the React source (see each function's comment);
//! `binaryValueBytes` is not consumed by the drawer and waits for T378.
//!
//! ## Deviations from the React source (deliberate)
//!
//! - `JSON.stringify(parsed, null, 2)` becomes
//!   `serde_json::to_string_pretty` (same two-space indent); object keys
//!   come out in dictionary order, not insertion order — the same
//!   documented deviation `to_edit_string` pins.
//! - `v.length` on a string is UTF-16 code units in React; no caller here
//!   measures strings, and the drawer's own byte-count line follows
//!   [`crate::support::to_edit_string::count_code_points`] (code points).

// T378's dialog will construct the viewer; until then the drawer's port is
// the only consumer of these helpers.
#![allow(dead_code)]

use serde_json::Value;

/// React's `BLOB_TYPE_KEYWORDS`: the substrings a column *type name* reads as
/// binary. A substring test on the type name, never a resolved cell type —
/// a query-result grid has no `cellTypeHints`, and a cell resolved as
/// `bytes` through the driver rules shows the viewer either way.
const BLOB_TYPE_KEYWORDS: [&str; 5] = ["blob", "binary", "bytea", "image", "raw"];

/// React's `isBlobType`: whether a column *type name* reads as binary (a
/// substring test against [`BLOB_TYPE_KEYWORDS`]; the caller lowercases the
/// name first, exactly as `DetailPanel.tsx` does).
pub(crate) fn is_blob_type(type_name: &str) -> bool {
    BLOB_TYPE_KEYWORDS.iter().any(|keyword| type_name.contains(keyword))
}

/// React's `isJsonValue`: whether a *value* reads as JSON — an object or
/// array is one outright; a string only when it parses as JSON and starts
/// with `{` or `[` (a bare `5` in a text column is a string, not JSON);
/// `null`/`undefined`/numbers/booleans are not.
pub(crate) fn is_json_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        // JavaScript's `typeof v === 'object'`: arrays and objects.
        Value::Object(_) | Value::Array(_) => true,
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
                return false;
            }
            serde_json::from_str::<Value>(trimmed).is_ok()
        }
        Value::Number(_) | Value::Bool(_) => false,
    }
}

/// React's `prettyJson`: the two-space pretty form of a JSON value, or
/// `None` when the value is not JSON (the string does not parse, or the
/// parsed result is not an object/array). A non-JSON value must return
/// `None` rather than the input so the caller's `pretty ?? formatCell(value)`
/// chain stays honest.
pub(crate) fn pretty_json(value: &Value) -> Option<String> {
    let parsed = match value {
        Value::Null => return None,
        Value::Object(_) | Value::Array(_) => value.clone(),
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
                return None;
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(parsed) => parsed,
                Err(_) => return None,
            }
        }
        // `typeof parsed !== 'object'` in the React source: numbers,
        // strings and booleans never pretty-print.
        Value::Number(_) | Value::Bool(_) => return None,
    };
    serde_json::to_string_pretty(&parsed).ok()
}
