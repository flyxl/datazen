//! F7: ClickHouse SQL target qualification binding.
//!
//! Dialect shape: `` `db`.`t` `` — a true cross-database inline qualifier, so
//! every unqualified table reference in the targeting contexts (FROM / JOIN /
//! INSERT INTO / UPDATE / DELETE FROM / TRUNCATE / CREATE | DROP | ALTER TABLE
//! / CREATE INDEX ON) is prefixed with the requested database. No session
//! switch; the host `ensure_session_database` pin keeps running as an
//! independent safety net.
//!
//! The `schema` argument has no meaning on ClickHouse (its databases are the
//! namespace dimension) and is ignored.
//!
//! Best-effort by contract: parse failures pass the original text through,
//! and the rewrite is idempotent.

use datazen_driver_api::{qualify_sql_with, QualifierQuote};
use sqlparser::dialect::ClickHouseDialect;

/// Rewrite `sql` so unqualified table references land on `database`.
pub(crate) fn qualify_sql(sql: &str, database: Option<&str>, schema: Option<&str>) -> String {
    // ClickHouse resolves objects per database; there is no separate schema
    // dimension to inline. Blank targets are treated as "no target".
    let _ = schema;
    let parts: Vec<&str> = database
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .into_iter()
        .collect();
    qualify_sql_with(&ClickHouseDialect {}, QualifierQuote::Backtick, &parts, sql).sql
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qualify(sql: &str, database: Option<&str>) -> String {
        qualify_sql(sql, database, None)
    }

    #[test]
    fn simple_select_gets_qualified() {
        assert_eq!(
            qualify("SELECT * FROM users", Some("analytics")),
            "SELECT * FROM `analytics`.`users`"
        );
    }

    #[test]
    fn multi_join_all_bare_refs_get_qualified() {
        let out = qualify(
            "SELECT * FROM users u JOIN orders o ON o.uid = u.id LEFT JOIN items i ON i.order_id = o.id",
            Some("shop"),
        );
        assert!(out.contains("`shop`.`users` AS u"), "{out}");
        assert!(out.contains("JOIN `shop`.`orders` AS o"), "{out}");
        assert!(out.contains("JOIN `shop`.`items` AS i"), "{out}");
    }

    #[test]
    fn cte_names_are_skipped_but_body_tables_qualify() {
        let out = qualify(
            "WITH top AS (SELECT id FROM orders LIMIT 10) SELECT * FROM top t JOIN users u ON u.id = t.id",
            Some("shop"),
        );
        assert!(out.contains("`shop`.`orders`"), "{out}");
        assert!(out.contains("`shop`.`users` AS u"), "{out}");
        assert!(out.contains("FROM top AS t"), "{out}");
        assert!(!out.contains("`shop`.`top`"), "{out}");
    }

    #[test]
    fn already_qualified_references_are_untouched() {
        let sql = "SELECT * FROM other.t WHERE x IN (SELECT y FROM db2.u)";
        assert_eq!(qualify(sql, Some("mydb")), sql);
    }

    #[test]
    fn string_literals_are_not_touched() {
        let out = qualify("SELECT * FROM logs WHERE msg = 'from users'", Some("mydb"));
        assert!(out.contains("'from users'"), "{out}");
        assert!(!out.contains("'from `mydb"), "{out}");
    }

    #[test]
    fn insert_update_delete_ddl_qualify() {
        assert!(qualify("INSERT INTO users (id) VALUES (1)", Some("mydb"))
            .contains("INSERT INTO `mydb`.`users`"));
        assert!(
            qualify("ALTER TABLE users ADD COLUMN c UInt32", Some("mydb"))
                .contains("ALTER TABLE `mydb`.`users`")
        );
        assert!(qualify(
            "CREATE TABLE users (id UInt32) ENGINE = MergeTree ORDER BY id",
            Some("mydb")
        )
        .contains("CREATE TABLE `mydb`.`users`"));
        assert!(qualify("DROP TABLE users", Some("mydb")).contains("DROP TABLE `mydb`.`users`"));
    }

    #[test]
    fn ch_specific_mutation_syntax_falls_back_to_passthrough() {
        // sqlparser's ClickHouseDialect does not model the CH-only
        // `ALTER TABLE … DELETE WHERE` mutation: parse fails and the SQL is
        // passed through untouched (documented best-effort fallback). The
        // host `ensure_session_database` pin still covers the target.
        let mutation = "ALTER TABLE users DELETE WHERE id = 1";
        assert_eq!(qualify(mutation, Some("mydb")), mutation);
    }

    #[test]
    fn rewrite_is_idempotent() {
        let once = qualify(
            "SELECT * FROM users WHERE id IN (SELECT uid FROM admins)",
            Some("mydb"),
        );
        let twice = qualify(&once, Some("mydb"));
        assert_eq!(once, twice);
    }

    #[test]
    fn parse_failure_passes_through_unchanged() {
        let broken = "SELEC * FORM users WHER";
        assert_eq!(qualify(broken, Some("mydb")), broken);
    }

    #[test]
    fn missing_target_is_noop() {
        let sql = "SELECT * FROM users";
        assert_eq!(qualify(sql, None), sql);
        assert_eq!(qualify(sql, Some("   ")), sql);
    }
}
