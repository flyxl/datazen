use super::*;

fn only(sql: &str) -> SqlStatementRisk {
    let assessment = classify_risk(sql);
    assert_eq!(
        assessment.statements.len(),
        1,
        "expected one statement for {sql:?}"
    );
    assessment.statements.into_iter().next().unwrap()
}

fn finding_kinds(assessment: &SqlRiskAssessment) -> Vec<&'static str> {
    assessment
        .findings
        .iter()
        .map(|finding| finding.kind.as_str())
        .collect()
}

#[test]
fn read_write_and_unknown_verbs_classify_as_expected() {
    assert_eq!(
        classify_risk("SELECT 1").classification,
        SqlRiskClassification::Read
    );
    assert_eq!(
        classify_risk("INSERT INTO t VALUES (1)").classification,
        SqlRiskClassification::Mutation
    );
    assert_eq!(
        classify_risk("PRAGMA table_info(t)").classification,
        SqlRiskClassification::Read
    );
    assert_eq!(
        classify_risk("FOO bar").classification,
        SqlRiskClassification::Unknown
    );
    assert_eq!(classify_risk("   ").statements.len(), 0);
    assert_eq!(
        classify_risk("   ").classification,
        SqlRiskClassification::Read
    );
    assert_eq!(only("?").verb, None);
    assert_eq!(only("?").classification, SqlRiskClassification::Unknown);
}

#[test]
fn select_carries_no_findings() {
    let statement = only("SELECT 1");
    assert_eq!(statement.verb.as_deref(), Some("SELECT"));
    assert_eq!(statement.classification, SqlRiskClassification::Read);
    assert!(statement.findings.is_empty());
    assert!(!statement.has_top_level_where);
    assert!(!statement.strict_write);
    assert!(!statement.comment_hides_write_verb);
}

#[test]
fn update_and_delete_without_where_produce_no_where_findings() {
    let update = only("UPDATE t SET a=1");
    assert_eq!(update.verb.as_deref(), Some("UPDATE"));
    assert_eq!(update.classification, SqlRiskClassification::Mutation);
    assert_eq!(
        finding_kinds(&classify_risk("UPDATE t SET a=1")),
        ["no-where"]
    );
    assert!(!update.has_top_level_where);

    let delete = only("DELETE FROM t");
    assert_eq!(delete.verb.as_deref(), Some("DELETE"));
    assert_eq!(finding_kinds(&classify_risk("DELETE FROM t")), ["no-where"]);

    assert!(only("UPDATE t SET a=1 WHERE id=1").has_top_level_where);
    assert!(only("DELETE FROM t WHERE id=1").has_top_level_where);
    assert!(classify_risk("UPDATE t SET a=1 WHERE id=1")
        .findings
        .is_empty());
    assert!(classify_risk("DELETE FROM t WHERE id=1")
        .findings
        .is_empty());
}

#[test]
fn drop_and_truncate_produce_destructive_findings() {
    let drop = only("DROP TABLE t");
    assert_eq!(drop.verb.as_deref(), Some("DROP"));
    assert!(drop.strict_write);
    assert_eq!(finding_kinds(&classify_risk("DROP TABLE t")), ["drop"]);
    let truncate = only("TRUNCATE TABLE t");
    assert_eq!(truncate.verb.as_deref(), Some("TRUNCATE"));
    assert_eq!(
        finding_kinds(&classify_risk("TRUNCATE TABLE t")),
        ["truncate"]
    );
}

#[test]
fn keywords_inside_literals_are_not_main_verbs() {
    for sql in [
        "SELECT 'DROP TABLE x'",
        "SELECT \"UPDATE t\"",
        "SELECT [DELETE] FROM t",
        "SELECT `DROP` FROM t",
        "SELECT $tag$TRUNCATE$tag$",
        "SELECT 'a; DROP TABLE x'",
    ] {
        let statement = only(sql);
        assert_eq!(statement.verb.as_deref(), Some("SELECT"), "{sql:?}");
        assert_eq!(
            statement.classification,
            SqlRiskClassification::Read,
            "{sql:?}"
        );
        assert!(statement.findings.is_empty(), "{sql:?}");
        assert!(!statement.comment_hides_write_verb, "{sql:?}");
    }
}

#[test]
fn block_comment_with_write_verb_is_flagged_but_not_a_finding() {
    let statement = only("SELECT 1 /* DROP TABLE x */");
    assert_eq!(statement.verb.as_deref(), Some("SELECT"));
    assert_eq!(statement.classification, SqlRiskClassification::Read);
    assert!(statement.findings.is_empty());
    assert!(statement.comment_hides_write_verb);

    let leading = only("/* INSERT INTO t */ SELECT 1");
    assert!(leading.comment_hides_write_verb);
    assert_eq!(leading.classification, SqlRiskClassification::Read);

    let line = only("SELECT 1 -- DROP TABLE x\nSELECT 2");
    assert!(!line.comment_hides_write_verb);
    assert!(line.findings.is_empty());
}

