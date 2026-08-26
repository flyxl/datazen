//! F7: SQL Server (T-SQL) SQL target qualification binding.
//!
//! Dialect shape: the three-part T-SQL name `[db].[schema].[t]`. The rewrite
//! only fires when an unambiguous prefix can be built:
//! - `database` + `schema` → `[db].[schema].t`
//! - `schema` only         → `[schema].t` (default database of the session)
//!
//! A **database-only** target is intentionally *not* inlined: in T-SQL a
//! two-part name `[db].t` means *schema* `db`, which would silently change
//! resolution. The database dimension is instead served by the host
//! `ensure_session_database` session pin (`USE db`), which stays active as
//! the safety net either way.
//!
//! Temp tables (`#tmp`) always stay in their tempdb/session scope and are
//! never qualified. Bracket quoting escapes `]` as `]]` (T-SQL rules).
//!
//! Best-effort by contract: parse failures pass the original text through,
//! and the rewrite is idempotent.

use datazen_driver_api::{qualify_sql_with, QualifierQuote};
use sqlparser::dialect::MsSqlDialect;

/// Rewrite `sql` so unqualified table references land on `[db].[schema]`.
pub(crate) fn qualify_sql(sql: &str, database: Option<&str>, schema: Option<&str>) -> String {
    let database = database.map(str::trim).filter(|s| !s.is_empty());
    let schema = schema.map(str::trim).filter(|s| !s.is_empty());
    // Build [db?, schema?] — but require at least the schema part so we never
    // emit an ambiguous two-part `[db].t`.
    let mut parts: Vec<&str> = Vec::with_capacity(2);
    if let Some(schema) = schema {
        if let Some(database) = database {
            parts.push(database);
        }
        parts.push(schema);
    }
    if parts.is_empty() {
        return sql.to_string();
    }
    qualify_sql_with(&MsSqlDialect {}, QualifierQuote::Bracket, &parts, sql).sql
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_select_gets_schema_qualified() {
        assert_eq!(
            qualify_sql("SELECT * FROM users", None, Some("sales")),
            "SELECT * FROM [sales].[users]"
        );
    }

    #[test]
    fn full_three_part_name_when_db_and_schema_given() {
        assert_eq!(
            qualify_sql("SELECT * FROM users", Some("AdventureWorks"), Some("sales")),
            "SELECT * FROM [AdventureWorks].[sales].[users]"
        );
    }

    #[test]
    fn multi_join_all_bare_refs_get_qualified() {
        let out = qualify_sql(
            "SELECT * FROM users u JOIN orders o ON o.uid = u.id LEFT JOIN items i ON i.order_id = o.id",
            Some("dw"),
            Some("dbo"),
        );
        assert!(out.contains("[dw].[dbo].[users] AS u"), "{out}");
        assert!(out.contains("JOIN [dw].[dbo].[orders] AS o"), "{out}");
        assert!(out.contains("JOIN [dw].[dbo].[items] AS i"), "{out}");
    }

    #[test]
    fn cte_names_are_skipped_but_body_tables_qualify() {
        let out = qualify_sql(
            "WITH top AS (SELECT id FROM orders) SELECT * FROM top t JOIN users u ON u.id = t.id",
            None,
            Some("dbo"),
        );
        assert!(out.contains("[dbo].[orders]"), "{out}");
        assert!(out.contains("[dbo].[users] AS u"), "{out}");
        assert!(out.contains("FROM top AS t"), "{out}");
        assert!(!out.contains("[dbo].[top]"), "{out}");
    }

    #[test]
    fn already_qualified_references_are_untouched() {
        let sql = "SELECT * FROM dbo.t WHERE x IN (SELECT y FROM other.u)";
        assert_eq!(qualify_sql(sql, Some("db"), Some("sales")), sql);
    }

    #[test]
    fn temp_tables_stay_unqualified() {
        let out = qualify_sql(
            "SELECT * FROM #tmp JOIN real_table r ON 1 = 1",
            None,
            Some("dbo"),
        );
        assert!(out.contains("#tmp"), "{out}");
        assert!(out.contains("[dbo].[real_table]"), "{out}");
    }

    #[test]
    fn bracket_quoting_preserves_quoted_idents() {
        // The pre-existing double-quoted identifier keeps its source form
        // (valid T-SQL); only the injected qualifier is bracketed.
        let out = qualify_sql("SELECT * FROM \"Users\"", None, Some("sales"));
        assert_eq!(out, "SELECT * FROM [sales].\"Users\"");
        let bracketed = qualify_sql("SELECT * FROM [Users]", None, Some("sales"));
        assert_eq!(bracketed, "SELECT * FROM [sales].[Users]");
    }

    #[test]
    fn string_literals_are_not_touched() {
        let out = qualify_sql(
            "SELECT * FROM logs WHERE msg = 'from users'",
            None,
            Some("dbo"),
        );
        assert!(out.contains("'from users'"), "{out}");
        assert!(!out.contains("'from [dbo"), "{out}");
    }

    #[test]
    fn insert_update_delete_ddl_qualify() {
        let cases = [
            ("INSERT INTO users (id) VALUES (1)", "[dbo].[users]"),
            ("UPDATE users SET a = 1", "[dbo].[users]"),
            ("DELETE FROM users", "[dbo].[users]"),
            ("TRUNCATE TABLE users", "[dbo].[users]"),
            ("CREATE TABLE users (id INT)", "[dbo].[users]"),
            ("DROP TABLE users", "[dbo].[users]"),
        ];
        for (sql, expect) in cases {
            let out = qualify_sql(sql, Some("db"), Some("dbo"));
            assert!(out.contains(expect), "{sql} -> {out}");
        }
    }

    #[test]
    fn rewrite_is_idempotent() {
        let once = qualify_sql(
            "SELECT * FROM users WHERE id IN (SELECT uid FROM admins)",
            Some("db"),
            Some("dbo"),
        );
        let twice = qualify_sql(&once, Some("db"), Some("dbo"));
        assert_eq!(once, twice);
    }

    #[test]
    fn parse_failure_passes_through_unchanged() {
        let broken = "SELEC * FORM users WHER";
        assert_eq!(qualify_sql(broken, Some("db"), Some("dbo")), broken);
    }

    #[test]
    fn database_only_target_is_not_inlined_two_part_name_is_ambiguous() {
        // `[db].t` would mean *schema* db — never emit it. The host session
        // pin (`USE db`) serves the database dimension.
        let sql = "SELECT * FROM users";
        assert_eq!(qualify_sql(sql, Some("other_db"), None), sql);
        assert_eq!(qualify_sql(sql, None, None), sql);
        assert_eq!(qualify_sql(sql, Some("  "), Some("   ")), sql);
    }
}
