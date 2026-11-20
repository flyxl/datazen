//! SQL editor assembly component (the React `SqlEditor.tsx` port).
//!
//! One module per React file, at the same path:
//! `components/sql-editor/SqlEditor.tsx` → `components/sql_editor/sql_editor.rs`.
//! The pure helpers it ports from `lib/sqlPathPrefix.ts` live in
//! `lib_helpers` until that ledger task lands and absorbs them;
//! `lib/sqlEditorContextMenu.ts` ported as
//! [`sql_editor_context_menu`] (T155) and took `toggleSqlLineComments`
//! home. The `sql-editor/metadata/` directory is ported as [`metadata`]
//! (T242), and the editor schema tree shape (`lib/sqlNamespace.ts`) is
//! carried as [`sql_namespace`] until T617 absorbs it.
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

/// `contracts.ts` (T105): the S6-D contract vocabulary — props enums,
/// the drop payload, the navigation targets, the CM dialect mapping.
pub(crate) mod contracts;
#[allow(unused_imports)] // the handle/extension tasks build on these
pub(crate) use contracts::{
    CmSqlDialect, DdlNavTarget, DroppedTablePayload, DroppedTableRef, SqlSchema,
    StructureNavTarget, TableNavTarget, resolve_cm_dialect,
};

/// `editorExtensions.ts` (T139, split): the assembly's decision logic —
/// theme config, the document-version rule, the execute dispatch, the
/// large-document windowing, the path dedup, the contextmenu SQL, the
/// completion anchors and admission classes.
pub(crate) mod editor_extensions;

pub(crate) mod lib_helpers;
#[allow(unused_imports)] // re-exports keep the module boundary flat for callers
pub(crate) use lib_helpers::parse_qualified_path_parents;

/// `lib/sqlEditorContextMenu.ts` (T155): the SQL editor's context-menu
/// builder (OS edit block, run/format/comment/favorite items) plus the
/// driver-sdk menu item contract and the carried `Platform` /
/// `formatModKey` seams.
pub(crate) mod sql_editor_context_menu;
#[allow(unused_imports)] // `toggle_sql_line_comments` keeps its pre-T155 path stable
pub(crate) use sql_editor_context_menu::toggle_sql_line_comments;

/// `components/sql-editor/extensions/{executionState,types}.ts` (T170):
/// the execution-state field machine (start/cancel/finish, the running-
/// target match), the frame-degrade rules and the statement/execution
/// target contracts the frame and gutter extensions share.
pub(crate) mod extensions;

/// `components/sql-editor/fold/foldCompartment.ts` (T196, split): the
/// `fold` Pro compartment's host gate (the `codeFolding` key on the
/// shared settings bag). The registry itself is T335.
pub(crate) mod fold;

/// `components/sql-editor/paste/createPasteExtensions.ts` (T263, split):
/// the paste/drop/multiple-selection compartment's host factory (the
/// always-mounted multi-selection base plus the enhanced EP's half).
/// multipleSelections.ts is T320, the registry T335.
pub(crate) mod create_paste_extensions;

/// `components/sql-editor/paste/multiCursorEscape.ts` (T283, split): the
/// dedicated Escape leaving the multi-cursor state (S1's collapse to a
/// bare main-head cursor, S0's refusal).
pub(crate) mod multi_cursor_escape;
/// T302: the multi-cursor POINTER predicate (the platform chord
/// CodeMirror's `clickAddsSelectionRange` override had dropped).
pub(crate) mod multi_cursor_pointer;

/// `components/sql-editor/format/formatEditorDocument.ts` (T220, split):
/// the selection-aware beautifier command (the formatter stays
/// injected — T615's sqlFormat).
pub(crate) mod format_editor_document;

pub(crate) mod completion;
#[allow(unused_imports)] // consumers land with the S2/S3 slices
pub(crate) use completion::{
    RelatedTableBoost, SHORT_PREFIX_COLUMN_CAP, SHORT_PREFIX_LEN, SchemaCompletionItem,
    SchemaCompletionKind, SqlCompletionDialect, SqlToken, SqlTokenKind,
};

pub(crate) mod sql_editor;

/// `lib/sqlNamespace.ts`'s tree shape, carried (T617 absorbs).
pub(crate) mod sql_namespace;
#[allow(unused_imports)] // consumers: metadata/find_relation + completion S2/S3
pub(crate) use sql_namespace::{is_leaf, SqlNamespace};

/// `components/sql-editor/metadata/` (T242): findRelation + re-export shims.
pub(crate) mod metadata;
#[allow(unused_imports)] // consumers: the completion entry points (T062 S2/S3)
pub(crate) use metadata::{extract_columns_from_schema, find_relation_metadata};