#[test]
fn semicolons_in_literals_comments_and_parens_do_not_split() {
    assert_eq!(classify_risk("SELECT 'a;b'; SELECT 2").statements.len(), 2);
    assert_eq!(classify_risk("SELECT 1 /* a;b */").statements.len(), 1);
    assert_eq!(classify_risk("SELECT (1;2)").statements.len(), 1);
    let multi = classify_risk("SELECT 1; DROP TABLE t; SELECT 2");
    assert_eq!(multi.statements.len(), 3);
    assert_eq!(finding_kinds(&multi), ["drop"]);
    assert_eq!(multi.findings[0].statement_index, 1);
    assert_eq!(multi.findings[0].verb, "DROP");
    assert!(multi.statements[1].range.from > multi.statements[0].range.from);
    assert!(multi.statements[2].range.to > multi.statements[1].range.to);
}

#[test]
fn aggregate_classification_uses_mutation_then_unknown_precedence() {
    assert_eq!(
        classify_risk("SELECT 1; FOO x").classification,
        SqlRiskClassification::Unknown
    );
    assert_eq!(
        classify_risk("FOO x; SELECT 1").classification,
        SqlRiskClassification::Unknown
    );
    assert_eq!(
        classify_risk("SELECT 1; DELETE FROM t").classification,
        SqlRiskClassification::Mutation
    );
    assert_eq!(
        classify_risk("DROP TABLE t; FOO x").classification,
        SqlRiskClassification::Mutation
    );
}

#[test]
fn with_clause_skips_to_the_main_verb() {
    assert_eq!(
        only("WITH c AS (SELECT 1) SELECT * FROM c").verb.as_deref(),
        Some("SELECT")
    );
    let delete = only("WITH c AS (SELECT 1) DELETE FROM t");
    assert_eq!(delete.verb.as_deref(), Some("DELETE"));
    assert_eq!(
        finding_kinds(&classify_risk("WITH c AS (SELECT 1) DELETE FROM t")),
        ["no-where"]
    );
    assert_eq!(
        finding_kinds(&classify_risk("WITH c AS (SELECT 1) DROP TABLE t")),
        ["drop"]
    );
    let unknown = only("WITH c AS (SELECT 1) FOO");
    assert_eq!(unknown.verb, None);
    assert_eq!(unknown.classification, SqlRiskClassification::Unknown);
}

#[test]
fn data_modifying_ctes_force_mutation() {
    for sql in [
        "WITH c AS (DELETE FROM t) SELECT 1",
        "WITH c AS NOT MATERIALIZED (DELETE FROM t) SELECT 1",
        "WITH c AS MATERIALIZED (INSERT INTO t VALUES (1)) SELECT 1",
        "WITH c AS (UPDATE t SET a=1) SELECT 1",
    ] {
        let statement = only(sql);
        assert_eq!(statement.verb.as_deref(), Some("SELECT"), "{sql:?}");
        assert!(statement.strict_write, "{sql:?}");
        assert_eq!(
            statement.classification,
            SqlRiskClassification::Mutation,
            "{sql:?}"
        );
        assert!(statement.findings.is_empty(), "{sql:?}");
    }
    let read_cte = only("WITH c AS (SELECT 1) SELECT * FROM c");
    assert!(!read_cte.strict_write);
}

#[test]
fn select_into_is_strict_write_at_any_matching_depth() {
    let top = only("SELECT * INTO new FROM old");
    assert_eq!(top.verb.as_deref(), Some("SELECT"));
    assert!(top.strict_write);
    assert_eq!(top.classification, SqlRiskClassification::Mutation);
    assert!(top.findings.is_empty());
    let nested = only("SELECT * FROM (SELECT 1 INTO x)");
    assert!(nested.strict_write);
    assert_eq!(nested.classification, SqlRiskClassification::Mutation);
    let plain = only("SELECT * FROM t");
    assert!(!plain.strict_write);
}

#[test]
fn explain_only_analyzes_statements_when_analyze_is_on() {
    let plan_only = only("EXPLAIN SELECT 1");
    assert_eq!(plan_only.verb.as_deref(), Some("EXPLAIN"));
    assert!(!plan_only.strict_write);
    assert_eq!(plan_only.classification, SqlRiskClassification::Read);

    for sql in [
        "EXPLAIN ANALYZE DELETE FROM t",
        "EXPLAIN ANALYZE VERBOSE DELETE FROM t",
        "EXPLAIN VERBOSE ANALYZE DELETE FROM t",
        "EXPLAIN ANALYSE UPDATE t SET a=1",
        "EXPLAIN (ANALYZE TRUE) DELETE FROM t",
        "EXPLAIN (VERBOSE, ANALYZE) DELETE FROM t",
        "EXPLAIN (ANALYZE) DELETE FROM t",
    ] {
        let statement = only(sql);
        assert_eq!(statement.verb.as_deref(), Some("EXPLAIN"), "{sql:?}");
        assert!(statement.strict_write, "{sql:?}");
        assert_eq!(
            statement.classification,
            SqlRiskClassification::Mutation,
            "{sql:?}"
        );
        assert!(statement.findings.is_empty(), "{sql:?}");
        assert!(!classify_risk(sql).has_high_risk, "{sql:?}");
    }
}

