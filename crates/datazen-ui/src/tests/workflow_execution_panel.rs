//! T019: the `WorkflowExecutionResultPanel` port — head row states, step
//! rows, expansion, the four status marks, selective blocks (SQL / error /
//! final output), the 20-row preview cap with its `... N more` line, and the
//! chartable-only table/chart toggle.

use std::collections::BTreeSet;

use rune::{col, Cx, El, Env, Runtime};
use serde_json::json;

use super::runtime;
use crate::i18n::t;
use crate::views::{
    extract_step_column_names, is_chartable_preview, js_string, step_to_statement_preview,
    StepColumn, StepExecutionResult, StepUiSlots, WorkflowExecutionResult,
    WorkflowExecutionResultPanel, WorkflowStepStatus,
};

const PREFIX: &str = "workflow-execution-result";

/// The probe's own state: the run to show plus the two interaction sets.
#[derive(Default)]
struct PanelState {
    result: Option<WorkflowExecutionResult>,
    expanded: BTreeSet<String>,
    chart: BTreeSet<String>,
}

fn slots() -> StepUiSlots<PanelState> {
    StepUiSlots {
        expanded: |s: &PanelState| &s.expanded,
        toggle: |s: &mut PanelState, id| {
            if !s.expanded.remove(&id) {
                s.expanded.insert(id);
            }
        },
        chart: |s: &PanelState| &s.chart,
        set_chart: |s: &mut PanelState, id, chart| {
            if chart {
                s.chart.insert(id);
            } else {
                s.chart.remove(&id);
            }
        },
    }
}

fn panel_view(state: &PanelState, _cx: &mut Cx<PanelState>) -> El {
    let Some(result) = &state.result else {
        return col().w_full().h_full().padding(16.0);
    };
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(
            WorkflowExecutionResultPanel::<PanelState>::new(result, slots()).build(state, true),
        )
}

