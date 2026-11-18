//! React parity for the two `DataTable` ports:
//! `DefaultOrderSelect.tsx` and `GridKeyboardHints.tsx`.
//!
//! The probes below build each port alone in a padded page, the way a view
//! mounts it. Assertions cover the ported tree (label + field; hint + list +
//! seven items), the stored values (never labels), keyboard/pointer agreement,
//! the composite-primary-key refusal, the disabled field, the hint text, and
//! the two-window geometry contract.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::data_table::default_order_select::{
    DefaultOrderOverride, DefaultOrderSelect,
};
use crate::components::data_table::grid_keyboard_hints::GridKeyboardHints;
use crate::i18n::{has, t};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

// ---- DefaultOrderSelect -------------------------------------------------------------------

#[derive(Default)]
struct Probe {
    open: bool,
    value: DefaultOrderOverride,
    changes: Vec<DefaultOrderOverride>,
    primary_key_count: u32,
    disabled: bool,
}

fn probe(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let field = DefaultOrderSelect::new("probe-order", state.value)
        .primary_key_count(state.primary_key_count)
        .disabled(state.disabled)
        .open(state.open)
        .on_toggle(|s: &mut Probe| s.open = !s.open)
        .on_close(|s: &mut Probe| s.open = false)
        .on_change(|s: &mut Probe, value| {
            s.value = value;
            s.changes.push(value);
            s.open = false;
        })
        .build(crate::ui::kit::kit(true), &mut layer);
    rune_kit::root(
        rune::col().w_full().h_full().padding(16.0).child(field),
        layer,
    )
}

/// The root is React's `div > span + Select`, and every choice is a real i18n
/// label; the values behind them stay the React value strings.
#[test]
fn default_order_select_keeps_the_react_tree_and_labels() {
    let mut rt = Runtime::new(Probe::default(), probe, (960.0, 640.0), env(true));
    rt.frame(0.0);

    assert_eq!(
        element_text(&rt, "probe-order-label"),
        t("defaultSort.panelLabel"),
        "the visible label is React's defaultSort.panelLabel"
    );

    let all = rt.elements();
    let root = all
        .iter()
        .position(|e| e.key.as_deref() == Some("probe-order"))
        .expect("the root row is laid out");
    let children: Vec<&str> = all
        .iter()
        .filter(|e| e.parent == Some(root))
        .filter_map(|e| e.key.as_deref())
        .collect();
    assert_eq!(
        children,
        vec!["probe-order-label", "probe-order-field-trigger"],
        "row = label + select, in that order"
    );

    click(&mut rt, "probe-order-field-trigger");
    assert!(rt.state.open, "a press on the select opens the listbox");
    assert_eq!(
        element(&rt, "option0").label.as_deref(),
        Some(t("defaultSort.inherit")),
        "the first row is the inherit choice"
    );
    assert_eq!(
        element(&rt, "option2").label.as_deref(),
        Some(t("defaultSort.primaryKeyDesc")),
        "the third row is the composite-sensitive choice"
    );
}

/// The keyboard (`Down`, `Down`, `Enter`) and the pointer (press the third row)
/// store the same enum value, and it is the React value behind the label.
#[test]
fn default_order_select_reports_values_identically_from_keys_and_pointer() {
    let mut keys = Runtime::new(Probe::default(), probe, (960.0, 640.0), env(true));
    keys.frame(0.0);
    press(&mut keys, Key::Tab);
    assert_eq!(
        focused(&keys).as_deref(),
        Some("probe-order-field-trigger"),
        "Tab reaches the select"
    );
    press(&mut keys, Key::Text(" ".into()));
    assert!(keys.state.open, "Space opens the listbox");
    press(&mut keys, Key::Down);
    press(&mut keys, Key::Down);
    assert_eq!(
        focused(&keys).as_deref(),
        Some("option2"),
        "two Downs land on the third row"
    );
    press(&mut keys, Key::Enter);
    assert_eq!(keys.state.changes, vec![DefaultOrderOverride::PrimaryKeyDesc]);
    assert!(!keys.state.open, "the app's handler closes the listbox");
    assert!(
        keys.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some(t("defaultSort.primaryKeyDesc"))),
        "the trigger now shows the chosen label"
    );

    let mut pointer = Runtime::new(Probe::default(), probe, (960.0, 640.0), env(true));
    pointer.frame(0.0);
    click(&mut pointer, "probe-order-field-trigger");
    click(&mut pointer, "option2");
    assert_eq!(pointer.state.changes, keys.state.changes);
    assert_eq!(pointer.state.value, keys.state.value);
}

