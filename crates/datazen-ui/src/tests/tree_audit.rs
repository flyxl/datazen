//! Audit probes for the T0-09 tree base: virtual staging, page height ownership, the
//! empty/hug and icon-lane edges, and the selected state assistive tech reads.

use crate::ui::kit::kit;
use crate::ui::tree::DataTree;
use rune::{col, el, Cx, El, Env, Icon, Key, KeyEvent, Mods, Runtime};
use rune_kit::{glyphs, TreeModel, TreeStyle};

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
