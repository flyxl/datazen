//! T239: the diagnosis panel's coder tests — the two pure derivations
//! (the mount-effect gate, the `handleDiagnose` request spread with its
//! redaction guarantee) and the runtime probes of both branches (the
//! unconfigured notice's configure and close wires, the idle header with
//! the manual button and the prompt, the diagnosing spinner swap, the
//! error card with the retry button, the result's three sections, the
//! apply compound, and the inert invalid-context button).

use datazen_ui_api::DiagnosisResult;
use datazen_ui_api::{
    build_query_diagnosis_context, QueryActionBuildResult, QueryDiagnosisContext,
};
use rune::{Cx, El, Env, Runtime};
use serde_json::json;

use crate::components::ai::diagnosis_panel::{
    diagnose_request, DiagnosisPanel, DiagnosisPanelState, DiagnoseRequest,
};
use crate::i18n::t;

// ---------------------------------------------------------------------------
// The probe
// ---------------------------------------------------------------------------

/// The host: the panel's `aiStore` slice, the built context it renders
/// against, and the five callback logs.
struct Probe {
    panel: DiagnosisPanelState,
    dark: bool,
    context: QueryActionBuildResult<QueryDiagnosisContext>,
    diagnoses: Vec<DiagnoseRequest>,
    applied: Vec<String>,
    clears: u32,
    closes: u32,
    settings_opens: u32,
}

fn valid_input() -> serde_json::Value {
    json!({
        "sql": "SELECT bad",
        "errorMessage": "syntax error",
        "connectionId": "c1",
        "dbSessionId": "c1-session",
        "databaseType": "postgresql",
        "database": "db",
    })
}

fn probe(context: QueryActionBuildResult<QueryDiagnosisContext>) -> Probe {
    Probe {
        panel: DiagnosisPanelState::new(),
        dark: false,
        context,
        diagnoses: Vec::new(),
        applied: Vec::new(),
        clears: 0,
        closes: 0,
        settings_opens: 0,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    DiagnosisPanel::<Probe>::new(&s.panel, &s.context)
        .on_diagnose(|s, request| s.diagnoses.push(request))
        .on_apply_sql(|s, sql| s.applied.push(sql.to_owned()))
        .clear_diagnosis(|s| s.clears += 1)
        .on_close(|s| s.closes += 1)
        .on_open_settings(|s| s.settings_opens += 1)
        .build(s.dark)
}

fn runtime(p: Probe) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, (1440.0, 816.0), Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text.clone())
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---------------------------------------------------------------------------
// The mount-effect gate (React's useEffect condition)
// ---------------------------------------------------------------------------

#[test]
fn the_mount_gate_holds_only_when_the_panel_is_completely_idle() {
    let mut state = DiagnosisPanelState::new();
    assert!(state.should_auto_diagnose(), "the fresh panel auto-diagnoses");

    state.diagnosis = Some(DiagnosisResult {
        explanation: "Missing column".to_owned(),
        suggested_sql: None,
        changes: Vec::new(),
    });
    assert!(!state.should_auto_diagnose(), "a live result stops the effect");
    state.diagnosis = None;

    state.is_diagnosing = true;
    assert!(!state.should_auto_diagnose(), "an in-flight request stops the effect");
    state.is_diagnosing = false;

    state.diagnosis_error = Some("timeout".to_owned());
    assert!(!state.should_auto_diagnose(), "a failed diagnosis stops the effect");
}

// ---------------------------------------------------------------------------
// The request derivation (handleDiagnose)
// ---------------------------------------------------------------------------

#[test]
fn a_failed_context_yields_no_request() {
    // React: `buildQueryDiagnosisContext({ sql: 'SELECT 1' })` — no
    // error message — fails, so the button and the effect diagnose
    // nothing.
    let missing_error = build_query_diagnosis_context(Some(&json!({ "sql": "SELECT 1" })));
    assert!(!missing_error.is_ok());
    assert!(diagnose_request(&missing_error).is_none());

    let null = build_query_diagnosis_context(None);
    assert!(diagnose_request(&null).is_none());
}

#[test]
fn the_request_spreads_the_params_and_overrides_with_the_safe_copies() {
    // React's "auto-diagnoses with only redacted SQL and error payloads":
    // secrets in the SQL text and the error message must not reach the
    // host action.
    let input = json!({
        "sql": "SELECT '{\"token\":\"sql-json-secret\"}', password = 'sql-password-secret'",
        "errorMessage": "query failed: {\"token\":\"error-json-secret\"}",
        "connectionId": "c1",
        "dbSessionId": "c1-session",
        "databaseType": "postgresql",
        "database": "db",
    });
    let built = build_query_diagnosis_context(Some(&input));
    let request = diagnose_request(&built).expect("the valid context yields the request");
    assert_eq!(request.db_session_id, "c1-session");
    assert_eq!(request.database, "db");
    assert!(
        !request.sql.contains("sql-json-secret"),
        "the redacted SQL keeps the JSON token out: {}",
        request.sql
    );
    assert!(
        !request.sql.contains("sql-password-secret"),
        "the redacted SQL keeps the password out: {}",
        request.sql
    );
    assert!(
        !request.error_message.contains("error-json-secret"),
        "the redacted error keeps the JSON token out: {}",
        request.error_message
    );
}

