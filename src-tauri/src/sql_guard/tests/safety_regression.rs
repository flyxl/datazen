// Safety regression tests (included from mod.rs).

#[test]
fn read_only_blocks_update() {
    let err = check_sql("UPDATE t SET x = 1 WHERE id = 1", true, false).unwrap_err();
    assert!(err.contains("read-only"));
}

#[test]
fn read_only_allows_select() {
    assert!(check_sql("SELECT * FROM t", true, true).is_ok());
    assert!(check_sql("WITH c AS (SELECT 1) SELECT * FROM c", true, true).is_ok());
}

#[test]
fn safe_mode_blocks_update_without_where() {
    let err = check_sql("UPDATE users SET name = 'x'", false, true).unwrap_err();
    assert!(err.contains("WHERE"));
}

#[test]
fn safe_mode_allows_update_with_where() {
    assert!(check_sql("UPDATE users SET name = 'x' WHERE id = 1", false, true).is_ok());
}

#[test]
fn safe_mode_ignores_where_inside_subquery() {
    let err = check_sql(
        "UPDATE t SET x = (SELECT y FROM u WHERE id = 1)",
        false,
        true,
    )
    .unwrap_err();
    assert!(err.contains("WHERE"));
}

#[test]
fn safe_mode_blocks_delete_without_where() {
    assert!(check_sql("DELETE FROM t", false, true).is_err());
    assert!(check_sql("DELETE FROM t WHERE id = 1", false, true).is_ok());
}

#[test]
fn safe_mode_blocks_truncate() {
    let err = check_sql("TRUNCATE TABLE t", false, true).unwrap_err();
    assert!(err.contains("TRUNCATE"));
}

#[test]
fn safe_mode_blocks_drop() {
    for sql in [
        "DROP TABLE t",
        "DROP VIEW v",
        "DROP INDEX idx",
        "DROP TABLE IF EXISTS t",
    ] {
        let err = check_sql(sql, false, true).unwrap_err();
        assert!(err.contains("DROP"), "expected DROP block for {sql}: {err}");
    }
    assert!(check_sql("DROP TABLE t", false, false).is_ok());
}

#[test]
fn disabled_guards_allow_writes() {
    assert!(check_sql("UPDATE t SET x = 1", false, false).is_ok());
    assert!(check_sql("DELETE FROM t", false, false).is_ok());
    assert!(check_sql("DROP TABLE t", false, false).is_ok());
    assert!(check_sql("TRUNCATE TABLE t", false, false).is_ok());
}

#[test]
fn read_only_blocks_grant_and_ddl() {
    assert!(check_sql("GRANT SELECT ON t TO u", true, false).is_err());
    assert!(check_sql("CREATE TABLE t (id int)", true, false).is_err());
    assert!(check_sql("DROP TABLE t", true, false).is_err());
}

#[test]
fn safe_mode_allows_insert_and_grant_without_where() {
    assert!(check_sql("INSERT INTO t VALUES (1)", false, true).is_ok());
    assert!(check_sql("GRANT SELECT ON t TO u", false, true).is_ok());
}

#[test]
fn comments_and_strings_are_not_statements() {
    assert!(check_sql("-- UPDATE t SET x = 1\nSELECT 1", true, true).is_ok());
    assert!(check_sql("SELECT 'UPDATE t' FROM dual", true, true).is_ok());
    assert!(check_sql("/* DELETE FROM t */ SELECT 1", true, true).is_ok());
}

#[test]
fn where_at_end_of_statement_counts() {
    assert!(check_sql("DELETE FROM t WHERE id = 1", false, true).is_ok());
    assert!(check_sql("UPDATE t SET x = 1 WHERE", false, true).is_ok());
}

#[test]
fn is_write_sql_detects_verbs() {
    assert!(is_write_sql("UPDATE t SET x = 1"));
    assert!(!is_write_sql("SELECT 1"));
    assert!(!is_write_sql("-- comment only"));
}

