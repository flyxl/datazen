//! T096: the web context-menu host — the item contract, the position math,
//! the store semantics, and the mounted panels with their press behaviour.
//!
//! The production consumer is the wiring track (T157); these tests are the
//! current callers, mounted through `rune_kit::root` like the dialog tests.

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

// ---- item builders ----------------------------------------------------------

fn action_item(id: &str, log: &Rc<RefCell<Vec<String>>>) -> NativeMenuItemDef {
    let id = id.to_string();
    NativeMenuItemDef::Item {
        id: id.clone(),
        label: id.clone(),
        shortcut: None,
        enabled: None,
        action: {
            let log = Rc::clone(log);
            Rc::new(move || log.borrow_mut().push(id.clone()))
        },
    }
}

fn sep(id: Option<&str>) -> NativeMenuItemDef {
    NativeMenuItemDef::Separator {
        id: id.map(str::to_string),
    }
}

fn predefined(item: PredefinedEdit) -> NativeMenuItemDef {
    NativeMenuItemDef::Predefined {
        item,
        text: None,
    }
}

fn submenu(id: Option<&str>, label: &str, items: Vec<NativeMenuItemDef>) -> NativeMenuItemDef {
    NativeMenuItemDef::Submenu {
        id: id.map(str::to_string),
        label: label.to_string(),
        items,
    }
}

// ---- the pure contract (no Runtime) ------------------------------------------

#[test]
fn normalize_drops_empty_submenus_and_dedupes_separators() {
    let log = Rc::new(RefCell::new(Vec::new()));
    // Leading separator, a duplicate separator, an empty submenu, and a
    // trailing separator all go; the disabled item stays.
    let input = vec![
        sep(None),
        predefined(PredefinedEdit::Cut),
        sep(None),
        sep(Some("dup")),
        submenu(None, "More", Vec::new()),
        action_item("disabled", &log),
        sep(None),
    ];
    let out = normalize_native_menu_items(input);
    // [sep, Cut, sep, sep, empty submenu, disabled, sep] → the leading
    // separator, the SECOND of the adjacent pair, the empty submenu and
    // the trailing separator go; the separator BETWEEN Cut and disabled
    // is kept (React drops only leading/trailing/duplicates).
    assert_eq!(out.len(), 3, "Cut, the separator between, and the item: {out:?}");
    // The kind is not PartialEq (the action is a closure), so identify by
    // the label helper and the separator predicate.
    assert_eq!(out[0].item_label().as_deref(), Some("Cut"));
    assert_eq!(out[1].item_label(), None, "the middle separator stays a separator");
    assert!(is_separator(&out[1]));
    assert_eq!(out[2].item_label().as_deref(), Some("disabled"));
    assert!(!is_separator(&out[0]));
    assert!(!is_separator(&out[2]));

    // Nested normalization: the submenu's own leading separator is dropped
    // inside it, and an empty submenu inside is dropped too — but a submenu
    // with real content survives.
    let inner = vec![
        sep(None),
        predefined(PredefinedEdit::Copy),
        submenu(None, "Deep", Vec::new()),
    ];
    let input = vec![submenu(Some("tools"), "Tools", inner), action_item("plain", &log)];
    let out = normalize_native_menu_items(input);
    assert_eq!(out.len(), 2);
    let children = out[0].submenu_items().expect("the submenu survives");
    assert_eq!(children.len(), 1, "its leading separator and empty submenu are gone");
    assert_eq!(children[0].item_label().as_deref(), Some("Copy"));

    // All-separator input normalizes to nothing.
    assert!(normalize_native_menu_items(vec![sep(None), sep(None)]).is_empty());
    // And the predefined Separator is a separator for this purpose.
    assert!(is_separator(&predefined(PredefinedEdit::Separator)));
}