fn probe(result: WorkflowExecutionResult) -> Runtime<PanelState> {
    let mut rt = Runtime::new(
        PanelState {
            result: Some(result),
            ..PanelState::default()
        },
        panel_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<PanelState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<PanelState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<PanelState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn step(id: &str, status: WorkflowStepStatus) -> StepExecutionResult {
    StepExecutionResult {
        step_id: id.to_owned(),
        step_type: Some("query".to_owned()),
        status,
        result: None,
        execution_time_ms: 5,
        error: None,
        connection_name: Some("local".to_owned()),
        sql_executed: Some(format!("SELECT 1 /* {id} */")),
    }
}

fn run(steps: Vec<StepExecutionResult>) -> WorkflowExecutionResult {
    WorkflowExecutionResult {
        success: steps
            .iter()
            .all(|s| s.status == WorkflowStepStatus::Success),
        final_output: String::new(),
        steps,
        total_time_ms: 120,
        error: None,
    }
}

// ---- head row ---------------------------------------------------------------

#[test]
fn a_successful_run_reads_the_result_label_and_its_total_time() {
    let rt = probe(run(vec![step("s1", WorkflowStepStatus::Success)]));
    let status = element(&rt, &format!("{PREFIX}-status"));
    assert_eq!(
        status.text.as_deref(),
        Some(format!("✓ {}", t("workflows.result")).as_str()),
        "the success head says ✓ + t('workflows.result')"
    );
    let time = element(&rt, &format!("{PREFIX}-total-time"));
    assert_eq!(time.text.as_deref(), Some("120ms"));
    // The time sits right of the status (justify-between).
    assert!(time.rect[0] > status.rect[0] + status.rect[2]);
    assert!(!has_key(&rt, &format!("{PREFIX}-error")));
    assert!(!has_key(&rt, &format!("{PREFIX}-final-output")));
}

#[test]
fn a_failed_run_reads_the_execution_failed_label_and_tints_the_head() {
    let failed = WorkflowExecutionResult {
        success: false,
        final_output: String::new(),
        steps: Vec::new(),
        total_time_ms: 7,
        error: None,
    };
    let rt = probe(failed);
    let status = element(&rt, &format!("{PREFIX}-status"));
    assert_eq!(
        status.text.as_deref(),
        Some(format!("✗ {}", t("common.executionFailed")).as_str()),
        "the failure head says ✗ + t('common.executionFailed')"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-total-time")).text.as_deref(),
        Some("7ms")
    );
    // The failure tint differs from the success tint (React bg-red vs bg-green).
    let ok = probe(run(Vec::new()));
    let ok_head = element(&ok, &format!("{PREFIX}-head"));
    let fail_head = element(&rt, &format!("{PREFIX}-head"));
    assert_ne!(
        ok_head.fill, fail_head.fill,
        "the head tint follows the outcome"
    );
}

#[test]
fn workflow_error_and_final_output_render_only_when_present() {
    // Neither on a clean run.
    let clean = probe(run(vec![step("s1", WorkflowStepStatus::Success)]));
    assert!(!has_key(&clean, &format!("{PREFIX}-error")));
    assert!(!has_key(&clean, &format!("{PREFIX}-final-output")));

    // Both on a run that carries them.
    let with_both = WorkflowExecutionResult {
        success: false,
        final_output: "42".to_owned(),
        steps: Vec::new(),
        total_time_ms: 1,
        error: Some("boom".to_owned()),
    };
    let rt = probe(with_both);
    assert_eq!(
        element(&rt, &format!("{PREFIX}-error-text")).text.as_deref(),
        Some("boom")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-final-output-label")).text.as_deref(),
        Some(t("workflows.finalOutput"))
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-final-output-text")).text.as_deref(),
        Some("42")
    );
}

// ---- step rows ---------------------------------------------------------------

#[test]
fn step_rows_show_id_type_connection_and_time_and_toggle_their_bodies() {
    let mut rt = probe(run(vec![step("s1", WorkflowStepStatus::Success)]));
    let row_key = format!("{PREFIX}-step-0-row");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-id")).text.as_deref(),
        Some("s1")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-type")).text.as_deref(),
        Some("[query]")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-connection")).text.as_deref(),
        Some("local")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-time")).text.as_deref(),
        Some("5ms")
    );
    // The time hugs the row's right padding (React `ml-auto`).
    let row_rect = element(&rt, &row_key).rect;
    let time_rect = element(&rt, &format!("{PREFIX}-step-0-time")).rect;
    assert!(
        (row_rect[0] + row_rect[2] - 12.0 - (time_rect[0] + time_rect[2])).abs() < 1.0,
        "the time is at the row's right edge: row {row_rect:?}, time {time_rect:?}"
    );

    // Collapsed: chevron-right, no body.
    assert!(has_key(&rt, &format!("{PREFIX}-step-0-glyph-right")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-glyph-down")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-body")));
    assert_eq!(element(&rt, &row_key).expanded, Some(false));

    // A click expands: chevron-down, the body with its SQL block.
    click(&mut rt, &row_key);
    assert!(has_key(&rt, &format!("{PREFIX}-step-0-glyph-down")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-glyph-right")));
    assert_eq!(element(&rt, &row_key).expanded, Some(true));
    assert!(has_key(&rt, &format!("{PREFIX}-step-0-body")));
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-sql-text")).text.as_deref(),
        Some("SELECT 1 /* s1 */")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-sql-label")).text.as_deref(),
        Some("SQL")
    );
    assert!(
        rt.state.expanded.contains("s1"),
        "the toggle slot wrote the expansion into the host's state"
    );

    // A second click collapses back.
    click(&mut rt, &row_key);
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-body")));
    assert_eq!(element(&rt, &row_key).expanded, Some(false));
}

#[test]
fn all_four_status_marks_take_their_own_element_and_colour() {
    let marks = [
        (WorkflowStepStatus::Success, "status"),
        (WorkflowStepStatus::Failed, "status"),
        (WorkflowStepStatus::Skipped, "status-skip"),
        (WorkflowStepStatus::TimedOut, "status-clock"),
    ];
    for (status, suffix) in marks {
        let rt = probe(run(vec![step("s", status)]));
        let key = format!("{PREFIX}-step-0-{suffix}");
        assert!(has_key(&rt, &key), "status {status:?} mounts {key}");
        // The other marks do not mount for this step.
        for other in ["status", "status-skip", "status-clock"] {
            if other != suffix {
                assert!(
                    !has_key(&rt, &format!("{PREFIX}-step-0-{other}")),
                    "status {status:?} must not mount -{other}"
                );
            }
        }
    }
    // Success paints the kit's success green; every non-success mark differs.
    let green = probe(run(vec![step("s", WorkflowStepStatus::Success)]));
    let green_color = element(&green, &format!("{PREFIX}-step-0-status")).text_color;
    for status in [
        WorkflowStepStatus::Failed,
        WorkflowStepStatus::Skipped,
        WorkflowStepStatus::TimedOut,
    ] {
        let rt = probe(run(vec![step("s", status)]));
        let key = format!(
            "{PREFIX}-step-0-{}",
            match status {
                WorkflowStepStatus::Failed => "status",
                WorkflowStepStatus::Skipped => "status-skip",
                _ => "status-clock",
            }
        );
        assert_ne!(
            element(&rt, &key).text_color,
            green_color,
            "{status:?} must not paint the success green"
        );
    }
}

#[test]
fn type_and_connection_chips_are_selective() {
    // Without a type or a connection neither chip mounts.
    let bare = StepExecutionResult {
        step_id: "bare".to_owned(),
        step_type: None,
        status: WorkflowStepStatus::Success,
        result: None,
        execution_time_ms: 0,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let rt = probe(run(vec![bare]));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-type")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-connection")));
    // The row still mounts with its id and time.
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-id")).text.as_deref(),
        Some("bare")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-time")).text.as_deref(),
        Some("0ms")
    );
}

