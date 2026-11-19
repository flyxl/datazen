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
//! * **S1 — this module's landed half**: the pure core over the metadata
//!   snapshot and the token stream — the completion item shape, the
//!   dialect facade (quote / fold / compare / must-quote), the per-relation
//!   column family, the snapshot relation family, the all-columns fallback
//!   with its dedup rules, the SELECT-column extractor, and the
//!   columns-containing filter.
//! * **S2/S3 (not landed)**: the semantic model (`extension-points`
//!   `scopeModel.ts` — `SqlSemanticModel`, `SqlRelationBinding`,
//!   `resolveRelation`, `listVisibleRelations`), the editor schema tree
//!   (`SQLNamespace` / `buildEditorSchema`) and the CodeMirror
//!   `CompletionSource` wrapper (`createSchemaCompletionSource`) — they
//!   consume those not-yet-ported modules and land with them.
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

#[allow(unused_imports)] // consumers land with S2/S3 (the workspace view wiring)
pub(crate) use schema_completion::{
    RelatedTableBoost, SHORT_PREFIX_COLUMN_CAP, SHORT_PREFIX_LEN, SchemaCompletionItem,
    SchemaCompletionKind, SqlCompletionDialect, SqlToken, SqlTokenKind,
};
