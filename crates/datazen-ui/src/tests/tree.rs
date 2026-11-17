//! T0-09 tree base: rows on kit metrics, roles and keys, one focus stop with the kit keyboard,
//! virtualisation, selection (single and multi), pointer behaviour, boundaries and window fit.

use crate::ui::kit::kit;
use crate::ui::tree::DataTree;
use rune::{col, el, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use rune_kit::{TreeModel, TreeStyle};

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

fn item(label: impl Into<String>) -> Item {
    Item {
        label: label.into(),
    }
}

#[derive(Default)]
struct Probe {
    model: TreeModel<Item>,
    selects: Vec<String>,
    opens: Vec<String>,
    height: Option<f32>,
    empty_text: Option<&'static str>,
    /// A focusable sibling after the tree, so Tab has somewhere to go.
    outside: bool,
}

impl Probe {
    fn flat(labels: &[&'static str]) -> Probe {
        let mut model = TreeModel::new().key(KEY);
        for label in labels {
            model.insert(None, *label, item(*label));
        }
        Probe {
            model,
            height: Some(200.0),
            ..Probe::default()
        }
    }

    fn nested() -> Probe {
        let mut model = TreeModel::new().key(KEY);
        model.insert(None, "a", item("alpha"));
        model.insert(None, "b", item("beta"));
        model.insert(Some("a"), "a1", item("alpha-1"));
        model.insert(Some("a"), "a2", item("alpha-2"));
        model.insert(Some("a1"), "a11", item("alpha-1-1"));
        Probe {
            model,
            height: Some(240.0),
            ..Probe::default()
        }
    }

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

    fn selected_ids(&self) -> Vec<String> {
        self.model.selected().map(str::to_owned).collect()
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut tree = DataTree::new(
        KEY,
        |s: &Probe| &s.model,
        |s: &mut Probe| &mut s.model,
        |n: &Item| n.label.as_str(),
    )
    .on_select(|s: &mut Probe, id| s.selects.push(id.to_owned()))
    .on_open(|s: &mut Probe, id| s.opens.push(id.to_owned()));
    tree = match state.height {
        Some(height) => tree.height(height),
        None => tree.grow(),
    };
    if let Some(text) = state.empty_text {
        tree = tree.empty_text(text);
    }
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

fn shift_press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
    });
    rt.frame(1.0 / 60.0);
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn double_click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn settle<S>(rt: &mut Runtime<S>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn near(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.5
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
fn rows_take_the_kit_geometry_and_roles() {
    let mut rt = probe(Probe::nested(), (1440.0, 816.0));
    rt.state.model.expand_all();
    rt.frame(0.0);

    let tree = element(&rt, KEY);
    assert_eq!(tree.role, "tree");
    assert!(
        near(tree.rect[0], 24.0)
            && near(tree.rect[1], 24.0)
            && near(tree.rect[2], 1392.0)
            && near(tree.rect[3], 240.0),
        "tree rect {:?}",
        tree.rect
    );

    // Five rows (a, a1, a11, a2, b) at the kit row height, in visible order.
    for i in 0..5 {
        let row = element(&rt, &format!("{KEY}-row-{i}"));
        assert_eq!(row.role, "treeitem", "row {i} role");
        assert!(
            near(row.rect[1], 24.0 + i as f32 * 24.0),
            "row {i} y {}",
            row.rect[1]
        );
        assert!(near(row.rect[3], 24.0), "row {i} h {}", row.rect[3]);
        assert!(near(row.rect[2], 1392.0), "row {i} w {}", row.rect[2]);
    }

    // The chevron sits in the row's left padding (6), 16 wide; the label follows it.
    let depth0 = element(&rt, &format!("{KEY}-row-0-chevron"));
    assert!(
        near(depth0.rect[0], 30.0) && near(depth0.rect[2], 16.0),
        "depth-0 chevron {:?}",
        depth0.rect
    );
    let label0 = element(&rt, &format!("{KEY}-row-0-label"));
    let label1 = element(&rt, &format!("{KEY}-row-1-label"));
    assert!(near(label0.rect[0], 50.0), "label0 x {}", label0.rect[0]);
    assert!(
        near(label1.rect[0] - label0.rect[0], 14.0),
        "one indent step: {} -> {}",
        label0.rect[0],
        label1.rect[0]
    );
    // A depth-1 row keeps one indent guide in the first lane.
    let guide = element(&rt, &format!("{KEY}-row-1-guide-0"));
    assert!(
        near(guide.rect[0], 24.0 + 13.5) && near(guide.rect[2], 1.0),
        "guide {:?}",
        guide.rect
    );
    // Expanded folders and leaves are marked for assistive tech.
    assert_eq!(
        element(&rt, &format!("{KEY}-row-0")).label.as_deref(),
        Some("alpha")
    );
    assert_eq!(
        element(&rt, &format!("{KEY}-row-2-label")).text.as_deref(),
        Some("alpha-1-1")
    );
}

#[test]
fn the_tree_is_one_focus_stop_and_the_arrows_move_the_cursor() {
    let mut state = Probe::nested();
    state.outside = true;
    let mut rt = probe(state, (1440.0, 816.0));

    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt).as_deref(), Some(KEY), "Tab reaches the tree");
    assert!(
        rt.elements()
            .iter()
            .filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with(KEY)))
            .filter(|e| e.key.as_deref() != Some(KEY))
            .all(|e| !e.focusable),
        "no row joins the tab order"
    );
    // Down from no cursor selects the first row; the model keeps the selection.
    press(&mut rt, Key::Down);
    settle(&mut rt);
    assert_eq!(rt.state.selects, vec!["a".to_string()]);
    assert_eq!(rt.state.selected_ids(), vec!["a"]);
    let selected = TreeStyle::from_kit(kit(true)).selected;
    let row0 = element(&rt, &format!("{KEY}-row-0"));
    assert_eq!(row0.fill, selected, "the cursor row is painted as selected");
    let row1 = element(&rt, &format!("{KEY}-row-1"));
    assert_ne!(row1.fill, selected, "its neighbour is not");

    press(&mut rt, Key::Down);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("b"));
    press(&mut rt, Key::Up);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("a"));

    // Right opens the folder; Right again steps into its first child.
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.model.visible_len(), 4, "alpha opened");
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("a1"));
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.model.visible_len(), 5, "alpha-1 opened");
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("a1"));
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.model.visible_len(), 4, "alpha-1 closed");
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("a"));
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.model.visible_len(), 2, "alpha closed");

    // Home and End reach the ends of the visible projection.
    press(&mut rt, Key::End);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("b"));
    press(&mut rt, Key::Home);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("a"));

    // Return toggles a folder and opens a leaf.
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.model.visible_len(), 4, "Return opened alpha");
    assert!(rt.state.opens.is_empty(), "a folder is not an open event");
    press(&mut rt, Key::End);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.opens, vec!["b".to_string()], "Return opens a leaf");

    // A second Tab leaves the tree: the container is the only stop it adds.
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused(&rt).as_deref(),
        Some("outside"),
        "a second Tab leaves the tree for the next stop"
    );
}

