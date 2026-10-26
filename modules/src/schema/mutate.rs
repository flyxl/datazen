//! Heuristics for deciding when SQL may change schema metadata.

/// Returns true when `sql` likely mutates schema (DDL), so cached columns/tables
/// for the connection should be invalidated.
pub fn sql_may_mutate_schema(sql: &str) -> bool {
    for stmt in sql.split(';') {
        let s = strip_leading_sql_noise(stmt);
        if s.is_empty() {
            continue;
        }
        let upper = s.to_ascii_uppercase();
        if upper.starts_with("CREATE ")
            || upper.starts_with("ALTER ")
            || upper.starts_with("DROP ")
            || upper.starts_with("TRUNCATE ")
            || upper.starts_with("RENAME ")
            || upper.starts_with("COMMENT ON")
        {
            return true;
        }
    }
    false
}

fn strip_leading_sql_noise(sql: &str) -> &str {
    let mut s = sql.trim_start();
    loop {
        if s.starts_with("--") {
            if let Some(rest) = s.split_once('\n') {
                s = rest.1.trim_start();
                continue;
            }
            return "";
        }
        if s.starts_with("/*") {
            if let Some(idx) = s.find("*/") {
                s = s[idx + 2..].trim_start();
                continue;
            }
            return "";
        }
        break;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_ddl() {
        assert!(sql_may_mutate_schema("CREATE TABLE t (id INT)"));
        assert!(sql_may_mutate_schema("alter table t add column x int"));
        assert!(sql_may_mutate_schema("DROP TABLE t"));
        assert!(sql_may_mutate_schema("TRUNCATE t"));
        assert!(sql_may_mutate_schema("RENAME TABLE a TO b"));
        assert!(sql_may_mutate_schema("COMMENT ON COLUMN t.x IS 'x'"));
    }

    #[test]
    fn ignores_dml_and_select() {
        assert!(!sql_may_mutate_schema("SELECT * FROM t"));
        assert!(!sql_may_mutate_schema("INSERT INTO t VALUES (1)"));
        assert!(!sql_may_mutate_schema("UPDATE t SET x = 1"));
        assert!(!sql_may_mutate_schema("DELETE FROM t"));
    }

    #[test]
    fn detects_ddl_after_comments_or_prior_statements() {
        assert!(sql_may_mutate_schema("-- note\nCREATE TABLE t (id INT)"));
        assert!(sql_may_mutate_schema("SELECT 1; DROP TABLE t"));
        assert!(sql_may_mutate_schema("/* c */ ALTER TABLE t ADD x INT"));
    }
}
