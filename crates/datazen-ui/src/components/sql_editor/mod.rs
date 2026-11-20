//! SQL editor assembly component (the React `SqlEditor.tsx` port).
//!
//! One module per React file, at the same path:
//! `components/sql-editor/SqlEditor.tsx` → `components/sql_editor/sql_editor.rs`.
//! The pure helpers it ports from `lib/sqlPathPrefix.ts` and
//! `lib/sqlEditorContextMenu.ts` live in `lib_helpers` until those ledger
//! tasks land and absorb them. The `sql-editor/metadata/` directory is
//! ported as [`metadata`] (T242), and the editor schema tree shape
//! (`lib/sqlNamespace.ts`) is carried as [`sql_namespace`] until T617
//! absorbs it.
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

pub(crate) mod lib_helpers;
#[allow(unused_imports)] // re-exports keep the module boundary flat for callers
pub(crate) use lib_helpers::{parse_qualified_path_parents, toggle_sql_line_comments};

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