#[test]
fn page_keys_move_by_the_measured_page() {
    let mut rt = probe(Probe::range(20), (1440.0, 816.0));
    press(&mut rt, Key::Tab);
    // 200pt viewport / 24pt rows = 8 rows, a page is one row short of that.
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("node-6"));
    let key = format!("{KEY}-row-6");
    assert!(has(&rt, &key), "the cursor row was revealed");
    assert_eq!(
        element(&rt, &key).fill,
        TreeStyle::from_kit(kit(true)).selected
    );
    press(&mut rt, Key::PageUp);
    assert_eq!(rt.state.selects.last().map(String::as_str), Some("node-0"));
}

#[test]
fn virtual_rows_mount_only_the_window_and_scroll_without_repeating_keys() {
    let mut rt = probe(Probe::range(500), (1440.0, 816.0));
    let mounted = row_keys(&rt);
    let visible = (200.0f32 / 24.0).ceil() as usize;
    // `virtual_rows` keeps two viewports of overscan per side, so the mounted window is the
    // visible rows plus that budget, never the 500-row model.
    assert!(
        mounted.len() >= visible && mounted.len() <= visible * 3 + 2,
        "{} rows mounted for a {visible}-row window",
        mounted.len()
    );
    assert!(
        mounted.len() < 40,
        "500-row model, {} rows mounted",
        mounted.len()
    );
    assert_eq!(rt.state.model.visible_len(), 500);

    let (x, y) = rt
        .find(&format!("{KEY}-viewport"))
        .expect("viewport laid out");
    rt.wheel(x + 10.0, y + 10.0, -400.0);
    rt.frame(1.0 / 60.0);
    let after = row_keys(&rt);
    assert!(
        after.iter().any(|k| {
            k.strip_prefix(&format!("{KEY}-row-"))
                .and_then(|i| i.parse::<usize>().ok())
                .is_some_and(|i| i >= visible)
        }),
        "scrolling mounted later rows: {:?}",
        after
    );
    let mut unique = after.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), after.len(), "row keys repeat: {after:?}");
}

