//! T260 independent audit: fresh vectors, none shared with the coder
//! file. The angles: the plan shapes that are objects but not result
//! sets (numeric column lists, nested rows), the analysis with empty
//! bottleneck/suggestion lists rendering only the summary card (React's
//! `length > 0` gates), the severity card's paints read back from the
//! mounted tree (the danger wash for high, the accent wash for low),
//! a suggestion without SQL carrying neither the SQL block nor the
//! apply button, the raw-table counts for an asymmetric result set, the
//! unconfigured panel never analyzing, and a window-wide geometry
//! sweep.

use rune::{col, Cx, El, Env, Runtime};
use serde_json::json;

use crate::ai_store::{ExplainAnalysis, ExplainBottleneck, ExplainSeverity, ExplainSuggestion};
use crate::components::ai::explain_panel::{
    raw_explain_table, ExplainPanel, ExplainPanelVm,
};
use crate::components::query::explain_plan_tree::ExpandOverrides;
use crate::i18n::t;
use crate::ui::kit::kit;

const W: f32 = 900.0;
const H: f32 = 600.0;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

#[derive(Default)]
struct Scene {
    vm: ExplainPanelVm,
    overrides: ExpandOverrides,
    analyze_requests: usize,
    applied: Vec<String>,
    settings_opens: usize,
    dark: bool,
}

fn overrides_of(s: &Scene) -> &ExpandOverrides {
    &s.overrides
}

