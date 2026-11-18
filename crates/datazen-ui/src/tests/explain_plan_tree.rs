//! T030: `ExplainPlanTree` port — recursive rows, expansion state kept in `S`,
//! React badge/detail formatting, bounded scroll container and window fit.

use std::collections::HashSet;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::query::explain_plan_tree::{
    default_expanded, is_node_expanded, toggle_node, ExplainPlanNode, ExplainPlanTree,
};
use crate::i18n::t;

const PREFIX: &str = "explain-plan-tree";

type ExpandOverrides = HashSet<String>;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Default)]
struct PlanState {
    overrides: HashSet<String>,
    toggles: usize,
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

/// root(0) -> c1(1) -> g1(2) -> g2(3), plus a leaf sibling c2(1).
/// Default expansion is depth 0/1 open, depth 2 closed, so g2 starts unmounted.
fn sample_tree() -> ExplainPlanNode {
    let g2 = leaf("g2", "Filter");
    let g1 = ExplainPlanNode {
        id: "g1".into(),
        label: "Index Scan".into(),
        cost: Some(1.0),
        rows: Some(7),
        details: Vec::new(),
        children: vec![g2],
    };
    let c1 = ExplainPlanNode {
        id: "c1".into(),
        label: "Nested Loop".into(),
        cost: None,
        rows: None,
        details: vec![("join".into(), "inner".into())],
        children: vec![g1],
    };
    let c2 = ExplainPlanNode {
        id: "c2".into(),
        label: "Sort".into(),
        cost: Some(0.5),
        rows: None,
        details: Vec::new(),
        children: Vec::new(),
    };
    ExplainPlanNode {
        id: "root".into(),
        label: "Seq Scan on users".into(),
        cost: Some(3.14159),
        rows: Some(42),
        // Eight details, with colliding keys in the first two, to prove the
        // six-line cap and the collision-safe index suffix.
        details: (0..8)
            .map(|i| {
                (
                    if i < 2 { "dup".into() } else { format!("key{i}") },
                    format!("value {i}"),
                )
            })
            .collect(),
        children: vec![c1, c2],
    }
}

/// A 20-child tree with 8 details per child; content far exceeds 280pt.
fn wide_tree() -> ExplainPlanNode {
    let long = "x".repeat(240);
    let children = (0..20)
        .map(|i| ExplainPlanNode {
            id: format!("w{i}"),
            label: format!("child {i}"),
            cost: Some(i as f64),
            rows: Some(i),
            details: (0..8)
                .map(|j| ("dup".to_string(), format!("{long}-{j}")))
                .collect(),
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
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(build(Some(sample_tree()), state, true))
}

fn none_view(state: &PlanState, _cx: &mut Cx<PlanState>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(build(None, state, true))
}

fn wide_view(state: &PlanState, _cx: &mut Cx<PlanState>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(build(Some(wide_tree()), state, true))
}

fn probe(view: fn(&PlanState, &mut Cx<PlanState>) -> El, size: (f32, f32)) -> Runtime<PlanState> {
    let mut rt = Runtime::new(PlanState::default(), view, size, env(true));
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    // Exiting nodes stay listed while their exit transition plays; they are
    // gone from key lookups (`rt.find` skips them), so mirror that here.
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

fn own_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with(PREFIX))
        .collect()
}

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else { continue };
        if !key.starts_with(PREFIX) {
            continue;
        }
        // Only horizontal fit is in scope: rows inside the 280pt scroll box
        // legitimately extend past the window's bottom and are clipped there.
        let [x, _y, ew, _eh] = e.rect;
        if x < -0.5 || x + ew > w + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---- helper semantics (lazy default lives in S, not in the component) ----

#[test]
fn expansion_helpers_use_depth_default_and_xor_overrides() {
    assert!(default_expanded(0));
    assert!(default_expanded(1));
    assert!(!default_expanded(2));
    assert!(!default_expanded(3));

    let mut overrides = ExpandOverrides::default();
    assert!(is_node_expanded(&overrides, "n", 0));
    assert!(is_node_expanded(&overrides, "n", 1));
    assert!(!is_node_expanded(&overrides, "n", 2));

    toggle_node(&mut overrides, "n");
    assert!(!is_node_expanded(&overrides, "n", 0), "toggled depth 0 collapses");
    assert!(!is_node_expanded(&overrides, "n", 1));
    assert!(is_node_expanded(&overrides, "n", 2), "toggled depth 2 expands");
    assert_eq!(overrides.len(), 1, "one id toggled once");

    toggle_node(&mut overrides, "n");
    assert!(is_node_expanded(&overrides, "n", 0), "second toggle restores default");
    assert!(overrides.is_empty());
}

// ---- rendering ----

#[test]
fn no_plan_renders_nothing() {
    let rt = probe(none_view, (960.0, 640.0));
    assert!(
        own_keys(&rt).is_empty(),
        "a missing plan mounts no explain-plan-tree keys: {:?}",
        own_keys(&rt)
    );
}

#[test]
fn default_expansion_matches_depth_and_children_render_at_depth_plus_one() {
    let rt = probe(plan_view, (1440.0, 816.0));

    for key in [
        format!("{PREFIX}-title"),
        format!("{PREFIX}-scroll"),
        format!("{PREFIX}-node-root-row"),
        format!("{PREFIX}-node-c1-row"),
        format!("{PREFIX}-node-g1-row"),
        format!("{PREFIX}-node-c2-row"),
    ] {
        assert!(has_key(&rt, &key), "expected key {key}");
    }
    assert!(
        !has_key(&rt, &format!("{PREFIX}-node-g2-row")),
        "depth-2 branch starts collapsed, so its depth-3 child is not mounted"
    );

    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-root-row")).expanded,
        Some(true),
        "depth 0 expanded"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-c1-row")).expanded,
        Some(true),
        "depth 1 expanded"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-g1-row")).expanded,
        Some(false),
        "depth 2 collapsed"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-c2-row")).expanded,
        None,
        "a leaf is not an expandable node"
    );

    // Each level shifts labels by 12pt (React paddingLeft = depth * 12).
    let label_x = |id: &str| element(&rt, &format!("{PREFIX}-node-{id}-label")).rect[0];
    let (x0, x1, x2) = (label_x("root"), label_x("c1"), label_x("g1"));
    assert!((x1 - x0 - 12.0).abs() < 0.5, "depth0 -> depth1: {x0} -> {x1}");
    assert!((x2 - x1 - 12.0).abs() < 0.5, "depth1 -> depth2: {x1} -> {x2}");

    assert_eq!(
        element(&rt, &format!("{PREFIX}-title")).text.as_deref(),
        Some(t("explain.planTree")),
        "title uses the i18n key, not a hard-coded label"
    );
}

#[test]
fn a_click_toggles_only_nodes_with_children_and_leaves_are_inert() {
    let mut rt = probe(plan_view, (1440.0, 816.0));

    // A branch click expands and mounts its child one level deeper.
    click(&mut rt, &format!("{PREFIX}-node-g1-row"));
    assert!(has_key(&rt, &format!("{PREFIX}-node-g2-row")), "depth-3 child mounted");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-g1-row")).expanded,
        Some(true)
    );
    assert_eq!(rt.state.toggles, 1);
    assert!(rt.state.overrides.contains("g1"));

