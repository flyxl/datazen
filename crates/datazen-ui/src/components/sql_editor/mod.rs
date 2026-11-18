//! SQL editor assembly component (the React `SqlEditor.tsx` port).
//!
//! One module per React file, at the same path:
//! `components/sql-editor/SqlEditor.tsx` → `components/sql_editor/sql_editor.rs`.
//! The pure helpers it ports from `lib/sqlPathPrefix.ts` and
//! `lib/sqlEditorContextMenu.ts` live in `lib_helpers` until those ledger
//! tasks land and absorb them.
//!
//! No production consumer yet; the workspace view track will wire this
//! component (today the workspace renders the engine demo panel directly).
//! Reusable API items are kept with per-item `#[allow(dead_code)]` until
//! wired, like `components/history`.

#[allow(unused_imports)] // consumers will be added by the workspace view wiring
pub(crate) use sql_editor::{
    on_ime, on_key, CompletionQuotePolicy, EditorEvent, ExecutionMark, ExecutionStatus,
    SqlEditorComponent, SqlEditorProps, SqlEditorState,
};

pub(crate) mod lib_helpers;
#[allow(unused_imports)] // re-exports keep the module boundary flat for callers
pub(crate) use lib_helpers::{parse_qualified_path_parents, toggle_sql_line_comments};

pub(crate) mod sql_editor;