// ---- expanded body blocks ----------------------------------------------------

#[test]
fn sql_error_and_ai_blocks_render_only_when_present() {
    // A step with neither SQL nor error shows neither.
    let clean = StepExecutionResult {
        step_id: "clean".to_owned(),
        step_type: None,
        status: WorkflowStepStatus::Success,
        result: None,
        execution_time_ms: 0,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let mut rt = probe(run(vec![clean]));
    click(&mut rt, &format!("{PREFIX}-step-0-row"));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-sql")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-error")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-ai-result")));

    // A failing step with SQL and an error shows both.
    let dirty = StepExecutionResult {
        step_id: "dirty".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Failed,
        result: None,
        execution_time_ms: 3,
        error: Some("syntax error".to_owned()),
        connection_name: None,
        sql_executed: Some("SELEC 1".to_owned()),
    };
    let mut rt = probe(run(vec![dirty]));
    click(&mut rt, &format!("{PREFIX}-step-0-row"));
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-sql-text")).text.as_deref(),
        Some("SELEC 1")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-error")).text.as_deref(),
        Some("syntax error")
    );

    // An AI step with a result shows it; missing or null render nothing.
    let ai = |result: Option<serde_json::Value>| StepExecutionResult {
        step_id: "ai".to_owned(),
        step_type: Some("ai".to_owned()),
        status: WorkflowStepStatus::Success,
        result,
        execution_time_ms: 0,
        error: None,
        connection_name: None,
        sql_executed: None,
    };
    let mut with = probe(run(vec![ai(Some(json!({"result": "the answer"})))]));
    click(&mut with, &format!("{PREFIX}-step-0-row"));
    assert_eq!(
        element(&with, &format!("{PREFIX}-step-0-ai-result")).text.as_deref(),
        Some("the answer")
    );
    let mut without = probe(run(vec![ai(None)]));
    click(&mut without, &format!("{PREFIX}-step-0-row"));
    assert!(!has_key(&without, &format!("{PREFIX}-step-0-ai-result")));
    // React: `step.result?.result != null` — a JSON null renders nothing.
    let mut null_result = probe(run(vec![ai(Some(json!({"result": null})))]));
    click(&mut null_result, &format!("{PREFIX}-step-0-row"));
    assert!(!has_key(&null_result, &format!("{PREFIX}-step-0-ai-result")));
}

// ---- data preview ------------------------------------------------------------

fn data_step(rows: serde_json::Value, columns: Option<serde_json::Value>) -> StepExecutionResult {
    let mut result = json!({ "rows": rows });
    if let Some(cols) = columns {
        result["columns"] = cols;
    }
    StepExecutionResult {
        step_id: "data".to_owned(),
        step_type: Some("query".to_owned()),
        status: WorkflowStepStatus::Success,
        result: Some(result),
        execution_time_ms: 10,
        error: None,
        connection_name: None,
        sql_executed: None,
    }
}