    // A leaf is not focusable and its click is not a toggle.
    let leaf_row = element(&rt, &format!("{PREFIX}-node-c2-row"));
    assert!(!leaf_row.focusable, "a leaf row is not a Tab stop");
    click(&mut rt, &format!("{PREFIX}-node-c2-row"));
    assert_eq!(rt.state.toggles, 1, "leaf click did not toggle anything");
    assert!(!rt.state.overrides.contains("c2"));
    assert!(has_key(&rt, &format!("{PREFIX}-node-g2-row")), "nothing was hidden");

    // Branch rows are focusable; collapsing c1 removes its whole subtree.
    assert!(element(&rt, &format!("{PREFIX}-node-c1-row")).focusable);
    click(&mut rt, &format!("{PREFIX}-node-c1-row"));
    assert_eq!(rt.state.toggles, 2);
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-c1-row")).expanded,
        Some(false)
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-node-g1-row")));
    assert!(!has_key(&rt, &format!("{PREFIX}-node-g2-row")));
}

#[test]
fn chevron_glyph_key_and_expanded_type_flip_on_toggle() {
    let mut rt = probe(plan_view, (1440.0, 816.0));
    let right = format!("{PREFIX}-node-c1-glyph-right");
    let down = format!("{PREFIX}-node-c1-glyph-down");

    assert!(has_key(&rt, &down), "depth-1 branch starts expanded");
    assert!(!has_key(&rt, &right));

    click(&mut rt, &format!("{PREFIX}-node-c1-row"));
    assert!(!has_key(&rt, &down), "expanded glyph is gone");
    assert!(has_key(&rt, &right), "collapsed glyph replaced it");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-c1-row")).expanded,
        Some(false)
    );

    click(&mut rt, &format!("{PREFIX}-node-c1-row"));
    assert!(has_key(&rt, &down));
    assert!(!has_key(&rt, &right));

    // Leaves keep only the empty 12×12 placeholder lane, never a glyph.
    let placeholder = element(&rt, &format!("{PREFIX}-node-c2-chevron"));
    assert!(
        (placeholder.rect[2] - 12.0).abs() < 0.5 && (placeholder.rect[3] - 12.0).abs() < 0.5,
        "leaf placeholder is 12x12: {:?}",
        placeholder.rect
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-node-c2-glyph-down")));
    assert!(!has_key(&rt, &format!("{PREFIX}-node-c2-glyph-right")));
}

