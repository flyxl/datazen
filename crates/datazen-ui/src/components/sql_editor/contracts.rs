//! Port of `packages/frontend/src/components/sql-editor/contracts.ts`.
//!
//! The S6-D contract module: the props/handle vocabulary the SqlEditor
//! assembly, its extensions and the workspace wiring share. The types
//! first landed inlined in `sql_editor.rs` (T011's seam, its doc note
//! promised this absorption); this module is their single source now.
//!
//! ## What maps where
//!
//! - `SqlSchema` is React's alias of `SqlNamespace` — same alias here.
//! - `SqlEditorHandle` (the `useImperativeHandle` surface) is
//!   [`SqlEditorState`][super::sql_editor::SqlEditorState]'s methods,
//!   1:1 (T011's mapping table); no trait object — the handle and the
//!   state are one thing in immediate mode.
//! - The callback props are [`EditorEvent`][super::sql_editor::EditorEvent]
//!   values (T011): `onChange`/`onSelectionChange`/`onQualifiedPath`/
//!   `onExecute`/`onExecuteAll`/`onSaveQuery` map to its arms. The
//!   extension-fed callbacks (`onContextMenu`, `onDropTable`,
//!   `onNavigateTo*`) have no engine extensions yet; their payload
//!   shapes ([`DroppedTablePayload`], the navigation targets) land here
//!   so those tasks build on the contract, not on ad-hoc shapes.
//! - `resolveCmDialect`: CodeMirror's dialect objects become
//!   [`CmSqlDialect`] (the engine's highlighting is dialect-agnostic
//!   today; the enum is the wiring point the highlight task keys on).
//!   The `DB_REGISTRY` lookup (`meta?.sqlDialect` family, e.g. kiwi →
//!   mysql) is the host seam — the resolved family rides in as
//!   `registry_family`, exactly the `resolve_dialect_profile` precedent.
//! - `className` has no Rune counterpart (styling is structural); the
//!   omission is the documented decision, not an oversight.
//!
//! Known deltas: none behavioural — every field, arm and mapping key is
//! the React file's verbatim.

use crate::components::sql_editor::sql_namespace::SqlNamespace;

/// React `SqlSchema` — the nested schema tree for SQL autocompletion.
pub type SqlSchema = SqlNamespace;

// ---------------------------------------------------------------------------
// The drop payload (SchemaTree → editor drag-and-drop)
// ---------------------------------------------------------------------------

/// One dropped table reference (`{ tableName, schema? }`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // wired by the drop-extension task
pub struct DroppedTableRef {
    pub table_name: String,
    pub schema: Option<String>,
}

/// React `DroppedTablePayload` — the `onDropTable` payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // wired by the drop-extension task
pub struct DroppedTablePayload {
    pub tables: Vec<DroppedTableRef>,
    /// Cross-connection drop validation.
    pub connection_id: Option<String>,
    pub db_session_id: Option<String>,
    pub database_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Execution + completion preferences (moved from sql_editor.rs, T011's inline)
// ---------------------------------------------------------------------------

/// `executionStatus` from `contracts.ts`: `'idle' | 'running' | 'cancelling'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub enum ExecutionStatus {
    #[default]
    Idle,
    Running,
    Cancelling,
}

/// `'unquoted' | 'always' | 'both'` from `contracts.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub enum CompletionQuotePolicy {
    #[default]
    Unquoted,
    Always,
    Both,
}

// ---------------------------------------------------------------------------
// The navigation targets (Mod+Click / hover-action payloads)
// ---------------------------------------------------------------------------

/// `onNavigateToTable`'s target (Mod+Click on a table name).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // wired by the navigation-extension task
pub struct TableNavTarget {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub name: String,
}

/// `onNavigateToStructure`'s target (Mod+Click on a column name).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // wired by the navigation-extension task
pub struct StructureNavTarget {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    pub column_name: Option<String>,
}

/// `onNavigateToDdl`'s target (the hover action): the relation kind rides
/// with the name.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // wired by the hover-extension task
pub struct DdlNavTarget {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    /// `'table' | 'view'`.
    pub is_view: bool,
}

// ---------------------------------------------------------------------------
// The dialect mapping (`CM_DIALECT_MAP` + `resolveCmDialect`)
// ---------------------------------------------------------------------------

