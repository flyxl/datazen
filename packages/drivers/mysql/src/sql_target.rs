//! F7: MySQL-family SQL target qualification binding.
//!
//! Dialect shape: `` `db`.`t` `` — a true cross-database inline qualifier, so
//! every unqualified table reference in the targeting contexts (FROM / JOIN /
//! INSERT INTO / UPDATE / DELETE FROM / TRUNCATE / CREATE | DROP | ALTER TABLE
//! / CREATE INDEX ON) is prefixed with the requested database. No `USE`, no
//! session switch; the host `ensure_session_database` pin keeps running as an
//! independent safety net.
//!
//! The `schema` argument has no meaning on the MySQL family and is ignored.
//! This binding is shared by every variant built on [`crate::MysqlDriver`]
//! (`mysql`, `mariadb`, `doris`, `starrocks`, `manticore`, `ob_oracle`).

use datazen_driver_api::{qualify_sql_with, QualifierQuote};
use sqlparser::dialect::MySqlDialect;

/// Rewrite `sql` so unqualified table references land on `database`.
///
/// Best-effort by contract: when the statement fails to parse the original
/// text is returned unchanged (a warning is logged inside the shared engine).
/// The rewrite is idempotent — already-qualified references are skipped.
pub(crate) fn qualify_sql(sql: &str, database: Option<&str>, schema: Option<&str>) -> String {
    // MySQL family resolves objects per database; there is no separate schema
    // dimension to inline. Blank targets are treated as "no target".
    let _ = schema;
    let parts: Vec<&str> = database
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .into_iter()
        .collect();
    qualify_sql_with(&MySqlDialect {}, QualifierQuote::Backtick, &parts, sql).sql
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
            qualify("SELECT * FROM users", Some("mydb")),
            "SELECT * FROM `mydb`.`users`"
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
        assert!(!out.contains("FROM users"), "{out}");
    }

    #[test]
    fn cte_names_are_skipped_but_body_tables_qualify() {
        let out = qualify(
            "WITH top AS (SELECT id FROM orders LIMIT 10) SELECT * FROM top t JOIN users u ON u.id = t.id",
            Some("shop"),
        );
        // CTE reference stays bare; the CTE body and other tables qualify.
        assert!(out.contains("`shop`.`orders`"), "{out}");
        assert!(out.contains("`shop`.`users` AS u"), "{out}");
        assert!(out.contains("FROM top AS t"), "{out}");
        assert!(!out.contains("`shop`.`top`"), "{out}");
    }

    #[test]
    fn subqueries_in_expressions_qualify() {
        let out = qualify(
            "SELECT * FROM users WHERE id IN (SELECT uid FROM admins)",
            Some("mydb"),
        );
        assert!(out.contains("`mydb`.`users`"), "{out}");
        assert!(out.contains("(SELECT uid FROM `mydb`.`admins`)"), "{out}");
    }

    #[test]
    fn already_qualified_references_are_untouched() {
        let sql = "SELECT * FROM other.t WHERE x IN (SELECT y FROM db2.u)";
        assert_eq!(qualify(sql, Some("mydb")), sql);
    }

    #[test]
    fn quoted_identifiers_are_preserved_and_qualified() {
        let out = qualify("SELECT * FROM `Users`", Some("mydb"));
        assert_eq!(out, "SELECT * FROM `mydb`.`Users`");
    }

    #[test]
    fn string_literals_are_not_touched() {
        let out = qualify("SELECT * FROM logs WHERE msg = 'from users'", Some("mydb"));
        assert!(out.contains("'from users'"), "{out}");
        assert!(out.contains("`mydb`.`logs`"), "{out}");
        assert!(!out.contains("'from `mydb"), "{out}");
    }

    #[test]
    fn insert_update_delete_qualify() {
        assert_eq!(
            qualify("INSERT INTO users (id, name) VALUES (1, 'a')", Some("mydb")),
            "INSERT INTO `mydb`.`users` (id, name) VALUES (1, 'a')"
        );
        let update = qualify("UPDATE users SET name = 'b' WHERE id = 1", Some("mydb"));
        assert!(update.starts_with("UPDATE `mydb`.`users` SET"), "{update}");
        assert_eq!(
            qualify("DELETE FROM users WHERE id = 1", Some("mydb")),
            "DELETE FROM `mydb`.`users` WHERE id = 1"
        );
    }

    #[test]
    fn ddl_statements_qualify() {
        assert!(
            qualify("TRUNCATE TABLE users", Some("mydb")).contains("TRUNCATE TABLE `mydb`.`users`")
        );
        assert!(
            qualify("CREATE TABLE users (id INT PRIMARY KEY)", Some("mydb"))
                .contains("CREATE TABLE `mydb`.`users`")
        );
        assert!(qualify("ALTER TABLE users ADD COLUMN c INT", Some("mydb"))
            .contains("ALTER TABLE `mydb`.`users`"));
        assert!(
            qualify("CREATE INDEX ix ON users (id)", Some("mydb")).contains("ON `mydb`.`users`")
        );
        assert!(qualify("DROP TABLE users", Some("mydb")).contains("DROP TABLE `mydb`.`users`"));
    }

    #[test]
    fn rewrite_is_idempotent() {
        let once = qualify(
            "SELECT * FROM users WHERE name = 'x' AND id IN (SELECT uid FROM admins)",
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

    #[test]
    fn multi_statement_strings_keep_every_statement_targeted() {
        let out = qualify("SELECT * FROM a; INSERT INTO b VALUES (1);", Some("mydb"));
        assert!(out.contains("`mydb`.`a`"), "{out}");
        assert!(out.contains("INSERT INTO `mydb`.`b`"), "{out}");
    }
}
