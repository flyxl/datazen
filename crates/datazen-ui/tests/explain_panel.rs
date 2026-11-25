//! T260: the EXPLAIN panel — coder tests.
//!
//! The pure half: the `rawTable` derivation reading the result-set
//! shape (columns + rows) and falling back for every other plan shape,
//! and the severity wire degradation.
//!
//! The mounted half: the raw `<pre>` branch, the raw-table branch with
//! the count status bar, the analyze button's mount gate and its
//! three-field request, the analyzing spinner row, the error card, the
//! analysis sections (the summary, the severity cards, the suggestion
//! cards with their SQL, apply, and impact), the unconfigured notice
//! and its configure button, and the plan-tree section's mount
//! condition.

use rune::{col, Cx, El, Env, Runtime};
use serde_json::json;

use datazen_ui::ai_store::{ExplainBottleneck, ExplainSeverity, ExplainSuggestion};
use datazen_ui::components::ai::explain_panel::{
    raw_explain_table, AnalyzeExplainRequest, ExplainPanel, ExplainPanelVm,
};
use datazen_ui::components::query::explain_plan_tree::{
    toggle_node, ExplainPlanNode, ExpandOverrides,
};
use datazen_ui::i18n::t;

const W: f32 = 800.0;
const H: f32 = 600.0;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

// ---------------------------------------------------------------------------
// The pure half (React 52–70, 25–35)
// ---------------------------------------------------------------------------

#[test]
fn raw_explain_table_reads_the_result_set_shape() {
    // MySQL / Kiwi: the plan JSON carries the raw result set.
    let plan = json!({
        "columns": ["id", "select_type"],
        "rows": [[1, "SIMPLE"], [2, "SIMPLE"]]
    });
    let raw = raw_explain_table(Some(&plan)).expect("the result set reads");
    assert_eq!(raw.columns.len(), 2);
    assert_eq!(raw.columns[0].name, "id");
    assert_eq!(raw.rows, vec![
        vec![json!(1), json!("SIMPLE")],
        vec![json!(2), json!("SIMPLE")]
    ]);
}

#[test]
fn raw_explain_table_falls_back_for_every_non_result_shape() {
    // React 52–57: `!planJson`, a non-object, a missing or non-array
    // `columns` / `rows`, and (60) an empty column list after the
    // string filter all leave the `<pre>` branch.
    assert!(raw_explain_table(None).is_none(), "no plan at all");
    assert!(raw_explain_table(Some(&json!([1, 2]))).is_none(), "an array is not an object");
    assert!(raw_explain_table(Some(&json!({}))).is_none(), "no columns key");
    assert!(raw_explain_table(Some(&json!({"columns": "id"}))).is_none(), "a non-array column list");
    assert!(raw_explain_table(Some(&json!({"columns": ["id"]}))).is_none(), "no rows key");
    assert!(raw_explain_table(Some(&json!({"columns": ["id"], "rows": 3}))).is_none(), "a non-array row list");
    assert!(raw_explain_table(Some(&json!({"columns": [7, true], "rows": [[1]]}))).is_none(), "no string column at all");
    assert!(raw_explain_table(Some(&json!({"columns": [], "rows": []}))).is_none(), "the string filter empties the list");
}

#[test]
fn the_severity_wire_degrades_unknown_words_to_low() {
    // The typed union knows three words; anything else takes React's
    // `?? severityColors.low` degradation at the parse boundary.
    assert_eq!(ExplainSeverity::from_wire("high"), ExplainSeverity::High);
    assert_eq!(ExplainSeverity::from_wire("medium"), ExplainSeverity::Medium);
    assert_eq!(ExplainSeverity::from_wire("low"), ExplainSeverity::Low);
    assert_eq!(ExplainSeverity::from_wire("critical"), ExplainSeverity::Low);
    assert_eq!(ExplainSeverity::from_wire(""), ExplainSeverity::Low);
}

// ---------------------------------------------------------------------------
// The mounted half
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Scene {
    vm: ExplainPanelVm,
    overrides: ExpandOverrides,
    analyze_requests: Vec<AnalyzeExplainRequest>,
    applied: Vec<String>,
    settings_opens: usize,
    dark: bool,
}

fn overrides_of(s: &Scene) -> &ExpandOverrides {
    &s.overrides
}

fn toggle_override(s: &mut Scene, id: String) {
    toggle_node(&mut s.overrides, id);
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let panel = ExplainPanel::<Scene>::new(&scene.vm)
        .plan_expansions(overrides_of, toggle_override)
        .on_analyze(|s: &mut Scene, request| s.analyze_requests.push(request))
        .on_apply_sql(|s: &mut Scene, sql| s.applied.push(sql.to_owned()))
        .on_open_ai_settings(|s: &mut Scene| s.settings_opens += 1)
        .build(scene, scene.dark, &mut layer);
    let mut root = col().key("panel-host").w_full().h_full().child(panel);
    for el in layer {
        root = root.child(el);
    }
    root
}

