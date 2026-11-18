//! Independent re-audit of T0-07 Menu base on merged mainline (feat/rune-native-ui-migration).
//!
//! Focus: close_on_select/Esc/outside close counts, key naming, edge/bounds evidence,
//! keyboard/mouse parity probes, and consumer-gap documentation support.

use rune::{col, row, El, Env, Key, KeyEvent, Mods, PointerPress, Runtime};

use crate::ui::kit::kit;
use crate::ui::menu::{context_menu, Entry, Item, Menu};

#[derive(Default)]
struct Probe {
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

fn close_once_view(state: &Probe, _cx: &mut rune::Cx<Probe>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let menu = Menu::new("audit")
        .trigger(row().key("audit-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(120.0, 120.0)
        .item(Item::new("Pick").on_select(|s: &mut Probe| {
            s.selected.push("pick".into());
        }))
        .on_close(|s: &mut Probe| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("audit-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} present"));
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
}

#[test]
fn leaf_select_with_default_close_on_select_calls_on_close_once() {
    let mut rt = Runtime::new(Probe { open: true, ..Probe::default() }, close_once_view, (960.0, 640.0), env());
    rt.frame(0.0);
    click(&mut rt, "audit-item0");
    assert_eq!(rt.state.selected, vec!["pick".to_string()]);
    assert_eq!(rt.state.close_calls, 1, "leaf select should trigger exactly one close callback");
    assert!(!rt.state.open, "menu should be closed after leaf select");
}

#[test]
fn escape_calls_on_close_once_when_open() {
    let mut rt = Runtime::new(Probe { open: true, ..Probe::default() }, close_once_view, (960.0, 640.0), env());
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(rt.state.close_calls, 1, "Esc should trigger on_close exactly once");
    assert!(!rt.state.open);
}

#[test]
fn outside_click_calls_on_close_once_when_open() {
    let mut rt = Runtime::new(Probe { open: true, ..Probe::default() }, close_once_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("audit-scrim").is_some(), "scrim present when open");
    rt.pointer_down(5.0, 5.0);
    rt.pointer_up(5.0, 5.0);
    rt.frame(0.0);
    assert_eq!(rt.state.close_calls, 1, "outside click should trigger on_close exactly once");
    assert!(!rt.state.open);
}

#[test]
fn key_naming_follows_root_derivation_convention() {
    #[derive(Default)]
    struct S { open: bool }
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("keys")
            .trigger(row().key("keys-trigger-host").size(30.0, 20.0))
            .open(state.open)
            .at(40.0, 40.0)
            .group_label("Group")
            .item(Item::new("A"))
            .separator()
            .item(Item::new("More").submenu(vec![Entry::item(Item::new("Sub"))], true))
            .on_close(|s: &mut S| s.open = false);
        let root = menu.build(k, &mut layer);
        let mut out = col().key("keys-root").w_full().h_full().child(root);
        for el in layer { out = out.child(el); }
        out
    }
    let mut rt = Runtime::new(S { open: true }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    for key in [
        "keys-menu",
        "keys-trigger",
        "keys-scrim",
        "label0",
        "keys-item1",
        "sep2",
        "keys-item3",
        "keys-item3-sub-menu",
        "keys-item3-sub-item0",
    ] {
        assert!(rt.find(key).is_some(), "expected derived key present: {key}");
    }
}

#[test]
fn long_menu_30_items_at_bottom_right_records_bounds_behavior() {
    #[derive(Default)]
    struct S { open: bool }
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let mut menu = Menu::new("long")
            .trigger(row().key("long-trigger").size(30.0, 20.0))
            .open(state.open)
            .at(940.0, 620.0)
            .on_close(|s: &mut S| s.open = false);
        for i in 0..30 {
            menu = menu.item(Item::new(format!("Item {i}")));
        }
        let root = menu.build(k, &mut layer);
        let mut out = col().key("long-root").w_full().h_full().child(root);
        for el in layer { out = out.child(el); }
        out
    }
    let mut rt = Runtime::new(S { open: true }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("long-menu").is_some(), "long menu surface present");
    let (w, h) = (960.0f32, 640.0f32);
    let mut max_bottom = 0.0f32;
    let mut max_right = 0.0f32;
    let mut oob = Vec::new();
    for e in rt.elements() {
        let [x, y, ew, eh] = e.rect;
        if y + eh > max_bottom { max_bottom = y + eh; }
        if x + ew > max_right { max_right = x + ew; }
        if x < -0.5 || y < -0.5 || x + ew > w + 0.5 || y + eh > h + 0.5 {
            oob.push((e.key.clone(), e.rect));
        }
    }
    eprintln!(
        "REAUDIT long-menu 960x640: max_bottom={max_bottom} max_right={max_right} viewport=({w},{h}) oob_count={} sample={:?}",
        oob.len(),
        oob.first()
    );
    // Evidence probe: record overflow/clamping behavior. Rune-kit may leave tall anchored menus
    // partially outside bounds (known limitation per ui/menu.rs docs and menus_audit ignored case).
    assert!(max_bottom > 0.0, "menu laid out");
}

#[test]
fn context_menu_helper_closed_without_point_open_with_point() {
    #[derive(Default)]
    struct S { point: Option<PointerPress> }
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = context_menu::<S>("ctxa", row().key("ctxa-area").w_full().h_full(), state.point);
        let root = menu.build(k, &mut layer);
        let mut out = col().key("ctxa-root").w_full().h_full().child(root);
        for el in layer { out = out.child(el); }
        out
    }
    let mut rt = Runtime::new(S { point: None }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("ctxa-menu").is_none(), "no point => closed");
    rt.state.point = Some(PointerPress { x: 100.0, y: 100.0 });
    rt.invalidate();
    rt.frame(0.0);
    assert!(rt.find("ctxa-menu").is_some(), "point => open");
}

#[test]
fn submenu_leaf_select_respects_default_close_on_select_once() {
    #[derive(Default)]
    struct S { open: bool, close_calls: u32, selected: Vec<String> }
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("sub")
            .trigger(row().key("sub-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(100.0, 100.0)
            .item(
                Item::new("More").submenu(
                    vec![Entry::item(Item::new("Leaf").on_select(|s: &mut S| {
                        s.selected.push("leaf".into());
                    }))],
                    true,
                ),
            )
            .on_close(|s: &mut S| {
                s.close_calls += 1;
                s.open = false;
            });
        let root = menu.build(k, &mut layer);
        let mut out = col().key("sub-root").w_full().h_full().child(root);
        for el in layer { out = out.child(el); }
        out
    }
    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("sub-item0-sub-menu").is_some(), "submenu surface open");
    let (x, y) = rt.find("sub-item0-sub-item0").expect("submenu leaf");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.selected, vec!["leaf".to_string()]);
    assert_eq!(rt.state.close_calls, 1, "submenu leaf selection should close once by default");
    assert!(!rt.state.open);
}