#[test]
fn the_request_copies_the_safe_fields_not_the_originals() {
    // The spread's overrides are the point: `sql` / `errorMessage` come
    // from `safeSql` / `safeErrorMessage`, never from the byte-for-byte
    // `sql` the Fix SQL track relies on.
    let input = json!({
        "sql": "SELECT bad",
        "errorMessage": "syntax error",
        "connectionId": "c1",
        "dbSessionId": "c1-session",
        "databaseType": "postgresql",
        "database": "db",
    });
    let built = build_query_diagnosis_context(Some(&input));
    let request = diagnose_request(&built).expect("the valid context yields the request");
    assert_eq!(request.sql, "SELECT bad");
    assert_eq!(request.error_message, "syntax error");
}

// ---------------------------------------------------------------------------
// The unconfigured branch (React's "shows not configured state with close")
// ---------------------------------------------------------------------------

#[test]
fn the_unconfigured_notice_mounts_the_configure_button_and_the_close() {
    let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
    p.panel.is_configured = false;
    let mut rt = runtime(p);

    assert!(!has(&rt, "diagnosis-panel"), "the configured tree is gone");
    assert!(has(&rt, "diagnosis-panel-unconfigured"));
    assert_eq!(
        text_of(&rt, "diagnosis-panel-unconfigured-text"),
        t("common.aiNotConfigured")
    );

    // The configure button opens the AI settings window.
    click(&mut rt, "diagnosis-panel-unconfigured-settings");
    assert_eq!(rt.state.settings_opens, 1);

    // The close control is React's `handleClose`: `clearDiagnosis` then
    // `onClose`.
    click(&mut rt, "diagnosis-panel-unconfigured-close");
    assert_eq!(rt.state.clears, 1);
    assert_eq!(rt.state.closes, 1);
}

// ---------------------------------------------------------------------------
// The configured branch
// ---------------------------------------------------------------------------

#[test]
fn the_idle_panel_mounts_the_header_the_manual_button_and_the_prompt() {
    let rt = runtime(probe(build_query_diagnosis_context(Some(&valid_input()))));

    assert_eq!(text_of(&rt, "diagnosis-panel-title-text"), t("diagnosis.title"));
    assert!(has(&rt, "diagnosis-panel-diagnose"), "the idle panel offers the manual button");
    assert!(has(&rt, "diagnosis-panel-close"));
    assert_eq!(text_of(&rt, "diagnosis-panel-idle"), t("diagnosis.diagnose"));
    // Nothing else mounts: no spinner, no error, no result.
    assert!(!has(&rt, "diagnosis-panel-diagnosing"));
    assert!(!has(&rt, "diagnosis-panel-error"));
    assert!(!has(&rt, "diagnosis-panel-result"));
}

#[test]
fn the_manual_press_sends_the_request() {
    let mut rt = runtime(probe(build_query_diagnosis_context(Some(&valid_input()))));
    click(&mut rt, "diagnosis-panel-diagnose");
    let state = &rt.state;
    assert_eq!(state.diagnoses.len(), 1);
    let request = &state.diagnoses[0];
    assert_eq!(request.db_session_id, "c1-session");
    assert_eq!(request.database, "db");
    assert_eq!(request.sql, "SELECT bad");
    assert_eq!(request.error_message, "syntax error");
}

#[test]
fn an_invalid_context_keeps_the_manual_button_inert() {
    // React's "does not diagnose when the context is missing or invalid":
    // the button mounts, the press refuses at the `ok` gate.
    let mut rt = runtime(probe(build_query_diagnosis_context(Some(&json!({
        "sql": "SELECT 1",
    })))));
    assert!(has(&rt, "diagnosis-panel-diagnose"), "the button still mounts");
    click(&mut rt, "diagnosis-panel-diagnose");
    assert!(rt.state.diagnoses.is_empty(), "the invalid context diagnoses nothing");
}

#[test]
fn the_diagnosing_state_swaps_the_manual_button_for_the_spinner() {
    let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
    p.panel.is_diagnosing = true;
    let rt = runtime(p);

    assert!(has(&rt, "diagnosis-panel-diagnosing"));
    assert_eq!(text_of(&rt, "diagnosis-panel-diagnosing-text"), t("diagnosis.diagnosing"));
    assert!(!has(&rt, "diagnosis-panel-diagnose"), "React hides the button while in flight");
    assert!(!has(&rt, "diagnosis-panel-idle"));
}

