//! PostgreSQL dialect strategy — the React
//! `lib/sqlDialects/postgresql.ts` port (T123).

use super::strategy::{
    BackupOption, CreateIndexOptions, DdlQuery, DialectDdl, DialectIndex, IndexMethod,
    SqlDialectStrategy,
};

/// `PG_OPTIONS` (format-custom removed upstream: it requires pg_dump's
/// binary format, not supported in-process).
const PG_OPTIONS: &[BackupOption] = &[
    BackupOption { id: "data-only", label: "--data-only" },
    BackupOption { id: "clean", label: "--clean" },
    BackupOption { id: "create", label: "--create" },
    BackupOption { id: "no-owner", label: "--no-owner" },
    BackupOption { id: "schema-only", label: "--schema-only" },
    BackupOption { id: "routines", label: "--routines" },
    BackupOption { id: "triggers", label: "--triggers" },
];

/// `postgresqlDialect.ddl.getTableDdlQuery` — the column-list DDL from
/// the catalog, byte-identical to the React template (`E'\n'` are the
/// SQL escape sequences, kept as backslash-n).
fn table_ddl_query(table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery {
        sql: format!(
            r#"
      SELECT
        'CREATE TABLE ' || quote_ident(schemaname) || '.' || quote_ident(tablename) || E'(\n' ||
        string_agg(
          '  ' || quote_ident(attname) || ' ' || format_type(atttypid, atttypmod) ||
          CASE WHEN NOT attnotnull THEN '' ELSE ' NOT NULL' END ||
          CASE WHEN pg_get_expr(adbin, adrelid) IS NOT NULL
               THEN ' DEFAULT ' || pg_get_expr(adbin, adrelid)
               ELSE '' END,
          E',\n' ORDER BY attnum
        ) || E'\n);' AS ddl
      FROM pg_tables t
      JOIN pg_attribute a ON a.attrelid = (quote_ident(schemaname) || '.' || quote_ident(tablename))::regclass
      LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
      WHERE a.attnum > 0 AND NOT a.attisdropped
        AND tablename = '{table_name}'
      GROUP BY schemaname, tablename;
    "#
        ),
        extract_column_index: 0,
    }
}

/// `postgresqlDialect.ddl.getViewDdlQuery` — single quotes doubled into
/// the view name.
fn view_ddl_query(view_name: &str, _schema: Option<&str>) -> DdlQuery {
    let escaped = view_name.replace('\'', "''");
    DdlQuery {
        sql: format!(
            "SELECT 'CREATE OR REPLACE VIEW ' || quote_ident(schemaname) || '.' || \
             quote_ident(viewname) || ' AS ' || E'\\n' || definition AS ddl FROM pg_views WHERE \
             viewname = '{escaped}'"
        ),
        extract_column_index: 0,
    }
}

/// `postgresqlDialect.index.getDropIndexSql`.
fn drop_index_sql(index_name: &str, _table_name: &str, quote_char: char) -> String {
    format!("DROP INDEX {quote_char}{index_name}{quote_char}")
}

/// `postgresqlDialect.index.getCreateIndexSql` — `USING` only for the
/// non-default methods.
fn create_index_sql(opts: &CreateIndexOptions<'_>) -> String {
    let unique_kw = if opts.unique { "UNIQUE " } else { "" };
    let using_kw = match opts.method {
        Some(method) if method != IndexMethod::Btree => {
            format!(" USING {}", method_name(method))
        }
        _ => String::new(),
    };
    let q = opts.quote_char;
    let quoted_cols = opts
        .columns
        .iter()
        .map(|c| format!("{q}{c}{q}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE {unique_kw}INDEX {q}{index_name}{q} ON {q}{table_name}{q}{using_kw} ({quoted_cols})",
        index_name = opts.index_name,
        table_name = opts.table_name,
    )
}

/// The lowercase method names React stringifies into `USING …`.
fn method_name(method: IndexMethod) -> &'static str {
    match method {
        IndexMethod::Btree => "btree",
        IndexMethod::Hash => "hash",
        IndexMethod::Gin => "gin",
        IndexMethod::Gist => "gist",
    }
}

/// `postgresqlDialect` — double-quote table SQL, no truncate override,
/// no row-limit declaration (the standard trailing `LIMIT n`).
pub(crate) static POSTGRESQL_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "postgresql",
    ddl: DialectDdl {
        table_ddl: table_ddl_query,
        view_ddl: Some(view_ddl_query),
    },
    index: DialectIndex {
        supported_methods: &[IndexMethod::Btree, IndexMethod::Hash, IndexMethod::Gin, IndexMethod::Gist],
        drop_index: drop_index_sql,
        create_index: create_index_sql,
    },
    backup_options: PG_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::new('"')),
    row_limit: None,
};
