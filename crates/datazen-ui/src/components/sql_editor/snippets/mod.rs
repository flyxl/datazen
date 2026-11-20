//! Port of `packages/frontend/src/components/sql-editor/snippets/` —
//! the SQL snippet template system.
//!
//! One module per React file, at the same path:
//! `snippets/types.ts` → [`types`] (T370). The builtin library
//! (`builtinSnippets.ts` + `index.ts`, T347) and the completion source
//! (`snippetCompletionSource.ts`, T359) land in their own tasks.
//!
//! No production consumer yet; the editor assembly wires the
//! completion source once it lands. Reusable API items carry
//! per-item `#[allow(dead_code)]` until then, like the rest of
//! `components/sql_editor`.

/// `snippets/types.ts` (T370): the `SqlSnippetItem` contract.
pub(crate) mod types;

#[allow(unused_imports)] // consumers land with T347/T359
pub(crate) use types::SqlSnippetItem;
