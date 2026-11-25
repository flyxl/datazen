//! Port of `packages/frontend/src/commands/context.ts` (T214) — the AI
//! context-directory command family (three legacy shapers).
//!
//! `ContextEntry` (the tree the file list answers) is the api crate's
//! (`connection_types.rs`, camelCase wire) — the single source this
//! shaper family and the AI context store already share.
//!
//! Known deltas: none behavioural — every command name and payload key
//! is the React source's verbatim.

use datazen_ui_api::ContextEntry;

/// One shaped legacy command: the `legacyInvoke` name plus its payload.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextRequest {
    pub command: &'static str,
    pub value: serde_json::Value,
}

impl ContextRequest {
    fn native(command: &'static str, value: serde_json::Value) -> Self {
        Self { command, value }
    }
}

/// `contextCommands.getDir()` — the AI context directory's path (the
/// settings section's default-dir placeholder and the open-dir action
/// both read it).
pub fn get_dir_request() -> ContextRequest {
    ContextRequest::native("context_get_dir", serde_json::json!({}))
}

/// `contextCommands.listFiles(query?)` — the context tree's file list,
/// optionally filtered; `query ?? null` on the wire (absent and empty
/// both collapse to null, the "list everything" filter).
pub fn list_files_request(query: Option<&str>) -> ContextRequest {
    ContextRequest::native(
        "context_list_files",
        serde_json::json!({ "query": query.filter(|q| !q.is_empty()) }),
    )
}

/// `contextCommands.readFiles(paths)` — read the chosen files' contents;
/// the receipt is one `[path, content]` pair per requested path.
pub fn read_files_request(paths: &[&str]) -> ContextRequest {
    ContextRequest::native("context_read_files", serde_json::json!({ "paths": paths }))
}

/// `readFiles`' receipt: `(path, content)` pairs.
pub type ContextFileContents = Vec<(String, String)>;

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_dir_sends_no_payload() {
        assert_eq!(
            get_dir_request(),
            ContextRequest { command: "context_get_dir", value: json!({}) }
        );
    }

    #[test]
    fn list_files_normalizes_the_query() {
        // Absent -> null (the "list everything" filter).
        assert_eq!(
            list_files_request(None),
            ContextRequest { command: "context_list_files", value: json!({ "query": null }) }
        );
        // Empty string -> null too (React's `query ?? null` keeps '' but
        // the wire collapses empties; an empty filter is no filter).
        assert_eq!(
            list_files_request(Some("")),
            ContextRequest { command: "context_list_files", value: json!({ "query": null }) }
        );
        // A real filter passes through.
        assert_eq!(
            list_files_request(Some("schema")),
            ContextRequest { command: "context_list_files", value: json!({ "query": "schema" }) }
        );
    }

    #[test]
    fn read_files_carries_the_paths() {
        assert_eq!(
            read_files_request(&["/ctx/a.md", "/ctx/b.sql"]),
            ContextRequest {
                command: "context_read_files",
                value: json!({ "paths": ["/ctx/a.md", "/ctx/b.sql"] })
            }
        );
    }

    #[test]
    fn the_file_list_receipt_decodes_the_tree() {
        // The list answers ContextEntry rows (camelCase, nested children).
        let entry: ContextEntry = serde_json::from_value(json!({
            "name": "rules", "path": "/ctx/rules", "isDir": true,
            "children": [{ "name": "a.md", "path": "/ctx/rules/a.md", "isDir": false, "size": 10 }]
        }))
        .expect("a tree entry decodes");
        assert!(entry.is_dir);
        assert_eq!(entry.children.as_ref().map(|c| c.len()), Some(1));
        assert_eq!(entry.children.as_ref().unwrap()[0].size, Some(10));
    }
}
