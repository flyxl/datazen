//! F7: SQLite SQL target qualification binding.
//!
//! Dialect shape: `alias.t` — SQLite has no database/schema qualifier of its
//! own; a second database only becomes addressable after a manual
//! `ATTACH '<file>' AS <alias>`, and is then referenced as `<alias>.t`.
//!
//! **Default no-op.** A DataZen SQLite connection is a single file resolved to
//! the implicit `main` schema, and `get_databases` reports exactly `main`, so
//! there is normally nothing to inline: with `database == None` or `"main"`
//! (or blank) the SQL is returned unchanged. Only when an explicit target that
//! differs from `main` is requested does this binding qualify references as
//! `<target>.t` — useful for sessions where the caller ATTACHed another
//! database under that alias. If no such alias exists the server-side error
//! (`no such table: alias.t`) makes the mismatch visible instead of silently
//! reading the wrong data.
//!
//! Best-effort by contract: parse failures pass the original text through,
//! and the rewrite is idempotent.

use datazen_driver_api::{qualify_sql_with, QualifierQuote};
use sqlparser::dialect::SQLiteDialect;

/// Rewrite `sql` so unqualified table references land on the ATTACH `alias`.
pub(crate) fn qualify_sql(sql: &str, database: Option<&str>, schema: Option<&str>) -> String {
    let _ = schema; // SQLite has no schema dimension beyond ATTACH aliases.
    let target = database.map(str::trim).filter(|s| !s.is_empty());
    match target {
        // Default no-op: main / absent targets resolve naturally.
        None | Some("main") | Some("temp") => return sql.to_string(),
        // Connection configs store the SQLite file in `database`.  That value
        // is the backing file for the implicit `main` schema, not an ATTACH
        // alias, so never qualify SQL with a filesystem path.
        Some(path) if path.contains('/') || path.contains('\\') => return sql.to_string(),
        Some(alias) => {
            qualify_sql_with(
                &SQLiteDialect {},
                QualifierQuote::DoubleQuote,
                std::slice::from_ref(&alias),
                sql,
            )
            .sql
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_target_is_noop() {
        for db in [
            None,
            Some(""),
            Some("   "),
            Some("main"),
            Some("temp"),
            Some("/tmp/datazen/test.db"),
            Some(r"C:\\datazen\\test.db"),
        ] {
            assert_eq!(
                qualify_sql("SELECT * FROM users", db, None),
                "SELECT * FROM users",
                "db = {db:?} must stay untouched"
            );
        }
    }

    #[test]
    fn attached_alias_qualifies_simple_select() {
        assert_eq!(
            qualify_sql("SELECT * FROM users", Some("backup_db"), None),
            "SELECT * FROM \"backup_db\".\"users\""
        );
    }

    #[test]
    fn joins_and_subqueries_qualify() {
        let out = qualify_sql(
            "SELECT * FROM users u JOIN orders o ON o.uid = u.id WHERE id IN (SELECT uid FROM admins)",
            Some("stats"),
            None,
        );
        assert!(out.contains("\"stats\".\"users\" AS u"), "{out}");
        assert!(out.contains("\"stats\".\"orders\" AS o"), "{out}");
        assert!(
            out.contains("(SELECT uid FROM \"stats\".\"admins\")"),
            "{out}"
        );
    }

    #[test]
    fn cte_names_are_skipped() {
        let out = qualify_sql(
            "WITH t AS (SELECT 1) SELECT * FROM t JOIN real r ON true",
            Some("stats"),
            None,
        );
        assert!(out.contains("FROM t"), "{out}");
        assert!(out.contains("\"stats\".\"real\""), "{out}");
    }

    #[test]
    fn already_qualified_references_are_untouched() {
        let sql = "SELECT * FROM main.t WHERE x IN (SELECT y FROM other.u)";
        assert_eq!(qualify_sql(sql, Some("stats"), None), sql);
    }

    #[test]
    fn quoted_identifiers_are_preserved_and_qualified() {
        let out = qualify_sql("SELECT * FROM \"Users\"", Some("stats"), None);
        assert_eq!(out, "SELECT * FROM \"stats\".\"Users\"");
    }

    #[test]
    fn writes_and_ddl_qualify() {
        assert!(
            qualify_sql("INSERT INTO users (id) VALUES (1)", Some("stats"), None)
                .contains("INSERT INTO \"stats\".\"users\"")
        );
        assert!(qualify_sql("UPDATE users SET a = 1", Some("stats"), None)
            .contains("UPDATE \"stats\".\"users\" SET"));
        assert!(
            qualify_sql("DELETE FROM users WHERE id = 1", Some("stats"), None)
                .contains("DELETE FROM \"stats\".\"users\" WHERE")
        );
        assert!(
            qualify_sql("CREATE TABLE users (id INTEGER)", Some("stats"), None)
                .contains("CREATE TABLE \"stats\".\"users\"")
        );
        assert!(
            qualify_sql("ALTER TABLE users ADD COLUMN c TEXT", Some("stats"), None)
                .contains("ALTER TABLE \"stats\".\"users\"")
        );
        assert!(qualify_sql("DROP TABLE users", Some("stats"), None)
            .contains("DROP TABLE \"stats\".\"users\""));
    }

    #[test]
    fn rewrite_is_idempotent() {
        let once = qualify_sql(
            "SELECT * FROM users WHERE id IN (SELECT uid FROM admins)",
            Some("stats"),
            None,
        );
        let twice = qualify_sql(&once, Some("stats"), None);
        assert_eq!(once, twice);
    }

    #[test]
    fn parse_failure_passes_through_unchanged() {
        let broken = "SELEC * FORM users WHER";
        assert_eq!(qualify_sql(broken, Some("stats"), None), broken);
    }
}
