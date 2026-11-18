//! Independent audit for T030 (`ExplainPlanTree` port). Coverage-driven tests
//! for branches the coder's own suite does not pin: localized title (no key
//! echo), the exact chevron icon constants, two independent state slots,
//! colliding detail keys across different nodes, clipped+scrollable overflow,
//! and light-theme window fit. No existing assertion is weakened here.

use std::collections::HashSet;

use rune::{col, Cx, El, Env, Runtime};

use crate::components::query::explain_plan_tree::{
    toggle_node, ExplainPlanNode, ExplainPlanTree,
};
use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT};

const PREFIX: &str = "explain-plan-tree";

#[derive(Default)]
struct PlanState {
    overrides: HashSet<String>,
    toggles: usize,
}

#[derive(Default)]
struct DualState {
    a: HashSet<String>,
    b: HashSet<String>,
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn leaf(id: &str, label: &str) -> ExplainPlanNode {
    ExplainPlanNode {
        id: id.to_string(),
        label: label.to_string(),
        cost: None,
        rows: None,
        details: Vec::new(),
        children: Vec::new(),
    }
}

/// root(0) -> c1(1) -> g1(2) -> g2(3). Depth 2 is collapsed by default, so
/// g1 starts with a right chevron and its depth-3 child g2 is unmounted.
fn sample_tree() -> ExplainPlanNode {
    let g1 = ExplainPlanNode {
        id: "g1".into(),
        label: "Index Scan".into(),
        cost: None,
        rows: None,
        details: Vec::new(),
        children: vec![leaf("g2", "Filter")],
    };
    let c1 = ExplainPlanNode {
        id: "c1".into(),
        label: "Nested Loop".into(),
        cost: None,
        rows: None,
        details: Vec::new(),
        children: vec![g1],
    };
    ExplainPlanNode {
        id: "root".into(),
        label: "Seq Scan".into(),
        cost: Some(1.0),
        rows: None,
        details: Vec::new(),
        children: vec![c1],
    }
}

/// Parent and child carry a detail with the *same* key name ("phase") but with
/// different values, to prove per-node element keys cannot collide.
fn shared_detail_key_tree() -> ExplainPlanNode {
    let child = ExplainPlanNode {
        id: "child".into(),
        label: "Child".into(),
        cost: None,
        rows: None,
        details: vec![("phase".into(), "inner".into())],
        children: Vec::new(),
    };
    ExplainPlanNode {
        id: "parent".into(),
        label: "Parent".into(),
        cost: None,
        rows: None,
        details: vec![("phase".into(), "outer".into())],
        children: vec![child],
    }
}

/// A 20-row tree whose content far exceeds the 280pt scroll viewport.
fn wide_tree() -> ExplainPlanNode {
    let long = "x".repeat(240);
    let children = (0..20)
        .map(|i| ExplainPlanNode {
            id: format!("w{i}"),
            label: format!("child {i}"),
            cost: Some(i as f64),
            rows: Some(i),
            details: (0..8).map(|j| ("dup".into(), format!("{long}-{j}"))).collect(),
            children: Vec::new(),
        })
        .collect();
    ExplainPlanNode {
        id: "wr".into(),
        label: "wide root".into(),
        cost: None,
        rows: None,
        details: Vec::new(),
        children,
    }
}

fn build(plan: Option<ExplainPlanNode>, state: &PlanState, dark: bool) -> El {
    ExplainPlanTree::<PlanState>::new(
        plan,
        |s: &PlanState| &s.overrides,
        |s: &mut PlanState, id| {
            s.toggles += 1;
            toggle_node(&mut s.overrides, id);
        },
    )
    .build(state, dark)
}

fn plan_view(state: &PlanState, _cx: &mut Cx<PlanState>) -> El {
    col().w_full().h_full().padding(16.0).child(build(Some(sample_tree()), state, true))
}

fn plan_view_light(state: &PlanState, _cx: &mut Cx<PlanState>) -> El {
    col().w_full().h_full().padding(16.0).child(build(Some(sample_tree()), state, false))
}

fn wide_view(state: &PlanState, _cx: &mut Cx<PlanState>) -> El {
    col().w_full().h_full().padding(16.0).child(build(Some(wide_tree()), state, true))
}

fn wide_view_light(state: &PlanState, _cx: &mut Cx<PlanState>) -> El {
    col().w_full().h_full().padding(16.0).child(build(Some(wide_tree()), state, false))
}

fn detail_view(state: &PlanState, _cx: &mut Cx<PlanState>) -> El {
    col().w_full().h_full().padding(16.0).child(build(Some(shared_detail_key_tree()), state, true))
}

fn dual_view(state: &DualState, _cx: &mut Cx<DualState>) -> El {
    let a = ExplainPlanTree::<DualState>::new(
        Some(sample_tree()),
        |s: &DualState| &s.a,
        |s: &mut DualState, id| {
            toggle_node(&mut s.a, id);
        },
    )
    .key("tree-a")
    .build(state, true);
    let b = ExplainPlanTree::<DualState>::new(
        Some(sample_tree()),
        |s: &DualState| &s.b,
        |s: &mut DualState, id| {
            toggle_node(&mut s.b, id);
        },
    )
    .key("tree-b")
    .build(state, true);
    col().w_full().h_full().padding(16.0).gap(8.0).child(a).child(b)
}

fn probe(view: fn(&PlanState, &mut Cx<PlanState>) -> El, size: (f32, f32), dark: bool) -> Runtime<PlanState> {
    let mut rt = Runtime::new(PlanState::default(), view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter_map(|e| {
            let key = e.key.as_deref()?;
            if !key.starts_with(PREFIX) {
                return None;
            }
            let [x, _y, ew, _eh] = e.rect;
            (x < -0.5 || x + ew > w + 0.5).then(|| format!("{key} {:?}", e.rect))
        })
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) past {w}pt: {outside:?}", outside.len());
}

// ---- title: localized text, no key echo ----

#[test]
fn title_is_the_localized_zh_text_not_a_key_or_english_echo() {
    let rt = probe(plan_view, (1440.0, 816.0), true);
    let title = element(&rt, &format!("{PREFIX}-title")).text.unwrap();
    let resolved = t("explain.planTree");

    assert_eq!(resolved, "计划树", "the i18n key resolves to a zh-CN translation");
    assert_eq!(title, resolved, "the title renders that resolved value");
    assert_ne!(title, "explain.planTree", "the raw i18n key must not be echoed");
    assert_ne!(title, "Plan Tree", "the English source string must not leak through");
}

// ---- chevrons: exact registry constants, and they flip ----

#[test]
fn chevrons_use_the_shared_registry_constants_and_flip_with_expansion() {
    // Functional flip: g1 is depth 2 (collapsed => right), click => down.
    let mut rt = probe(plan_view, (1440.0, 816.0), true);
    assert!(has_key(&rt, &format!("{PREFIX}-node-g1-glyph-right")));
    assert!(!has_key(&rt, &format!("{PREFIX}-node-g1-glyph-down")));
    click(&mut rt, &format!("{PREFIX}-node-g1-row"));
    assert!(has_key(&rt, &format!("{PREFIX}-node-g1-glyph-down")));
    assert!(!has_key(&rt, &format!("{PREFIX}-node-g1-glyph-right")));

    // The glyph keys alone would still pass if both arms drew the same icon,
    // so pin the exact constants used by each arm at the source level.
    let src = include_str!("../components/query/explain_plan_tree.rs");
    let start = src.find("let chevron = if has_children").expect("chevron block");
    let end = src.find("// Header:").expect("header marker");
    let block = &src[start..end];
    let expanded_pos = block.find("if expanded").expect("expanded arm");
    let down_pos = block.find("ICON_CHEVRON_DOWN").expect("down constant");
    let right_pos = block.find("ICON_CHEVRON_RIGHT").expect("right constant");
    assert!(
        down_pos > expanded_pos && down_pos < right_pos,
        "expanded arm must select ICON_CHEVRON_DOWN before the collapsed arm selects RIGHT"
    );
    assert!(!src.contains("Icon {"), "no inline icon definition; registry is the only source");

    // The two constants are genuinely different glyphs from that registry.
    assert_eq!(ICON_CHEVRON_DOWN.name, "chevron-down");
    assert_eq!(ICON_CHEVRON_RIGHT.name, "chevron-right");
    assert_ne!(ICON_CHEVRON_DOWN.body, ICON_CHEVRON_RIGHT.body);
}

// ---- state lives in S: two slots independent, toggle survives frames ----

#[test]
fn two_trees_own_separate_state_slots_and_toggles_survive_reframes() {
    let mut rt = Runtime::new(DualState::default(), dual_view, (1440.0, 816.0), env(true));
    rt.frame(0.0);

    assert!(!has_key(&rt, "tree-a-node-g2-row"));
    assert!(!has_key(&rt, "tree-b-node-g2-row"));

    click(&mut rt, "tree-a-node-g1-row");
    assert!(has_key(&rt, "tree-a-node-g2-row"), "tree A expanded");
    assert!(!has_key(&rt, "tree-b-node-g2-row"), "tree B must NOT inherit A's toggle");
    assert_eq!(rt.state.a.len(), 1);
    assert!(rt.state.b.is_empty());

    // Re-render many frames: the toggle is held in S, not lost on rebuild.
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    assert!(has_key(&rt, "tree-a-node-g2-row"), "toggle persists across frames");
    assert!(!has_key(&rt, "tree-b-node-g2-row"), "still no cross-tree leak after reframes");

    click(&mut rt, "tree-b-node-g1-row");
    assert!(has_key(&rt, "tree-b-node-g2-row"), "tree B toggles on its own slot");
    assert!(has_key(&rt, "tree-a-node-g2-row"), "tree A unaffected");
    assert_eq!(rt.state.a.len(), 1);
    assert_eq!(rt.state.b.len(), 1);
}

// ---- detail keys are unique even when the name repeats under other nodes ----

#[test]
fn identical_detail_key_names_under_different_nodes_do_not_collide() {
    let rt = probe(detail_view, (1440.0, 816.0), true);

    let pk = format!("{PREFIX}-node-parent-detail-0-key");
    let ck = format!("{PREFIX}-node-child-detail-0-key");
    assert!(has_key(&rt, &pk) && has_key(&rt, &ck), "both same-named details mount");
    assert_ne!(pk, ck, "per-node prefixes keep the element keys distinct");
    assert_eq!(element(&rt, &pk).text.as_deref(), Some("phase: "));
    assert_eq!(element(&rt, &ck).text.as_deref(), Some("phase: "));
    assert_eq!(element(&rt, &format!("{PREFIX}-node-parent-detail-0-value")).text.as_deref(), Some("outer"));
    assert_eq!(element(&rt, &format!("{PREFIX}-node-child-detail-0-value")).text.as_deref(), Some("inner"));

    let mut detail_keys: Vec<String> = rt
        .elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with(PREFIX) && k.contains("-detail-"))
        .collect();
    let count = detail_keys.len();
    detail_keys.sort();
    detail_keys.dedup();
    assert_eq!(detail_keys.len(), count, "no duplicate detail element keys anywhere");
}

