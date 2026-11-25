//! Port of `packages/frontend/src/commands/namedViews.ts` (T344).
//!
//! The command layer as the saved-views menu drives it. The React
//! module holds no logic of its own: it is the adapter that hands the
//! menu's two prop bundles their real implementations —
//!
//! - `namedViewOperations` → the three database domain-client calls
//!   (`listNamedViews({ scope })`, `saveNamedView({ view })`,
//!   `deleteNamedView({ id })`), the [`DatabaseMethod`] rail T258
//!   landed;
//! - `namedViewFiles` → the two file dialog calls with the JSON
//!   filter and the `json` extension, the [`FileCommandRequest`]
//!   shapers T298 landed (`saveTextWithDialog(content, fileName,
//!   'JSON', ['json'])` for the export, `openTextWithDialog('JSON',
//!   ['json'])` for the import).
//!
//! The menu's own prop **interfaces** (`NamedViewOperations`,
//! `NamedViewFileOperations`) are the menu component's ledger
//! (`components/filter/NamedViewMenu.tsx`, T192 — another segment);
//! when that port lands it consumes these shapers directly, so no
//! interface mirror is carried here.
//!
//! The menu deliberately takes its calls as props (its own doc
//! records why: the component can be exercised without an IPC and
//! without a native dialog); the adapter living next to — not inside
//! — `database.rs` and `file.rs` is the same seam in Rust: a caller
//! that swaps either rail does not have to know about this module.
//!
//! Known deltas: none behavioural — every method name, payload key
//! and filter literal is the React source's verbatim. `scope:
//! undefined` is an absent key (JS drops `undefined`; the
//! `listNamedViews` request type itself marks `scope?`).

use serde_json::{json, Value};

use crate::commands::database::DatabaseMethod;
use crate::commands::file::{FileCommandRequest, save_text_with_dialog_request};
use crate::commands::file::open_text_with_dialog_request;

// ---------------------------------------------------------------------------
// `namedViewOperations` — the three host commands the menu drives
// ---------------------------------------------------------------------------

/// `namedViewOperations.list(scope)`: the views of a scope; omitting
/// the scope lists every view (the request type's `scope?`).
pub fn list_named_views_request(scope: Option<&Value>) -> (DatabaseMethod, Value) {
    match scope {
        Some(scope) => (DatabaseMethod::ListNamedViews, json!({ "scope": scope })),
        None => (DatabaseMethod::ListNamedViews, json!({})),
    }
}

/// `namedViewOperations.save(view)`: save a view; the answer carries
/// the minted id and the timestamps.
pub fn save_named_view_request(view: &Value) -> (DatabaseMethod, Value) {
    (DatabaseMethod::SaveNamedView, json!({ "view": view }))
}

/// `namedViewOperations.remove(id)`: delete a view; the answer
/// carries the remaining ones.
pub fn delete_named_view_request(id: &str) -> (DatabaseMethod, Value) {
    (DatabaseMethod::DeleteNamedView, json!({ "id": id }))
}

// ---------------------------------------------------------------------------
// `namedViewFiles` — the two file commands the export and the import drive
// ---------------------------------------------------------------------------

/// `namedViewFiles.write(fileName, content)`: write the export file
/// via the native save dialog; the receipt is `false` when the user
/// cancelled. The React wrapper flips the menu's argument order into
/// the file command's (`contents` first) and pins the JSON filter.
pub fn named_view_export_request(file_name: &str, content: &str) -> FileCommandRequest {
    save_text_with_dialog_request(content, file_name, "JSON", &["json"])
}

/// `namedViewFiles.read()`: read an import file via the native open
/// dialog; the receipt is [`crate::commands::file::OpenedTextFile`]
/// or `null` when the user cancelled.
pub fn named_view_import_request() -> FileCommandRequest {
    open_text_with_dialog_request("JSON", &["json"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::file::save_text_with_dialog_request;

    /// The scope the React suite's own fixture uses (namedViews.test's
    /// `SCOPE`): the persistent connection id, the database, the
    /// schema and the table — never a `dbSessionId`.
    fn scope() -> Value {
        json!({
            "connectionId": "conn-1",
            "database": "app",
            "schema": null,
            "table": "users",
        })
    }

    #[test]
    fn operations_ride_the_database_rail_with_verbatim_wire_names() {
        let (list, _) = list_named_views_request(None);
        assert_eq!(list, DatabaseMethod::ListNamedViews);
        let (save, _) = save_named_view_request(&json!({ "id": "" }));
        assert_eq!(save, DatabaseMethod::SaveNamedView);
        let (remove, _) = delete_named_view_request("nv-1");
        assert_eq!(remove, DatabaseMethod::DeleteNamedView);
        // The rail's own wire spellings (T258's audit pins the same).
        assert_eq!(DatabaseMethod::ListNamedViews.wire_name(), "listNamedViews");
        assert_eq!(DatabaseMethod::SaveNamedView.wire_name(), "saveNamedView");
        assert_eq!(DatabaseMethod::DeleteNamedView.wire_name(), "deleteNamedView");
    }

    #[test]
    fn list_shapes_the_scope_bag_and_drops_the_absent_key() {
        // A scope rides as `{ scope }`, field for field.
        let (method, payload) = list_named_views_request(Some(&scope()));
        assert_eq!(method, DatabaseMethod::ListNamedViews);
        assert_eq!(payload, json!({ "scope": scope() }));
        // `scope: undefined` — the React `list(scope?)` default — is an
        // absent key, not a null (JS drops `undefined`).
        let (_, payload) = list_named_views_request(None);
        assert_eq!(payload, json!({}));
        assert!(payload.get("scope").is_none());
    }

    #[test]
    fn save_and_remove_shape_their_payload_keys() {
        let view = json!({
            "id": "",
            "name": "Active users",
            "scope": scope(),
            "filters": [],
            "filterLogic": "and",
            "sorts": [],
            "visibleColumns": null,
            "columnOrder": null,
            "isDefault": false,
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": null,
        });
        let (_, payload) = save_named_view_request(&view);
        assert_eq!(payload, json!({ "view": view }));
        let (_, payload) = delete_named_view_request("nv-1");
        assert_eq!(payload, json!({ "id": "nv-1" }));
    }

    #[test]
    fn export_flips_the_argument_order_and_pins_the_json_filter() {
        // The menu hands (fileName, content); the file command takes
        // (contents, defaultFileName) — the adapter does the flip.
        let shaped = named_view_export_request("users-view.json", "{\"name\":\"Active users\"}");
        assert_eq!(
            shaped,
            save_text_with_dialog_request(
                "{\"name\":\"Active users\"}",
                "users-view.json",
                "JSON",
                &["json"],
            )
        );
        // The whole envelope: the dialog command plus the literal bag.
        assert_eq!(shaped.command, "save_text_with_dialog");
        assert_eq!(
            shaped.value,
            json!({
                "contents": "{\"name\":\"Active users\"}",
                "defaultFileName": "users-view.json",
                "filterName": "JSON",
                "extensions": ["json"],
            })
        );
    }

    #[test]
    fn import_pins_the_json_filter_and_extensions() {
        let shaped = named_view_import_request();
        assert_eq!(shaped.command, "open_text_with_dialog");
        assert_eq!(shaped.value, json!({ "filterName": "JSON", "extensions": ["json"] }));
    }
}