fn mount(scene: Scene) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, (W, H), env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// The coder vectors' props: a configured panel over a plain-text plan.
fn base_vm() -> ExplainPanelVm {
    ExplainPanelVm {
        db_session_id: "sess-9".to_owned(),
        sql: "SELECT * FROM orders".to_owned(),
        explain_output: "Seq Scan on orders  (cost=0.00..18.50 rows=500)".to_owned(),
        is_configured: true,
        ..Default::default()
    }
}

/// The coder vectors' analysis: one bottleneck per severity pair and
/// two suggestions, one carrying SQL.
fn analysis_vm() -> ExplainPanelVm {
    let mut vm = base_vm();
    vm.analysis = Some(datazen_ui::ai_store::ExplainAnalysis {
        summary: "The plan scans the whole table.".to_owned(),
        bottlenecks: vec![
            ExplainBottleneck {
                node: "orders_seq_scan".to_owned(),
                description: "A sequential scan reads every row.".to_owned(),
                severity: ExplainSeverity::High,
            },
            ExplainBottleneck {
                node: "orders_sort".to_owned(),
                description: "An in-memory sort carries the whole set.".to_owned(),
                severity: ExplainSeverity::Low,
            },
        ],
        suggestions: vec![
            ExplainSuggestion {
                description: "Add an index on the customer id.".to_owned(),
                sql: Some("CREATE INDEX ON orders (customer_id)".to_owned()),
                impact: "The scan becomes an index seek.".to_owned(),
            },
            ExplainSuggestion {
                description: "Keep the result small early.".to_owned(),
                sql: None,
                impact: "Fewer rows travel through the sort.".to_owned(),
            },
        ],
        ..Default::default()
    });
    vm
}

#[test]
fn the_raw_pre_branch_shows_the_plain_output() {
    let rt = mount(Scene { vm: base_vm(), dark: true, ..Default::default() });

    assert!(has_key(&rt, "explain-panel-raw-pre"), "the plain-text branch mounts");
    assert_eq!(
        text_of(&rt, "explain-panel-raw-pre"),
        "Seq Scan on orders  (cost=0.00..18.50 rows=500)"
    );
    assert!(!has_key(&rt, "explain-panel-raw-table"), "no result set to mount");
}

#[test]
fn the_raw_table_branch_mounts_the_result_set_with_the_count_bar() {
    let mut vm = base_vm();
    vm.plan_json = Some(json!({
        "columns": ["id", "select_type"],
        "rows": [[1, "SIMPLE"], [2, "SIMPLE"]]
    }));
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    assert!(has_key(&rt, "explain-panel-raw-table"), "the result-set branch mounts");
    assert!(!has_key(&rt, "explain-panel-raw-pre"), "the pre branch stays down");
    // React 113–117: the count bar — 2 rows, 2 columns.
    assert_eq!(text_of(&rt, "explain-panel-raw-status-rows"), format!("2 {}", t("common.rows")));
    assert_eq!(text_of(&rt, "explain-panel-raw-status-columns"), format!("2 {}", t("common.columns")));
    // The grid itself mounts (its own root key).
    assert!(has_key(&rt, "data-table"));
}

#[test]
fn the_analyze_button_mounts_only_while_configured_and_idle() {
    // React 92: `isConfigured && !analysis && !isAnalyzing`.
    let rt = mount(Scene { vm: base_vm(), dark: true, ..Default::default() });
    assert!(has_key(&rt, "explain-panel-analyze"));

    let rt = mount(Scene { vm: analysis_vm(), dark: true, ..Default::default() });
    assert!(!has_key(&rt, "explain-panel-analyze"), "an analysis is already showing");

    let mut vm = base_vm();
    vm.is_analyzing = true;
    let rt = mount(Scene { vm, dark: true, ..Default::default() });
    assert!(!has_key(&rt, "explain-panel-analyze"), "a run is in flight");

    let mut vm = base_vm();
    vm.is_configured = false;
    let rt = mount(Scene { vm, dark: true, ..Default::default() });
    assert!(!has_key(&rt, "explain-panel-analyze"), "the unconfigured panel never analyzes");
}

#[test]
fn the_analyze_press_carries_the_three_fields() {
    // React 76–79: `analyzeExplain({ dbSessionId, explainOutput,
    // originalSql })`.
    let mut rt = mount(Scene { vm: base_vm(), dark: true, ..Default::default() });

    click(&mut rt, "explain-panel-analyze");
    let requests = &rt.state.analyze_requests;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].db_session_id, "sess-9");
    assert_eq!(requests[0].explain_output, "Seq Scan on orders  (cost=0.00..18.50 rows=500)");
    assert_eq!(requests[0].original_sql, "SELECT * FROM orders");
}