#[test]
fn explain_analyze_off_or_absent_stays_read_only() {
    for sql in [
        "EXPLAIN ANALYZE FALSE DELETE FROM t",
        "EXPLAIN ANALYZE OFF DELETE FROM t",
        "EXPLAIN ANALYZE 0 DELETE FROM t",
        "EXPLAIN (ANALYZE OFF) DELETE FROM t",
        "EXPLAIN (ANALYZE NO) DELETE FROM t",
        "EXPLAIN (VERBOSE) DELETE FROM t",
        "EXPLAIN",
    ] {
        let statement = only(sql);
        assert!(!statement.strict_write, "{sql:?}");
        assert_eq!(
            statement.classification,
            SqlRiskClassification::Read,
            "{sql:?}"
        );
    }
}

#[test]
fn explain_analyze_on_marks_may_write_even_for_a_select() {
    let statement = only("EXPLAIN (ANALYZE) SELECT 1");
    assert_eq!(statement.verb.as_deref(), Some("EXPLAIN"));
    assert!(!statement.strict_write);
    assert_eq!(statement.classification, SqlRiskClassification::Mutation);
    assert!(statement.findings.is_empty());
    assert!(!classify_risk("EXPLAIN (ANALYZE) SELECT 1").has_high_risk);
}

#[test]
fn where_inside_parens_or_literals_is_not_top_level() {
    let nested = only("UPDATE t SET a=(SELECT 1 WHERE x=1)");
    assert!(!nested.has_top_level_where);
    assert_eq!(
        finding_kinds(&classify_risk("UPDATE t SET a=(SELECT 1 WHERE x=1)")),
        ["no-where"]
    );
    let quoted = only("DELETE FROM \"WHERE\"");
    assert!(!quoted.has_top_level_where);
    assert_eq!(
        finding_kinds(&classify_risk("DELETE FROM \"WHERE\"")),
        ["no-where"]
    );
    let top = only("SELECT 1 WHERE 1=1");
    assert!(top.has_top_level_where);
}

#[test]
fn fullwidth_verbs_are_normalized_before_classification() {
    let drop = only("ＤＲＯＰ TABLE t");
    assert_eq!(drop.verb.as_deref(), Some("DROP"));
    assert_eq!(finding_kinds(&classify_risk("ＤＲＯＰ TABLE t")), ["drop"]);
    let select = only("ＳＥＬＥＣＴ 1");
    assert_eq!(select.verb.as_deref(), Some("SELECT"));
    assert_eq!(select.classification, SqlRiskClassification::Read);
}

#[test]
fn finding_offsets_point_at_the_triggering_verb() {
    let finding = &only("  DROP TABLE t").findings[0];
    assert_eq!(finding.kind, SqlRiskFindingType::Drop);
    assert_eq!(finding.from, 2);
    assert_eq!(finding.to, 6);
    assert_eq!(finding.verb, "DROP");
    let multi = classify_risk("SELECT 1; UPDATE t SET a=1");
    assert_eq!(multi.findings[0].verb, "UPDATE");
    assert_eq!(multi.findings[0].statement_index, 1);
    let range = only("  SELECT 1  ").range;
    assert_eq!(range.from, 2);
    assert_eq!(range.to, 10);
}

#[test]
fn back_compat_helpers_only_detect_drop_and_truncate() {
    assert!(is_dangerous_write_statement("DROP TABLE t"));
    assert!(is_dangerous_write_statement("TRUNCATE TABLE t"));
    assert!(sql_contains_dangerous_write("SELECT 1; TRUNCATE TABLE x"));
    assert!(!is_dangerous_write_statement("UPDATE t SET a=1"));
    assert!(!is_dangerous_write_statement("SELECT 'DROP TABLE x'"));
    assert!(!sql_contains_dangerous_write("   "));
}

#[test]
fn enum_wire_literals_are_pinned() {
    assert_eq!(SqlRiskClassification::Read.as_str(), "read");
    assert_eq!(SqlRiskClassification::Mutation.as_str(), "mutation");
    assert_eq!(SqlRiskClassification::Unknown.as_str(), "unknown");
    assert_eq!(SqlRiskFindingType::Drop.as_str(), "drop");
    assert_eq!(SqlRiskFindingType::Truncate.as_str(), "truncate");
    assert_eq!(SqlRiskFindingType::NoWhere.as_str(), "no-where");
}