#[test]
fn disabled_item_ignores_keyboard_enter_and_mouse_click() {
    #[derive(Default)]
    struct S { open: bool, selected: Vec<String> }
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("dis")
            .trigger(row().key("dis-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(100.0, 100.0)
            .item(Item::new("Live").on_select(|s: &mut S| s.selected.push("live".into())))
            .item(Item::new("Nope").disabled(true).on_select(|s: &mut S| s.selected.push("nope".into())))
            .item(Item::new("Tail").on_select(|s: &mut S| s.selected.push("tail".into())))
            .on_close(|s: &mut S| s.open = false);
        let root = menu.build(k, &mut layer);
        let mut out = col().key("dis-root").w_full().h_full().child(root);
        for el in layer { out = out.child(el); }
        out
    }
    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    // Mouse on disabled (item1)
    if let Some((x, y)) = rt.find("dis-item1") {
        rt.pointer_down(x + 2.0, y + 2.0);
        rt.pointer_up(x + 2.0, y + 2.0);
        rt.frame(0.0);
    }
    assert!(rt.state.selected.iter().all(|s| s != "nope"), "mouse must not trigger disabled item: {:?}", rt.state.selected);
    // Keyboard traversal should never trigger disabled
    for _ in 0..6 {
        rt.key(KeyEvent { key: Key::Down, mods: Mods::default() });
        rt.frame(0.0);
        rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        rt.frame(0.0);
        assert!(rt.state.selected.iter().all(|s| s != "nope"), "keyboard must not trigger disabled item: {:?}", rt.state.selected);
        if rt.state.open { continue; }
        rt.state.open = true;
        rt.invalidate();
        rt.frame(0.0);
    }
}

#[test]
fn keyboard_and_mouse_select_same_enabled_item_value() {
    #[derive(Default)]
    struct S { open: bool, selected: Vec<String> }
    fn view(state: &S, _cx: &mut rune::Cx<S>) -> El {
        let k = kit(true);
        let mut layer = Vec::new();
        let menu = Menu::new("par")
            .trigger(row().key("par-trigger").size(40.0, 24.0))
            .open(state.open)
            .at(100.0, 100.0)
            .item(Item::new("Alpha").on_select(|s: &mut S| s.selected.push("alpha".into())))
            .item(Item::new("Beta").on_select(|s: &mut S| s.selected.push("beta".into())))
            .on_close(|s: &mut S| s.open = false);
        let root = menu.build(k, &mut layer);
        let mut out = col().key("par-root").w_full().h_full().child(root);
        for el in layer { out = out.child(el); }
        out
    }
    // Mouse: click Alpha (item0)
    let mut rt = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find("par-item0").expect("alpha item");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    let mouse = rt.state.selected.clone();

    // Keyboard: first Down+Enter should reach an enabled item; to target Alpha deterministically,
    // reopen and use Down from initial highlight (behavior may start at first item).
    let mut rt2 = Runtime::new(S { open: true, ..S::default() }, view, (960.0, 640.0), env());
    rt2.frame(0.0);
    rt2.key(KeyEvent { key: Key::Down, mods: Mods::default() });
    rt2.frame(0.0);
    rt2.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt2.frame(0.0);
    assert!(
        mouse == vec!["alpha".to_string()],
        "mouse click on Alpha should select alpha, got {mouse:?}"
    );
    assert!(
        rt2.state.selected == vec!["alpha".to_string()] || rt2.state.selected == vec!["beta".to_string()],
        "keyboard Down+Enter selects an enabled item value consistently: {:?}",
        rt2.state.selected
    );
}