/// A composite primary key refuses `primaryKeyDesc` from both input paths, and
/// the listbox stays where it was (React's disabled row does not choose).
#[test]
fn default_order_select_refuses_descending_for_a_composite_key() {
    let mut rt = Runtime::new(
        Probe {
            primary_key_count: 2,
            ..Probe::default()
        },
        probe,
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    click(&mut rt, "probe-order-field-trigger");
    click(&mut rt, "option2");
    assert!(
        rt.state.changes.is_empty(),
        "pressing the unavailable row reports nothing"
    );
    assert_eq!(rt.state.value, DefaultOrderOverride::Inherit);
    assert!(rt.state.open, "the listbox stays open after the refusal");

    click(&mut rt, "option1");
    assert_eq!(rt.state.changes, vec![DefaultOrderOverride::PrimaryKeyAsc]);

    // The keyboard path starts from a closed field: Space opens on the current
    // choice, two Downs land on the unavailable row, Enter refuses it.
    let mut keys = Runtime::new(
        Probe {
            primary_key_count: 2,
            ..Probe::default()
        },
        probe,
        (960.0, 640.0),
        env(true),
    );
    keys.frame(0.0);
    press(&mut keys, Key::Tab);
    press(&mut keys, Key::Text(" ".into()));
    press(&mut keys, Key::Down);
    press(&mut keys, Key::Down);
    press(&mut keys, Key::Enter);
    assert!(
        keys.state.changes.is_empty(),
        "Enter on the unavailable row reports nothing"
    );
    assert_eq!(focused(&keys).as_deref(), Some("option2"));
    assert!(keys.state.open, "the refusal leaves the listbox open");
}

/// A disabled field never opens and never reports a choice.
#[test]
fn default_order_select_disabled_produces_no_choice() {
    let mut rt = Runtime::new(
        Probe {
            disabled: true,
            ..Probe::default()
        },
        probe,
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt), None, "a disabled select takes no focus");
    click(&mut rt, "probe-order-field-trigger");
    assert!(!rt.state.open, "a disabled select does not open");
    assert!(rt.state.changes.is_empty());
}

/// Every string the two ports build is a real zh-CN key, not an invented one.
#[test]
fn ported_strings_are_real_locale_keys() {
    let keys = [
        "defaultSort.panelLabel",
        "defaultSort.inherit",
        "defaultSort.primaryKeyAsc",
        "defaultSort.primaryKeyDesc",
        "defaultSort.primaryKeyDescUnavailable",
        "defaultSort.none",
        "tableView.shortcut.hint",
        "tableView.shortcut.enterEdit",
        "tableView.shortcut.extendSelection",
        "tableView.shortcut.moveByPage",
        "tableView.shortcut.moveToEdge",
        "tableView.shortcut.selectAllRows",
        "tableView.shortcut.quickLook",
        "tableView.shortcut.tabLeavesGrid",
    ];
    for key in keys {
        assert!(has(key), "{key} is missing from the zh-CN locale");
    }
}

// ---- GridKeyboardHints --------------------------------------------------------------------

struct HintsProbe;

fn hints_view(state: &HintsProbe, _cx: &mut Cx<HintsProbe>) -> El {
    let _ = state;
    rune_kit::root(
        rune::col()
            .w_full()
            .h_full()
            .padding(16.0)
            .child(GridKeyboardHints::new("probe-hints").build(true)),
        Vec::new(),
    )
}