#[test]
fn predefined_commands_are_reacts_execcommand_ids() {
    assert_eq!(PredefinedEdit::Cut.command(), Some("cut"));
    assert_eq!(PredefinedEdit::Copy.command(), Some("copy"));
    assert_eq!(PredefinedEdit::Paste.command(), Some("paste"));
    assert_eq!(PredefinedEdit::SelectAll.command(), Some("selectAll"));
    assert_eq!(PredefinedEdit::Undo.command(), Some("undo"));
    assert_eq!(PredefinedEdit::Redo.command(), Some("redo"));
    assert_eq!(PredefinedEdit::Separator.command(), None);
    // The predefined label falls back to the item name (`text ?? item`).
    assert_eq!(
        predefined(PredefinedEdit::SelectAll).item_label().as_deref(),
        Some("SelectAll")
    );
    assert_eq!(
        NativeMenuItemDef::Predefined {
            item: PredefinedEdit::Copy,
            text: Some("Copy table".to_string()),
        }
        .item_label()
        .as_deref(),
        Some("Copy table")
    );
    // Enabled: an explicit false on a plain item, true otherwise.
    assert!(NativeMenuItemDef::Item {
        id: "a".to_string(),
        label: "a".to_string(),
        shortcut: None,
        enabled: Some(true),
        action: Rc::new(|| {}),
    }
    .enabled());
    assert!(!NativeMenuItemDef::Item {
        id: "a".to_string(),
        label: "a".to_string(),
        shortcut: None,
        enabled: Some(false),
        action: Rc::new(|| {}),
    }
    .enabled());
    assert!(predefined(PredefinedEdit::Cut).enabled());
    assert!(sep(None).enabled());
    // Submenu ids: the given id, or `sub-{index}` fallback.
    assert_eq!(
        submenu(Some("given"), "g", Vec::new()).submenu_id(3),
        Some("given".to_string())
    );
    assert_eq!(
        submenu(None, "f", Vec::new()).submenu_id(4),
        Some("sub-4".to_string())
    );
    assert!(action_item("x", &Rc::new(RefCell::new(Vec::new()))).submenu_id(0).is_none());
}

