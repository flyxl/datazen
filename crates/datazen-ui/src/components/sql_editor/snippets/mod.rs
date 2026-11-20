//! Port of `packages/frontend/src/components/sql-editor/snippets/` —
//! the SQL snippet template system.
//!
//! One module per React file, at the same path:
//! `snippets/types.ts` → [`types`] (T370), `snippets/builtinSnippets.ts`
//! → [`builtin`] (T347). The completion source
//! (`snippetCompletionSource.ts`, T359) lands in its own task; the
//! React `index.ts` barrel's re-exports grow here as each lands.
//!
//! No production consumer yet; the editor assembly wires the
//! completion source once it lands. Reusable API items carry
//! per-item `#[allow(dead_code)]` until then, like the rest of
//! `components/sql_editor`.

/// `snippets/types.ts` (T370): the `SqlSnippetItem` contract.
pub(crate) mod types;

/// `snippets/builtinSnippets.ts` (T347): the built-in library.
pub(crate) mod builtin;

// The `index.ts` barrel (T347): `export type { SqlSnippetItem }` +
// `export { BUILTIN_SQL_SNIPPETS }`.
#[allow(unused_imports)] // consumers land with T359
pub(crate) use builtin::builtin_sql_snippets;
#[allow(unused_imports)] // consumers land with T359
pub(crate) use types::SqlSnippetItem;
