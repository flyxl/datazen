//! T0-09 tree base: rows on kit metrics, roles and keys, one focus stop with the kit keyboard,
//! virtualisation, selection (single and multi), pointer behaviour, boundaries and window fit.

use crate::ui::kit::kit;
use crate::ui::tree::DataTree;
use rune::{col, el, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
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
    /// Hands the viewport height to the tree through state, the `.grow()` contract.
    wired: bool,
    /// Hug the content instead of growing or taking a fixed height.
    hug: bool,
    /// Draw a glyph on the first row only, to prove the icon lane stays reserved.
    icons: bool,
    viewport: f32,
    /// A fixed frame around a `.grow()` tree, so the measured viewport is known.
    frame: Option<f32>,
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

    fn range_at(n: usize, height: f32) -> Probe {
        Probe {
            height: Some(height),
            ..Probe::range(n)
        }
    }

    /// A `.grow()` tree inside a 200pt frame that pages by the height the app stores in `S`.
    fn growing(n: usize) -> Probe {
        Probe {
            height: None,
            wired: true,
            frame: Some(200.0),
            ..Probe::range(n)
        }
    }

    fn hugging(labels: &[&'static str]) -> Probe {
        Probe {
            height: None,
            hug: true,
            ..Probe::flat(labels)
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
        None if state.hug => tree,
        None => tree.grow(),
    };
    if state.icons {
        tree = tree.icon(|n: &Item| (n.label == "node-0").then_some(&glyphs::CHECK));
    }
    if state.wired {
        tree = tree.viewport(|s: &Probe| s.viewport, |s: &mut Probe, h| s.viewport = h);
    }
    if let Some(text) = state.empty_text {
        tree = tree.empty_text(text);
    }
    let built = tree.build(state, true);
    let mut root = col().w_full().h_full().padding(24.0);
    root = match state.frame {
        Some(height) => root.child(col().w_full().h(height).child(built)),
        None => root.child(built),
    };
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
    settle(&mut rt);

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

    // The chevron sits in the row's left padding (6), 16 wide, then the always-reserved
    // 16pt icon lane; the label follows both (6 + 16 + 4 + 16 + 4 = 46 from the row's left).
    let depth0 = element(&rt, &format!("{KEY}-row-0-chevron"));
    assert!(
        near(depth0.rect[0], 30.0) && near(depth0.rect[2], 16.0),
        "depth-0 chevron {:?}",
        depth0.rect
    );
    let label0 = element(&rt, &format!("{KEY}-row-0-label"));
    let label1 = element(&rt, &format!("{KEY}-row-1-label"));
    assert!(near(label0.rect[0], 70.0), "label0 x {}", label0.rect[0]);
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
    let visible = (200.0f32 / 24.0).ceil() as usize;
    // `virtual_rows` stages rows in across frames; the first frame mounts fewer than the
    // settled window.
    let first = row_keys(&rt).len();
    settle(&mut rt);
    let mounted = row_keys(&rt);
    assert!(
        first < mounted.len(),
        "rows stage in: {first} first frame, {} settled",
        mounted.len()
    );
    // 9 visible rows with two viewports of overscan per side settle at exactly 24.
    assert_eq!(
        mounted.len(),
        24,
        "settled window for a {visible}-row viewport: {:?}",
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
    assert!(near(xs[0], 70.0), "first label {}", xs[0]);
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
    settle(&mut rt);
    let tree = element(&rt, KEY);
    assert!(
        near(tree.rect[2], w - 48.0) && near(tree.rect[3], 240.0),
        "tree rect {:?} in {w}x{h}",
        tree.rect
    );
    let mut outside = 0;
    let mut nodes = 0;
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else {
            continue;
        };
        if !key.starts_with(KEY) {
            continue;
        }
        nodes += 1;
        // The row list is the scroll viewport's virtual extent: it is deliberately taller
        // than the window and clipped, so it is not part of the visible-node fit.
        if key == format!("{KEY}-rows") {
            continue;
        }
        let [x, y, ew, eh] = e.rect;
        if x < -0.5 || y < -0.5 || x + ew > w + 0.5 || y + eh > h + 0.5 {
            outside += 1;
            eprintln!("{key} leaves {w}x{h}: {:?}", e.rect);
        }
    }
    eprintln!(
        "{w}x{h}: tree {:?}, {nodes} tree nodes, {outside} outside",
        tree.rect
    );
    assert_eq!(outside, 0, "tree nodes leave a {w}x{h} window");
}

fn find_json_key<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    if node.get("key").and_then(Value::as_str) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|c| find_json_key(c, key))
}

