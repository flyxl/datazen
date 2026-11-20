//! SQL completion sources (the React `components/sql-editor/completion/`
//! directory port, T062).
//!
//! One module per React file, at the same path:
//! `completion/schemaCompletion.ts` → `completion/schema_completion.rs`.
//!
//! The React module is alias-aware schema completion: given a semantic model
//! and a metadata snapshot it produces CodeMirror completions filtered by
//! cursor intent, reading synchronously (no IPC). The port lands in slices
//! (the ledger task is marked `split`):
//!
//! * **S1 — landed**: the pure core over the metadata snapshot and the
//!   token stream — the completion item shape, the dialect facade (quote /
//!   fold / compare / must-quote), the per-relation column family, the
//!   snapshot relation family, the all-columns fallback with its dedup
//!   rules, the SELECT-column extractor, and the columns-containing
//!   filter.
//! * **S2/S3 — landed**: the semantic-model resolver half
//!   ([`semantic_model`]: scopes, bindings, cursor intent,
//!   `resolveRelation`, `listVisibleRelations` — shapes and resolvers
//!   only, the builder half stays with extension-points), the editor
//!   schema tree consumption (schema-dot resolution, the editor-schema
//!   relation and all-columns fallbacks) and the four public entry points
//!   (`resolveAliasDotCompletions`, `resolveSchemaDotCompletions`,
//!   `produceSchemaCompletions`, `createSchemaCompletionSource`).
//!
//! The dependency slices carried here (documented per item, the
//! `lib_helpers` precedent):
//!
//! * `SqlToken`/`SqlTokenKind` are the `extension-points` scanner's shapes
//!   (`sql-editor/tokens.ts`), carried because the SELECT-column extractor
//!   reads token streams; the scanner itself is its own task.
//! * The dialect facade wraps a resolved `SqlDialectProfile` and carries
//!   the reserved-keyword gate from `lib/sqlDialects/identifierQuoting.ts`
//!   (T084); when that task lands it absorbs the list and the
//!   `needs_quoting` rule verbatim.

pub(crate) mod schema_completion;
pub(crate) mod semantic_model;

#[allow(unused_imports)] // consumers land with the workspace view wiring
pub(crate) use schema_completion::{
    RelatedTableBoost, SchemaCompletionItem, SchemaCompletionKind, SchemaCompletionOptions,
    SchemaCompletionResult, SHORT_PREFIX_COLUMN_CAP, SHORT_PREFIX_LEN, SqlCompletionDialect,
    SqlToken, SqlTokenKind, all_columns_from_editor_schema, column_completions_from_relation,
    completion_valid_for, create_schema_completion_source, produce_schema_completions,
    relation_completions_from_snapshot, resolve_alias_dot_completions,
    resolve_schema_dot_completions,
};
#[allow(unused_imports)] // consumers land with the workspace view wiring
pub(crate) use semantic_model::{
    find_scope_at_cursor, get_scope_chain, list_visible_relations, resolve_relation,
    RelationResolveResult, SqlCteBinding, SqlCursorIntent, SqlCursorIntentKind, SqlProjectionAlias,
    SqlRelationBinding, SqlScope, SqlScopeKind, SqlSemanticModel, SqlTextRange,
};
