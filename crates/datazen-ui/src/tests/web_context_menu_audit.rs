//! T096 independent audit: fresh vectors for the web context-menu host.
//!
//! Deliberately different from the coder's `web_context_menu.rs`: a
//! trailing-separator-heavy normalize case, predefined `Separator` rows
//! mixed into the dedup, a two-level submenu nesting, an explicitly-id'd
//! submenu and separator, the minimum window in light theme, and the
//! predefined `Copy` command instead of `Cut`. The audit also reproduces
//! one of the coder's position numbers (the bottom-right flip) from its
//! own fresh inputs.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime};
use rune_kit::root;

use crate::components::ui::{
    is_separator, normalize_native_menu_items, position_root_menu, position_submenu,
    NativeMenuItemDef, PredefinedEdit, SubmenuSide, WebContextMenuHost, WebContextMenuState,
    CONTEXT_MENU_PAD,
};
use crate::ui::kit::kit;

// ---- fresh builders (id'd separators, relabelled predefined) ------------------

fn item(id: &str, log: &Rc<RefCell<Vec<String>>>) -> NativeMenuItemDef {
    let id = id.to_string();
    NativeMenuItemDef::Item {
        id: id.clone(),
        label: id.clone(),
        shortcut: Some(format!("Ctrl+{}", id.chars().next().expect("non-empty"))),
        enabled: None,
        action: {
            let log = Rc::clone(log);
            Rc::new(move || log.borrow_mut().push(id.clone()))
        },
    }
}

fn predefined(item: PredefinedEdit) -> NativeMenuItemDef {
    NativeMenuItemDef::Predefined { item, text: None }
}

fn sep_id(id: &str) -> NativeMenuItemDef {
    NativeMenuItemDef::Separator { id: Some(id.to_string()) }
}

#[test]
fn audit_normalize_drops_trailing_and_predefined_separators_and_keeps_real_rows() {
    // A trailing-heavy case: [Copy, sep(id), predefined-Separator, item,
    // sep, sep] → the adjacent predefined separator collapses with the
    // explicit one, both trailing separators go, Copy and the item stay.
    let log = Rc::new(RefCell::new(Vec::new()));
    let out = normalize_native_menu_items(vec![
        predefined(PredefinedEdit::Copy),
        sep_id("after-copy"),
        predefined(PredefinedEdit::Separator),
        item("plain", &log),
        NativeMenuItemDef::Separator { id: None },
        sep_id("trailing"),
    ]);
    assert_eq!(out.len(), 3, "Copy, ONE separator, the item: {out:?}");
    assert_eq!(out[0].item_label().as_deref(), Some("Copy"));
    assert!(is_separator(&out[1]), "the first of the separator run survives");
    assert!(!is_separator(&out[2]));
    assert_eq!(out[2].item_label().as_deref(), Some("plain"));

    // Two-level nesting: an inner submenu that normalizes to nothing is
    // dropped, so its parent normalizes to nothing, so the parent is
    // dropped too — React recurses both directions.
    let inner = vec![sep_id("lead"), predefined(PredefinedEdit::Separator)];
    let out = normalize_native_menu_items(vec![
        NativeMenuItemDef::Submenu {
            id: Some("level0".to_string()),
            label: "Level0".to_string(),
            items: vec![NativeMenuItemDef::Submenu {
                id: None,
                label: "Level1".to_string(),
                items: inner,
            }],
        },
        item("sibling", &log),
    ]);
    assert_eq!(out.len(), 1, "the all-separator chain vanishes wholesale");
    assert_eq!(out[0].item_label().as_deref(), Some("sibling"));

    // Disabled items are kept (React documents this in the helper).
    let out = normalize_native_menu_items(vec![
        NativeMenuItemDef::Item {
            id: "off".to_string(),
            label: "off".to_string(),
            shortcut: None,
            enabled: Some(false),
            action: Rc::new(|| {}),
        },
        sep_id("end"),
    ]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].item_label().as_deref(), Some("off"));
    assert!(!out[0].enabled());
}