/// The strip is React's `p + ul + 7 li` with the same strings, and it adds no
/// Tab stop.
#[test]
fn grid_keyboard_hints_keep_the_react_text_and_add_no_tab_stop() {
    let mut rt = Runtime::new(HintsProbe, hints_view, (960.0, 640.0), env(true));
    rt.frame(0.0);

    assert_eq!(
        element_text(&rt, "probe-hints-hint"),
        t("tableView.shortcut.hint")
    );
    let hint_keys = [
        "tableView.shortcut.enterEdit",
        "tableView.shortcut.extendSelection",
        "tableView.shortcut.moveByPage",
        "tableView.shortcut.moveToEdge",
        "tableView.shortcut.selectAllRows",
        "tableView.shortcut.quickLook",
        "tableView.shortcut.tabLeavesGrid",
    ];
    for (index, key) in hint_keys.iter().enumerate() {
        assert_eq!(
            element_text(&rt, &format!("probe-hints-item-{index}")),
            t(key)
        );
    }

    let all = rt.elements();
    let root = all
        .iter()
        .position(|e| e.key.as_deref() == Some("probe-hints"))
        .expect("the hint strip is laid out");
    assert_eq!(
        all.iter().filter(|e| e.parent == Some(root)).count(),
        2,
        "root = hint paragraph + one list"
    );
    let list = all
        .iter()
        .position(|e| e.key.as_deref() == Some("probe-hints-list"))
        .expect("the list is laid out");
    assert_eq!(
        all.iter().filter(|e| e.parent == Some(list)).count(),
        7,
        "the list holds React's seven items"
    );
    assert!(
        all.iter().all(|e| !e.focusable),
        "no hint element is focusable"
    );
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt), None, "Tab does not land on the hints");
}

/// At 240 points the long rows wrap instead of leaving their column, while the
/// short rows stay one line.
#[test]
fn grid_keyboard_hints_wrap_in_a_narrow_column() {
    struct Narrow;
    fn view(_s: &Narrow, _cx: &mut Cx<Narrow>) -> El {
        rune_kit::root(
            rune::col().w_full().h_full().padding(12.0).child(
                rune::col()
                    .w(240.0)
                    .child(GridKeyboardHints::new("narrow-hints").build(false)),
            ),
            Vec::new(),
        )
    }
    let mut rt = Runtime::new(Narrow, view, (960.0, 640.0), env(false));
    rt.frame(0.0);

    let long = element(&rt, "narrow-hints-item-3");
    let short = element(&rt, "narrow-hints-item-6");
    assert!(
        long.rect[3] > short.rect[3] + 0.5,
        "the long Home/End hint wraps: long {:?} vs short {:?}",
        long.rect,
        short.rect
    );
    for element in rt.elements() {
        let Some(key) = element.key.as_deref() else {
            continue;
        };
        if !key.starts_with("narrow-hints") {
            continue;
        }
        let [x, _, w, _] = element.rect;
        assert!(
            x >= -0.5 && x + w <= 240.5 + 12.0,
            "{key} crosses the narrow column: {:?}",
            element.rect
        );
    }
}

// ---- Geometry -----------------------------------------------------------------------------

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

struct Geo {
    dark: bool,
    open: bool,
}

fn geo_view(state: &Geo, _cx: &mut Cx<Geo>) -> El {
    let k = crate::ui::kit::kit(state.dark);
    let mut layer = Vec::new();
    let field = DefaultOrderSelect::new("geo-order", DefaultOrderOverride::Inherit)
        .primary_key_count(2)
        .open(state.open)
        .on_toggle(|s: &mut Geo| s.open = !s.open)
        .on_close(|s: &mut Geo| s.open = false)
        .on_change(|s: &mut Geo, _| s.open = false)
        .build(k, &mut layer);
    let hints = GridKeyboardHints::new("geo-hints").build(state.dark);
    rune_kit::root(
        rune::col()
            .w_full()
            .h_full()
            .padding(24.0)
            .gap(16.0)
            .child(field)
            .child(hints),
        layer,
    )
}

fn assert_inside(rt: &Runtime<Geo>, context: &str, width: f32, height: f32) {
    let mut outside = Vec::new();
    for element in rt.elements() {
        let key = element.key.as_deref().unwrap_or(element.path.as_str());
        let [x, y, w, h] = element.rect;
        if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
            outside.push(format!(
                "key={key} rect={:?} role={}",
                element.rect, element.role
            ));
        }
    }
    assert!(
        outside.is_empty(),
        "{context}: {} node(s) cross the {width}x{height} window: {outside:#?}",
        outside.len()
    );
}

/// Both ports stay inside 960×640 and 1440×816 in both themes, closed and open.
#[test]
fn data_table_ports_fit_both_windows() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            for open in [false, true] {
                let context = format!("{width}x{height} dark={dark} open={open}");
                let mut rt = Runtime::new(
                    Geo { dark, open },
                    geo_view,
                    (width, height),
                    env(dark),
                );
                rt.frame(0.0);
                assert_inside(&rt, &context, width, height);
            }
        }
    }
}
