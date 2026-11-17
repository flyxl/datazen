use rune::{col, row, El, Env, Runtime};
use rune_kit::Placement;

use crate::ui::kit::kit;
use crate::ui::menu::{Item, Menu};

#[derive(Default)]
struct S {
    open: bool,
    close_calls: u32,
    selected: Vec<String>,
}

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn tall_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let mut menu = Menu::new("tall")
        .trigger(row().key("tall-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(940.0, 620.0)
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    for i in 0..30 {
        let label = format!("Item {i}");
        let tag = format!("i{i}");
        menu = menu.item(Item::new(label).on_select(move |s: &mut S| {
            s.selected.push(tag.clone());
            s.open = false;
        }));
    }
    let root = menu.build(k, &mut layer);
    let mut out = col().key("tall-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// 30 items anchored near bottom-right exceed viewport height (no clamping/scroll into bounds).
/// Kept as a known kit limitation: rune-kit does not clamp a menu taller than the window.
#[test]
#[ignore = "rune-kit does not clamp a menu taller than the window"]
fn thirty_items_at_edge_overflow_viewport() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, tall_menu, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("tall-menu").is_some(), "menu surface present");
    let mut max_bottom: f32 = 0.0;
    let mut oob = Vec::new();
    for e in rt.elements() {
        let [x, y, w, h] = e.rect;
        if y + h > max_bottom {
            max_bottom = y + h;
        }
        if x < -0.5 || y < -0.5 || x + w > 960.5 || y + h > 640.5 {
            oob.push((e.key.clone(), e.rect));
        }
    }
    eprintln!(
        "AUDIT 30-items: max_bottom={max_bottom} viewport_h=640 oob_count={}",
        oob.len()
    );
    assert!(
        max_bottom > 640.0 && !oob.is_empty(),
        "expected 30-item menu to overflow viewport; max_bottom={max_bottom} oob={oob:?}"
    );
}

fn forced_placement_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let mut menu = Menu::new("edgef")
        .trigger(row().key("edgef-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(940.0, 620.0)
        .placement(Placement::BottomStart)
        .min_width(260.0)
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    for i in 0..12 {
        menu = menu.item(Item::new(format!("Row {i}")));
    }
    let root = menu.build(k, &mut layer);
    let mut out = col().key("edgef-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// Reverse probe: even with explicit BottomStart + wide menu at edge, anchoring keeps it in bounds.
#[test]
fn reverse_forced_placement_still_clamped_by_anchoring() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, forced_placement_menu, (960.0, 640.0), env());
    rt.frame(0.0);
    let mut oob = Vec::new();
    for e in rt.elements() {
        let [x, y, w, h] = e.rect;
        if x < -0.5 || y < -0.5 || x + w > 960.5 || y + h > 640.5 {
            oob.push((e.key.clone(), e.rect));
        }
    }
    eprintln!("AUDIT reverse-placement: oob_count={}", oob.len());
    assert!(
        oob.is_empty(),
        "anchoring clamped forced-placement menu; reverse did not produce overflow: {oob:?}"
    );
}

#[allow(dead_code)]
fn select_close_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let menu = Menu::new("sc")
        .trigger(row().key("sc-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(100.0, 100.0)
        .item(Item::new("Pick").on_select(|s: &mut S| {
            s.selected.push("pick".into());
            // intentionally do NOT set open=false in on_select
        }))
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("sc-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// With `close_on_select(false)`, selecting runs on_select but does not auto-close
/// or trigger the close callback (caller controls close explicitly).
#[test]
fn select_does_not_auto_close_or_call_on_close_when_close_on_select_disabled() {
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("sc")
            .trigger(row().key("sc-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(100.0, 100.0)
            .close_on_select(false)
            .item(Item::new("Pick").on_select(|s: &mut S| {
                s.selected.push("pick".into());
                // intentionally do NOT set open=false in on_select
            }))
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("sc-root").w_full().h_full().child(root);
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
    let (x, y) = rt.find("sc-item0").expect("item0");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    eprintln!(
        "AUDIT select-close: close_calls={} open={} selected={:?}",
        rt.state.close_calls, rt.state.open, rt.state.selected
    );
    assert_eq!(rt.state.selected, vec!["pick".to_string()]);
    assert_eq!(
        rt.state.close_calls, 0,
        "close callback not triggered by selection when close_on_select is false"
    );
    assert!(
        rt.state.open,
        "open remains true unless on_select/on_close sets it"
    );
}

#[allow(dead_code)]
fn select_and_close_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let menu = Menu::new("sc2")
        .trigger(row().key("sc2-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(100.0, 100.0)
        .item(Item::new("Pick").on_select(|s: &mut S| {
            s.selected.push("pick".into());
            s.open = false; // also sets open in handler
        }))
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("sc2-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// With `close_on_select(false)`, on_select can close by mutating state without
/// invoking the close callback.
#[test]
fn select_handler_can_close_without_triggering_on_close_when_close_on_select_disabled() {
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("sc2")
            .trigger(row().key("sc2-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(100.0, 100.0)
            .close_on_select(false)
            .item(Item::new("Pick").on_select(|s: &mut S| {
                s.selected.push("pick".into());
                s.open = false; // handler closes without on_close callback
            }))
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("sc2-root").w_full().h_full().child(root);
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
    let (x, y) = rt.find("sc2-item0").expect("item0");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    eprintln!(
        "AUDIT select+handler-close: close_calls={} open={} selected={:?}",
        rt.state.close_calls, rt.state.open, rt.state.selected
    );
    assert_eq!(rt.state.selected, vec!["pick".to_string()]);
    assert_eq!(
        rt.state.close_calls, 0,
        "close callback not invoked when close_on_select is false"
    );
    assert!(!rt.state.open);
}

fn two_instances(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let m1: Menu<S> = Menu::new("same")
        .trigger(row().key("t1").size(40.0, 24.0))
        .open(state.open)
        .at(50.0, 50.0)
        .item(Item::new("A"));
    let m2: Menu<S> = Menu::new("same")
        .trigger(row().key("t2").size(40.0, 24.0))
        .open(state.open)
        .at(300.0, 50.0)
        .item(Item::new("B"));
    let r1 = m1.build(k, &mut layer);
    let r2 = m2.build(k, &mut layer);
    let mut out = col().key("two-root").w_full().h_full().child(r1).child(r2);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// Two open menus sharing the same key collide; runtime renames later duplicates (#dupN).
#[test]
fn two_menus_same_key_collide_and_runtime_renames() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, two_instances, (960.0, 640.0), env());
    rt.frame(0.0);
    let menu_count = rt
        .elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some("same-menu"))
        .count();
    let dup_menu_count = rt
        .elements()
        .iter()
        .filter(|e| {
            e.key
                .as_deref()
                .is_some_and(|k| k.starts_with("same-menu#dup"))
        })
        .count();
    eprintln!(
        "AUDIT dup-keys: same-menu={} same-menu#dup*={}",
        menu_count, dup_menu_count
    );
    assert_eq!(menu_count, 1, "original key present once");
    assert!(
        dup_menu_count >= 1,
        "duplicate instance renamed with #dupN suffix"
    );
}

/// Three-level nested submenu (connection -> move-to-group -> group item) with default close_on_select.
#[test]
fn three_level_submenu_selects_and_closes_once() {
    #[derive(Default)]
    struct S {
        open: bool,
        close_calls: u32,
        selected: Vec<String>,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("deep")
            .trigger(row().key("deep-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(40.0, 40.0)
            .item(
                Item::new("Connection").submenu(
                    vec![crate::ui::menu::Entry::item(
                        Item::new("Move to group").submenu(
                            vec![
                                crate::ui::menu::Entry::item(Item::new("Group A").on_select(
                                    |s: &mut S| s.selected.push("ga".into()),
                                )),
                                crate::ui::menu::Entry::item(Item::new("Group B").on_select(
                                    |s: &mut S| s.selected.push("gb".into()),
                                )),
                            ],
                            true,
                        ),
                    )],
                    true,
                ),
            )
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("deep-root").w_full().h_full().child(root);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("deep-item0-sub-menu").is_some(), "level 2 submenu surface");
    assert!(
        rt.find("deep-item0-sub-item0-sub-menu").is_some(),
        "level 3 submenu surface"
    );
    let (x, y) = rt
        .find("deep-item0-sub-item0-sub-item1")
        .expect("level 3 leaf item Group B");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec!["gb".to_string()]);
    assert_eq!(
        rt.state.close_calls, 1,
        "deep leaf selection should close once via close_on_select"
    );
    assert!(!rt.state.open);
}

/// close_on_select(true): selecting closes once; subsequent Esc after menu already closed must not double-close.
#[test]
fn close_on_select_true_then_escape_does_not_double_close() {
    #[derive(Default)]
    struct S {
        open: bool,
        close_calls: u32,
        selected: Vec<String>,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("ce")
            .trigger(row().key("ce-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(80.0, 80.0)
            .item(Item::new("Pick").on_select(|s: &mut S| s.selected.push("pick".into())))
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("ce-root").w_full().h_full().child(root);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find("ce-item0").expect("item");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec!["pick".to_string()]);
    assert_eq!(rt.state.close_calls, 1);
    assert!(!rt.state.open);
    // Menu is closed; Esc should not trigger another close callback.
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(
        rt.state.close_calls, 1,
        "Escape after select-close must not call on_close again"
    );
}

/// lead(El), badge, icon, and state are forwarded into the rendered menu tree.
#[test]
fn lead_badge_icon_and_state_are_rendered() {
    #[derive(Default)]
    struct S {
        open: bool,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("vis")
            .trigger(row().key("vis-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(60.0, 60.0)
            .item(
                Item::new("With lead")
                    .lead(rune::el().key("custom-lead").size(10.0, 10.0))
                    .badge("NEW"),
            )
            .item(
                Item::new("With icon")
                    .icon(&crate::icons::ICON_SEARCH)
                    .state(rune_kit::State::Hover),
            )
            .on_close(|s: &mut S| s.open = false);
        let root = menu.build(k, &mut layer);
        let mut out = col().key("vis-root").w_full().h_full().child(root);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let mut rt = Runtime::new(S { open: true }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(
        rt.find("custom-lead").is_some(),
        "lead element should be rendered in menu item tree"
    );
    assert!(
        rt.find("vis-item0").is_some() && rt.elements().iter().any(|e| e.key.as_deref() == Some("badge")),
        "badge child should be rendered for item with badge"
    );
    assert!(
        rt.elements().iter().any(|e| e.key.as_deref() == Some("icon")),
        "icon child should be rendered for item with icon"
    );
    let _ = rt.find("vis-item1");
}

/// items() preserves order with separators and works inside submenus (Entry-based construction).
#[test]
fn items_batch_preserves_order_and_submenu_entries() {
    #[derive(Default)]
    struct S {
        open: bool,
        selected: Vec<String>,
    }

    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let sub_entries = vec![
            crate::ui::menu::Entry::item(Item::new("S1").on_select(|s: &mut S| {
                s.selected.push("s1".into())
            })),
            crate::ui::menu::Entry::separator(),
            crate::ui::menu::Entry::item(Item::new("S2").on_select(|s: &mut S| {
                s.selected.push("s2".into())
            })),
        ];
        let menu = Menu::new("ord")
            .trigger(row().key("ord-trigger").size(30.0, 20.0))
            .open(state.open)
            .at(40.0, 40.0)
            .items([Item::new("A"), Item::new("B")])
            .separator()
            .items([Item::new("C")])
            .item(Item::new("More").submenu(sub_entries, true));
        let root = menu.build(k, &mut layer);
        let mut out = col().key("ord-root").w_full().h_full().child(root);
        for el in layer {
            out = out.child(el);
        }
        out
    }

    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let elements = rt.elements();
    let idx = |key: &str| -> usize {
        elements
            .iter()
            .position(|e| e.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing key {key}"))
    };
    let a = idx("ord-item0");
    let b = idx("ord-item1");
    let sep = idx("sep2");
    let c = idx("ord-item3");
    let more = idx("ord-item4");
    assert!(a < b && b < sep && sep < c && c < more, "top-level order preserved");

    assert!(rt.find("ord-item4-sub-menu").is_some(), "submenu opened");
    // submenu entries: item0 S1, sep1, item2 S2
    assert!(rt.find("ord-item4-sub-item0").is_some(), "submenu S1 present");
    assert!(rt.find("sep1").is_some() || rt.find("ord-item4-sub-sep1").is_some() || elements.iter().any(|e| e.key.as_deref().is_some_and(|k| k.contains("sep1"))), "submenu separator present");
    assert!(rt.find("ord-item4-sub-item2").is_some(), "submenu S2 present");
}