fn toggle_override(s: &mut Scene, id: String) {
    crate::components::query::explain_plan_tree::toggle_node(&mut s.overrides, id);
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let panel = ExplainPanel::<Scene>::new(&scene.vm)
        .plan_expansions(overrides_of, toggle_override)
        .on_analyze(|s: &mut Scene, _request| s.analyze_requests += 1)
        .on_apply_sql(|s: &mut Scene, sql| s.applied.push(sql.to_owned()))
        .on_open_ai_settings(|s: &mut Scene| s.settings_opens += 1)
        .build(scene, scene.dark, &mut layer);
    let mut root = col().key("audit-host").w_full().h_full().child(panel);
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

/// The audit props: a light-mode panel over a MySQL-style result set
/// with an asymmetric shape.
fn audit_vm() -> ExplainPanelVm {
    ExplainPanelVm {
        db_session_id: "sess-a".to_owned(),
        sql: "EXPLAIN ANALYZE SELECT 1".to_owned(),
        explain_output: "plan text".to_owned(),
        is_configured: true,
        ..Default::default()
    }
}

fn bottleneck(word: &str, node: &str) -> ExplainBottleneck {
    ExplainBottleneck {
        node: node.to_owned(),
        description: format!("the {node} cost"),
        severity: ExplainSeverity::from_wire(word),
    }
}

#[test]
fn an_object_plan_that_is_not_a_result_set_falls_back_to_the_pre() {
    // Objects, but not the columns/rows result set: a detail-only plan,
    // a plan with a nested rows object, and a plan whose rows hold
    // non-array members (React filters those away — the table reads
    // only the array rows).
    let plan = json!({"plan_text": "Hash Join  (cost=1.1..2.2)"});
    assert!(raw_explain_table(Some(&plan)).is_none(), "a text plan is not a result set");

    let plan = json!({"columns": ["id"], "rows": {"first": [1]}});
    assert!(raw_explain_table(Some(&plan)).is_none(), "a rows object is not an array");

    // The mixed-rows shape: the array rows survive the filter, the
    // others drop without failing the branch.
    let plan = json!({
        "columns": ["id"],
        "rows": [[1], "not a row", {"key": 2}]
    });
    let raw = raw_explain_table(Some(&plan)).expect("the array rows still read");
    assert_eq!(raw.rows, vec![vec![json!(1)]]);
}

#[test]
fn an_analysis_with_empty_lists_renders_only_the_summary() {
    // React 163/191: `analysis.bottlenecks.length > 0` and
    // `analysis.suggestions.length > 0` gate the two lists — an
    // empty-list analysis still shows the summary card alone.
    let mut vm = audit_vm();
    vm.analysis = Some(ExplainAnalysis {
        summary: "The plan is already tight.".to_owned(),
        ..Default::default()
    });
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    assert_eq!(text_of(&rt, "explain-panel-summary-text"), "The plan is already tight.");
    assert!(!has_key(&rt, "explain-panel-bottlenecks"), "no bottleneck list");
    assert!(!has_key(&rt, "explain-panel-suggestions"), "no suggestion list");
}

#[test]
fn the_severity_cards_paint_their_own_washes() {
    // React 25–29: `high` is the danger wash, `low` the accent wash —
    // read back from the mounted card fills, through the kit the panel
    // itself painted with.
    let mut vm = audit_vm();
    vm.analysis = Some(ExplainAnalysis {
        summary: "two costs".to_owned(),
        bottlenecks: vec![
            bottleneck("high", "the_scan"),
            bottleneck("medium", "the_join"),
            bottleneck("low", "the_filter"),
        ],
        ..Default::default()
    });
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    let k = kit(true);
    assert_eq!(maybe(&rt, "explain-panel-bottleneck-0").unwrap().fill, k.alpha(k.c.danger.bg, 0.10));
    assert_eq!(
        maybe(&rt, "explain-panel-bottleneck-1").unwrap().fill,
        k.alpha(k.c.warning.bg, 0.10)
    );
    assert_eq!(maybe(&rt, "explain-panel-bottleneck-2").unwrap().fill, k.alpha(k.c.bg_accent, 0.10));
    // The dot carries the plain severity fill (React 31–35).
    assert_eq!(maybe(&rt, "explain-panel-bottleneck-0-dot").unwrap().fill, k.c.danger.bg);
    assert_eq!(maybe(&rt, "explain-panel-bottleneck-2-dot").unwrap().fill, k.c.bg_accent);
}

#[test]
fn a_suggestion_without_sql_has_no_block_and_no_apply() {
    let mut vm = audit_vm();
    vm.analysis = Some(ExplainAnalysis {
        summary: "one prose suggestion".to_owned(),
        suggestions: vec![ExplainSuggestion {
            description: "Rewrite the predicate by hand.".to_owned(),
            sql: None,
            impact: "The filter stays sargable.".to_owned(),
        }],
        ..Default::default()
    });
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    assert!(has_key(&rt, "explain-panel-suggestion-0"));
    assert!(!has_key(&rt, "explain-panel-suggestion-0-sql"));
    assert!(!has_key(&rt, "explain-panel-suggestion-0-sql-block"));
    assert!(!has_key(&rt, "explain-panel-suggestion-0-apply"));
    // The impact line still lands.
    assert_eq!(
        text_of(&rt, "explain-panel-suggestion-0-impact"),
        format!("{}: {}", t("explain.expectedImpact"), "The filter stays sargable.")
    );
}

#[test]
fn the_raw_table_counts_the_actual_rows_and_columns() {
    let mut vm = audit_vm();
    vm.plan_json = Some(json!({
        "columns": ["step"],
        "rows": [["query_plan"], ["index_scan"], ["nested_loop"], "dropped"]
    }));
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    // Three array rows survive the filter, one string drops.
    assert_eq!(
        text_of(&rt, "explain-panel-raw-status-rows"),
        format!("3 {}", t("common.rows"))
    );
    assert_eq!(
        text_of(&rt, "explain-panel-raw-status-columns"),
        format!("1 {}", t("common.columns"))
    );
}

#[test]
fn the_unconfigured_panel_mounts_no_analyze_button_and_never_analyzes() {
    let mut vm = audit_vm();
    vm.is_configured = false;
    let mut rt = mount(Scene { vm, dark: false, ..Default::default() });

    assert!(!has_key(&rt, "explain-panel-analyze"), "React 92 needs isConfigured");
    assert!(has_key(&rt, "explain-panel-unconfigured"));
    // The configure press opens settings; it must not start an analysis.
    click(&mut rt, "explain-panel-unconfigured-settings");
    assert_eq!(rt.state.settings_opens, 1);
    assert_eq!(rt.state.analyze_requests, 0);
}

#[test]
fn the_panel_stays_inside_the_window() {
    let mut vm = audit_vm();
    vm.plan_json = Some(json!({
        "columns": ["id", "select_type", "table", "rows", "filtered"],
        "rows": [[1, "SIMPLE", "orders", 500, 100.0]]
    }));
    vm.analysis = Some(ExplainAnalysis {
        summary: "a long-enough summary to wrap".to_owned(),
        bottlenecks: vec![bottleneck("high", "the_scan")],
        suggestions: vec![ExplainSuggestion {
            description: "Add the index.".to_owned(),
            sql: Some("CREATE INDEX ON orders (customer_id)".to_owned()),
            impact: "The scan becomes a seek.".to_owned(),
        }],
        ..Default::default()
    });
    let rt = mount(Scene { vm, dark: true, ..Default::default() });

    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else { continue };
        if !key.starts_with("explain-panel") {
            continue;
        }
        let [x, _y, w, _h] = e.rect;
        if x < -0.5 || x + w > W + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(outside.is_empty(), "{outside:?} escape the {W}x{H} window");
}