#[test]
fn the_analyzing_row_mounts_the_spinner_and_label() {
    let mut vm = base_vm();
    vm.is_analyzing = true;
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    assert!(has_key(&rt, "explain-panel-analyzing"));
    assert!(has_key(&rt, "explain-panel-analyzing-spinner"));
    assert_eq!(text_of(&rt, "explain-panel-analyzing-text"), t("explain.analyzing"));
}

#[test]
fn the_error_card_carries_the_text() {
    let mut vm = base_vm();
    vm.explain_error = Some("the provider refused the plan".to_owned());
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    assert!(has_key(&rt, "explain-panel-error"));
    assert_eq!(text_of(&rt, "explain-panel-error"), "the provider refused the plan");
}

#[test]
fn the_analysis_sections_render_summary_bottlenecks_and_suggestions() {
    let rt = mount(Scene { vm: analysis_vm(), dark: true, ..Default::default() });

    // The summary card.
    assert_eq!(text_of(&rt, "explain-panel-summary-label"), t("explain.summary").to_uppercase());
    assert_eq!(text_of(&rt, "explain-panel-summary-text"), "The plan scans the whole table.");

    // The bottlenecks: one card each, with node and description.
    assert!(has_key(&rt, "explain-panel-bottleneck-0"));
    assert!(has_key(&rt, "explain-panel-bottleneck-0-dot"));
    assert_eq!(text_of(&rt, "explain-panel-bottleneck-0-node"), "orders_seq_scan");
    assert_eq!(
        text_of(&rt, "explain-panel-bottleneck-0-description"),
        "A sequential scan reads every row."
    );
    assert_eq!(text_of(&rt, "explain-panel-bottleneck-1-node"), "orders_sort");

    // The suggestions: the first carries SQL, an apply button, and the
    // impact line; the second carries no SQL block at all.
    assert_eq!(
        text_of(&rt, "explain-panel-suggestion-0-description"),
        "Add an index on the customer id."
    );
    assert_eq!(
        text_of(&rt, "explain-panel-suggestion-0-sql"),
        "CREATE INDEX ON orders (customer_id)"
    );
    assert!(has_key(&rt, "explain-panel-suggestion-0-apply"));
    assert_eq!(
        text_of(&rt, "explain-panel-suggestion-0-impact"),
        format!("{}: {}", t("explain.expectedImpact"), "The scan becomes an index seek.")
    );
    assert!(has_key(&rt, "explain-panel-suggestion-1"));
    assert!(!has_key(&rt, "explain-panel-suggestion-1-sql"), "no SQL to show");
    assert!(!has_key(&rt, "explain-panel-suggestion-1-apply"), "nothing to apply");
    assert_eq!(
        text_of(&rt, "explain-panel-suggestion-1-impact"),
        format!("{}: {}", t("explain.expectedImpact"), "Fewer rows travel through the sort.")
    );
}

#[test]
fn pressing_apply_carries_the_suggestion_sql() {
    let mut rt = mount(Scene { vm: analysis_vm(), dark: true, ..Default::default() });

    click(&mut rt, "explain-panel-suggestion-0-apply");
    assert_eq!(
        rt.state.applied,
        vec!["CREATE INDEX ON orders (customer_id)".to_owned()]
    );
    // The apply press is only the apply — no analysis clear, no close.
    assert!(rt.state.analyze_requests.is_empty());
}

#[test]
fn the_unconfigured_notice_mounts_with_the_configure_button() {
    let mut vm = base_vm();
    vm.is_configured = false;
    let mut rt = mount(Scene { vm, dark: true, ..Default::default() });

    assert!(has_key(&rt, "explain-panel-unconfigured"));
    assert_eq!(text_of(&rt, "explain-panel-unconfigured-text"), t("common.aiNotConfigured"));
    assert!(has_key(&rt, "explain-panel-unconfigured-settings"));

    click(&mut rt, "explain-panel-unconfigured-settings");
    assert_eq!(rt.state.settings_opens, 1);
}

#[test]
fn the_plan_tree_section_mounts_while_either_plan_input_exists() {
    // React 130–135: `planTree != null || planJson != null`.
    let rt = mount(Scene { vm: base_vm(), dark: true, ..Default::default() });
    assert!(!has_key(&rt, "explain-panel-plan"), "no plan input, no section");

    let mut vm = base_vm();
    vm.plan_json = Some(json!({"plan_text": "the text plan"}));
    let rt = mount(Scene { vm, dark: true, ..Default::default() });
    assert!(has_key(&rt, "explain-panel-plan"), "the plan JSON alone mounts the section");

    let mut vm = base_vm();
    vm.plan_tree = Some(ExplainPlanNode {
        id: "node-0".to_owned(),
        label: "Seq Scan".to_owned(),
        cost: None,
        rows: None,
        details: Vec::new(),
        children: Vec::new(),
    });
    let rt = mount(Scene { vm, dark: true, ..Default::default() });
    assert!(has_key(&rt, "explain-panel-plan"), "the tree alone mounts the section");
    assert!(has_key(&rt, "explain-plan-tree"), "the tree itself mounts");
}
