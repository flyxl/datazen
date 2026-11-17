use rune::{col, row, text, El, Env, Key, KeyEvent, Mods, PointerPress, Runtime};
use rune_kit::Placement;

use crate::ui::kit::kit;
use crate::ui::menu::{context_menu, Item, Menu};

#[derive(Default)]
struct ProbeState {
    open: bool,
    close_calls: u32,
    selected: Vec<String>,
    point: Option<PointerPress>,
}

fn menu_layer(state: &ProbeState, cx: &mut rune::Cx<ProbeState>) -> El {
    let _ = cx;
    let k = kit(true);
    let mut layer: Vec<El> = Vec::new();
    let trigger = row()
        .key("menu-trigger")
        .size(80.0, 28.0)
        .center()
        .child(text("Open"));
    let menu = Menu::new("probe")
        .trigger(trigger)
        .open(state.open)
        .at(100.0, 100.0)
        .placement(Placement::BottomStart)
        .min_width(160.0)
        .group_label("Group")
        .item(Item::new("Alpha").on_select(|s: &mut ProbeState| {
            s.selected.push("alpha".into());
            s.open = false;
        }))
        .separator()
        .item(Item::new("Beta").on_select(|s: &mut ProbeState| {
            s.selected.push("beta".into());
            s.open = false;
        }))
        .item(
            Item::new("Gamma")
                .disabled(true)
                .on_select(|s: &mut ProbeState| {
                    s.selected.push("gamma".into());
                    s.open = false;
                }),
        )
        .item(Item::new("Delta").on_select(|s: &mut ProbeState| {
            s.selected.push("delta".into());
            s.open = false;
        }))
        .on_close(|s: &mut ProbeState| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("menu-probe-root").w_full().h_full();
    out = out.child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

fn edge_menu_layer(state: &ProbeState, _cx: &mut rune::Cx<ProbeState>) -> El {
    let k = kit(true);
    let mut layer: Vec<El> = Vec::new();
    let menu = Menu::new("edge")
        .trigger(row().key("edge-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(940.0, 620.0)
        .item(Item::new("One").on_select(|s: &mut ProbeState| {
            s.selected.push("one".into());
            s.open = false;
        }))
        .item(Item::new("Two").on_select(|s: &mut ProbeState| {
            s.selected.push("two".into());
            s.open = false;
        }))
        .on_close(|s: &mut ProbeState| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("edge-root").w_full().h_full();
    out = out.child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

fn edge_menu_layer_wide(state: &ProbeState, _cx: &mut rune::Cx<ProbeState>) -> El {
    let k = kit(true);
    let mut layer: Vec<El> = Vec::new();
    let menu = Menu::new("edge")
        .trigger(row().key("edge-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(1420.0, 796.0)
        .item(Item::new("One").on_select(|s: &mut ProbeState| {
            s.selected.push("one".into());
            s.open = false;
        }))
        .item(Item::new("Two").on_select(|s: &mut ProbeState| {
            s.selected.push("two".into());
            s.open = false;
        }))
        .on_close(|s: &mut ProbeState| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("edge-root").w_full().h_full();
    out = out.child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

fn context_layer(state: &ProbeState, _cx: &mut rune::Cx<ProbeState>) -> El {
    let k = kit(true);
    let mut layer: Vec<El> = Vec::new();
    let trigger = row()
        .key("ctx-area")
        .w_full()
        .h_full()
        .child(text("area"));
    let menu = context_menu::<ProbeState>("ctx", trigger, state.point)
        .item(Item::new("Copy").on_select(|s: &mut ProbeState| {
            s.selected.push("copy".into());
            s.point = None;
        }))
        .on_close(|s: &mut ProbeState| {
            s.close_calls += 1;
            s.point = None;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("ctx-root").w_full().h_full();
    out = out.child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[test]
fn menu_keyboard_selects_and_escape_closes() {
    let state = ProbeState {
        open: true,
        ..ProbeState::default()
    };
    let mut rt = Runtime::new(state, menu_layer, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("probe-menu").is_some(), "open menu surface is present");

    // Entry indices: label0, item1 Alpha, sep2, item3 Beta, item4 Gamma(disabled), item5 Delta.
    // Down moves through enabled items in order; Enter selects the highlighted item.
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    let first = rt.state.selected.clone();
    assert!(
        first == vec!["alpha".to_string()] || first == vec!["beta".to_string()],
        "first Down+Enter should select an enabled item, got {first:?}"
    );

    // Reopen, move down twice from initial highlight and select: reaches Beta or Delta depending on start.
    rt.state.open = true;
    rt.state.selected.clear();
    rt.invalidate();
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    assert!(
        !rt.state.selected.is_empty(),
        "keyboard navigation should select an enabled item"
    );
    assert!(
        rt.state.selected[0] != "gamma",
        "keyboard navigation must not select disabled item"
    );

    // Verify Up moves highlight (select something, reopen, go down then up then enter differs or returns).
    rt.state.open = true;
    rt.state.selected.clear();
    rt.invalidate();
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    let down_target = rt.state.selected.clone();
    rt.state.open = true;
    rt.state.selected.clear();
    rt.invalidate();
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Up,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    assert_ne!(
        rt.state.selected, down_target,
        "Up should move highlight to a different item than staying down"
    );

    rt.state.open = true;
    rt.invalidate();
    rt.frame(0.0);
    let before = rt.state.close_calls;
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(
        rt.state.close_calls,
        before + 1,
        "Esc should trigger on_close"
    );
    assert!(!rt.state.open, "menu closed by Escape");
}

#[test]
fn menu_mouse_selects_and_click_outside_closes() {
    let state = ProbeState {
        open: true,
        ..ProbeState::default()
    };
    let mut rt = Runtime::new(state, menu_layer, (960.0, 640.0), env());
    rt.frame(0.0);

    // Entry indices: label0, item1 Alpha, sep2, item3 Beta, item4 Gamma, item5 Delta.
    let (x, y) = rt.find("probe-item5").expect("Delta item present");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec!["delta".to_string()]);

    // Click outside closes via scrim.
    rt.state.open = true;
    rt.state.selected.clear();
    rt.invalidate();
    rt.frame(0.0);
    let before = rt.state.close_calls;
    assert!(
        rt.find("probe-scrim").is_some(),
        "dismiss scrim present when open"
    );
    // Click near top-left corner, outside the menu surface.
    rt.pointer_down(5.0, 5.0);
    rt.pointer_up(5.0, 5.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.close_calls,
        before + 1,
        "outside click should trigger on_close"
    );
    assert!(!rt.state.open);
}

#[test]
fn menu_disabled_item_does_not_trigger_select() {
    let state = ProbeState {
        open: true,
        ..ProbeState::default()
    };
    let mut rt = Runtime::new(state, menu_layer, (960.0, 640.0), env());
    rt.frame(0.0);

    // Mouse click on disabled Gamma (item4).
    let (x, y) = rt.find("probe-item4").expect("Gamma item present");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert!(
        rt.state.selected.is_empty(),
        "disabled item must not trigger on_select via mouse, got {:?}",
        rt.state.selected
    );

    // Keyboard: traversing all items must never select the disabled one.
    rt.state.open = true;
    rt.state.selected.clear();
    rt.invalidate();
    rt.frame(0.0);
    for _ in 0..6 {
        rt.key(KeyEvent {
            key: Key::Down,
            mods: Mods::default(),
        });
        rt.frame(0.0);
        rt.key(KeyEvent {
            key: Key::Enter,
            mods: Mods::default(),
        });
        rt.frame(0.0);
        if !rt.state.selected.is_empty() {
            assert_ne!(
                rt.state.selected.last().map(String::as_str),
                Some("gamma"),
                "keyboard must not trigger disabled item"
            );
            // reopen for next step
            rt.state.open = true;
            rt.invalidate();
            rt.frame(0.0);
        }
    }
    assert!(
        !rt.state.selected.is_empty(),
        "keyboard traversal should select enabled items"
    );
}

#[test]
fn menu_edge_positions_stay_inside_viewport() {
    for (width, height, wide) in [(960.0f32, 640.0f32, false), (1440.0, 816.0, true)] {
        let state = ProbeState {
            open: true,
            ..ProbeState::default()
        };
        let mut rt = if wide {
            Runtime::new(state, edge_menu_layer_wide, (width, height), env())
        } else {
            Runtime::new(state, edge_menu_layer, (width, height), env())
        };
        rt.frame(0.0);
        assert!(
            rt.find("edge-menu").is_some(),
            "menu surface present at {width}x{height}"
        );
        for e in rt.elements() {
            let [x, y, w, h] = e.rect;
            assert!(
                x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                "{width}x{height}: {:?} {:?} is out of bounds",
                e.key,
                e.rect
            );
        }
    }
}

#[test]
fn menu_separator_and_group_label_keys_are_ordered() {
    let state = ProbeState {
        open: true,
        ..ProbeState::default()
    };
    let mut rt = Runtime::new(state, menu_layer, (960.0, 640.0), env());
    rt.frame(0.0);

    let elements = rt.elements();
    let idx = |key: &str| -> usize {
        elements
            .iter()
            .position(|e| e.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing key {key}"))
    };

    let label = idx("label0");
    let alpha = idx("probe-item1");
    let sep = idx("sep2");
    let beta = idx("probe-item3");
    let gamma = idx("probe-item4");
    let delta = idx("probe-item5");

    assert!(label < alpha, "group label should precede first item");
    assert!(alpha < sep, "first item should precede separator");
    assert!(sep < beta, "separator should precede following item");
    assert!(beta < gamma && gamma < delta, "item order should be preserved");
}

#[test]
fn context_menu_wrapper_mounts_at_point() {
    let point = PointerPress { x: 200.0, y: 150.0 };
    let state = ProbeState {
        point: Some(point),
        ..ProbeState::default()
    };
    let mut rt = Runtime::new(state, context_layer, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("ctx-menu").is_some(), "context menu surface is open");
    assert!(rt.find("ctx-item0").is_some(), "context menu item present");

    // Closed when no point.
    rt.state.point = None;
    rt.invalidate();
    rt.frame(0.0);
    assert!(
        rt.find("ctx-menu").is_none(),
        "context menu closes when point is cleared"
    );
}

#[test]
fn submenu_item_selection_runs_handler_and_close_on_select() {
    #[derive(Default)]
    struct S {
        open: bool,
        close_calls: u32,
        selected: Vec<String>,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("subm")
            .trigger(row().key("subm-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(100.0, 100.0)
            .item(
                Item::new("More").submenu(
                    vec![crate::ui::menu::Entry::item(Item::new("Copy").on_select(|s: &mut S| {
                        s.selected.push("copy".into());
                    }))],
                    true,
                ),
            )
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("subm-root").w_full().h_full().child(root);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("subm-item0-sub-menu").is_some(), "submenu surface present");
    let (x, y) = rt
        .find("subm-item0-sub-item0")
        .expect("submenu leaf item present");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec!["copy".to_string()]);
    assert_eq!(
        rt.state.close_calls, 1,
        "leaf submenu selection should trigger close callback by default"
    );
}

#[test]
fn menu_item_capabilities_are_reflected_in_tree() {
    #[derive(Default)]
    struct S {
        open: bool,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("cap")
            .trigger(row().key("cap-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(80.0, 80.0)
            .item(
                Item::new("Cut")
                    .shortcut("Ctrl+X")
                    .description("Cut selection"),
            )
            .item(Item::new("Check").checked(true))
            .item(Item::new("Delete").danger(true))
            .on_close(|s: &mut S| {
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("cap-root").w_full().h_full().child(root);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let state = S { open: true };
    let mut rt = Runtime::new(state, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let tree = rt.accessibility_json();

    fn find<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
        if v["key"] == key {
            return Some(v);
        }
        v["children"]
            .as_array()?
            .iter()
            .find_map(|c| find(c, key))
    }

    let cut = find(&tree, "cap-item0").expect("cut item in a11y tree");
    assert_eq!(
        cut["shortcut"].as_str(),
        Some("Ctrl+X"),
        "shortcut should be exposed in accessibility tree"
    );
    assert!(
        cut["description"]
            .as_str()
            .is_some_and(|d| d.contains("Cut selection")),
        "description should be exposed in accessibility tree: {cut}"
    );

    let check = find(&tree, "cap-item1").expect("checked item in a11y tree");
    let states = check["states"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(
        states.iter().any(|s| s == "checked"),
        "checked item should expose checked state: {check}"
    );

    let del = find(&tree, "cap-item2").expect("danger item in a11y tree");
    assert!(
        rt.find("cap-item2").is_some() && del["name"].as_str() == Some("Delete"),
        "danger item should render and be present in accessibility tree: {del}"
    );
}

#[test]
fn items_batch_matches_item_calls_for_keys() {
    #[derive(Default)]
    struct S {
        open: bool,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let by_item = Menu::<S>::new("mi")
            .trigger(row().key("mi-trigger").size(30.0, 20.0))
            .open(state.open)
            .at(40.0, 40.0)
            .item(Item::new("A"))
            .item(Item::new("B"))
            .separator()
            .item(Item::new("C"));
        let by_items = Menu::<S>::new("mb")
            .trigger(row().key("mb-trigger").size(30.0, 20.0))
            .open(state.open)
            .at(240.0, 40.0)
            .items([Item::new("A"), Item::new("B")])
            .separator()
            .items([Item::new("C")]);
        let r1 = by_item.build(k, &mut layer);
        let r2 = by_items.build(k, &mut layer);
        let mut out = col().key("batch-root").w_full().h_full().child(r1).child(r2);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let state = S { open: true };
    let mut rt = Runtime::new(state, view, (960.0, 640.0), env());
    rt.frame(0.0);

    for (a, b) in [
        ("mi-item0", "mb-item0"),
        ("mi-item1", "mb-item1"),
        ("sep2", "sep2"),
        ("mi-item3", "mb-item3"),
    ] {
        assert!(rt.find(a).is_some(), "expected key {a} from item() form");
        assert!(rt.find(b).is_some(), "expected key {b} from items() form");
    }
}

#[test]
fn close_on_select_true_closes_and_false_keeps_open() {
    #[derive(Default)]
    struct S {
        open: bool,
        close_calls: u32,
        selected: Vec<String>,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let m_true = Menu::new("ct")
            .trigger(row().key("ct-trigger").size(30.0, 20.0))
            .open(state.open)
            .at(40.0, 40.0)
            .item(Item::new("A").on_select(|s: &mut S| {
                s.selected.push("a".into());
            }))
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let r1 = m_true.build(k, &mut layer);
        let mut out = col().key("ct-root").w_full().h_full().child(r1);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    fn view_false(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let m_false = Menu::new("cf")
            .trigger(row().key("cf-trigger").size(30.0, 20.0))
            .open(state.open)
            .at(40.0, 40.0)
            .close_on_select(false)
            .item(Item::new("A").on_select(|s: &mut S| {
                s.selected.push("a".into());
            }))
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let r1 = m_false.build(k, &mut layer);
        let mut out = col().key("cf-root").w_full().h_full().child(r1);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find("ct-item0").expect("item");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec!["a".to_string()]);
    assert_eq!(rt.state.close_calls, 1, "close_on_select(true) should invoke close callback");
    assert!(!rt.state.open, "close_on_select(true) should close");

    let mut rt = Runtime::new(S { open: true, ..S::default() }, view_false, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find("cf-item0").expect("item");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec!["a".to_string()]);
    assert_eq!(rt.state.close_calls, 0, "close_on_select(false) should not invoke close callback");
    assert!(rt.state.open, "close_on_select(false) should keep menu open");
}

#[test]
fn outside_click_triggers_close_once() {
    #[derive(Default)]
    struct S {
        open: bool,
        close_calls: u32,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("once")
            .trigger(row().key("once-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(120.0, 120.0)
            .item(Item::new("Only"))
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("once-root").w_full().h_full().child(root);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("once-scrim").is_some(), "scrim present");
    rt.pointer_down(5.0, 5.0);
    rt.pointer_up(5.0, 5.0);
    rt.frame(0.0);
    assert_eq!(rt.state.close_calls, 1, "outside click should call on_close exactly once");
    assert!(!rt.state.open);
}
