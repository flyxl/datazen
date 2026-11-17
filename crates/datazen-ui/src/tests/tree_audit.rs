//! Audit probes for the T0-09 tree base: virtual staging, page height ownership, the
//! empty/hug and icon-lane edges, and the selected state assistive tech reads.

use crate::ui::kit::kit;
use crate::ui::tree::DataTree;
use rune::{col, el, Cx, El, Env, Icon, Key, KeyEvent, Mods, Runtime};
use rune_kit::{glyphs, TreeModel, TreeStyle};
use serde_json::Value;

const KEY: &str = "tree";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone, Default)]
struct Item {
    label: String,
}

fn item_icon(n: &Item) -> Option<&'static Icon> {
    (n.label == "node-0").then_some(&glyphs::CHECK)
}

#[derive(Default)]
struct Probe {
    model: TreeModel<Item>,
    selects: Vec<String>,
    height: Option<f32>,
    grow: bool,
    icons: bool,
    outside: bool,
}

impl Probe {
    fn range(n: usize) -> Probe {
        let mut model = TreeModel::new().key(KEY);
        for i in 0..n {
            let label = format!("node-{i}");
            model.insert(None, label.clone(), Item { label });
        }
        Probe {
            model,
            height: Some(200.0),
            ..Probe::default()
        }
    }

    fn empty_hug() -> Probe {
        Probe {
            model: TreeModel::new().key(KEY),
            height: None,
            grow: false,
            ..Probe::default()
        }
    }

