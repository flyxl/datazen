//! SQL batch framing and the default read-only guard.
//!
//! A batch is split with the same scanner the restore pipelines use, so a
//! semicolon inside a quoted string, dollar-quoted body, or comment never
//! changes the statement count. Statement-level permission still belongs to
//! the caller; this module only proves the frame and classifies each statement
//! with the lexical read-only predicate shared with the MCP policy.

use datazen_driver_api::sql_split::{is_comment_only_or_empty, split_sql_statements};

/// A batch is a bounded, explicit list of statements. This keeps a single tool
/// call from turning into an unbounded script runner.
pub const MAX_BATCH_STATEMENTS: usize = 50;

pub fn parse_sql_batch(sql: &str) -> Result<Vec<String>, String> {
    let statements: Vec<String> = split_sql_statements(sql)
        .into_iter()
        .map(|statement| statement.trim().to_owned())
        .filter(|statement| !statement.is_empty() && !is_comment_only_or_empty(statement))
        .collect();
    if statements.is_empty() {
        return Err("execute_batch requires at least one complete SQL statement".into());
    }
    if statements.len() > MAX_BATCH_STATEMENTS {
        return Err(format!(
            "execute_batch accepts at most {MAX_BATCH_STATEMENTS} statements (received {})",
            statements.len()
        ));
    }
    Ok(statements)
}

/// Indexes (zero-based) of statements that are not lexically read-only.
pub fn write_statement_indexes(statements: &[String]) -> Vec<usize> {
    statements
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            (!super::policy::is_read_only_sql(statement)).then_some(index)
        })
        .collect()
}

/// Reject a batch before any statement executes when writes were not explicitly
/// allowed. The error names the first offending statement so an MCP client can
/// correct the request without guessing.
pub fn ensure_default_read_only(statements: &[String]) -> Result<(), String> {
    let writes = write_statement_indexes(statements);
    if let Some(index) = writes.first() {
        return Err(format!(
            "execute_batch is read-only by default; statement {} is a write (set allow_writes=true and use an approval-capable permission mode to execute writes)",
            index + 1
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_respects_quotes_comments_and_dollar_quotes() {
        let statements =
            parse_sql_batch("SELECT 'a;b'; /* keep; me */ SELECT $$x;y$$; -- tail\nSELECT 3")
                .expect("valid batch");
        assert_eq!(
            statements,
            vec![
                "SELECT 'a;b'",
                "/* keep; me */ SELECT $$x;y$$",
                "-- tail\nSELECT 3"
            ]
        );
    }

    #[test]
    fn empty_and_comment_only_batches_are_rejected() {
        assert!(parse_sql_batch("  -- nothing\n/* here */ ").is_err());
        assert!(parse_sql_batch("").is_err());
    }

    #[test]
    fn write_indexes_use_the_shared_lexical_predicate() {
        let statements = parse_sql_batch(
            "SELECT 1; UPDATE t SET x = 1; SHOW TABLES; DELETE FROM t; EXPLAIN ANALYZE INSERT INTO t VALUES (1)",
        )
        .expect("valid batch");
        assert_eq!(write_statement_indexes(&statements), vec![1, 3, 4]);
        let error = ensure_default_read_only(&statements).unwrap_err();
        assert!(error.contains("statement 2"), "{error}");
    }

    #[test]
    fn statement_cap_is_enforced() {
        let sql = (0..=MAX_BATCH_STATEMENTS)
            .map(|i| format!("SELECT {i}"))
            .collect::<Vec<_>>()
            .join(";");
        assert!(parse_sql_batch(&sql).is_err());
    }
}