#[test]
fn split_respects_semicolons_inside_strings() {
    assert!(check_sql("SELECT 'a; UPDATE t'; SELECT 1", true, true).is_ok());
}

#[test]
fn read_only_blocks_insert_and_delete() {
    assert!(check_sql("INSERT INTO t VALUES (1)", true, false).is_err());
    assert!(check_sql("DELETE FROM t WHERE id = 1", true, false).is_err());
}

#[test]
fn mixed_statements_read_only_blocks_any_write() {
    let err = check_sql("SELECT 1; UPDATE t SET x = 1 WHERE id = 1", true, false).unwrap_err();
    assert!(err.contains("read-only"));
    let err = check_sql("SELECT 1; INSERT INTO t VALUES (1)", true, false).unwrap_err();
    assert!(err.contains("read-only"));
}

#[test]
fn mixed_statements_safe_mode_blocks_drop_and_truncate() {
    for sql in [
        "SELECT 1; DROP TABLE t",
        "SELECT 1; TRUNCATE TABLE t",
        "DROP TABLE t; SELECT 1",
    ] {
        let err = check_sql(sql, false, true).unwrap_err();
        assert!(
            err.contains("DROP") || err.contains("TRUNCATE"),
            "expected block for {sql}: {err}"
        );
    }
}

#[test]
fn mixed_statements_safe_mode_blocks_update_without_where() {
    let err = check_sql("SELECT 1; UPDATE t SET x = 1", false, true).unwrap_err();
    assert!(err.contains("WHERE"));
    let err = check_sql("DELETE FROM t; SELECT 1", false, true).unwrap_err();
    assert!(err.contains("WHERE"));
}

#[test]
fn mixed_statements_safe_mode_allows_safe_writes() {
    assert!(check_sql("SELECT 1; SELECT 2", false, true).is_ok());
    assert!(check_sql(
        "SELECT 1; UPDATE t SET x = 1 WHERE id = 1; DELETE FROM t WHERE id = 2",
        false,
        true
    )
    .is_ok());
}

#[test]
fn read_only_and_safe_mode_both_apply() {
    assert!(check_sql("INSERT INTO t VALUES (1)", true, true).is_err());
    assert!(check_sql("DROP TABLE t", true, true).is_err());
    assert!(check_sql("UPDATE t SET x = 1", true, true).is_err());
}

#[test]
fn test_tester_inline_block_comment_inside_drop_is_intercepted() {
    assert!(check_sql("DROP/**/TABLE t", false, true).is_err());
    assert!(check_sql("TRUNCATE/**/TABLE t", false, true).is_err());
}

#[test]
fn test_tester_nested_block_comment_with_drop_still_blocked() {
    assert!(check_sql("/* outer /* inner */ DROP TABLE t */", false, true).is_err());
}

#[test]
fn test_tester_drop_only_in_leading_comment_is_allowed() {
    assert!(check_sql("/* DROP TABLE t */ SELECT 1", false, true).is_ok());
    assert!(check_sql("-- DROP TABLE t\nSELECT 1", false, true).is_ok());
}

#[test]
fn test_tester_read_only_inline_comment_drop_is_intercepted() {
    assert!(check_sql("DROP/**/TABLE t", true, false).is_err());
}

#[test]
fn test_tester_unicode_fullwidth_drop_is_intercepted() {
    assert!(check_sql("ＤＲＯＰ TABLE t", false, true).is_err());
    assert!(check_sql("ＤＲＯＰ TABLE t", true, false).is_err());
}

#[test]
fn test_tester_null_byte_is_rejected() {
    assert!(check_sql("DROP\u{0000}TABLE t", false, true).is_err());
    assert!(check_sql("DROP\u{0000}TABLE t", true, false).is_err());
}

#[test]
fn test_tester_control_chars_in_select_do_not_crash() {
    assert!(check_sql("SELECT\u{0001}1", true, true).is_ok());
}

