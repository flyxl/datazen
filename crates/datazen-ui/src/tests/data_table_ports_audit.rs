//! Independent audit of the two `DataTable` ports.
//!
//! These probes are deliberately written against the public component tree and
//! the `ElementInfo`/accessibility surface, not by copying the port's own tests.
//! They pin the fixed contracts: the one-point `sr-only` box with list/listitem
//! roles, the derived select keys, and the composite field's three-choice list.
//!
//! Known defect discovered here: `opacity(0.0)` makes rune drop the whole
//! subtree from the accessibility tree (`vendor/rune/crates/rune/src/a11y/tree.rs:332`),
//! so the visually hidden hint text is currently invisible to assistive tech.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::Value;

use crate::components::data_table::default_order_select::{
    DefaultOrderOverride, DefaultOrderSelect,
};
use crate::components::data_table::grid_keyboard_hints::GridKeyboardHints;
use crate::i18n::t;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

struct Hints;
fn hints(_: &Hints, _: &mut Cx<Hints>) -> El {
    rune_kit::root(
        col()
            .w_full()
            .h_full()
            .padding(16.0)
            .child(GridKeyboardHints::new("audit-hints").build(true)),
        Vec::new(),
    )
}

#[test]
fn the_hint_strip_is_a_one_point_sr_only_box() {
    // React: `div.sr-only` is a 1px clipped box (the text is for AT only).
    let mut rt = Runtime::new(Hints, hints, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let root = element(&rt, "audit-hints");
    assert!(
        root.rect[3] <= 2.0,
        "sr-only is a one-point box; the port lays it out at {:?}",
        root.rect
    );
    assert!(
        root.opacity <= 0.01,
        "sr-only paints nothing; opacity is {}",
        root.opacity
    );
}

#[test]
fn the_hint_strip_keeps_the_react_list_semantics() {
    // React: <p> + <ul> + seven <li>, referenced by aria-describedby.
    let mut rt = Runtime::new(Hints, hints, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let list = element(&rt, "audit-hints-list");
    assert_eq!(
        list.role, "list",
        "the hint list should expose list semantics"
    );
    for i in 0..7 {
        let item = element(&rt, &format!("audit-hints-item-{i}"));
        assert_eq!(item.role, "listitem", "hint item {i} should be a listitem");
    }
}

fn find_json<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    if node.get("key").and_then(Value::as_str) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|child| find_json(child, key))
}

#[test]
#[ignore = "known defect: opacity(0.0) drops the sr-only subtree from the a11y tree"]
fn the_sr_only_text_stays_in_the_accessibility_tree() {
    let mut rt = Runtime::new(Hints, hints, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let json = rt.accessibility_json();
    let hint = find_json(&json, "audit-hints-hint").expect("hint node is in the a11y tree");
    assert_eq!(
        hint.get("name").and_then(Value::as_str),
        Some(t("tableView.shortcut.hint"))
    );
    for i in 0..7 {
        assert!(
            find_json(&json, &format!("audit-hints-item-{i}")).is_some(),
            "hint item {i} is still readable by AT"
        );
    }
}

#[test]
fn the_hint_strip_adds_no_tab_stop() {
    let mut rt = Runtime::new(Hints, hints, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(rt.elements().iter().all(|e| !e.focusable));
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt), None);
}

#[test]
fn the_hint_text_is_the_react_copy_in_order() {
    const KEYS: [&str; 7] = [
        "tableView.shortcut.enterEdit",
        "tableView.shortcut.extendSelection",
        "tableView.shortcut.moveByPage",
        "tableView.shortcut.moveToEdge",
        "tableView.shortcut.selectAllRows",
        "tableView.shortcut.quickLook",
        "tableView.shortcut.tabLeavesGrid",
    ];
    let mut rt = Runtime::new(Hints, hints, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "audit-hints-hint").text.as_deref(),
        Some(t("tableView.shortcut.hint"))
    );
    for (i, key) in KEYS.into_iter().enumerate() {
        assert_eq!(
            element(&rt, &format!("audit-hints-item-{i}"))
                .text
                .as_deref(),
            Some(t(key)),
            "hint {i}"
        );
    }
}

#[derive(Default)]
struct Field {
    open: bool,
    primary_key_count: u32,
}

fn field(s: &Field, _: &mut Cx<Field>) -> El {
    let mut layer = Vec::new();
    let control = DefaultOrderSelect::new("audit-order", DefaultOrderOverride::Inherit)
        .primary_key_count(s.primary_key_count)
        .open(s.open)
        .on_toggle(|s: &mut Field| s.open = !s.open)
        .on_close(|s: &mut Field| s.open = false)
        .on_change(|s: &mut Field, _| s.open = false)
        .build(crate::ui::kit::kit(true), &mut layer);
    rune_kit::root(col().w_full().h_full().padding(16.0).child(control), layer)
}

