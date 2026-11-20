//! Port of `packages/frontend/src/components/sql-editor/snippets/` —
//! the SQL snippet template system.
//!
//! One module per React file, at the same path:
//! `snippets/types.ts` → [`types`] (T370), `snippets/builtinSnippets.ts`
//! → [`builtin`] (T347), `snippets/snippetCompletionSource.ts` →
//! [`snippet_completion_source`] (T359). The React `index.ts` barrel's
//! re-exports are mirrored below as each module lands.
//!
//! No production consumer yet; the editor assembly wires the
//! completion source into its source list. Reusable API items carry
//! per-item `#[allow(dead_code)]` until then, like the rest of
//! `components/sql_editor`.

/// `snippets/types.ts` (T370): the `SqlSnippetItem` contract.
pub(crate) mod types;

/// `snippets/builtinSnippets.ts` (T347): the built-in library.
pub(crate) mod builtin;

/// `lib/sqlCompletionContext.ts` + extension-points
/// `completionContext.ts` helpers carried for the snippet source
/// (T359) until their own ledger tasks land and absorb them.
pub(crate) mod completion_context_helpers;

/// `snippets/snippetCompletionSource.ts` (T359): the completion
/// source (context boosts, the dot hard-exclusion, the build
/// projection).
pub(crate) mod snippet_completion_source;

// The `index.ts` barrel: `export type { SqlSnippetItem }`,
// `export { BUILTIN_SQL_SNIPPETS }`, `export { buildSnippetCompletions,
// createSnippetCompletionSource, type SnippetCompletionOptions }`.
#[allow(unused_imports)] // consumers land with the editor assembly wiring
pub(crate) use builtin::builtin_sql_snippets;
#[allow(unused_imports)] // consumers land with the editor assembly wiring
pub(crate) use snippet_completion_source::{
    BOOST_IDENTIFIER_CONTEXT, BOOST_STATEMENT_CONTEXT, SnippetCompletionOptions,
    SnippetSourceResult, build_snippet_completions, snippet_completion_source,
    snippet_completion_valid_for,
};
#[allow(unused_imports)] // consumers land with the editor assembly wiring
pub(crate) use types::SqlSnippetItem;
