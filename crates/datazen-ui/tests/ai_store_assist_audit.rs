//! Independent audit of T087-S2 (NL2SQL / diagnosis / EXPLAIN / filter).
//!
//! The React branches under test, with fixtures the coder did not use:
//! * whitespace-only `generateSql` returns before `set`, so a running
//!   request id is left alone;
//! * a failed generate keeps that request id and does not clear `input`;
//! * `parseFilter` on an empty string returns null without clearing
//!   filters already on the store;
//! * `clearNl2Sql` does not touch the diagnosis slot.

use datazen_ui::ai_store::{AiStore, DiagnosisResult, ExplainAnalysis, FilterCondition};

#[test]
fn audit_blank_generate_does_not_clobber_an_in_flight_request() {
    let mut store = AiStore::new();
    store.set_nl2sql_input("explain this query");
    assert!(store.begin_generate_sql("rid-9"));
    store.set_nl2sql_input("\n\t");
    assert!(!store.begin_generate_sql("rid-10"));
    assert!(store.nl2sql.is_generating);
    assert_eq!(store.nl2sql.request_id.as_deref(), Some("rid-9"));
    assert_eq!(store.nl2sql.input, "\n\t");

    store.set_nl2sql_input("retry");
    assert!(store.begin_generate_sql("rid-11"));
    store.fail_generate_sql("overloaded");
    assert_eq!(store.nl2sql.input, "retry");
    assert_eq!(store.nl2sql.request_id.as_deref(), Some("rid-11"));
    assert!(!store.nl2sql.is_generating);
}

#[test]
fn audit_clear_nl2sql_leaves_diagnosis_and_blank_parse_keeps_filters() {
    let mut store = AiStore::new();
    store.complete_diagnose(DiagnosisResult {
        summary: "missing comma".into(),
    });
    // complete_diagnose without begin leaves is_diagnosing false.
    assert!(!store.is_diagnosing);
    store.set_nl2sql_input("select");
    let _ = store.begin_generate_sql("rid");
    store.clear_nl2sql();
    assert!(store.nl2sql.input.is_empty());
    assert_eq!(
        store.diagnosis.as_ref().map(|item| item.summary.as_str()),
        Some("missing comma")
    );

    store.set_nl_filter_input("owner is ada");
    assert!(store.begin_parse_filter());
    let _ = store.complete_parse_filter(vec![FilterCondition {
        column: "owner".into(),
        operator: "eq".into(),
        value: "ada".into(),
    }]);
    store.set_nl_filter_input("");
    assert!(!store.begin_parse_filter());
    assert_eq!(
        store
            .parsed_filters
            .as_ref()
            .and_then(|items| items.first())
            .map(|item| item.column.as_str()),
        Some("owner")
    );

    store.explain_error = Some("kept".into());
    store.clear_explain_analysis();
    assert!(store.explain_error.is_none());
    store.begin_analyze_explain();
    store.complete_analyze_explain(ExplainAnalysis {
        summary: "nested loop".into(),
    ..Default::default()
    });
    assert_eq!(
        store
            .explain_analysis
            .as_ref()
            .map(|item| item.summary.as_str()),
        Some("nested loop")
    );
    assert!(store.nl_filter_input.is_empty());
}
