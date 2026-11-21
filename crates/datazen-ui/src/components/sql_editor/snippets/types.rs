//! Port of `packages/frontend/src/components/sql-editor/snippets/types.ts`
//! (T370).
//!
//! SQL snippet template contracts. Templates use CodeMirror's native
//! snippet syntax: `${1:defaultText}` marks an ordered tabstop whose
//! default text is selected on arrival. The type is the wire the
//! builtin library (`builtinSnippets.ts`, T347) fills, the completion
//! source (`snippetCompletionSource.ts`, T359) matches, and the
//! settings card imports/exports — one shape, no second mirror.
//!
//! The four fields are the React interface's verbatim; `Object.freeze`
//! on the library is a JS immutability gesture with no Rust
//! counterpart (a `Vec<SqlSnippetItem>` shared by reference is already
//! immutable to its readers).
//!
//! Known deltas: none.

use serde::{Deserialize, Serialize};

/// `SqlSnippetItem`: one SQL snippet template.
// consumers: the builtin library and the completion source (T347/T359,
// this crate); delete this allow when they land.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SqlSnippetItem {
    /// Stable identity, also used as the import/export key.
    pub(crate) id: String,
    /// Typed prefix that expands the template (e.g. `sel*`).
    pub(crate) prefix: String,
    /// i18n key for the human-readable description.
    pub(crate) description_key: String,
    /// CodeMirror snippet template with `${n:default}` tabstops.
    pub(crate) template: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_round_trips_the_react_field_names() {
        let item = SqlSnippetItem {
            id: "select-all".to_owned(),
            prefix: "sel*".to_owned(),
            description_key: "query.editor.snippet.selectAll".to_owned(),
            template: "SELECT *\nFROM ${1:table_name};${2}".to_owned(),
        };
        let wire = serde_json::to_value(&item).expect("serializes");
        assert_eq!(
            wire,
            serde_json::json!({
                "id": "select-all",
                "prefix": "sel*",
                "descriptionKey": "query.editor.snippet.selectAll",
                "template": "SELECT *\nFROM ${1:table_name};${2}",
            })
        );
        let back: SqlSnippetItem = serde_json::from_value(wire).expect("decodes");
        assert_eq!(back, item);
    }

    #[test]
    fn the_wire_shape_is_the_import_export_contract() {
        // The settings card's import/export key is the id; a foreign
        // JSON that misses any of the four fields must not decode.
        let full = serde_json::json!({
            "id": "x", "prefix": "p", "descriptionKey": "k", "template": "t"
        });
        assert!(serde_json::from_value::<SqlSnippetItem>(full).is_ok());
        for missing in ["id", "prefix", "descriptionKey", "template"] {
            let mut holed = serde_json::json!({
                "id": "x", "prefix": "p", "descriptionKey": "k", "template": "t"
            });
            holed.as_object_mut().unwrap().remove(missing);
            assert!(
                serde_json::from_value::<SqlSnippetItem>(holed).is_err(),
                "a payload without `{missing}` must not decode"
            );
        }
    }
}