#[test]
fn a_press_selects_and_a_double_press_opens_a_leaf() {
    let mut rt = probe(Probe::nested(), (1440.0, 816.0));
    click(&mut rt, &format!("{KEY}-row-1"));
    settle(&mut rt);
    assert_eq!(rt.state.selected_ids(), vec!["b".to_string()]);
    assert_eq!(
        focused(&rt).as_deref(),
        Some(KEY),
        "a press focuses the tree"
    );
    assert_eq!(
        element(&rt, &format!("{KEY}-row-1")).fill,
        TreeStyle::from_kit(kit(true)).selected
    );

    double_click(&mut rt, &format!("{KEY}-row-1"));
    assert_eq!(rt.state.opens, vec!["b".to_string()]);

    // A single press on a folder toggles it, as the kit file tree does.
    click(&mut rt, &format!("{KEY}-row-0"));
    assert_eq!(rt.state.model.visible_len(), 4, "the press opened alpha");
}

#[test]
fn shift_extends_a_multi_selection() {
    let mut rt = probe(Probe::range(5), (1440.0, 816.0));
    rt.state.model = std::mem::take(&mut rt.state.model).multi_select(true);
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::Down);
    shift_press(&mut rt, Key::Down);
    shift_press(&mut rt, Key::Down);
    settle(&mut rt);
    assert_eq!(rt.state.selected_ids(), vec!["node-0", "node-1", "node-2"]);
    let selected = TreeStyle::from_kit(kit(true)).selected;
    for i in 0..3 {
        assert_eq!(
            element(&rt, &format!("{KEY}-row-{i}")).fill,
            selected,
            "row {i} is part of the extended selection"
        );
    }
    assert_ne!(
        element(&rt, &format!("{KEY}-row-3")).fill,
        selected,
        "the range stops at the cursor"
    );
}

#[test]
fn an_empty_model_shows_one_status_line() {
    let rt = probe(Probe::default(), (1440.0, 816.0));
    let empty = element(&rt, &format!("{KEY}-empty"));
    assert_eq!(empty.role, "status");
    assert_eq!(empty.label.as_deref(), Some("暂无可显示的行"));
    assert!(row_keys(&rt).is_empty(), "no rows for an empty model");
    assert_eq!(
        rt.elements()
            .iter()
            .filter(|e| e.key.as_deref() == Some(&format!("{KEY}-empty")))
            .count(),
        1
    );
    assert_eq!(element(&rt, KEY).role, "tree");
}

#[test]
fn a_fully_collapsed_tree_shows_only_its_roots() {
    let rt = probe(Probe::nested(), (1440.0, 816.0));
    assert_eq!(row_keys(&rt).len(), 2);
    assert!(has(&rt, &format!("{KEY}-row-0")) && has(&rt, &format!("{KEY}-row-1")));
    assert!(!has(&rt, &format!("{KEY}-row-2")));
}

#[test]
fn a_single_level_tree_indents_every_row_alike() {
    let mut rt = probe(Probe::flat(&["one", "two", "three"]), (1440.0, 816.0));
    let xs: Vec<f32> = (0..3)
        .map(|i| element(&rt, &format!("{KEY}-row-{i}-label")).rect[0])
        .collect();
    assert!(near(xs[0], 50.0), "first label {}", xs[0]);
    assert!(xs.iter().all(|x| near(*x, xs[0])), "labels {xs:?}");
    assert_eq!(row_keys(&rt).len(), 3);
    rt.frame(0.0);
}

#[test]
fn the_tree_stays_inside_a_960x640_window() {
    assert_inside((960.0, 640.0));
}

#[test]
fn the_tree_stays_inside_a_1440x816_window() {
    assert_inside((1440.0, 816.0));
}

fn assert_inside((w, h): (f32, f32)) {
    let mut rt = probe(Probe::nested(), (w, h));
    rt.state.model.expand_all();
    rt.frame(0.0);
    let tree = element(&rt, KEY);
    assert!(
        near(tree.rect[2], w - 48.0) && near(tree.rect[3], 240.0),
        "tree rect {:?} in {w}x{h}",
        tree.rect
    );
    let mut outside = 0;
    let mut nodes = 0;
    for e in rt.elements() {
        if e.key.as_deref().is_some_and(|k| k.starts_with(KEY)) {
            nodes += 1;
        }
        if e.key.as_deref().is_some_and(|k| k.starts_with(KEY)) {
            let [x, y, ew, eh] = e.rect;
            if x < -0.5 || y < -0.5 || x + ew > w + 0.5 || y + eh > h + 0.5 {
                outside += 1;
                eprintln!("{} leaves {w}x{h}: {:?}", e.key.unwrap_or_default(), e.rect);
            }
        }
    }
    eprintln!(
        "{w}x{h}: tree {:?}, {nodes} tree nodes, {outside} outside",
        tree.rect
    );
    assert_eq!(outside, 0, "tree nodes leave a {w}x{h} window");
}
