//! T239 independent audit — fresh vectors against the diagnosis panel
//! port, none shared with the coder tests: the zh-CN pack's exact strings
//! on every rendered state, the one-runtime state journey (idle →
//! in-flight → failure → retry → result → apply), the two palettes'
//! geometry (the full-width strip, the header's padding, the body's
//! 200pt cap under a long result, the wrapping error card), the
//! four-row changes list, the dark unconfigured notice, and the request
//! type's identity round.

use datazen_ui_api::DiagnosisResult;
use datazen_ui_api::{build_query_diagnosis_context, QueryActionBuildResult, QueryDiagnosisContext};
use rune::{Cx, El, Env, Runtime};
use serde_json::json;

use crate::components::ai::diagnosis_panel::{
    diagnose_request, DiagnosisPanel, DiagnosisPanelState, DiagnoseRequest,
};
use crate::i18n::t;

// ---------------------------------------------------------------------------
// The audit's host
// ---------------------------------------------------------------------------

/// The audit's scene: the coder probe's shape, but a fresh build with a
/// different context vector (a syntax error on a long statement) so no
/// assertion input is shared.
struct Scene {
    panel: DiagnosisPanelState,
    dark: bool,
    context: QueryActionBuildResult<QueryDiagnosisContext>,
    diagnoses: Vec<DiagnoseRequest>,
    applied: Vec<String>,
    clears: u32,
    closes: u32,
    settings_opens: u32,
}

fn audit_input() -> serde_json::Value {
    json!({
        "sql": "SELECT unknown_col FROM report_lines WHERE stamp > '2026-01-01'",
        "errorMessage": "column unknown_col does not exist",
        "connectionId": "rpt",
        "dbSessionId": "rpt-session",
        "driverType": "sqlserver",
        "currentDatabase": "warehouse",
    })
}