// ---- bounded + clipped + actually scrollable ----

#[test]
fn scroll_box_clips_overflow_and_can_be_scrolled() {
    let mut rt = probe(wide_view, (1440.0, 816.0), true);
    let scroll = element(&rt, &format!("{PREFIX}-scroll"));
    let last_key = format!("{PREFIX}-node-w19-row");

    let last = element(&rt, &last_key);
    assert!(
        last.rect[1] + last.rect[3] > scroll.rect[1] + scroll.rect[3],
        "the last row really overflows the scroll viewport"
    );
    assert!(
        last.visible[3] <= 1.0,
        "the overflowing row has no visible area (fully clipped by the scroll viewport): {:?}",
        last.visible
    );

    let before = last.rect[1];
    rt.update(|_, tx| tx.scroll_to_instant(format!("{PREFIX}-scroll"), 0.0, 600.0));
    rt.frame(1.0 / 60.0);
    let after = element(&rt, &last_key).rect[1];
    assert!(after < before - 100.0, "scrolling moved content up: {before} -> {after}");
}

// ---- light theme fits both windows (rect-based, not self-reported) ----

#[test]
fn light_and_dark_fit_both_window_sizes() {
    for (dark, view, label) in [
        (true, plan_view as fn(&PlanState, &mut Cx<PlanState>) -> El, "plan/dark"),
        (false, plan_view_light, "plan/light"),
    ] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let rt = probe(view, (w, h), dark);
            assert_no_horizontal_overflow(&rt, w, &format!("{label} {w}x{h}"));
        }
    }
    for (dark, view, label) in [
        (true, wide_view as fn(&PlanState, &mut Cx<PlanState>) -> El, "wide/dark"),
        (false, wide_view_light, "wide/light"),
    ] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let rt = probe(view, (w, h), dark);
            assert_no_horizontal_overflow(&rt, w, &format!("{label} {w}x{h}"));
            let scroll = element(&rt, &format!("{PREFIX}-scroll"));
            assert!(scroll.rect[3] <= 280.5, "{label} {w}x{h} scroll clamp: {:?}", scroll.rect);
        }
    }
}
