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
