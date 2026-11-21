pub fn strip_sql_noise(sql: &str) -> String {
    crate::mask_semicolons_in_literals(sql)
}

fn normalize_stmt_head(statement: &str) -> String {
    statement
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn is_begin_stmt(head: &str) -> bool {
    head == "BEGIN" || head.starts_with("BEGIN ") || head.starts_with("START TRANSACTION")
}

fn is_commit_stmt(head: &str) -> bool {
    head == "COMMIT" || head.starts_with("COMMIT ") || head == "END" || head.starts_with("END ")
}

fn is_rollback_stmt(head: &str) -> bool {
    if !head.starts_with("ROLLBACK") {
        return false;
    }
    !(head == "ROLLBACK TO" || head.starts_with("ROLLBACK TO "))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransactionSqlAnalysis {
    pub has_unclosed_begin: bool,
    pub begin_count: usize,
    pub end_count: usize,
    pub depth: usize,
}

pub fn analyze_transaction_sql(sql: &str) -> TransactionSqlAnalysis {
    let mut depth = 0usize;
    let mut begin_count = 0usize;
    let mut end_count = 0usize;

    for statement in crate::split_sql_statements(sql) {
        let head = normalize_stmt_head(&statement);
        if is_begin_stmt(&head) {
            begin_count += 1;
            depth += 1;
        } else if is_commit_stmt(&head) || is_rollback_stmt(&head) {
            end_count += 1;
            depth = depth.saturating_sub(1);
        }
    }

    TransactionSqlAnalysis {
        has_unclosed_begin: depth > 0,
        begin_count,
        end_count,
        depth,
    }
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn has_chinese_abort(message: &str) -> bool {
    if contains_ci(message, "直到事务块结束") || contains_ci(message, "当前事务已中止")
    {
        return true;
    }
    for line in message.lines() {
        let mut offset = 0usize;
        while let Some(found) = line[offset..].find("事务") {
            let after = offset + found + "事务".len();
            if line[after..].contains("中止") {
                return true;
            }
            offset = after;
        }
    }
    false
}

pub fn is_aborted_transaction_error(message: Option<&str>) -> bool {
    let message = match message {
        Some(message) if !message.is_empty() => message,
        _ => return false,
    };
    contains_ci(message, "current transaction is aborted")
        || contains_ci(message, "current transaction is in error")
        || contains_ci(message, "commands ignored until end of transaction block")
        || contains_ci(message, "25P02")
        || has_chinese_abort(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_noise_masks_semicolons_in_literals_and_comments() {
        assert_eq!(
            strip_sql_noise("select ';' from t -- ;\n"),
            crate::mask_semicolons_in_literals("select ';' from t -- ;\n")
        );
        assert!(!strip_sql_noise("select 'a;b'").contains("a;b"));
    }

    #[test]
    fn splitting_reuses_the_shared_statement_splitter() {
        let sql = "select 1; select 'a;b'; select 2";
        assert_eq!(
            crate::split_sql_statements(sql),
            vec!["select 1", "select 'a b'", "select 2"]
        );
    }

    #[test]
    fn begin_and_end_keywords_drive_the_depth() {
        let analysis = analyze_transaction_sql(
            "BEGIN; INSERT INTO t VALUES (1); COMMIT; START TRANSACTION; END;",
        );
        assert_eq!(analysis.begin_count, 2);
        assert_eq!(analysis.end_count, 2);
        assert_eq!(analysis.depth, 0);
        assert!(!analysis.has_unclosed_begin);
    }

    #[test]
    fn unclosed_begin_is_detected() {
        let analysis = analyze_transaction_sql("begin; insert into t values (1);");
        assert_eq!(analysis.begin_count, 1);
        assert_eq!(analysis.end_count, 0);
        assert_eq!(analysis.depth, 1);
        assert!(analysis.has_unclosed_begin);
    }

    #[test]
    fn rollback_to_savepoint_is_not_an_end_and_extra_ends_do_not_underflow() {
        let analysis = analyze_transaction_sql("BEGIN; ROLLBACK TO SAVEPOINT s; COMMIT; ROLLBACK;");
        assert_eq!(analysis.begin_count, 1);
        assert_eq!(analysis.end_count, 2);
        assert_eq!(analysis.depth, 0);

        let extra = analyze_transaction_sql("COMMIT; ROLLBACK;");
        assert_eq!(extra.end_count, 2);
        assert_eq!(extra.depth, 0);
    }

    #[test]
    fn statement_heads_ignore_case_and_extra_whitespace() {
        let analysis = analyze_transaction_sql("  begin   transaction ;\n end  ;");
        assert_eq!(analysis.begin_count, 1);
        assert_eq!(analysis.end_count, 1);
    }

    #[test]
    fn aborted_transaction_error_matches_all_four_families() {
        assert!(is_aborted_transaction_error(Some(
            "ERROR: current transaction is aborted"
        )));
        assert!(is_aborted_transaction_error(Some(
            "CURRENT TRANSACTION IS IN ERROR"
        )));
        assert!(is_aborted_transaction_error(Some(
            "commands ignored until end of transaction block"
        )));
        assert!(is_aborted_transaction_error(Some("SQLSTATE 25P02")));
        assert!(is_aborted_transaction_error(Some("当前事务已中止")));
        assert!(is_aborted_transaction_error(Some("事务已经中止")));
        assert!(is_aborted_transaction_error(Some("直到事务块结束")));
    }

    #[test]
    fn aborted_transaction_error_rejects_empty_and_unrelated_text() {
        assert!(!is_aborted_transaction_error(None));
        assert!(!is_aborted_transaction_error(Some("")));
        assert!(!is_aborted_transaction_error(Some(
            "syntax error at or near"
        )));
        assert!(!is_aborted_transaction_error(Some("中止事务")));
        assert!(!is_aborted_transaction_error(Some("ROLLBACK TO SAVEPOINT")));
    }
}