#[test]
fn the_root_menu_flips_left_up_and_clamps_to_the_pad() {
    // Away from every edge: the cursor is the panel origin.
    assert_eq!(
        position_root_menu((100.0, 100.0), (180.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (100.0, 100.0)
    );
    // Near the bottom-right corner: flip left and up, then clamp.
    assert_eq!(
        position_root_menu((1430.0, 800.0), (180.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (1250.0, 760.0)
    );
    // At the top-left corner: clamped onto the pad.
    assert_eq!(
        position_root_menu((0.0, 0.0), (180.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (CONTEXT_MENU_PAD, CONTEXT_MENU_PAD)
    );
    // A viewport smaller than the panel: the horizontal clamp's max is
    // below its min, so the pad wins on x; vertically the menu still fits
    // (50 + 40 < 92), so no flip and no clamp — y stays at the cursor.
    assert_eq!(
        position_root_menu((50.0, 50.0), (180.0, 40.0), (100.0, 100.0), CONTEXT_MENU_PAD),
        (CONTEXT_MENU_PAD, 50.0)
    );
}

#[test]
fn the_submenu_prefers_the_right_side_and_flips_when_it_clips() {
    // Plenty of room right: beside the item, aligned to its top.
    assert_eq!(
        position_submenu((100.0, 100.0, 180.0, 30.0), (160.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (280.0, 100.0, SubmenuSide::Right)
    );
    // No room right (the item touches the right edge), room left: flips.
    assert_eq!(
        position_submenu((1400.0, 100.0, 180.0, 30.0), (160.0, 40.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (1240.0, 100.0, SubmenuSide::Left)
    );
    // The right side clips the submenu but still has more room than the
    // left: stays right and the clamp pulls it inside.
    assert_eq!(
        position_submenu((100.0, 100.0, 40.0, 30.0), (160.0, 40.0), (300.0, 816.0), CONTEXT_MENU_PAD),
        (132.0, 100.0, SubmenuSide::Right)
    );
    // A tall submenu near the bottom: lifted so it fits.
    assert_eq!(
        position_submenu((100.0, 780.0, 180.0, 30.0), (160.0, 100.0), (1440.0, 816.0), CONTEXT_MENU_PAD),
        (280.0, 708.0, SubmenuSide::Right)
    );
}

// ---- the store + host state (no Runtime) -------------------------------------

#[test]
fn show_normalizes_and_an_empty_menu_stays_closed() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut state = WebContextMenuState::default();
    assert!(!state.open);

    // All separators: React's `show` sets open false and items [].
    state.show(vec![sep(None), sep(None)], (12.0, 34.0));
    assert!(!state.open);
    assert!(state.items.is_empty());
    assert_eq!((state.x, state.y), (0.0, 0.0), "a rejected menu keeps the old cursor");

    // A real menu: normalized and open at the cursor.
    state.show(
        vec![sep(None), action_item("open", &log), sep(None)],
        (12.0, 34.0),
    );
    assert!(state.open);
    assert_eq!((state.x, state.y), (12.0, 34.0));
    assert_eq!(state.items.len(), 1, "the separators went");

    // A submenu open from a previous show is cleared on re-show.
    state.show(
        vec![action_item("open", &log), submenu(None, "Tools", vec![action_item("inner", &log)])],
        (56.0, 78.0),
    );
    state.on_item_enter("sub-1", (0.0, 0.0, 10.0, 10.0));
    assert!(state.submenu.is_some());
    state.show(vec![action_item("again", &log)], (56.0, 78.0));
    assert!(state.submenu.is_none(), "re-show closes the submenu");
    assert_eq!((state.x, state.y), (56.0, 78.0));

    state.hide();
    assert!(!state.open);
    assert!(state.items.is_empty());
    assert!(state.submenu.is_none(), "hide closes the submenu with the host");
}

#[test]
fn on_item_enter_opens_the_submenu_beside_the_rect_and_refines_it() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut state = WebContextMenuState {
        viewport: (1440.0, 816.0),
        menu_size: (180.0, 40.0),
        submenu_size: (160.0, 40.0),
        ..Default::default()
    };
    state.show(
        vec![
            action_item("plain", &log),
            submenu(None, "Tools", vec![action_item("inner", &log)]),
        ],
        (100.0, 100.0),
    );

    // An unknown id and the empty id both close.
    state.on_item_enter("no-such", (0.0, 0.0, 0.0, 0.0));
    assert!(state.submenu.is_none());
    state.on_item_enter("", (0.0, 0.0, 0.0, 0.0));
    assert!(state.submenu.is_none());

    // The submenu def is index 1: the fallback id is `sub-1`.
    state.on_item_enter("sub-1", (300.0, 200.0, 180.0, 30.0));
    let sub = state.submenu.clone().expect("the submenu opened");
    assert_eq!(sub.id, "sub-1");
    assert_eq!(sub.items.len(), 1);
    // Before refinement: (rect.right, rect.top).
    assert_eq!(sub.pos, (480.0, 200.0));
    assert_eq!(sub.item_rect, (300.0, 200.0, 180.0, 30.0));

    // Refinement: positionSubmenu with the measured size — here it agrees
    // with the initial guess, so the guarded update leaves it alone.
    state.refine_submenu();
    assert_eq!(state.submenu.as_ref().expect("still open").pos, (480.0, 200.0));

    // A rect near the right edge refines onto the left side.
    state.on_item_enter("sub-1", (1300.0, 200.0, 180.0, 30.0));
    assert_eq!(state.submenu.as_ref().expect("still open").pos, (1480.0, 200.0));
    state.refine_submenu();
    assert_eq!(
        state.submenu.as_ref().expect("still open").pos,
        (1140.0, 200.0),
        "flipped to item.left - submenu.width = 1300 - 160"
    );

    // The root position: the cursor, flipped and clamped.
    assert_eq!(state.root_position(), (100.0, 100.0));
    state.x = 1430.0;
    state.y = 800.0;
    assert_eq!(state.root_position(), (1250.0, 760.0));
}

// ---- the mounted panels + presses --------------------------------------------

#[derive(Default)]
struct Probe {
    menu: WebContextMenuState,
    commands: Vec<&'static str>,
    /// The trigger rect the wiring resolved (the test sets it after a
    /// frame, the way React reads it off the element at event time).
    trigger_rect: (f32, f32, f32, f32),
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    WebContextMenuHost::<Probe>::new(&state.menu)
        .host_mut(|s: &mut Probe| &mut s.menu)
        .on_predefined(|s: &mut Probe, item| {
            s.commands.push(item.command().expect("an edit command"));
        })
        .on_open_submenu(|s: &mut Probe, id| {
            let rect = s.trigger_rect;
            s.menu.on_item_enter(id, rect);
        })
        .mount(dark, &mut layer);
    root(
        col().key("page").w_full().h_full().child(col().key("content").w_full().h_full()),
        layer,
    )
}

const DESIGN: (f32, f32) = (1440.0, 816.0);
const MIN: (f32, f32) = (960.0, 640.0);

fn runtime(log: &Rc<RefCell<Vec<String>>>, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut menu = WebContextMenuState {
        viewport: size,
        menu_size: (180.0, 88.0),
        submenu_size: (160.0, 40.0),
        ..Default::default()
    };
    // Cut, separator, open + disabled items, a submenu with one inner item.
    let items = vec![
        predefined(PredefinedEdit::Cut),
        sep(None),
        action_item("open", log),
        NativeMenuItemDef::Item {
            id: "disabled".to_string(),
            label: "disabled".to_string(),
            shortcut: None,
            enabled: Some(false),
            action: {
                let log = Rc::clone(log);
                Rc::new(move || log.borrow_mut().push("disabled".to_string()))
            },
        },
        submenu(None, "Tools", vec![action_item("inner", log)]),
    ];
    menu.show(items, (100.0, 100.0));
    let mut rt = Runtime::new(Probe { menu, ..Default::default() }, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
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

fn escape(rt: &mut Runtime<Probe>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn a_closed_menu_mounts_nothing_and_an_open_one_mounts_the_react_tree() {
    // Closed: nothing mounts (React's null).
    let closed = Probe {
        menu: WebContextMenuState::default(),
        ..Default::default()
    };
    let mut rt = Runtime::new(closed, view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    for key in ["web-context-menu", "web-context-scrim", "web-context-submenu"] {
        assert!(!has(&rt, key), "a closed menu mounts no {key}");
    }

    // Open: the scrim, the root panel at the root position, and every row
    // under its React test id.
    let log = Rc::new(RefCell::new(Vec::new()));
    let rt = runtime(&log, DESIGN, true);
    let panel = element(&rt, "web-context-menu");
    assert_eq!(panel.role, "menu");
    assert_eq!(panel.rect[0], 100.0, "the panel sits at the clamped cursor x");
    assert_eq!(panel.rect[1], 100.0, "and the cursor y");
    assert!(panel.rect[2] >= 180.0, "min-w-[180px]");
    assert!(has(&rt, "web-context-scrim"));
    for key in [
        "web-context-pre-Cut",
        "web-context-item-open",
        "web-context-item-disabled",
        "web-context-submenu-trigger-sub-4",
    ] {
        assert!(has(&rt, key), "the row mounts under its React key: {key}");
        let row = element(&rt, key);
        assert_eq!(row.role, "menuitem", "{key} is a menuitem");
        assert_eq!(row.rect[0], panel.rect[0], "{key} starts at the panel's padding edge (Rune borders draw over the edge, they do not inset children)");
    }
    // The separator row is a separator, not a menuitem.
    let separator = element(&rt, "sep-1");
    assert_eq!(separator.role, "separator");
    assert_eq!(separator.rect[3], 1.0, "h-px");
    assert!(separator.rect[1] > element(&rt, "web-context-pre-Cut").rect[1], "it sits between the rows");
    // The disabled row is muted at half layer opacity; the disabled row is
    // pressless (below); the muted ink is the observable half of React's
    // `text-fg-muted opacity-50`. The layer dim itself has no public probe
    // channel (`ElementInfo.opacity` excludes layer opacity — the filter
    // bar's loading rows hit the same wall and document it).
    let disabled = element(&rt, "web-context-item-disabled");
    assert!(!disabled.focusable, "a disabled row leaves the tab order");
    let label_of = |rt: &Runtime<Probe>, row: &str| {
        rt.elements()
            .into_iter()
            .find(|e| e.path.ends_with(&format!("{row}/label")) && !e.exiting)
            .unwrap_or_else(|| panic!("{row}/label is laid out"))
    };
    let k = kit(true);
    assert_eq!(
        label_of(&rt, "web-context-item-disabled").text_color,
        Some(k.c.text_muted),
        "React text-fg-muted"
    );
    assert_eq!(
        label_of(&rt, "web-context-item-open").text_color,
        Some(k.c.text_base),
        "React text-fg"
    );
    // The trigger carries its chevron and no submenu panel yet.
    assert!(!has(&rt, "web-context-submenu"));
    let trigger_row = element(&rt, "web-context-submenu-trigger-sub-4");
    let chevron_path = format!("{}/chevron", trigger_row.path);
    let chevron = rt
        .elements()
        .into_iter()
        .find(|e| e.path == chevron_path && !e.exiting)
        .unwrap_or_else(|| panic!("{chevron_path}: the chevron is laid out"));
    assert_eq!(chevron.rect[2], 14.0, "h-3.5 w-3.5 chevron");
    assert_eq!(chevron.text_color, Some(k.c.text_muted), "React text-fg-muted");
}

#[test]
fn a_submenu_mounts_beside_its_trigger_and_highlight_him() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut rt = runtime(&log, DESIGN, false);
    // The wiring resolves the trigger rect (here measured from the frame):
    // the row's own rect.
    let trigger = element(&rt, "web-context-submenu-trigger-sub-4");
    rt.state.trigger_rect = (trigger.rect[0], trigger.rect[1], trigger.rect[2], trigger.rect[3]);
    rt.frame(0.0);
    click(&mut rt, "web-context-submenu-trigger-sub-4");

    // The submenu panel is open beside the trigger, at its top.
    let sub = element(&rt, "web-context-submenu");
    assert_eq!(sub.role, "menu");
    assert!(
        (sub.rect[0] - (trigger.rect[0] + trigger.rect[2])).abs() <= 0.5,
        "the submenu starts at the trigger's right edge: {:?} vs trigger right {}",
        sub.rect,
        trigger.rect[0] + trigger.rect[2]
    );
    assert!(
        (sub.rect[1] - trigger.rect[1]).abs() <= 0.5,
        "aligned to the trigger's top: {:?}",
        sub.rect
    );
    assert!(has(&rt, "web-context-item-inner"), "the submenu rows mount");

    // The open trigger keeps the raised fill React gives it.
    let k = kit(false);
    let mut trigger_now = element(&rt, "web-context-submenu-trigger-sub-4");
    if trigger_now.fill != k.c.surface_raised {
        // A fill change between frames rides an animation track; let it
        // settle (the press already happened, this only advances paint).
        for _ in 0..12 {
            rt.frame(1.0 / 12.0);
        }
        trigger_now = element(&rt, "web-context-submenu-trigger-sub-4");
    }
    assert_eq!(trigger_now.fill, k.c.surface_raised, "React bg-surface-raised");

    // Choosing an inner item hides the menu and runs the action.
    click(&mut rt, "web-context-item-inner");
    assert!(!has(&rt, "web-context-menu"), "the menu unmounted");
    assert_eq!(*log.borrow(), vec!["inner".to_string()]);
}

#[test]
fn choosing_an_item_hides_first_then_runs_the_action() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut rt = runtime(&log, MIN, true);
    click(&mut rt, "web-context-item-open");
    assert!(!has(&rt, "web-context-menu"), "hide() ran before the action");
    assert_eq!(*log.borrow(), vec!["open".to_string()], "the item's action ran");
    assert!(rt.state.commands.is_empty(), "no predefined command ran");

    // The disabled row neither hides nor acts.
    let mut rt = runtime(&log, MIN, true);
    click(&mut rt, "web-context-item-disabled");
    assert!(has(&rt, "web-context-menu"), "a disabled row cannot close the menu");
    assert_eq!(*log.borrow(), vec!["open".to_string()], "nor run its action");
}

#[test]
fn predefined_rows_deliver_reacts_execcommand_ids() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut rt = runtime(&log, DESIGN, true);
    click(&mut rt, "web-context-pre-Cut");
    assert_eq!(rt.state.commands, vec!["cut"], "the command id React maps Cut onto");
    assert!(!has(&rt, "web-context-menu"), "the menu closed with the command");

    // An unwired host (no on_predefined): the menu still closes, matching
    // React's hide() running before runPredefinedEdit.
    let mut menu = WebContextMenuState {
        viewport: DESIGN,
        menu_size: (180.0, 88.0),
        ..Default::default()
    };
    menu.show(vec![predefined(PredefinedEdit::Copy)], (100.0, 100.0));
    let mut rt = Runtime::new(
        Probe { menu, ..Default::default() },
        |state: &Probe, cx: &mut Cx<Probe>| {
            let dark = cx.system_dark();
            let mut layer = Vec::new();
            WebContextMenuHost::<Probe>::new(&state.menu)
                .host_mut(|s: &mut Probe| &mut s.menu)
                .mount(dark, &mut layer);
            root(
                col().key("page").w_full().h_full().child(col().key("content").w_full().h_full()),
                layer,
            )
        },
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    click(&mut rt, "web-context-pre-Copy");
    assert!(!has(&rt, "web-context-menu"), "the unwired host still hides");
    assert!(rt.state.commands.is_empty());
}

#[test]
fn a_preopened_submenu_mounts_with_the_trigger_highlighted() {
    // The store can open the submenu without an interaction (the layout
    // effect path): the very first frame already carries the highlight.
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut menu = WebContextMenuState {
        viewport: DESIGN,
        menu_size: (180.0, 88.0),
        submenu_size: (160.0, 40.0),
        ..Default::default()
    };
    menu.show(
        vec![submenu(None, "Tools", vec![action_item("inner", &log)])],
        (100.0, 100.0),
    );
    menu.on_item_enter("sub-0", (280.0, 130.0, 180.0, 30.0));
    let mut rt = Runtime::new(
        Probe { menu, ..Default::default() },
        view,
        DESIGN,
        Env { system_dark: false, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has(&rt, "web-context-submenu"), "the submenu panel mounts");
    let trigger = element(&rt, "web-context-submenu-trigger-sub-0");
    let k = kit(false);
    assert_eq!(trigger.fill, k.c.surface_raised, "the trigger carries the raised fill from the first frame");
}

#[test]
fn the_scrim_and_escape_close_the_menu() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut rt = runtime(&log, DESIGN, true);
    assert!(has(&rt, "web-context-menu"));
    // The scrim covers the window under the panels: pressing it (away from
    // the panel at 100,100) is React's outside mousedown.
    let scrim = element(&rt, "web-context-scrim");
    assert_eq!(scrim.rect[2], DESIGN.0, "the scrim spans the window width");
    assert_eq!(scrim.rect[3], DESIGN.1, "and its height");
    click(&mut rt, "web-context-scrim");
    assert!(!has(&rt, "web-context-menu"), "an outside press closed the menu");
    assert!(log.borrow().is_empty());

    // Escape: the modal stack's dismissal, React's window keydown listener.
    let mut rt = runtime(&log, DESIGN, true);
    escape(&mut rt);
    assert!(!has(&rt, "web-context-menu"), "Escape closed the menu");
    assert!(log.borrow().is_empty());
    assert!(rt.state.commands.is_empty());
}

#[test]
fn the_menu_stays_inside_the_minimum_window() {
    // A cursor at the bottom-right corner of the minimum window flips and
    // clamps the panel fully inside, in both themes.
    for dark in [true, false] {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut menu = WebContextMenuState {
            viewport: MIN,
            menu_size: (180.0, 88.0),
            submenu_size: (160.0, 40.0),
            ..Default::default()
        };
        menu.show(
            vec![action_item("open", &log), submenu(None, "Tools", vec![action_item("inner", &log)])],
            (MIN.0 - 4.0, MIN.1 - 4.0),
        );
        let mut rt = Runtime::new(
            Probe { menu, ..Default::default() },
            view,
            MIN,
            Env { system_dark: dark, scale: 1.0 },
        );
        rt.frame(0.0);
        let panel = element(&rt, "web-context-menu");
        assert!(panel.rect[0] >= CONTEXT_MENU_PAD - 0.5, "left stays on the pad: {:?}", panel.rect);
        assert!(
            panel.rect[0] + panel.rect[2] <= MIN.0 - CONTEXT_MENU_PAD + 0.5,
            "right stays inside: {:?}",
            panel.rect
        );
        assert!(panel.rect[1] >= CONTEXT_MENU_PAD - 0.5, "top stays on the pad: {:?}", panel.rect);
        assert!(
            panel.rect[1] + panel.rect[3] <= MIN.1 - CONTEXT_MENU_PAD + 0.5,
            "bottom stays inside: {:?}",
            panel.rect
        );
    }
}