#[test]
fn the_table_preview_shows_the_first_twenty_rows_and_the_more_line() {
    let rows: Vec<serde_json::Value> = (0..25)
        .map(|i| json!({ "a": i, "b": format!("v{i}") }))
        .collect();
    let mut rt = probe(run(vec![data_step(json!(rows), None)]));
    click(&mut rt, &format!("{PREFIX}-step-0-row"));

    // The row count reads the row list's length.
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-rows-count")).text.as_deref(),
        Some("25 row(s)")
    );
    // Header columns come from the object keys.
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-table-col-0-title")).text.as_deref(),
        Some("a")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-table-col-1-title")).text.as_deref(),
        Some("b")
    );
    // Rows 0..19 render; row 20 does not.
    for r in 0..20 {
        assert!(
            has_key(&rt, &format!("{PREFIX}-step-0-table-row-{r}")),
            "row {r} renders"
        );
    }
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-table-row-20")));
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-table-cell-0-0")).text.as_deref(),
        Some("0")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-table-cell-19-0")).text.as_deref(),
        Some("19")
    );
    // The more line counts the unshown rows: 25 - 20 = 5.
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-table-more-text")).text.as_deref(),
        Some("... 5 more")
    );

    // A null cell paints the muted null (React `val == null`).
    let mut nulls = probe(run(vec![data_step(json!([{ "a": null, "b": 1 }]), None)]));
    click(&mut nulls, &format!("{PREFIX}-step-0-row"));
    assert_eq!(
        element(&nulls, &format!("{PREFIX}-step-0-table-cell-0-0")).text.as_deref(),
        Some("null")
    );
    assert_eq!(
        element(&nulls, &format!("{PREFIX}-step-0-table-cell-0-1")).text.as_deref(),
        Some("1")
    );

    // Exactly 20 rows: no more line.
    let rows: Vec<serde_json::Value> = (0..20).map(|i| json!({ "a": i })).collect();
    let mut exact = probe(run(vec![data_step(json!(rows), None)]));
    click(&mut exact, &format!("{PREFIX}-step-0-row"));
    assert!(!has_key(&exact, &format!("{PREFIX}-step-0-table-more")));

    // The rows_count the host reported wins over the list length.
    let mut reported = probe(run(vec![data_step(
        json!({ "rows": [{ "a": 1 }], "rows_count": 99 }),
        None,
    )]));
    click(&mut reported, &format!("{PREFIX}-step-0-row"));
    assert_eq!(
        element(&reported, &format!("{PREFIX}-step-0-rows-count")).text.as_deref(),
        Some("99 row(s)")
    );

    // Array-shaped rows render positionally, like React's second branch.
    let mut arrays = probe(run(vec![data_step(
        json!([[1, "x"], [2, "y"]]),
        Some(json!([
            { "name": "a", "dataType": "integer" },
            { "name": "b" }
        ])),
    )]));
    click(&mut arrays, &format!("{PREFIX}-step-0-row"));
    assert_eq!(
        element(&arrays, &format!("{PREFIX}-step-0-table-cell-0-0")).text.as_deref(),
        Some("1")
    );
    assert_eq!(
        element(&arrays, &format!("{PREFIX}-step-0-table-cell-1-1")).text.as_deref(),
        Some("y")
    );
}

