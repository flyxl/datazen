//! Independent audit for T124: the three `lib/structureEditor` helper ports.
//!
//! The coder's suite pins the headline branches. These tests fill the gaps in
//! the `resolveCreateTableSchemaFromMeta` decision tree that are easy to get
//! wrong in a `??` → `Option` translation: the no-meta non-PG fallback to
//! `currentDatabase` when `contextSchema` is absent, empty-string survival on
//! that path, and the branch order between the PG namespace check and the
//! `databaseFieldType === 'path'` unqualified check.

use crate::relation_metadata::TableSchema;
use crate::structure_editor::resolve_create_table_schema::{
    resolve_create_table_schema_from_meta, CreateTableSchemaMeta, ResolveCreateTableSchemaParams,
};

/// React: `if (!meta) return params.contextSchema ?? params.currentDatabase ?? null;`
/// With no contextSchema the logical database name must be used. The coder only
/// pinned the contextSchema-present side of this `??`, plus the both-absent
/// case, so this fallback was unguarded.
#[test]
fn no_meta_non_pg_falls_back_to_current_database_without_context() {
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "mysql",
            &ResolveCreateTableSchemaParams::new(Some("app"), None)
        )
        .as_deref(),
        Some("app"),
        "no meta + non-PG + no contextSchema uses currentDatabase"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "some-unknown-dialect",
            &ResolveCreateTableSchemaParams::new(Some("db"), None)
        )
        .as_deref(),
        Some("db"),
        "the fallback is dialect-agnostic, not special-cased to mysql"
    );
}

/// JS `'' ?? x` is `''`. On the no-meta non-PG path an empty currentDatabase
/// with no contextSchema must survive as `Some("")`, not collapse to `None`.
#[test]
fn no_meta_non_pg_preserves_an_empty_current_database() {
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "mysql",
            &ResolveCreateTableSchemaParams::new(Some(""), None)
        )
        .as_deref(),
        Some(""),
        "an empty currentDatabase is a value, not an absent qualifier"
    );
}

/// React checks `namespaceEnsure === 'postgresql'` before `databaseFieldType === 'path'`.
/// A meta carrying both must take the PG-schema branch (contextSchema or 'public'),
/// never the unqualified `null` path branch.
#[test]
fn pg_namespace_branch_wins_over_a_path_database_field() {
    let meta = CreateTableSchemaMeta::new(Some("postgresql"), Some("path"));
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&meta),
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), Some("sales"))
        )
        .as_deref(),
        Some("sales"),
        "PG namespace wins over path: contextSchema is the qualifier"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&meta),
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), None)
        )
        .as_deref(),
        Some("public"),
        "PG namespace wins over path and defaults to public"
    );
}

/// A schema with no columns and no indexes must produce two empty drafts, not
/// panic or invent ids. This guards the `.first().is_some_and(...)` unique
/// probe against an empty `columns` vector.
#[test]
fn schema_to_draft_accepts_an_empty_schema() {
    use crate::structure_editor::schema_to_draft::schema_to_draft;

    let draft = schema_to_draft(&TableSchema::default());
    assert!(draft.columns.is_empty(), "no columns in, no drafts out");
    assert!(draft.indexes.is_empty(), "no indexes in, no drafts out");
}