#[test]
fn the_error_state_mounts_the_card_and_the_retry_button() {
    let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
    p.panel.diagnosis_error = Some("timeout".to_owned());
    let mut rt = runtime(p);

    assert_eq!(text_of(&rt, "diagnosis-panel-error"), "timeout");
    assert!(has(&rt, "diagnosis-panel-diagnose"), "the error state offers the retry");
    assert!(!has(&rt, "diagnosis-panel-idle"));

    // React's "shows diagnosing and error states with manual retry": the
    // retry press goes through.
    click(&mut rt, "diagnosis-panel-diagnose");
    assert_eq!(rt.state.diagnoses.len(), 1);
}

#[test]
fn the_result_state_mounts_the_three_sections() {
    let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
    p.panel.diagnosis = Some(DiagnosisResult {
        explanation: "Missing column".to_owned(),
        suggested_sql: Some("SELECT id FROM users".to_owned()),
        changes: vec!["Use correct column name".to_owned(), "Drop the alias".to_owned()],
    });
    let rt = runtime(p);

    // The explanation, uppercase label included.
    assert_eq!(text_of(&rt, "diagnosis-panel-explanation-text"), "Missing column");
    assert_eq!(
        text_of(&rt, "diagnosis-panel-explanation-label"),
        t("diagnosis.explanation").to_uppercase()
    );

    // The changes list: label plus one row per change.
    assert_eq!(
        text_of(&rt, "diagnosis-panel-changes-label"),
        t("diagnosis.changes").to_uppercase()
    );
    assert_eq!(text_of(&rt, "diagnosis-panel-change-0"), "Use correct column name");
    assert_eq!(text_of(&rt, "diagnosis-panel-change-1"), "Drop the alias");

    // The suggested SQL pre and the apply button.
    assert_eq!(text_of(&rt, "diagnosis-panel-suggested-sql"), "SELECT id FROM users");
    assert!(has(&rt, "diagnosis-panel-apply"));

    // A result present: no manual button, no idle prompt.
    assert!(!has(&rt, "diagnosis-panel-diagnose"));
    assert!(!has(&rt, "diagnosis-panel-idle"));
}

#[test]
fn the_apply_press_runs_the_compound() {
    let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
    p.panel.diagnosis = Some(DiagnosisResult {
        explanation: "Missing column".to_owned(),
        suggested_sql: Some("SELECT id FROM users".to_owned()),
        changes: vec!["Use correct column name".to_owned()],
    });
    let mut rt = runtime(p);

    click(&mut rt, "diagnosis-panel-apply");
    let state = &rt.state;
    // React's handleApply: onApplySql(suggestedSql), clearDiagnosis(),
    // onClose().
    assert_eq!(state.applied, vec!["SELECT id FROM users".to_owned()]);
    assert_eq!(state.clears, 1);
    assert_eq!(state.closes, 1);
}

#[test]
fn a_result_without_a_suggestion_mounts_no_apply_button() {
    let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
    p.panel.diagnosis = Some(DiagnosisResult {
        explanation: "Missing column".to_owned(),
        suggested_sql: None,
        changes: vec!["Use correct column name".to_owned()],
    });
    let rt = runtime(p);

    assert!(!has(&rt, "diagnosis-panel-apply"));
    assert!(!has(&rt, "diagnosis-panel-suggested-label"), "no label without a suggestion");
    assert!(!has(&rt, "diagnosis-panel-suggested-sql"));
}

#[test]
fn a_result_without_changes_mounts_no_changes_list() {
    let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
    p.panel.diagnosis = Some(DiagnosisResult {
        explanation: "Missing column".to_owned(),
        suggested_sql: Some("SELECT id FROM users".to_owned()),
        changes: Vec::new(),
    });
    let rt = runtime(p);

    assert!(!has(&rt, "diagnosis-panel-changes-label"));
    assert!(!has(&rt, "diagnosis-panel-changes-list"));
    assert!(has(&rt, "diagnosis-panel-explanation-text"), "the explanation stays");
}

#[test]
fn the_configured_close_control_runs_the_clear_then_close_compound() {
    let mut rt = runtime(probe(build_query_diagnosis_context(Some(&valid_input()))));
    click(&mut rt, "diagnosis-panel-close");
    let state = &rt.state;
    assert_eq!(state.clears, 1);
    assert_eq!(state.closes, 1);
    assert!(state.diagnoses.is_empty(), "closing never diagnoses");
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

#[test]
fn the_body_is_bounded_by_the_react_maximum() {
    for dark in [false, true] {
        let mut p = probe(build_query_diagnosis_context(Some(&valid_input())));
        p.dark = dark;
        p.panel.diagnosis = Some(DiagnosisResult {
            explanation: "Missing column".to_owned(),
            suggested_sql: Some("SELECT id FROM users".to_owned()),
            changes: vec!["Use correct column name".to_owned()],
        });
        let rt = runtime(p);
        let body = rect_of(&rt, "diagnosis-panel-body");
        assert!(
            body[3] <= crate::components::ai::diagnosis_panel::BODY_MAX_HEIGHT,
            "max-h-[200px] caps the body at {} (got {}), dark={dark}",
            crate::components::ai::diagnosis_panel::BODY_MAX_HEIGHT,
            body[3]
        );
    }
}

fn rect_of<S>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .map(|e| e.rect)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}