    fn icon_first() -> Probe {
        Probe {
            icons: true,
            ..Probe::range(5)
        }
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut tree = DataTree::new(
        KEY,
        |s: &Probe| &s.model,
        |s: &mut Probe| &mut s.model,
        |n: &Item| n.label.as_str(),
    )
    .on_select(|s: &mut Probe, id| s.selects.push(id.to_owned()));
    if state.icons {
        tree = tree.icon(item_icon);
    }
    tree = match state.height {
        Some(height) => tree.height(height),
        None if state.grow => tree.grow(),
        None => tree,
    };
    let mut root = col()
        .w_full()
        .h_full()
        .padding(24.0)
        .child(tree.build(state, true));
    if state.outside {
        root = root.child(el().key("outside").focusable(true).h(20.0));
    }
    root
}

fn probe(state: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(state, view, size, env());
    rt.frame(0.0);
    rt
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn settle<S>(rt: &mut Runtime<S>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn near(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.5
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn row_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter_map(|e| e.key)
        .filter(|k| k.starts_with(&format!("{KEY}-row-")))
        .filter(|k| k.matches('-').count() == 2)
        .collect()
}

#[test]
fn virtual_rows_stage_in_and_settle_below_three_viewports() {
    let mut rt = probe(Probe::range(500), (1440.0, 816.0));
    let mut counts = Vec::new();
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
        counts.push(row_keys(&rt).len());
    }
    eprintln!("mounted rows per frame: {counts:?}");
    let first = counts[0];
    let last = *counts.last().expect("frames ran");
    assert!(
        first < last,
        "virtual_rows mounts in stages, not all at once: {counts:?}"
    );
    assert!(
        last >= 9,
        "at least the visible rows are mounted: {counts:?}"
    );
    assert!(
        last <= 25,
        "two viewports of overscan each side stay bounded: {counts:?}"
    );
    assert!(last < 40, "never the whole 500-row model: {counts:?}");
}

#[derive(Default)]
struct TwoTrees {
    a: TreeModel<Item>,
    b: TreeModel<Item>,
    selects: Vec<String>,
}

fn two_trees() -> TwoTrees {
    let mut state = TwoTrees::default();
    state.a = TreeModel::new().key("tree-a");
    state.b = TreeModel::new().key("tree-b");
    for m in [&mut state.a, &mut state.b] {
        for i in 0..60 {
            let label = format!("node-{i}");
            m.insert(None, label.clone(), Item { label });
        }
    }
    state
}

fn two_view(state: &TwoTrees, _cx: &mut Cx<TwoTrees>) -> El {
    let a = DataTree::new(
        "tree-a",
        |s: &TwoTrees| &s.a,
        |s: &mut TwoTrees| &mut s.a,
        |n: &Item| n.label.as_str(),
    )
    .height(200.0)
    .on_select(|s: &mut TwoTrees, id| s.selects.push(format!("a:{id}")))
    .build(state, true);
    let b = DataTree::new(
        "tree-b",
        |s: &TwoTrees| &s.b,
        |s: &mut TwoTrees| &mut s.b,
        |n: &Item| n.label.as_str(),
    )
    .height(400.0)
    .on_select(|s: &mut TwoTrees, id| s.selects.push(format!("b:{id}")))
    .build(state, true);
    col().w_full().h_full().padding(24.0).child(a).child(b)
}

#[test]
fn each_tree_pages_by_its_own_measured_viewport() {
    let mut rt = Runtime::new(two_trees(), two_view, (960.0, 640.0), env());
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(
        rt.state.selects.last().map(String::as_str),
        Some("a:node-6")
    );
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(
        rt.state.selects.last().map(String::as_str),
        Some("b:node-14")
    );
}

#[derive(Default)]
struct PageProbe {
    model: TreeModel<Item>,
    selects: Vec<String>,
    height: f32,
}

fn page_probe(height: f32) -> PageProbe {
    let mut model = TreeModel::new().key(KEY);
    for i in 0..60 {
        let label = format!("node-{i}");
        model.insert(None, label.clone(), Item { label });
    }
    PageProbe {
        model,
        selects: Vec::new(),
        height,
    }
}

fn page_view(state: &PageProbe, _cx: &mut Cx<PageProbe>) -> El {
    let tree = DataTree::new(
        KEY,
        |s: &PageProbe| &s.model,
        |s: &mut PageProbe| &mut s.model,
        |n: &Item| n.label.as_str(),
    )
    .height(state.height)
    .on_select(|s: &mut PageProbe, id| s.selects.push(id.to_owned()))
    .build(state, true);
    col().w_full().h_full().padding(24.0).child(tree)
}

#[test]
fn two_windows_with_the_same_key_page_by_their_own_height() {
    let mut short = Runtime::new(page_probe(200.0), page_view, (960.0, 640.0), env());
    short.frame(0.0);
    press(&mut short, Key::Tab);
    press(&mut short, Key::PageDown);
    settle(&mut short);
    assert_eq!(
        short.state.selects.last().map(String::as_str),
        Some("node-6")
    );

    let mut tall = Runtime::new(page_probe(400.0), page_view, (960.0, 640.0), env());
    tall.frame(0.0);
    press(&mut tall, Key::Tab);
    press(&mut tall, Key::PageDown);
    settle(&mut tall);
    assert_eq!(
        tall.state.selects.last().map(String::as_str),
        Some("node-14")
    );

    // The short window did not change height; a second page must still be six rows.
    press(&mut short, Key::PageDown);
    settle(&mut short);
    assert_eq!(
        short.state.selects.last().map(String::as_str),
        Some("node-12"),
        "the short window paged with the tall window's measured height"
    );
}

#[test]
fn an_empty_hugging_tree_keeps_its_status_line_inside() {
    let rt = probe(Probe::empty_hug(), (960.0, 640.0));
    let root = element(&rt, KEY);
    let empty = element(&rt, &format!("{KEY}-empty"));
    eprintln!("root {:?}, empty {:?}", root.rect, empty.rect);
    assert!(
        empty.rect[3] <= root.rect[3],
        "the 40pt empty line is clipped by a {}pt root",
        root.rect[3]
    );
}

#[test]
fn rows_without_an_icon_keep_the_icon_lane() {
    let rt = probe(Probe::icon_first(), (960.0, 640.0));
    let with = element(&rt, &format!("{KEY}-row-0-label")).rect[0];
    let without = element(&rt, &format!("{KEY}-row-1-label")).rect[0];
    eprintln!("label x with icon {with}, without {without}");
    assert!(
        (with - without).abs() < 0.5,
        "the doc promises the lane is kept: {with} vs {without}"
    );
}

#[test]
fn the_selected_row_reports_its_state() {
    let mut rt = probe(Probe::range(5), (960.0, 640.0));
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::Down);
    settle(&mut rt);
    let row = element(&rt, &format!("{KEY}-row-0"));
    let selected = TreeStyle::from_kit(kit(true)).selected;
    assert_eq!(row.fill, selected, "the row is painted as selected");
    assert!(
        row.item_selected,
        "the painted selection must also be the accessible one"
    );
}

#[test]
fn a_five_hundred_row_tree_keeps_its_nodes_in_the_window_except_the_virtual_extent() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = probe(Probe::range(500), (w, h));
        let extent = element(&rt, &format!("{KEY}-rows"));
        assert!(
            extent.rect[3] > h,
            "the virtual extent is the long scroll content: {:?}",
            extent.rect
        );
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else {
                continue;
            };
            if !key.starts_with(KEY) || key == format!("{KEY}-rows") {
                continue;
            }
            let [x, y, ew, eh] = e.rect;
            assert!(
                x >= -0.5 && y >= -0.5 && x + ew <= w + 0.5 && y + eh <= h + 0.5,
                "{key} leaves {w}x{h}: {:?}",
                e.rect
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Round-two audit: the per-instance viewport, the a11y cursor row under
// virtualisation, the icon lane's child geometry and the settled extent.

#[derive(Default)]
struct GrowingPair {
    a: TreeModel<Item>,
    b: TreeModel<Item>,
    a_viewport: f32,
    b_viewport: f32,
    selects: Vec<String>,
}

fn growing_pair(n: usize) -> GrowingPair {
    let mut state = GrowingPair::default();
    state.a = TreeModel::new().key("tree-a");
    state.b = TreeModel::new().key("tree-b");
    for model in [&mut state.a, &mut state.b] {
        for i in 0..n {
            let label = format!("node-{i}");
            model.insert(None, label.clone(), Item { label });
        }
    }
    state
}

fn growing_pair_view(state: &GrowingPair, _cx: &mut Cx<GrowingPair>) -> El {
    let a = DataTree::new(
        "tree-a",
        |s: &GrowingPair| &s.a,
        |s: &mut GrowingPair| &mut s.a,
        |n: &Item| n.label.as_str(),
    )
    .grow()
    .viewport(
        |s: &GrowingPair| s.a_viewport,
        |s: &mut GrowingPair, h| s.a_viewport = h,
    )
    .on_select(|s: &mut GrowingPair, id| s.selects.push(format!("a:{id}")))
    .build(state, true);
    let b = DataTree::new(
        "tree-b",
        |s: &GrowingPair| &s.b,
        |s: &mut GrowingPair| &mut s.b,
        |n: &Item| n.label.as_str(),
    )
    .grow()
    .viewport(
        |s: &GrowingPair| s.b_viewport,
        |s: &mut GrowingPair, h| s.b_viewport = h,
    )
    .on_select(|s: &mut GrowingPair, id| s.selects.push(format!("b:{id}")))
    .build(state, true);
    col()
        .w_full()
        .h_full()
        .padding(24.0)
        .child(col().w_full().h(200.0).child(a))
        .child(col().w_full().h(400.0).child(b))
}

#[test]
fn two_growing_trees_in_one_state_page_by_their_own_viewport() {
    let mut rt = Runtime::new(growing_pair(60), growing_pair_view, (960.0, 816.0), env());
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(
        rt.state.a_viewport, 200.0,
        "tree-a measured its own 200pt viewport"
    );
    assert_eq!(
        rt.state.b_viewport, 400.0,
        "tree-b measured its own 400pt viewport"
    );
    assert_eq!(
        rt.state.selects.last().map(String::as_str),
        Some("a:node-6"),
        "tree-a pages by 200pt, not by tree-b's height: {:?}",
        rt.state.selects
    );

    press(&mut rt, Key::Tab);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(
        rt.state.selects.last().map(String::as_str),
        Some("b:node-14"),
        "tree-b pages by its own 400pt viewport: {:?}",
        rt.state.selects
    );

    // Page tree-a again: tree-b's measurement must not have overwritten it.
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(
        rt.state.selects.last().map(String::as_str),
        Some("a:node-12"),
        "tree-a's second page still uses 200pt: {:?}",
        rt.state.selects
    );
}

#[derive(Default)]
struct ScrollProbe {
    model: TreeModel<Item>,
    selects: Vec<String>,
    height: f32,
}

fn scroll_probe(n: usize) -> ScrollProbe {
    let mut state = ScrollProbe {
        height: 200.0,
        ..ScrollProbe::default()
    };
    state.model = TreeModel::new().key(KEY);
    for i in 0..n {
        let label = format!("node-{i}");
        state.model.insert(None, format!("n{i}"), Item { label });
    }
    state.model.select("n30");
    state
}

fn scroll_view(state: &ScrollProbe, _cx: &mut Cx<ScrollProbe>) -> El {
    let tree = DataTree::new(
        KEY,
        |s: &ScrollProbe| &s.model,
        |s: &mut ScrollProbe| &mut s.model,
        |n: &Item| n.label.as_str(),
    )
    .height(state.height)
    .on_select(|s: &mut ScrollProbe, id| s.selects.push(id.to_owned()))
    .build(state, true);
    col().w_full().h_full().padding(24.0).child(tree)
}

fn find_json<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    if node.get("key").and_then(Value::as_str) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|c| find_json(c, key))
}