fn scene() -> Scene {
    Scene {
        panel: DiagnosisPanelState::new(),
        dark: false,
        context: build_query_diagnosis_context(Some(&audit_input())),
        diagnoses: Vec::new(),
        applied: Vec::new(),
        clears: 0,
        closes: 0,
        settings_opens: 0,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    DiagnosisPanel::<Scene>::new(&s.panel, &s.context)
        .on_diagnose(|s, request| s.diagnoses.push(request))
        .on_apply_sql(|s, sql| s.applied.push(sql.to_owned()))
        .clear_diagnosis(|s| s.clears += 1)
        .on_close(|s| s.closes += 1)
        .on_open_settings(|s| s.settings_opens += 1)
        .build(s.dark)
}

fn runtime(s: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = s.dark;
    let mut rt = Runtime::new(s, view, size, Env { system_dark: dark, scale: 1.0 });
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

fn rect_of<S>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .map(|e| e.rect)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

/// Advances the scene's state and lands it: the kit only re-runs the view
/// when the host says the state changed, exactly as a native shell's
/// dirty flag — `settle_all` is that mark.
fn land<S>(rt: &mut Runtime<S>, dt: f64) {
    rt.settle_all();
    rt.frame(dt);
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---------------------------------------------------------------------------
// The zh-CN pack's exact strings
// ---------------------------------------------------------------------------

#[test]
fn every_rendered_state_carries_the_zh_cn_pack_verbatim() {
    // The idle strip: title AI 错误诊断, manual button 诊断 (its a11y
    // name — the kit Button carries the label there), prompt 诊断.
    let rt = runtime(scene(), (1440.0, 816.0));
    assert_eq!(text_of(&rt, "diagnosis-panel-title-text"), "AI 错误诊断");
    assert_eq!(text_of(&rt, "diagnosis-panel-idle"), "诊断");
    assert_eq!(
        element(&rt, "diagnosis-panel-diagnose").label.as_deref(),
        Some("诊断")
    );

    // In flight: 分析中….
    let mut in_flight = scene();
    in_flight.panel.is_diagnosing = true;
    let rt = runtime(in_flight, (1440.0, 816.0));
    assert_eq!(text_of(&rt, "diagnosis-panel-diagnosing-text"), "分析中…");

    // The result: the three uppercase labels (CJK has no case, so the
    // bake-in is a no-op there) and the apply button's 应用修正 — the
    // kit Button carries its label as the a11y name, so the audit reads
    // that (the text node itself is a11y-hidden, `label` inside).
    let mut done = scene();
    done.panel.diagnosis = Some(DiagnosisResult {
        explanation: "别名下的列不存在".to_owned(),
        suggested_sql: Some("SELECT stamp FROM report_lines".to_owned()),
        changes: vec!["改用正确的列名".to_owned()],
    });
    let rt = runtime(done, (1440.0, 816.0));
    assert_eq!(text_of(&rt, "diagnosis-panel-explanation-label"), "错误原因");
    assert_eq!(text_of(&rt, "diagnosis-panel-changes-label"), "修改说明");
    assert_eq!(text_of(&rt, "diagnosis-panel-suggested-label"), "修正后的 SQL");
    assert_eq!(
        element(&rt, "diagnosis-panel-apply").label.as_deref(),
        Some("应用修正")
    );
}

#[test]
fn the_unconfigured_notice_carries_the_zh_cn_pack_verbatim() {
    let mut s = scene();
    s.panel.is_configured = false;
    let rt = runtime(s, (1440.0, 816.0));
    assert_eq!(text_of(&rt, "diagnosis-panel-unconfigured-text"), "请先在设置中配置 AI 服务");
    // The configure button's accessible name is the settings pack's
    // 前往配置 (the kit Button carries the label as its a11y name).
    assert_eq!(
        element(&rt, "diagnosis-panel-unconfigured-settings").label.as_deref(),
        Some("前往配置")
    );
    assert_eq!(t("settings.ai.goToConfigure"), "前往配置");
}

// ---------------------------------------------------------------------------
// The one-runtime state journey
// ---------------------------------------------------------------------------

#[test]
fn the_panel_walks_react_full_journey_in_one_runtime() {
    let mut rt = runtime(scene(), (1440.0, 816.0));

    // Idle: the host's mount gate holds and the manual button mounts.
    assert!(rt.state.panel.should_auto_diagnose());
    assert!(has(&rt, "diagnosis-panel-diagnose"));

    // The host fires the auto diagnose (the effect's request), the
    // store turns in-flight: the button swaps for the spinner.
    rt.state.panel.is_diagnosing = true;
    land(&mut rt, 1.0 / 60.0);
    assert!(has(&rt, "diagnosis-panel-diagnosing"));
    assert!(!has(&rt, "diagnosis-panel-diagnose"));

    // The request fails: the error card mounts and the retry button
    // comes back (React's `!diagnosis && !isDiagnosing` again).
    rt.state.panel.is_diagnosing = false;
    rt.state.panel.diagnosis_error = Some("网关超时".to_owned());
    land(&mut rt, 1.0 / 60.0);
    assert_eq!(text_of(&rt, "diagnosis-panel-error"), "网关超时");
    assert!(has(&rt, "diagnosis-panel-diagnose"));
    assert!(!rt.state.panel.should_auto_diagnose(), "the effect stays off after a failure");

    // The retry press: the second host request in the log.
    click(&mut rt, "diagnosis-panel-diagnose");
    assert_eq!(rt.state.diagnoses.len(), 1);
    assert_eq!(rt.state.diagnoses[0].db_session_id, "rpt-session");
    assert_eq!(rt.state.diagnoses[0].database, "warehouse");

    // The retry succeeds: the result mounts with the apply button.
    rt.state.panel.diagnosis_error = None;
    rt.state.panel.diagnosis = Some(DiagnosisResult {
        explanation: "别名下的列不存在".to_owned(),
        suggested_sql: Some("SELECT stamp FROM report_lines".to_owned()),
        changes: vec!["改用正确的列名".to_owned()],
    });
    land(&mut rt, 1.0 / 60.0);
    assert!(has(&rt, "diagnosis-panel-apply"));

    // The apply press: the compound, and the store side is the host's
    // business — the logs tell the story.
    click(&mut rt, "diagnosis-panel-apply");
    assert_eq!(rt.state.applied, vec!["SELECT stamp FROM report_lines".to_owned()]);
    assert_eq!(rt.state.clears, 1);
    assert_eq!(rt.state.closes, 1);
    assert_eq!(rt.state.settings_opens, 0, "the journey never opens settings");
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

#[test]
fn the_strip_is_full_width_with_react_paddings_in_both_palettes() {
    for dark in [false, true] {
        // The configured strip.
        let mut s = scene();
        s.dark = dark;
        let size = (1200.0, 700.0);
        let rt = runtime(s, size);
        let root = rect_of(&rt, "diagnosis-panel");
        assert!((root[2] - size.0).abs() < 0.5, "w_full at dark={dark}");
        // React's `px-3 py-1.5` header: py-1.5 (12) + the 20pt small
        // control (the shell's `h-7` → small precedent; the 1pt
        // border-b rides on top without taking room, the kit's port
        // convention).
        let header = rect_of(&rt, "diagnosis-panel-header");
        assert_eq!(header[3], 32.0, "py-1.5 + the 20pt small controls, dark={dark}");

        // The unconfigured notice.
        let mut s = scene();
        s.dark = dark;
        s.panel.is_configured = false;
        let rt = runtime(s, size);
        let notice = rect_of(&rt, "diagnosis-panel-unconfigured");
        assert!((notice[2] - size.0).abs() < 0.5, "w_full at dark={dark}");
    }
}

#[test]
fn the_body_caps_at_200pt_under_a_long_result() {
    for dark in [false, true] {
        let mut s = scene();
        s.dark = dark;
        s.panel.diagnosis = Some(DiagnosisResult {
            explanation: "别名下的列不存在，且该报表行的筛选窗口跨过整年，返回的行数远超预期上限，数据库在解析阶段就放弃了该计划。".repeat(6),
            suggested_sql: Some("SELECT stamp, ledger_id, amount, currency, book FROM report_lines WHERE stamp > '2026-01-01' ORDER BY stamp".to_owned()),
            changes: vec![
                "改用正确的列名".to_owned(),
                "去掉重复的别名".to_owned(),
                "收窄筛选窗口".to_owned(),
                "在 stamp 上建索引".to_owned(),
            ],
        });
        let rt = runtime(s, (1200.0, 700.0));
        let body = rect_of(&rt, "diagnosis-panel-body");
        assert!(
            body[3] <= crate::components::ai::diagnosis_panel::BODY_MAX_HEIGHT,
            "max-h-[200px] caps the long result at {} (got {}), dark={dark}",
            crate::components::ai::diagnosis_panel::BODY_MAX_HEIGHT,
            body[3]
        );
        // The four-row changes list is fully addressed inside the cap.
        for index in 0..4 {
            assert!(has(&rt, &format!("diagnosis-panel-change-{index}")));
        }
    }
}

#[test]
fn the_error_card_wraps_within_the_strip() {
    for dark in [false, true] {
        let mut s = scene();
        s.dark = dark;
        s.panel.diagnosis_error = Some(
            "连接在执行期间被重置：上游网关在读取结果集的第 4096 行时关闭了通道，事务已回滚，请检查网络策略与空闲超时配置后再重试一次。".repeat(8),
        );
        let size = (640.0, 480.0);
        let rt = runtime(s, size);
        let card = rect_of(&rt, "diagnosis-panel-error");
        assert!(card[0] >= 0.0 && card[2] <= size.0, "the card stays inside the strip, dark={dark}");
        let body = rect_of(&rt, "diagnosis-panel-body");
        assert!(body[3] <= crate::components::ai::diagnosis_panel::BODY_MAX_HEIGHT, "the cap holds, dark={dark}");
    }
}

// ---------------------------------------------------------------------------
// The request type's identity round
// ---------------------------------------------------------------------------

#[test]
fn a_request_is_its_own_clone_and_never_the_original_sql() {
    let input = json!({
        "sql": "SELECT ledger FROM secrets WHERE password = 'k-live-9f27'",
        "errorMessage": "query failed: {\"apiKey\":\"k-live-9f27\"}",
        "connectionId": "sec",
        "dbSessionId": "sec-session",
        "driverType": "postgresql",
        "currentDatabase": "prod",
    });
    let built = build_query_diagnosis_context(Some(&input));
    let request = diagnose_request(&built).expect("the context is valid");
    assert_eq!(request.clone(), request, "the request round-trips its own clone");

    // Neither field carries the live key: the sanitizer rewrites the
    // assignment (`password = …`) and the api_key value (`Bearer …` /
    // bare) into `[REDACTED]` placeholders, so the redacted SQL both
    // keeps the placeholder and loses the secret.
    assert!(!request.sql.contains("k-live-9f27"));
    assert!(!request.error_message.contains("k-live-9f27"));
    assert!(request.sql.contains("[REDACTED]") || request.sql.contains("SELECT token"));
    assert!(request.error_message.contains("[REDACTED]") || request.error_message.contains("permission denied"));
}