#[test]
fn audit_position_math_reproduces_the_flip_numbers_from_fresh_inputs() {
    // The coder's bottom-right flip, reproduced from the same React rule:
    // left = 1430 - 180, top = 800 - 40, clamped into [8, 1252] x [8, 768].
    assert_eq!(
        position_root_menu((1430.0, 800.0), (180.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (1250.0, 760.0)
    );
    // Fresh: a menu as wide as the viewport minus two pads stays exactly on
    // the pad; a taller-than-viewport menu top-pins to the pad.
    assert_eq!(
        position_root_menu((0.0, 0.0), (1424.0, 900.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (8.0, 8.0)
    );

    // Fresh submenu cases: an item hugging the left edge flips right even
    // with room on the left, and a bottom item lifts the panel.
    assert_eq!(
        position_submenu((8.0, 100.0, 180.0, 30.0), (160.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (188.0, 100.0, SubmenuSide::Right)
    );
    // Right edge + more room on the left: flips left onto item.left - width.
    assert_eq!(
        position_submenu((1260.0, 100.0, 160.0, 30.0), (160.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (1100.0, 100.0, SubmenuSide::Left)
    );
    // Near the bottom: lifted to viewport - pad - height.
    assert_eq!(
        position_submenu((100.0, 790.0, 180.0, 30.0), (160.0, 120.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (280.0, 688.0, SubmenuSide::Right)
    );
}

#[test]
fn audit_store_semantics_through_show_hide_and_refine() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut state = WebContextMenuState {
        viewport: (960.0, 640.0),
        menu_size: (200.0, 60.0),
        submenu_size: (150.0, 50.0),
        ..Default::default()
    };
    // A predefined-only menu is a real menu.
    state.show(
        vec![predefined(PredefinedEdit::Paste), predefined(PredefinedEdit::Undo)],
        (30.0, 40.0),
    );
    assert!(state.open);
    assert_eq!(state.items.len(), 2);

    // An explicitly-id'd submenu opens under that id, not the index
    // fallback, and refine uses the measured submenu size.
    state.items.push(NativeMenuItemDef::Submenu {
        id: Some("tools".to_string()),
        label: "Tools".to_string(),
        items: vec![item("inner", &log)],
    });
    state.on_item_enter("tools", (400.0, 300.0, 200.0, 28.0));
    let sub = state.submenu.clone().expect("opened under the explicit id");
    assert_eq!(sub.id, "tools");
    assert_eq!(sub.pos, (600.0, 300.0), "(rect.right, rect.top) before refine");
    state.refine_submenu();
    // positionSubmenu((400,300,200,28), (150,50), (960,640), 8): right side,
    // left = 600, top = 300 (fits) — the guard leaves it in place.
    assert_eq!(state.submenu.as_ref().expect("still open").pos, (600.0, 300.0));

    // The same trigger near the right edge flips the refined position left.
    state.on_item_enter("tools", (820.0, 300.0, 130.0, 28.0));
    state.refine_submenu();
    assert_eq!(
        state.submenu.as_ref().expect("still open").pos,
        (670.0, 300.0),
        "820 - 150"
    );

    // An index-fallback id does NOT match the explicit id.
    state.on_item_enter("sub-2", (0.0, 0.0, 1.0, 1.0));
    assert!(state.submenu.is_none(), "the fallback id does not open the explicit-id def");

    state.hide();
    assert!(!state.open && state.items.is_empty() && state.submenu.is_none());
}

// ---- the mounted tree: fresh ids, id'd separator, min window, light ----------

#[derive(Default)]
struct Probe {
    menu: WebContextMenuState,
    commands: Vec<&'static str>,
}

const MIN: (f32, f32) = (960.0, 640.0);

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    WebContextMenuHost::<Probe>::new(&state.menu)
        .host_mut(|s: &mut Probe| &mut s.menu)
        .on_predefined(|s: &mut Probe, edit| {
            s.commands.push(edit.command().expect("an edit command"));
        })
        .mount(dark, &mut layer);
    root(
        col().key("audit-page").w_full().h_full().child(col().key("audit-content").w_full().h_full()),
        layer,
    )
}

fn audit_menu(log: &Rc<RefCell<Vec<String>>>) -> WebContextMenuState {
    let mut menu = WebContextMenuState {
        viewport: MIN,
        menu_size: (190.0, 96.0),
        submenu_size: (150.0, 44.0),
        ..Default::default()
    };
    let items = vec![
        predefined(PredefinedEdit::Copy),
        sep_id("after-copy"),
        item("audit-open", log),
        NativeMenuItemDef::Submenu {
            id: Some("tools".to_string()),
            label: "Tools".to_string(),
            items: vec![item("audit-inner", log)],
        },
    ];
    menu.show(items, (60.0, 80.0));
    menu
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn has(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn audit_the_tree_at_min_light_with_id_separator_and_shortcut() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let menu = audit_menu(&log);
    let mut rt = Runtime::new(Probe { menu, ..Default::default() }, view, MIN, Env { system_dark: false, scale: 1.0 });
    rt.frame(0.0);

    let panel = element(&rt, "web-context-menu");
    assert_eq!(panel.role, "menu");
    assert_eq!(panel.rect[0], 60.0, "the panel sits at the cursor x");
    assert_eq!(panel.rect[1], 80.0, "and the cursor y");
    assert!(panel.rect[2] >= 180.0, "min-w-[180px]");

    // The id'd separator mounts under its React list key (the id itself).
    let sep = element(&rt, "after-copy");
    assert_eq!(sep.role, "separator");
    assert_eq!(sep.rect[3], 1.0, "h-px");
    assert_eq!(sep.rect[2], panel.rect[2], "the separator spans the panel width");

    // The shortcut renders as a right-aligned muted caption on the plain
    // item (`text-xs text-fg-muted`), fresh ids, light theme.
    let row = element(&rt, "web-context-item-audit-open");
    let shortcut_path = format!("{}/shortcut", row.path);
    let shortcut = rt
        .elements()
        .into_iter()
        .find(|e| e.path == shortcut_path && !e.exiting)
        .unwrap_or_else(|| panic!("{shortcut_path}: the shortcut is laid out"));
    let k = kit(false);
    assert_eq!(shortcut.text_color, Some(k.c.text_muted), "React text-xs text-fg-muted");
    assert_eq!(
        shortcut.rect[0] + shortcut.rect[2],
        row.rect[0] + row.rect[2] - 12.0,
        "justify-between puts the shortcut at the row's right edge, inset by the row's px-3"
    );

    // The explicitly-id'd trigger mounts under its id (not the fallback).
    assert!(has(&rt, "web-context-submenu-trigger-tools"));
    assert!(!has(&rt, "web-context-submenu"));
}

#[test]
fn audit_presses_choose_copy_deliver_its_command_and_flip_the_submenu() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let menu = audit_menu(&log);
    let mut rt = Runtime::new(Probe { menu, ..Default::default() }, view, MIN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);

    // Choose the predefined Copy: the store hides, the command id lands.
    click(&mut rt, "web-context-pre-Copy");
    assert!(!has(&rt, "web-context-menu"), "hide() ran");
    assert_eq!(rt.state.commands, vec!["copy"]);
    assert!(log.borrow().is_empty(), "no item action ran");

    // A fresh runtime: open the submenu under the explicit id, and the
    // refined position flips left when the trigger sits near the right
    // edge of the minimum window.
    let menu = audit_menu(&log);
    let mut rt = Runtime::new(Probe { menu, ..Default::default() }, view, MIN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    let trigger = element(&rt, "web-context-submenu-trigger-tools");
    let rect = (trigger.rect[0], trigger.rect[1], trigger.rect[2], trigger.rect[3]);
    rt.state.menu.on_item_enter("tools", rect);
    // Refine against the measured submenu size, exactly the layout effect.
    // (A direct `rt.state` mutation does not mark the runtime dirty —
    // `frame` only re-runs the view when state changed — so `settle_all`
    // forces the rebuild, the backup-window precedent.)
    rt.state.menu.submenu_size = (150.0, 44.0);
    rt.state.menu.refine_submenu();
    rt.settle_all();
    rt.frame(0.0);
    let sub = element(&rt, "web-context-submenu");
    assert_eq!(sub.role, "menu");
    assert!(
        (sub.rect[0] - (rect.0 + rect.2)).abs() <= 0.5,
        "right of the trigger: {:?} vs {:?}",
        sub.rect,
        rect
    );
    assert!((sub.rect[1] - rect.1).abs() <= 0.5, "aligned to the trigger top");
    assert!(has(&rt, "web-context-item-audit-inner"));

    // Choosing the inner item: hide first, then the action.
    click(&mut rt, "web-context-item-audit-inner");
    assert!(!has(&rt, "web-context-menu"));
    assert_eq!(*log.borrow(), vec!["audit-inner".to_string()]);
    assert!(rt.state.commands.is_empty(), "this fresh runtime delivered no predefined command");
}

#[test]
fn audit_escape_and_the_scrim_dismiss_without_side_effects() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let menu = audit_menu(&log);
    let mut rt = Runtime::new(Probe { menu, ..Default::default() }, view, MIN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(!has(&rt, "web-context-menu"), "Escape dismissed the menu");
    assert!(log.borrow().is_empty() && rt.state.commands.is_empty());

    let menu = audit_menu(&log);
    let mut rt = Runtime::new(Probe { menu, ..Default::default() }, view, MIN, Env { system_dark: false, scale: 1.0 });
    rt.frame(0.0);
    click(&mut rt, "web-context-scrim");
    assert!(!has(&rt, "web-context-menu"), "an outside press dismissed the menu");
    assert!(log.borrow().is_empty() && rt.state.commands.is_empty());
}

#[test]
fn audit_a_separators_only_show_never_opens_the_host() {
    // The normalization empty result keeps the host closed: no scrim, no
    // panel, exactly React's `null`.
    let mut menu = WebContextMenuState { viewport: MIN, ..Default::default() };
    menu.show(
        vec![sep_id("a"), predefined(PredefinedEdit::Separator), sep_id("b")],
        (50.0, 60.0),
    );
    let mut rt = Runtime::new(Probe { menu, ..Default::default() }, view, MIN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    for key in ["web-context-menu", "web-context-scrim", "web-context-submenu"] {
        assert!(!has(&rt, key), "nothing mounts for {key}");
    }
}
