//! T087-S2: `stores/aiStore.ts` NL2SQL, diagnosis, EXPLAIN, smart filter.

use datazen_ui::ai_store::{AiStore, DiagnosisResult, ExplainAnalysis, FilterCondition};

fn filter(column: &str) -> FilterCondition {
    FilterCondition {
        column: column.into(),
        operator: "=".into(),
        value: "1".into(),
    }
}

#[test]
fn assist_fields_start_empty() {
    let store = AiStore::new();
    assert!(store.nl2sql.input.is_empty());
    assert!(!store.nl2sql.is_generating);
    assert!(store.nl2sql.request_id.is_none());
    assert!(store.nl2sql_error.is_none());
    assert!(store.diagnosis.is_none());
    assert!(!store.is_diagnosing);
    assert!(store.explain_analysis.is_none());
    assert!(!store.is_analyzing_explain);
    assert!(store.nl_filter_input.is_empty());
    assert!(store.parsed_filters.is_none());
    assert!(!store.is_parsing_filter);
}

#[test]
fn generate_sql_ignores_blank_input_and_keeps_the_request_on_failure() {
    let mut store = AiStore::new();
    store.set_nl2sql_input("   ");
    assert!(!store.begin_generate_sql("req-1"));
    assert!(!store.nl2sql.is_generating);
    assert!(store.nl2sql.request_id.is_none());

    store.nl2sql.generated_sql = "select 1".into();
    store.nl2sql.memory_used = vec!["m".into()];
    store.nl2sql_error = Some("old".into());
    store.set_nl2sql_input("  count users  ");
    assert!(store.begin_generate_sql("req-2"));
    assert!(store.nl2sql.is_generating);
    assert_eq!(store.nl2sql.request_id.as_deref(), Some("req-2"));
    assert!(store.nl2sql.generated_sql.is_empty());
    assert!(store.nl2sql.memory_used.is_empty());
    assert!(store.nl2sql_error.is_none());
    assert_eq!(store.nl2sql.input, "  count users  ");

    store.fail_generate_sql("model down");
    assert!(!store.nl2sql.is_generating);
    assert_eq!(store.nl2sql.request_id.as_deref(), Some("req-2"));
    assert_eq!(store.nl2sql_error.as_deref(), Some("model down"));

    store.clear_nl2sql();
    assert!(store.nl2sql.input.is_empty());
    assert!(store.nl2sql.request_id.is_none());
    assert!(store.nl2sql_error.is_none());
}

#[test]
fn diagnose_and_explain_clear_the_previous_result_before_the_call() {
    let mut store = AiStore::new();
    store.diagnosis = Some(DiagnosisResult {
        summary: "old".into(),
    });
    store.diagnosis_error = Some("old".into());
    store.begin_diagnose();
    assert!(store.is_diagnosing);
    assert!(store.diagnosis.is_none());
    assert!(store.diagnosis_error.is_none());
    store.complete_diagnose(DiagnosisResult {
        summary: "index missing".into(),
    });
    assert!(!store.is_diagnosing);
    assert_eq!(
        store.diagnosis.as_ref().map(|item| item.summary.as_str()),
        Some("index missing")
    );

    store.begin_diagnose();
    store.fail_diagnose("timeout");
    assert!(!store.is_diagnosing);
    assert!(store.diagnosis.is_none());
    assert_eq!(store.diagnosis_error.as_deref(), Some("timeout"));
    store.clear_diagnosis();
    assert!(store.diagnosis_error.is_none());

    store.explain_analysis = Some(ExplainAnalysis {
        summary: "old plan".into(),
    ..Default::default()
    });
    store.begin_analyze_explain();
    assert!(store.explain_analysis.is_none());
    assert!(store.is_analyzing_explain);
    store.fail_analyze_explain("no plan");
    assert!(!store.is_analyzing_explain);
    assert_eq!(store.explain_error.as_deref(), Some("no plan"));
    store.begin_analyze_explain();
    store.complete_analyze_explain(ExplainAnalysis {
        summary: "seq scan".into(),
        ..Default::default()
    });
    assert!(!store.is_analyzing_explain);
    store.clear_explain_analysis();
    assert!(store.explain_analysis.is_none());
    assert!(store.explain_error.is_none());
}

#[test]
fn parse_filter_keeps_the_previous_result_when_the_input_is_blank() {
    let mut store = AiStore::new();
    store.parsed_filters = Some(vec![filter("id")]);
    store.set_nl_filter_input("  ");
    assert!(!store.begin_parse_filter());
    assert!(!store.is_parsing_filter);
    assert_eq!(store.parsed_filters.as_ref().map(|items| items.len()), Some(1));

    store.set_nl_filter_input("status is open");
    store.nl_filter_error = Some("old".into());
    assert!(store.begin_parse_filter());
    assert!(store.is_parsing_filter);
    assert!(store.parsed_filters.is_none());
    assert!(store.nl_filter_error.is_none());
    let got = store.complete_parse_filter(vec![filter("status")]);
    assert_eq!(got.len(), 1);
    assert!(!store.is_parsing_filter);

    store.begin_parse_filter();
    store.fail_parse_filter("parse");
    assert!(!store.is_parsing_filter);
    assert!(store.parsed_filters.is_none());
    assert_eq!(store.nl_filter_error.as_deref(), Some("parse"));

    store.clear_nl_filter();
    assert!(store.nl_filter_input.is_empty());
    assert!(store.parsed_filters.is_none());
    assert!(store.nl_filter_error.is_none());
}