#[test]
fn the_view_toggle_exists_only_for_chartable_results() {
    // A numeric column is chartable: the toggle mounts and switches views.
    let numeric = data_step(json!([{ "a": 1 }, { "a": 2 }]), None);
    let mut rt = probe(run(vec![numeric]));
    click(&mut rt, &format!("{PREFIX}-step-0-row"));
    assert!(
        has_key(&rt, &format!("{PREFIX}-step-0-view-toggle")),
        "a numeric result is chartable, so the toggle mounts"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-view-table-label")).text.as_deref(),
        Some(t("workflows.viewTable"))
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-step-0-view-chart-label")).text.as_deref(),
        Some(t("workflows.viewChart"))
    );
    // Table is the default view; the chart frame is not mounted.
    assert!(has_key(&rt, &format!("{PREFIX}-step-0-table")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-chart")));

    // Pressing the chart entry flips to the chart frame (T104's honest
    // placeholder inside React's 260pt geometry).
    click(&mut rt, &format!("{PREFIX}-step-0-view-chart"));
    assert!(has_key(&rt, &format!("{PREFIX}-step-0-chart")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-table")));
    let chart = element(&rt, &format!("{PREFIX}-step-0-chart"));
    assert!(
        (chart.rect[3] - 260.0).abs() < 0.5,
        "the chart frame is 260pt tall: {:?}",
        chart.rect
    );
    assert!(has_key(&rt, &format!("{PREFIX}-step-0-chart-pending")));
    assert!(
        rt.state.chart.contains("data"),
        "the chart slot wrote the view mode into the host's state"
    );

    // Pressing the table entry flips back.
    click(&mut rt, &format!("{PREFIX}-step-0-view-table"));
    assert!(has_key(&rt, &format!("{PREFIX}-step-0-table")));
    assert!(!has_key(&rt, &format!("{PREFIX}-step-0-chart")));
    assert!(!rt.state.chart.contains("data"));

    // A purely categorical result is not chartable: no toggle at all —
    // exactly React's reachable behaviour (the control renders only when
    // chartable, so its disabled branch never runs).
    let categorical = data_step(json!([{ "a": "x" }, { "a": "y" }]), None);
    let mut cat = probe(run(vec![categorical]));
    click(&mut cat, &format!("{PREFIX}-step-0-row"));
    assert!(
        !has_key(&cat, &format!("{PREFIX}-step-0-view-toggle")),
        "a categorical result has no toggle"
    );
    assert!(has_key(&cat, &format!("{PREFIX}-step-0-table")));
    assert!(!has_key(&cat, &format!("{PREFIX}-step-0-chart")));

    // No data at all: no data section either.
    let empty = data_step(json!([]), None);
    let mut none = probe(run(vec![empty]));
    click(&mut none, &format!("{PREFIX}-step-0-row"));
    assert!(!has_key(&none, &format!("{PREFIX}-step-0-data")));
}

#[test]
fn the_chart_frame_warns_when_more_than_a_thousand_rows() {
    let rows: Vec<serde_json::Value> = (0..1001).map(|i| json!({ "a": i })).collect();
    let mut rt = probe(run(vec![data_step(json!(rows), None)]));
    click(&mut rt, &format!("{PREFIX}-step-0-row"));
    click(&mut rt, &format!("{PREFIX}-step-0-view-chart"));
    let warning = element(&rt, &format!("{PREFIX}-step-0-sampled-warning-text"));
    assert_eq!(
        warning.text.as_deref(),
        Some(t("workflows.sampledWarning").replace("{limit}", "1000").as_str())
    );

    // Exactly 1000 rows: no warning (React `> 1000`).
    let rows: Vec<serde_json::Value> = (0..1000).map(|i| json!({ "a": i })).collect();
    let mut edge = probe(run(vec![data_step(json!(rows), None)]));
    click(&mut edge, &format!("{PREFIX}-step-0-row"));
    click(&mut edge, &format!("{PREFIX}-step-0-view-chart"));
    assert!(!has_key(&edge, &format!("{PREFIX}-step-0-sampled-warning")));
}

#[test]
fn the_panel_fits_both_window_sizes_with_no_horizontal_overflow() {
    let rows: Vec<serde_json::Value> = (0..30)
        .map(|i| json!({ "a": i, "b": format!("value-{i}") }))
        .collect();
    let result = run(vec![
        data_step(json!(rows), None),
        step("fail", WorkflowStepStatus::Failed),
        step("skip", WorkflowStepStatus::Skipped),
    ]);
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(
            PanelState {
                result: Some(result.clone()),
                ..PanelState::default()
            },
            panel_view,
            (w, h),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        // Expand every step so the bodies are part of the layout, and put
        // the data step into chart mode for its widest frame.
        rt.state.expanded.insert("data".to_owned());
        rt.state.expanded.insert("fail".to_owned());
        rt.state.expanded.insert("skip".to_owned());
        rt.state.chart.insert("data".to_owned());
        rt.frame(0.0);
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with(PREFIX) {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(
            outside.is_empty(),
            "{w}x{h}: {} panel element(s) overflow horizontally: {outside:?}",
            outside.len()
        );
    }
}

// ---- the helper slice (T176-pending) ----------------------------------------

#[test]
fn the_helper_slice_matches_react_semantics() {
    // from_step_results: the honest projection of the lite report.
    let lite = vec![
        datazen_ui_api::WorkflowStepResult {
            step: "SELECT 1".into(),
            ok: true,
            summary: "ok".into(),
        },
        datazen_ui_api::WorkflowStepResult {
            step: "apply plan".into(),
            ok: false,
            summary: "refused".into(),
        },
    ];
    let vm = WorkflowExecutionResult::from_step_results(&lite);
    assert!(!vm.success);
    assert_eq!(vm.steps.len(), 2);
    assert_eq!(vm.steps[0].step_id, "SELECT 1");
    assert_eq!(vm.steps[0].status, WorkflowStepStatus::Success);
    assert_eq!(vm.steps[0].error, None);
    assert_eq!(vm.steps[1].status, WorkflowStepStatus::Failed);
    assert_eq!(vm.steps[1].error.as_deref(), Some("refused"));
    assert_eq!(vm.total_time_ms, 0);
    assert!(vm.final_output.is_empty());
    assert!(vm.error.is_none());

    // The status ladder: unknown strings are failures.
    assert_eq!(
        WorkflowStepStatus::from_str("success"),
        WorkflowStepStatus::Success
    );
    assert_eq!(
        WorkflowStepStatus::from_str("skipped"),
        WorkflowStepStatus::Skipped
    );
    assert_eq!(
        WorkflowStepStatus::from_str("timed_out"),
        WorkflowStepStatus::TimedOut
    );
    assert_eq!(
        WorkflowStepStatus::from_str("failed"),
        WorkflowStepStatus::Failed
    );
    assert_eq!(
        WorkflowStepStatus::from_str("bogus"),
        WorkflowStepStatus::Failed
    );

    // Column extraction: declared columns win, then object keys.
    let declared = json!({
        "columns": [{ "name": "a", "dataType": "integer" }],
        "rows": [{ "a": 1, "b": 2 }]
    });
    assert_eq!(
        extract_step_column_names(&declared),
        vec![StepColumn {
            name: "a".into(),
            data_type: Some("integer".into())
        }]
    );
    let cols = extract_step_column_names(&json!({ "rows": [{ "b": 2, "a": 1 }] }));
    assert_eq!(cols.len(), 2);
    assert!(cols.iter().any(|c| c.name == "a" && c.data_type.is_none()));
    assert!(extract_step_column_names(&json!({})).is_empty());

    // js_string mirrors String(value).
    assert_eq!(js_string(&json!("x")), "x");
    assert_eq!(js_string(&json!(1.5)), "1.5");
    assert_eq!(js_string(&json!(true)), "true");
    assert_eq!(js_string(&json!([1, "y"])), "1,y");
    assert_eq!(js_string(&json!({ "o": 1 })), "[object Object]");
    assert_eq!(js_string(&json!(null)), "null");

    // Chartability: declared numeric, sampled numeric, numeric-like strings,
    // categorical, boolean, datetime-typed, empty.
    let preview_of = |step: StepExecutionResult| step_to_statement_preview(&step);
    let declared_numeric = preview_of(data_step(
        json!([{ "a": "x" }]),
        Some(json!([{ "name": "a", "dataType": "integer" }])),
    ))
    .expect("a preview");
    assert!(
        is_chartable_preview(&declared_numeric),
        "a declared integer column is chartable"
    );
    let sampled_numeric = preview_of(data_step(json!([{ "a": 3 }]), None)).expect("a preview");
    assert!(is_chartable_preview(&sampled_numeric));
    let sampled_string_numbers =
        preview_of(data_step(json!([{ "a": "3" }]), None)).expect("a preview");
    assert!(
        is_chartable_preview(&sampled_string_numbers),
        "numeric-like strings are chartable"
    );
    let categorical = preview_of(data_step(json!([{ "a": "x" }]), None)).expect("a preview");
    assert!(!is_chartable_preview(&categorical));
    let bool_declared = preview_of(data_step(
        json!([{ "a": 1 }]),
        Some(json!([{ "name": "a", "dataType": "boolean" }])),
    ))
    .expect("a preview");
    assert!(!is_chartable_preview(&bool_declared), "a boolean column is not numeric");
    // A datetime-typed column stays non-numeric even with numeric samples —
    // React resolves the datetime family before the sample checks.
    let datetime_declared = preview_of(data_step(
        json!([{ "a": 20240101 }]),
        Some(json!([{ "name": "a", "dataType": "timestamp" }])),
    ))
    .expect("a preview");
    assert!(
        !is_chartable_preview(&datetime_declared),
        "a timestamp-typed column infers datetime, not numeric"
    );
    assert!(
        step_to_statement_preview(&data_step(json!([]), None)).is_none(),
        "no rows → no preview"
    );
}

// ---- the app wiring ------------------------------------------------------------

/// The panel and the action bar mount on the real workflows page over the
/// real run paths (the memory port's audit workflow fails its applying
/// step), through the same keys the page pins.
#[test]
fn the_workflows_page_mounts_the_bar_and_the_panel_over_a_real_run() {
    let mut rt = runtime();
    rt.frame(0.0);
    // Navigate to the workflows page and open the audit workflow.
    let (x, y) = rt.find("rail-workflows").expect("workflows navigation");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
    let (x, y) = rt.find("workflow-audit").expect("the audit workflow row");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);

    // The action bar replaced the form head's Save/Close pair.
    assert!(rt.find("workflow-editor-action-bar").is_some(), "the bar mounts");
    assert!(rt.find("workflow-save").is_some(), "Save keeps its pinned key");
    assert!(rt.find("workflow-editor-cancel").is_some(), "Cancel mounts");
    assert!(rt.find("workflow-run").is_some(), "Run stays in the head");
    // Cancel closes the editor (React's onCancel → closePanel).
    let (x, y) = rt.find("workflow-editor-cancel").expect("the cancel button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
    assert!(rt.state.workflow_editing.is_none(), "Cancel closed the editor");
    assert!(rt.find("workflow-form").is_none());

    // Reopen, run, and read the panel the projection filled.
    let (x, y) = rt.find("workflow-audit").expect("the audit workflow row");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
    let (x, y) = rt.find("workflow-run").expect("the run button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
    assert!(rt.state.workflow_execution.is_some(), "the run filled the VM");

    // The failed run's head: ✗ + t('common.executionFailed') and 0ms —
    // the lite report carries no timing, and the projection says so.
    let head = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-execution-result-status"))
        .expect("the panel head status");
    assert_eq!(
        head.text.as_deref(),
        Some(format!("✗ {}", t("common.executionFailed")).as_str())
    );
    let time = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-execution-result-total-time"))
        .expect("the panel head time");
    assert_eq!(time.text.as_deref(), Some("0ms"));

    // Two steps: the failing "apply plan" carries its summary as the error
    // line once expanded; the lite projection types no steps (no chip).
    assert!(rt.find("workflow-execution-result-step-0-row").is_some());
    assert!(rt.find("workflow-execution-result-step-1-row").is_some());
    let (x, y) = rt
        .find("workflow-execution-result-step-0-row")
        .expect("the first step row");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
    let error = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("workflow-execution-result-step-0-error"))
        .expect("the failing step's error line");
    assert_eq!(error.text.as_deref(), Some("the replica refused the plan"));
    assert!(
        rt.find("workflow-execution-result-step-0-type").is_none(),
        "the lite projection types no steps"
    );
    assert!(
        rt.find("workflow-execution-result-final-output").is_none(),
        "the lite report has no final output"
    );
}

/// The app wiring: closing the editor clears the panel's interaction state,
/// like React unmounting the panel resets its row `useState`s.
#[test]
fn closing_the_editor_clears_the_panel_state() {
    let mut rt = runtime();
    rt.frame(0.0);
    rt.state.active = crate::types::Section::Workflows;
    rt.state.edit_workflow("audit");
    rt.state.run_edited_workflow();
    rt.state.toggle_workflow_step_expanded("apply plan");
    assert!(rt.state.workflow_step_expanded.contains("apply plan"));
    assert!(rt.state.workflow_execution.is_some());
    rt.state.close_workflow_editor();
    assert!(rt.state.workflow_execution.is_none());
    assert!(rt.state.workflow_step_expanded.is_empty());
    assert!(rt.state.workflow_step_chart.is_empty());
}