/// CodeMirror's `SQLDialect` objects, as the engine's wiring enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // keyed by the highlight task; resolveCmDialect returns it
pub enum CmSqlDialect {
    PostgreSql,
    MySql,
    MariaSql,
    Sqlite,
    MsSql,
    StandardSql,
}

impl CmSqlDialect {
    /// The React import each arm maps to (`@codemirror/lang-sql`).
    pub fn react_name(self) -> &'static str {
        match self {
            Self::PostgreSql => "PostgreSQL",
            Self::MySql => "MySQL",
            Self::MariaSql => "MariaSQL",
            Self::Sqlite => "SQLite",
            Self::MsSql => "MSSQL",
            Self::StandardSql => "StandardSQL",
        }
    }
}

/// React `CM_DIALECT_MAP`'s keys (`postgresql`, `mysql`, `mariadb`,
/// `sqlite`, `sqlserver`).
fn cm_dialect_for(key: &str) -> Option<CmSqlDialect> {
    match key {
        "postgresql" => Some(CmSqlDialect::PostgreSql),
        "mysql" => Some(CmSqlDialect::MySql),
        "mariadb" => Some(CmSqlDialect::MariaSql),
        "sqlite" => Some(CmSqlDialect::Sqlite),
        "sqlserver" => Some(CmSqlDialect::MsSql),
        _ => None,
    }
}

/// React `resolveCmDialect`: the db type's own key first, then the
/// registry-resolved family (the host seam — plugins map via
/// `sqlDialect`, e.g. kiwi → mysql), then StandardSQL. An absent db type
/// is StandardSQL too.
pub fn resolve_cm_dialect(db_type: Option<&str>, registry_family: Option<&str>) -> CmSqlDialect {
    if let Some(db_type) = db_type {
        if let Some(dialect) = cm_dialect_for(db_type) {
            return dialect;
        }
        if let Some(family) = registry_family {
            if let Some(dialect) = cm_dialect_for(family) {
                return dialect;
            }
        }
    }
    CmSqlDialect::StandardSql
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn dialect_map_covers_the_react_keys() {
        assert_eq!(cm_dialect_for("postgresql"), Some(CmSqlDialect::PostgreSql));
        assert_eq!(cm_dialect_for("mysql"), Some(CmSqlDialect::MySql));
        assert_eq!(cm_dialect_for("mariadb"), Some(CmSqlDialect::MariaSql));
        assert_eq!(cm_dialect_for("sqlite"), Some(CmSqlDialect::Sqlite));
        assert_eq!(cm_dialect_for("sqlserver"), Some(CmSqlDialect::MsSql));
        // Everything else — including the family aliases the profile
        // table knows but CM_DIALECT_MAP does not — is absent.
        assert_eq!(cm_dialect_for("duckdb"), None);
        assert_eq!(cm_dialect_for("postgres"), None);
        assert_eq!(cm_dialect_for("mssql"), None);
        assert_eq!(cm_dialect_for("generic"), None);
        assert_eq!(cm_dialect_for(""), None);
    }

    #[test]
    fn resolve_follows_the_react_order() {
        // No db type: StandardSQL.
        assert_eq!(resolve_cm_dialect(None, None), CmSqlDialect::StandardSql);
        assert_eq!(resolve_cm_dialect(None, Some("mysql")), CmSqlDialect::StandardSql);
        // The db type's own key wins over the family.
        assert_eq!(resolve_cm_dialect(Some("sqlite"), Some("mysql")), CmSqlDialect::Sqlite);
        // An unmapped db type falls to the registry family (kiwi → mysql).
        assert_eq!(resolve_cm_dialect(Some("kiwi"), Some("mysql")), CmSqlDialect::MySql);
        // Both unmapped: StandardSQL.
        assert_eq!(resolve_cm_dialect(Some("duckdb"), None), CmSqlDialect::StandardSql);
        assert_eq!(resolve_cm_dialect(Some("duckdb"), Some("oracle")), CmSqlDialect::StandardSql);
        // The arm names are the React imports (the wiring pin).
        assert_eq!(CmSqlDialect::StandardSql.react_name(), "StandardSQL");
        assert_eq!(CmSqlDialect::MariaSql.react_name(), "MariaSQL");
    }
}
