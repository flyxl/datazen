//! F7: DuckDB SQL target qualification binding.
//!
//! Dialect shape: same as PostgreSQL — `"schema"."t"`. Only the **schema**
//! dimension is rewritten inline; the **database** dimension keeps using the
//! existing connection/session switch (host `ensure_session_database` pin),
//! mirroring the PG-family behavior from the design baseline. With no schema
//! target this binding is a no-op and the statement runs on the session's
//! current default schema.
//!
//! Best-effort by contract: parse failures pass the original text through,
//! and the rewrite is idempotent.

use datazen_driver_api::{qualify_sql_with, QualifierQuote};
use sqlparser::dialect::DuckDbDialect;

/// Rewrite `sql` so unqualified table references land inside `schema`.
///
/// `database` is intentionally not inlined — it is served by the host pool /
/// session switch, not by name qualification.
pub(crate) fn qualify_sql(sql: &str, database: Option<&str>, schema: Option<&str>) -> String {
    let _ = database;
    let parts: Vec<&str> = schema
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .into_iter()
        .collect();
    qualify_sql_with(&DuckDbDialect {}, QualifierQuote::DoubleQuote, &parts, sql).sql
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
    fn already_qualified_references_are_untouched() {
        let sql = "SELECT * FROM main.t WHERE x IN (SELECT y FROM other.u)";
        assert_eq!(qualify(sql, Some("sales")), sql);
    }

    #[test]
    fn string_literals_are_not_touched() {
        let out = qualify("SELECT * FROM logs WHERE msg = 'from users'", Some("sales"));
        assert!(out.contains("'from users'"), "{out}");
        assert!(!out.contains("'from \"sales"), "{out}");
    }

    #[test]
    fn insert_update_delete_ddl_qualify() {
        assert!(qualify("INSERT INTO users (id) VALUES (1)", Some("sales"))
            .contains("INSERT INTO \"sales\".\"users\""));
        assert!(
            qualify("UPDATE users SET a = 1", Some("sales")).contains("UPDATE \"sales\".\"users\"")
        );
        assert!(
            qualify("DELETE FROM users", Some("sales")).contains("DELETE FROM \"sales\".\"users\"")
        );
        assert!(qualify("CREATE TABLE users (id INTEGER)", Some("sales"))
            .contains("CREATE TABLE \"sales\".\"users\""));
        assert!(
            qualify("DROP TABLE users", Some("sales")).contains("DROP TABLE \"sales\".\"users\"")
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
        assert_eq!(qualify(sql, None), sql);
        assert_eq!(qualify(sql, Some("   ")), sql);
        // A database-only target must NOT be inlined — the database dimension
        // is served by the host session/pool switch, like PG.
        let db_only = qualify_sql(sql, Some("other_db"), None);
        assert_eq!(db_only, sql);
    }
}