#[test]
fn the_derived_keys_are_the_ones_in_the_tree() {
    // Docs: `<key>-field` is a logical prefix, not a node. The kit derives
    // `<key>-field-trigger` always, and `-content`/`-scrim` while open.
    let mut closed = Runtime::new(
        Field {
            open: false,
            primary_key_count: 0,
        },
        field,
        (960.0, 640.0),
        env(true),
    );
    closed.frame(0.0);
    assert!(closed.find("audit-order-label").is_some());
    assert!(closed.find("audit-order-field-trigger").is_some());
    assert!(closed.find("audit-order-field").is_none());
    assert!(closed.find("audit-order-field-content").is_none());

    let mut open = Runtime::new(
        Field {
            open: true,
            primary_key_count: 0,
        },
        field,
        (960.0, 640.0),
        env(true),
    );
    open.frame(0.0);
    assert!(open.find("audit-order-field-content").is_some());
    assert!(open.find("audit-order-field-scrim").is_some());
    for index in 0..4 {
        assert!(
            open.find(&format!("option{index}")).is_some(),
            "non-composite row {index}"
        );
    }
}

#[test]
fn the_sr_only_text_is_currently_dropped_from_the_accessibility_tree() {
    // Defect pin: this is the opposite of React's `sr-only`. Keeping the check
    // explicit makes the regression visible even while the failing probe above
    // is ignored.
    let mut rt = Runtime::new(Hints, hints, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let json = rt.accessibility_json();
    assert!(
        find_json(&json, "audit-hints-hint").is_none(),
        "opacity(0.0) should have dropped the hint from the a11y tree"
    );
    assert!(
        !serde_json::to_string(&json)
            .unwrap()
            .contains(t("tableView.shortcut.hint")),
        "no hint text should reach the current a11y tree"
    );
}

#[test]
fn the_composite_field_omits_descending_and_explains_it() {
    // Approved acceptance: the kit has no per-option disabled state, so the
    // composite field does not offer `primaryKeyDesc`; the reason is the
    // trigger's accessible description.
    let mut closed = Runtime::new(
        Field {
            open: false,
            primary_key_count: 2,
        },
        field,
        (960.0, 640.0),
        env(true),
    );
    closed.frame(0.0);
    let json = closed.accessibility_json();
    let trigger =
        find_json(&json, "audit-order-field-trigger").expect("closed trigger in a11y tree");
    let description = trigger
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        description.contains(t("defaultSort.primaryKeyDescUnavailable")),
        "the closed trigger describes why descending is absent; got {description:?}"
    );

    let mut composite = Runtime::new(
        Field {
            open: true,
            primary_key_count: 2,
        },
        field,
        (960.0, 640.0),
        env(true),
    );
    composite.frame(0.0);
    assert!(composite.find("audit-order-field-content").is_some());
    assert!(
        composite.find("option3").is_none(),
        "there is no fourth row"
    );
    let labels: Vec<Option<String>> = (0..3)
        .map(|i| element(&composite, &format!("option{i}")).label)
        .collect();
    assert_eq!(
        labels,
        vec![
            Some(t("defaultSort.inherit").to_owned()),
            Some(t("defaultSort.primaryKeyAsc").to_owned()),
            Some(t("defaultSort.none").to_owned()),
        ]
    );
    assert!(
        !composite
            .elements()
            .iter()
            .any(|e| e.text.as_deref() == Some(t("defaultSort.primaryKeyDesc"))),
        "the descending label never appears while composite"
    );

    let mut full = Runtime::new(
        Field {
            open: true,
            primary_key_count: 1,
        },
        field,
        (960.0, 640.0),
        env(true),
    );
    full.frame(0.0);
    for index in 0..4 {
        assert!(
            full.find(&format!("option{index}")).is_some(),
            "single-column row {index}"
        );
    }
}

struct Ports {
    open: bool,
}

fn ports(s: &Ports, _: &mut Cx<Ports>) -> El {
    let mut layer = Vec::new();
    let control = DefaultOrderSelect::new("audit-ports-order", DefaultOrderOverride::PrimaryKeyAsc)
        .primary_key_count(2)
        .open(s.open)
        .on_toggle(|s: &mut Ports| s.open = !s.open)
        .on_close(|s: &mut Ports| s.open = false)
        .on_change(|s: &mut Ports, _| s.open = false)
        .build(crate::ui::kit::kit(true), &mut layer);
    rune_kit::root(
        col()
            .w_full()
            .h_full()
            .padding(16.0)
            .gap(12.0)
            .child(control)
            .child(GridKeyboardHints::new("audit-ports-hints").build(true)),
        layer,
    )
}

#[test]
fn both_ports_stay_inside_both_windows() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for open in [false, true] {
                let mut rt = Runtime::new(Ports { open }, ports, (w, h), env(dark));
                rt.frame(0.0);
                for e in rt.elements() {
                    let Some(key) = e.key.as_deref() else {
                        continue;
                    };
                    if !(key.starts_with("audit-ports") || key.starts_with("option")) {
                        continue;
                    }
                    let [x, y, ew, eh] = e.rect;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + ew <= w + 0.5 && y + eh <= h + 0.5,
                        "{key} leaves {w}x{h}: {:?} (dark={dark}, open={open})",
                        e.rect
                    );
                }
                let trigger = rt.find("audit-ports-order-field-trigger");
                assert!(trigger.is_some(), "trigger missing at {w}x{h}");
            }
        }
    }
}