#[test]
fn test_tester_mixed_case_and_extra_whitespace_drop_blocked() {
    assert!(check_sql("  \n  DrOp  \t  TaBlE t", false, true).is_err());
    assert!(check_sql("  truncate   table   t  ", false, true).is_err());
}

#[test]
fn test_tester_drop_keyword_split_across_comment_is_intercepted() {
    assert!(check_sql("/* DROP */ TABLE t", false, true).is_err());
}

#[test]
fn test_harden_fullwidth_drop_read_only() {
    assert!(check_sql("ＤＲＯＰ TABLE t", true, false).is_err());
    assert!(check_sql("ＤＲＯＰ TABLE t", true, true).is_err());
}

#[test]
fn test_harden_fullwidth_update_safe_mode() {
    assert!(check_sql("ＵＰＤＡＴＥ t SET x = 1", false, true).is_err());
}

#[test]
fn test_harden_null_byte_variants() {
    assert!(check_sql("DROP\u{0}TABLE t", false, true).is_err());
    assert!(check_sql("SELECT\u{0}1", true, true).is_err());
    assert!(check_sql("UPDATE\u{0}t SET x=1", true, false).is_err());
}

#[test]
fn test_harden_comment_strip_update_needs_where() {
    assert!(check_sql("/* UPDATE */ DELETE FROM t", false, true).is_err());
    assert!(check_sql("/* UPDATE */ DELETE FROM t WHERE id=1", false, true).is_ok());
}

#[test]
fn test_harden_comment_strip_interleaved_with_real_sql() {
    assert!(check_sql("SELECT 1; DROP/**/TABLE t", false, true).is_err());
    assert!(check_sql("SELECT 1; /* DROP */ TABLE t", false, true).is_err());
}

// ---------------------------------------------------------------------------
// Shared risk fixture: guarantees the frontend assessor (classifyRisk /
// assessExecutionRisk) and the Host guard agree on the same SQL — no drift.
// ---------------------------------------------------------------------------