fn json_states(node: &Value) -> Vec<&str> {
    node.get("states")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn a11y_cursor(rt: &Runtime<ScrollProbe>) -> (Option<String>, Option<String>) {
    let json = rt.accessibility_json();
    let tree = find_json(&json, KEY).expect("the tree is in the a11y tree");
    let active = tree
        .get("activeDescendant")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let target = active.as_deref().and_then(|key| find_json(&json, key));
    let selected = target.map(|row| {
        json_states(row).contains(&"selected").then(|| {
            row.get("key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        })
    });
    (active, selected.flatten())
}

#[test]
fn the_active_descendant_is_the_mounted_visible_row_under_scrolling() {
    let mut rt = Runtime::new(scroll_probe(60), scroll_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find(&format!("{KEY}-viewport")).expect("viewport");
    // Scroll ten rows: the cursor row (30) is still inside the 24-row mounted window.
    rt.wheel(x + 10.0, y + 10.0, -240.0);
    settle(&mut rt);
    assert!(
        has(&rt, &format!("{KEY}-row-30")),
        "the cursor row is mounted after the scroll"
    );
    let (active, selected) = a11y_cursor(&rt);
    assert_eq!(active.as_deref(), Some("tree-row-30"));
    assert_eq!(
        selected.as_deref(),
        Some("tree-row-30"),
        "the active descendant points at the selected row: active={active:?}"
    );
}

#[test]
fn the_active_descendant_is_not_left_dangling_when_the_cursor_scrolls_out() {
    let mut rt = Runtime::new(scroll_probe(60), scroll_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find(&format!("{KEY}-viewport")).expect("viewport");
    rt.wheel(x + 10.0, y + 10.0, -1200.0);
    settle(&mut rt);
    assert!(
        !has(&rt, &format!("{KEY}-row-30")),
        "the cursor row is now outside the mounted window"
    );
    let (active, selected) = a11y_cursor(&rt);
    eprintln!("active descendant after the cursor row unmounts: {active:?}");
    // The a11y tree resolves active_descendant to a mounted row and drops it when the
    // cursor row is virtualised out; it never emits a key that is absent from the tree.
    if let Some(key) = active.as_deref() {
        let json = rt.accessibility_json();
        assert!(
            find_json(&json, key).is_some(),
            "the tree's active descendant ({key}) must be a mounted row, not a dangling key"
        );
        assert_eq!(selected.as_deref(), Some(key));
    }
}

#[test]
fn the_icon_lane_is_a_real_sixteen_point_child_of_every_row() {
    let rt = probe(Probe::icon_first(), (960.0, 640.0));
    let elements = rt.elements();
    let index = |key: &str| {
        elements
            .iter()
            .position(|e| e.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("{key} is mounted"))
    };
    let row0 = index(&format!("{KEY}-row-0"));
    let lane0 = index(&format!("{KEY}-row-0-icon"));
    let lane1 = index(&format!("{KEY}-row-1-icon"));
    assert_eq!(elements[lane0].parent, Some(row0));
    assert!(
        near(elements[lane0].rect[2], 16.0) && near(elements[lane0].rect[3], 16.0),
        "the lane is 16x16: {:?}",
        elements[lane0].rect
    );
    assert_eq!(
        elements[lane1].rect[2..],
        elements[lane0].rect[2..],
        "the empty lane has the same box"
    );
    let glyphs = elements.iter().filter(|e| e.parent == Some(lane0)).count();
    assert!(glyphs > 0, "row 0's glyph is a child of its lane");
    let empty_children = elements.iter().filter(|e| e.parent == Some(lane1)).count();
    assert_eq!(empty_children, 0, "row 1's lane stays empty");
}

#[test]
fn empty_hug_opens_forty_and_a_fixed_height_still_contains_it() {
    let rt = probe(Probe::empty_hug(), (960.0, 640.0));
    let root = element(&rt, KEY);
    let empty = element(&rt, &format!("{KEY}-empty"));
    assert!(root.rect[3] >= 40.0, "hug root {:?}", root.rect);
    assert!(empty.rect[1] >= root.rect[1]);
    assert!(empty.rect[1] + empty.rect[3] <= root.rect[1] + root.rect[3] + 0.5);

    let rt = probe(
        Probe {
            height: Some(200.0),
            ..Probe::default()
        },
        (960.0, 640.0),
    );
    let root = element(&rt, KEY);
    let empty = element(&rt, &format!("{KEY}-empty"));
    assert!(near(root.rect[3], 200.0), "fixed root {:?}", root.rect);
    assert!(empty.rect[1] >= root.rect[1]);
    assert!(empty.rect[1] + empty.rect[3] <= root.rect[1] + root.rect[3] + 0.5);
    assert!(empty.rect[0] >= 0.0 && empty.rect[0] + empty.rect[2] <= 960.0);
}

#[test]
fn the_virtual_window_is_stable_and_the_extent_is_exactly_the_model() {
    let mut counts = Vec::new();
    for _ in 0..5 {
        let mut rt = probe(Probe::range(500), (960.0, 640.0));
        settle(&mut rt);
        counts.push(row_keys(&rt).len());
    }
    assert_eq!(counts, vec![24; 5], "settled mounted rows: {counts:?}");

    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = probe(Probe::range(500), (w, h));
        let extent = element(&rt, &format!("{KEY}-rows"));
        assert!(extent.virtual_extent, "the row list is the virtual extent");
        assert!(
            near(extent.rect[3], 500.0 * 24.0),
            "500 rows x 24pt = {:?}",
            extent.rect
        );
        assert!(
            row_keys(&rt).len() < 40,
            "500 rows did not all mount: {}",
            row_keys(&rt).len()
        );
    }
}

#[derive(Default)]
struct GrowOnly {
    model: TreeModel<Item>,
}

fn grow_only() -> GrowOnly {
    let mut state = GrowOnly::default();
    state.model = TreeModel::new().key(KEY);
    for i in 0..500 {
        let label = format!("node-{i}");
        state.model.insert(None, label.clone(), Item { label });
    }
    state
}

fn grow_only_view(state: &GrowOnly, _cx: &mut Cx<GrowOnly>) -> El {
    let tree = DataTree::new(
        KEY,
        |s: &GrowOnly| &s.model,
        |s: &mut GrowOnly| &mut s.model,
        |n: &Item| n.label.as_str(),
    )
    .grow()
    .build(state, true);
    col()
        .w_full()
        .h_full()
        .padding(24.0)
        .child(col().w_full().h(200.0).child(tree))
}

#[test]
fn a_grow_tree_without_a_viewport_writer_still_virtualizes() {
    let mut rt = Runtime::new(grow_only(), grow_only_view, (960.0, 640.0), env());
    rt.frame(0.0);
    settle(&mut rt);
    let counts = row_keys(&rt).len();
    eprintln!("grow tree without viewport writer mounted {counts} rows");
    assert!(counts >= 9, "the viewport rows are mounted: {counts}");
    assert!(counts < 40, "not all 500 rows: {counts}");
}