fn json_states(node: &Value) -> Vec<&str> {
    node.get("states")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[test]
fn the_selected_row_is_the_accessible_selection() {
    let mut rt = probe(Probe::range(5), (960.0, 640.0));
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::Down);
    settle(&mut rt);
    let row = element(&rt, &format!("{KEY}-row-0"));
    assert!(row.item_selected, "the row reports item_selected");

    let json = rt.accessibility_json();
    let tree = find_json_key(&json, KEY).expect("the tree is in the a11y tree");
    assert_eq!(
        tree.get("activeDescendant").and_then(Value::as_str),
        Some("tree-row-0"),
        "the tree points at its cursor row: {tree}"
    );
    let row = find_json_key(&json, &format!("{KEY}-row-0")).expect("the row is in the a11y tree");
    assert!(
        json_states(row).contains(&"selected"),
        "the painted selection is the accessible one: {row}"
    );
}

#[test]
fn two_trees_with_the_same_key_page_by_their_own_height() {
    let mut short = probe(Probe::range_at(60, 200.0), (960.0, 640.0));
    press(&mut short, Key::Tab);
    press(&mut short, Key::PageDown);
    settle(&mut short);
    assert_eq!(
        short.state.selects.last().map(String::as_str),
        Some("node-6")
    );
    press(&mut short, Key::PageDown);
    settle(&mut short);
    assert_eq!(
        short.state.selects.last().map(String::as_str),
        Some("node-12")
    );

    // A second window with the same tree key pages by its own 400pt viewport.
    let mut tall = probe(Probe::range_at(60, 400.0), (960.0, 640.0));
    press(&mut tall, Key::Tab);
    press(&mut tall, Key::PageDown);
    settle(&mut tall);
    assert_eq!(
        tall.state.selects.last().map(String::as_str),
        Some("node-14"),
        "the tall window paged by the short window's height"
    );
    press(&mut short, Key::PageDown);
    settle(&mut short);
    assert_eq!(
        short.state.selects.last().map(String::as_str),
        Some("node-18"),
        "the short window kept its own page height"
    );
}

#[test]
fn a_growing_tree_pages_by_the_viewport_stored_in_state() {
    let mut rt = probe(Probe::growing(60), (960.0, 640.0));
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(
        rt.state.viewport, 200.0,
        "the viewport wrote its height to S"
    );
    assert_eq!(
        rt.state.selects.last().map(String::as_str),
        Some("node-6"),
        "a 200pt viewport pages seven rows"
    );
}

#[test]
fn rows_without_a_glyph_keep_the_icon_lane() {
    let mut rt = probe(
        Probe {
            icons: true,
            ..Probe::range(4)
        },
        (960.0, 640.0),
    );
    rt.frame(0.0);
    let with = element(&rt, &format!("{KEY}-row-0-label")).rect[0];
    let without = element(&rt, &format!("{KEY}-row-1-label")).rect[0];
    assert!(
        near(with, without),
        "the icon lane is reserved for every row: {with} vs {without}"
    );
    assert!(
        has(&rt, &format!("{KEY}-row-1-icon")),
        "the empty lane is mounted"
    );
}

#[test]
fn an_empty_hugging_tree_keeps_its_status_line_inside() {
    let rt = probe(Probe::hugging(&[]), (960.0, 640.0));
    let root = element(&rt, KEY);
    let empty = element(&rt, &format!("{KEY}-empty"));
    eprintln!("root {:?}, empty {:?}", root.rect, empty.rect);
    assert!(
        empty.rect[3] <= root.rect[3] + 0.5,
        "the 40pt empty line is clipped by a {}pt root",
        root.rect[3]
    );
}

#[test]
fn a_five_hundred_row_tree_stays_inside_both_windows_except_the_extent() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = probe(Probe::range(500), (w, h));
        let extent = element(&rt, &format!("{KEY}-rows"));
        assert!(
            extent.rect[3] > 200.0,
            "the virtual extent is the long scroll content: {:?}",
            extent.rect
        );
        let mut outside = 0;
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else {
                continue;
            };
            if !key.starts_with(KEY) || key == format!("{KEY}-rows") {
                continue;
            }
            let [x, y, ew, eh] = e.rect;
            if x < -0.5 || y < -0.5 || x + ew > w + 0.5 || y + eh > h + 0.5 {
                outside += 1;
                eprintln!("{key} leaves {w}x{h}: {:?}", e.rect);
            }
        }
        assert_eq!(outside, 0, "500-row tree nodes leave a {w}x{h} window");
    }
}
