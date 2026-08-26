//! F7: PostgreSQL SQL target qualification binding.
//!
//! Dialect shape: `"schema"."t"` — the PG engine cannot inline a cross-database
//! qualifier, so the **database** dimension keeps using the existing
//! connection-pool switch (host `ensure_session_database` pin) and only the
//! **schema** dimension is rewritten inline. When no schema target is present
//! this binding is a no-op and the statement runs on the session's current
//! `search_path`.

use datazen_driver_api::{qualify_sql_with, QualifierQuote};
use sqlparser::dialect::PostgreSqlDialect;

/// Rewrite `sql` so unqualified table references land inside `schema`.
///
/// Best-effort by contract: parse failures pass the original text through,
/// and the rewrite is idempotent (already-qualified references are skipped).
/// `database` is intentionally not inlined — PG resolves cross-database
/// access through connection pooling, not name qualification.
pub(crate) fn qualify_sql(sql: &str, database: Option<&str>, schema: Option<&str>) -> String {
    let _ = database;
    let parts: Vec<&str> = schema
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .into_iter()
        .collect();
    qualify_sql_with(
        &PostgreSqlDialect {},
        QualifierQuote::DoubleQuote,
        &parts,
        sql,
    )
    .sql
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qualify(sql: &str, schema: Option<&str>) -> String {
        qualify_sql(sql, None, schema)
    }

    #[test]
    fn simple_select_gets_schema_qualified() {
        assert_eq!(
            qualify("SELECT * FROM users", Some("sales")),
            "SELECT * FROM \"sales\".\"users\""
        );
    }

    #[test]
    fn multi_join_all_bare_refs_get_qualified() {
        let out = qualify(
            "SELECT * FROM users u JOIN orders o ON o.uid = u.id LEFT JOIN items i ON i.order_id = o.id",
            Some("sales"),
        );
        assert!(out.contains("\"sales\".\"users\" AS u"), "{out}");
        assert!(out.contains("JOIN \"sales\".\"orders\" AS o"), "{out}");
        assert!(out.contains("JOIN \"sales\".\"items\" AS i"), "{out}");
    }

    #[test]
    fn cte_names_are_skipped_but_body_tables_qualify() {
        let out = qualify(
            "WITH top AS (SELECT id FROM orders LIMIT 10) SELECT * FROM top t JOIN users u ON u.id = t.id",
            Some("sales"),
        );
        assert!(out.contains("\"sales\".\"orders\""), "{out}");
        assert!(out.contains("\"sales\".\"users\" AS u"), "{out}");
        assert!(out.contains("FROM top AS t"), "{out}");
        assert!(!out.contains("\"sales\".\"top\""), "{out}");
    }

    #[test]
    fn subqueries_in_expressions_qualify() {
        let out = qualify(
            "SELECT * FROM users WHERE id IN (SELECT uid FROM admins)",
            Some("sales"),
        );
        assert!(out.contains("\"sales\".\"users\""), "{out}");
        assert!(
            out.contains("(SELECT uid FROM \"sales\".\"admins\")"),
            "{out}"
        );
    }

    #[test]
    fn already_qualified_references_are_untouched() {
        let sql = "SELECT * FROM public.t WHERE x IN (SELECT y FROM other.u)";
        assert_eq!(qualify(sql, Some("sales")), sql);
    }

    #[test]
    fn quoted_identifiers_are_preserved_and_qualified() {
        let out = qualify("SELECT * FROM \"Users\"", Some("sales"));
        assert_eq!(out, "SELECT * FROM \"sales\".\"Users\"");
    }

    #[test]
    fn string_literals_are_not_touched() {
        let out = qualify("SELECT * FROM logs WHERE msg = 'from users'", Some("sales"));
        assert!(out.contains("'from users'"), "{out}");
        assert!(!out.contains("'from \"sales"), "{out}");
    }

    #[test]
    fn insert_update_delete_qualify() {
        assert_eq!(
            qualify(
                "INSERT INTO users (id, name) VALUES (1, 'a')",
                Some("sales")
            ),
            "INSERT INTO \"sales\".\"users\" (id, name) VALUES (1, 'a')"
        );
        let update = qualify("UPDATE users SET name = 'b' WHERE id = 1", Some("sales"));
        assert!(
            update.starts_with("UPDATE \"sales\".\"users\" SET"),
            "{update}"
        );
        assert_eq!(
            qualify("DELETE FROM users WHERE id = 1", Some("sales")),
            "DELETE FROM \"sales\".\"users\" WHERE id = 1"
        );
    }

    #[test]
    fn ddl_statements_qualify() {
        assert!(qualify("TRUNCATE TABLE users", Some("sales"))
            .contains("TRUNCATE TABLE \"sales\".\"users\""));
        assert!(
            qualify("CREATE TABLE users (id integer PRIMARY KEY)", Some("sales"))
                .contains("CREATE TABLE \"sales\".\"users\"")
        );
        assert!(qualify("ALTER TABLE users ADD COLUMN c int", Some("sales"))
            .contains("ALTER TABLE \"sales\".\"users\""));
        assert!(qualify("CREATE INDEX ix ON users (id)", Some("sales"))
            .contains("ON \"sales\".\"users\""));
        // DROP TABLE qualifies; other object kinds are left alone.
        assert!(
            qualify("DROP TABLE users", Some("sales")).contains("DROP TABLE \"sales\".\"users\"")
        );
        assert_eq!(
            qualify("DROP VIEW users_view", Some("sales")),
            "DROP VIEW users_view"
        );
    }

    #[test]
    fn rewrite_is_idempotent() {
        let once = qualify(
            "SELECT * FROM users WHERE id IN (SELECT uid FROM admins)",
            Some("sales"),
        );
        let twice = qualify(&once, Some("sales"));
        assert_eq!(once, twice);
    }

    #[test]
    fn parse_failure_passes_through_unchanged() {
        let broken = "SELEC * FORM users WHER";
        assert_eq!(qualify(broken, Some("sales")), broken);
    }

    #[test]
    fn missing_schema_is_noop_inline_database_is_pool_switch() {
        let sql = "SELECT * FROM users";
        // No schema: nothing to inline…
        assert_eq!(qualify(sql, None), sql);
        assert_eq!(qualify(sql, Some("   ")), sql);
        // …and a database-only target must NOT be inlined either — the PG
        // database dimension is served by the host pool switch.
        let db_only = qualify_sql(sql, Some("other_db"), None);
        assert_eq!(db_only, sql);
    }
}
