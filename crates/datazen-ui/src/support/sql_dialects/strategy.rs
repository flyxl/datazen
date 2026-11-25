//! SQL dialect strategy types — the **strategy half** of React
//! `@datazen/extension-points` `sql-editor/sqlDialectTypes.ts`, carried
//! for the per-dialect modules (T123) and the registry (T084).
//!
//! The profile half of that file (quote style, folding, parameter
//! policy) already landed as [`super::dialect_profile`]; this module
//! carries the other export the frontend dialects consume: the strategy
//! objects themselves — DDL queries, index SQL, backup options, the
//! truncate override, the table-SQL generator and the row-limit shape.
//!
//! The strategies are `static`-friendly: plain data plus `fn` pointers,
//! no trait objects, so each dialect module exports one `const`-named
//! strategy exactly like the React `export const postgresqlDialect`.

/// `BackupOption`: a dialect's backup flag (`--data-only` &c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackupOption {
    pub id: &'static str,
    pub label: &'static str,
}

/// `supportedIndexMethods` entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexMethod {
    Btree,
    Hash,
    Gin,
    Gist,
}

/// A DDL fetch query plus the column its first result row carries the
/// DDL string in (`{ sql, extractColumnIndex }`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdlQuery {
    pub sql: String,
    pub extract_column_index: usize,
}

/// `getCreateIndexSql`'s options (already-quoted fields stay the
/// caller's; `columns` are raw names or `name(length)` forms).
pub struct CreateIndexOptions<'a> {
    pub index_name: &'a str,
    pub table_name: &'a str,
    pub columns: &'a [String],
    pub unique: bool,
    pub method: Option<IndexMethod>,
    pub quote_char: char,
}

/// `RowLimitShape.placement`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowLimitPlacement {
    /// The cap ends the statement (`… FETCH FIRST 10 ROWS ONLY`).
    Trailing,
    /// The cap sits right behind `SELECT [DISTINCT]`.
    SelectList,
}

/// `RowLimitShape`: where a dialect's row cap goes and how it spells it.
#[derive(Clone, Copy)]
pub struct RowLimitShape {
    pub placement: RowLimitPlacement,
    /// The cap clause for a row count.
    pub clause: fn(limit: usize) -> String,
}

/// `GeneratedSqlType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedSqlType {
    Select,
    Insert,
    Update,
    Delete,
}

/// `DdlDialect`: table/view DDL fetch queries.
#[derive(Clone, Copy)]
pub struct DialectDdl {
    /// `getTableDdlQuery(tableName, schema?)`.
    pub table_ddl: fn(table_name: &str, schema: Option<&str>) -> DdlQuery,
    /// `getViewDdlQuery(viewName, schema?)` — React falls back to the
    /// table query when absent; the port models that as `None` +
    /// [`SqlDialectStrategy::view_ddl_query`]'s fallback.
    pub view_ddl: Option<fn(view_name: &str, schema: Option<&str>) -> DdlQuery>,
}

/// `IndexDialect`: index SQL builders and the supported methods.
#[derive(Clone, Copy)]
pub struct DialectIndex {
    pub supported_methods: &'static [IndexMethod],
    pub drop_index: fn(index_name: &str, table_name: &str, quote_char: char) -> String,
    pub create_index: fn(opts: &CreateIndexOptions<'_>) -> String,
}

/// `SqlDialectStrategy`: one dialect's SQL behavior. `static`-friendly
/// (data + `fn` pointers); the React object's optional members map to
/// `Option` with the documented defaults applied by the accessors below.
pub struct SqlDialectStrategy {
    pub family: &'static str,
    pub ddl: DialectDdl,
    pub index: DialectIndex,
    pub backup_options: &'static [BackupOption],
    /// SQL to empty a table; `None` defaults to
    /// `TRUNCATE TABLE <quoted>` ([`Self::truncate_table_sql`]).
    pub truncate_table_sql: Option<fn(quoted_name: &str) -> String>,
    /// Table template SQL generator (SELECT/INSERT/UPDATE/DELETE).
    pub table_sql: Option<super::base_table_sql::BaseTableSqlGenerator>,
    /// How the dialect caps returned rows; `None` is the standard
    /// trailing `LIMIT n` ([`Self::row_limit_shape`]).
    pub row_limit: Option<RowLimitShape>,
}

impl SqlDialectStrategy {
    /// `getViewDdlQuery` — the table query when the dialect did not
    /// override it (React's documented fallback).
    pub fn view_ddl_query(&self, view_name: &str, schema: Option<&str>) -> DdlQuery {
        match self.ddl.view_ddl {
            Some(f) => f(view_name, schema),
            None => (self.ddl.table_ddl)(view_name, schema),
        }
    }

    /// `getTruncateTableSql` — the `TRUNCATE TABLE <quoted>` default.
    pub fn truncate_table_sql(&self, quoted_name: &str) -> String {
        match self.truncate_table_sql {
            Some(f) => f(quoted_name),
            None => format!("TRUNCATE TABLE {quoted_name}"),
        }
    }

    /// The row-limit shape, defaulting to the standard `LIMIT n`.
    pub fn row_limit_shape(&self) -> RowLimitShape {
        self.row_limit.unwrap_or(super::row_limit::LIMIT_ROW_LIMIT)
    }
}