#[test]
fn test_tester_shared_risk_fixture_no_drift() {
    let raw = include_str!("../../../../src/lib/__tests__/fixtures/sqlRiskCases.json");
    let fixture: JsonValue = serde_json::from_str(raw).expect("risk fixture JSON");
    let cases = fixture["cases"].as_array().expect("cases array");
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let sql = case["sql"].as_str().unwrap();
        let read_only = case["mode"]["readOnly"].as_bool().unwrap();
        let safe_mode = case["mode"]["safeMode"].as_bool().unwrap();
        let guard_allowed = case["guardAllowed"].as_bool().unwrap();
        let got = check_sql(sql, read_only, safe_mode).is_ok();
        assert_eq!(
            got, guard_allowed,
            "semantic drift on shared fixture case '{id}' for {sql:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// binder -> guard order (execute.rs and streaming.rs both apply params first,
// then check_sql). These prove the guard sees the *bound* SQL, so a mutation
// that a bound value cannot hide still needs its WHERE / stays read-only.
// ---------------------------------------------------------------------------

#[test]
fn test_tester_bind_then_guard_blocks_no_where_after_binding() {
    let params = json!({
        "version": 2,
        "values": { "question:1": "v" },
        "occurrences": [{ "from": 17, "to": 18, "id": "question:1", "token": "?" }]
    });
    let bound = apply_params("UPDATE t SET x = ?", &params).unwrap();
    assert_eq!(bound, "UPDATE t SET x = 'v'");
    let err = check_sql(&bound, false, true).unwrap_err();
    assert!(err.contains("WHERE"), "bound UPDATE without WHERE must be blocked: {err}");
}

#[test]
fn test_tester_bind_then_guard_shared_by_execute_and_stream_paths() {
    let params = json!({
        "version": 2,
        "values": { "question:1": 1 },
        "occurrences": [{ "from": 25, "to": 26, "id": "question:1", "token": "?" }]
    });
    let bound = apply_params("DELETE FROM t WHERE id = ?", &params).unwrap();
    assert_eq!(bound, "DELETE FROM t WHERE id = 1");
    // read_only blocks DELETE on both the execute and the stream path after binding.
    let err = check_sql(&bound, true, false).unwrap_err();
    assert!(err.contains("read-only"), "bound DELETE must stay blocked read-only: {err}");
    // The same bound SQL is allowed when guards are off, so the block is strictly
    // the guard, never the binder.
    assert!(check_sql(&bound, false, false).is_ok());
}

#[test]
fn test_tester_safe_mode_blocks_where_hidden_in_block_comment() {
    let err =
        check_sql("UPDATE t SET x = 1 /* WHERE g */", false, true).unwrap_err();
    assert!(
        err.contains("WHERE"),
        "block-comment WHERE must not bypass Safe Mode: {err}"
    );
}

#[test]
fn test_tester_safe_mode_blocks_where_hidden_in_line_comment() {
    let err = check_sql("DELETE FROM t -- WHERE g", false, true).unwrap_err();
    assert!(
        err.contains("WHERE"),
        "line-comment WHERE must not bypass Safe Mode: {err}"
    );
}

#[test]
fn test_tester_safe_mode_blocks_where_hidden_in_hash_comment() {
    let err = check_sql("UPDATE t SET x = 1 # WHERE g", false, true).unwrap_err();
    assert!(
        err.contains("WHERE"),
        "hash-comment WHERE must not bypass Safe Mode: {err}"
    );
}

#[test]
fn test_tester_safe_mode_blocks_where_hidden_in_dollar_quote() {
    let err = check_sql("UPDATE t SET x = $q$WHERE$q$", false, true).unwrap_err();
    assert!(
        err.contains("WHERE"),
        "dollar-quote WHERE must not bypass Safe Mode: {err}"
    );
}

#[test]
fn test_tester_safe_mode_blocks_where_hidden_in_hash_comment_delete() {
    let err = check_sql("DELETE FROM t # WHERE g", false, true).unwrap_err();
    assert!(
        err.contains("WHERE"),
        "hash-comment WHERE in DELETE must not bypass Safe Mode: {err}"
    );
}

#[test]
fn test_tester_safe_mode_blocks_where_hidden_in_dollar_quote_named_tag() {
    let err = check_sql("UPDATE t SET x = $body$WHERE$body$", false, true).unwrap_err();
    assert!(
        err.contains("WHERE"),
        "named-tag dollar-quote WHERE must not bypass Safe Mode: {err}"
    );
}

#[test]
fn test_tester_safe_mode_blocks_where_hidden_in_dollar_quote_delete() {
    let err = check_sql("DELETE FROM t $q$WHERE$q$", false, true).unwrap_err();
    assert!(
        err.contains("WHERE"),
        "dollar-quote WHERE in DELETE must not bypass Safe Mode: {err}"
    );
}

#[test]
fn test_tester_safe_mode_allows_real_where_with_dollar_quote_value() {
    // A genuine top-level WHERE followed by a dollar-quoted value must still
    // pass the Safe Mode gate (positive control — no false positive).
    assert!(check_sql("DELETE FROM t WHERE id = $q$WHERE$q$", false, true).is_ok());
}

#[test]
fn test_tester_hash_inside_string_is_not_comment() {
    // `#` inside a quoted string is not a comment; the real WHERE must survive.
    assert!(check_sql("UPDATE t SET x = 'a#b' WHERE id = 1", false, true).is_ok());
}

#[test]
fn test_tester_dollar_quote_inside_string_is_not_dollar_quote() {
    // `$q$` inside a quoted string is not a dollar-quote opener; real WHERE survives.
    assert!(check_sql("UPDATE t SET x = 'a$q$b' WHERE id = 1", false, true).is_ok());
}