#[test]
fn details_are_capped_at_six_with_collision_safe_keys() {
    let rt = probe(plan_view, (1440.0, 816.0));
    for i in 0..6 {
        assert!(has_key(&rt, &format!("{PREFIX}-node-root-detail-{i}")));
    }
    assert!(
        !has_key(&rt, &format!("{PREFIX}-node-root-detail-6")),
        "only the first six details render"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-root-detail-0-key"))
            .text
            .as_deref(),
        Some("dup: ")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-root-detail-1-key"))
            .text
            .as_deref(),
        Some("dup: "),
        "two details share the same key in the VM"
    );
    let mut keys = own_keys(&rt);
    keys.retain(|k| k.contains("-detail-"));
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), keys.len(), "duplicate detail element keys: {keys:?}");
    assert!(
        keys.contains(&format!("{PREFIX}-node-root-detail-0-key"))
            && keys.contains(&format!("{PREFIX}-node-root-detail-1-key")),
        "both colliding detail keys are mounted under distinct keys"
    );

    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-root-detail-0-value"))
            .text
            .as_deref(),
        Some("value 0")
    );
    assert!(
        has_key(&rt, &format!("{PREFIX}-node-root-details")),
        "details column exists only when the node has details"
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-node-c2-details")));
}

#[test]
fn cost_and_rows_badges_use_react_format() {
    let rt = probe(plan_view, (1440.0, 816.0));
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-root-cost")).text.as_deref(),
        Some("cost 3.14"),
        "cost is formatted to two decimals"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-root-rows")).text.as_deref(),
        Some("rows 42")
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-c2-cost")).text.as_deref(),
        Some("cost 0.50"),
        "trailing zeros are kept like Number.toFixed(2)"
    );
    assert!(
        !has_key(&rt, &format!("{PREFIX}-node-c2-rows")),
        "no rows badge when the VM has no rows"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-node-root-label")).text.as_deref(),
        Some("Seq Scan on users")
    );
}

#[test]
fn scroll_container_is_bounded_to_280pt() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = probe(wide_view, (w, h));
        let scroll = element(&rt, &format!("{PREFIX}-scroll"));
        assert!(
            scroll.rect[3] <= 280.5,
            "scroll container clamped to 280pt at {w}x{h}: {:?}",
            scroll.rect
        );
        assert!(
            scroll.rect[3] > 200.0,
            "content really exceeds the clamp: {:?}",
            scroll.rect
        );
        assert!(scroll.rect[1] + scroll.rect[3] <= h + 0.5, "scroll box inside window");
        // Every wide-tree child is mounted; the scroll clips the overflow.
        assert!(
            has_key(&rt, &format!("{PREFIX}-node-w19-row")),
            "all children are built, not virtualised away"
        );
    }
}

#[test]
fn both_sizes_have_no_horizontal_overflow() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let nested = probe(plan_view, (w, h));
        assert_no_horizontal_overflow(&nested, w, h, &format!("nested {w}x{h}"));

        let wide = probe(wide_view, (w, h));
        assert_no_horizontal_overflow(&wide, w, h, &format!("wide {w}x{h}"));
    }
}

#[test]
fn a_focused_branch_row_toggles_from_the_keyboard() {
    let mut rt = probe(plan_view, (1440.0, 816.0));
    let row = format!("{PREFIX}-node-g1-row");
    let (x, y) = rt.find(&row).expect("row laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.focused && e.key.as_deref() == Some(&row)),
        "a branch row takes focus from the click"
    );
    assert_eq!(rt.state.toggles, 1);

    // Enter is handled on the row; Space reaches `on_press` natively. Both go
    // through the same toggle callback in `S`.
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.toggles, 2, "Enter on the focused branch toggles it back");
    assert_eq!(element(&rt, &row).expanded, Some(false));
}
