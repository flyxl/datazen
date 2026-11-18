//! Independent audit of the two `DataTable` ports.
//!
//! These probes are deliberately written against the public component tree and
//! the `ElementInfo`/accessibility surface, not by copying the port's own tests.
//! Two probes are ignored because they encode unfixed React-parity defects:
//! the `sr-only` hint block is laid out as a visible 110pt strip, and the
//! composite `primaryKeyDesc` row is exposed as an enabled option.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

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
#[ignore = "parity defect: React sr-only is a 1px clipped box; this port lays out a 110pt visible strip"]
fn the_hint_strip_is_not_visually_hidden_like_sr_only() {
    // React: `div.sr-only` is a 1px clipped box (the text is for AT only).
    // Rune: the same content is a normal layout block.
    let mut rt = Runtime::new(Hints, hints, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let root = element(&rt, "audit-hints");
    assert!(
        root.rect[3] <= 2.0,
        "React sr-only occupies ~1pt; the port lays the hint strip out at {:?}",
        root.rect
    );
}

#[test]
#[ignore = "parity defect: React ul/li exposes list/listitem; this port uses group/text"]
fn the_hint_strip_keeps_the_react_list_semantics() {
    // React: <p> + <ul> + seven <li>, referenced by aria-describedby.
    // Rune: group/text/group/text, with no list or listitem role.
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
}

fn field(s: &Field, _: &mut Cx<Field>) -> El {
    let mut layer = Vec::new();
    let control = DefaultOrderSelect::new("audit-order", DefaultOrderOverride::Inherit)
        .primary_key_count(2)
        .open(s.open)
        .on_toggle(|s: &mut Field| s.open = !s.open)
        .on_close(|s: &mut Field| s.open = false)
        .on_change(|s: &mut Field, _| s.open = false)
        .build(crate::ui::kit::kit(true), &mut layer);
    rune_kit::root(col().w_full().h_full().padding(16.0).child(control), layer)
}

#[test]
fn the_documented_field_key_is_actually_the_derived_trigger_key() {
    // The module docs table says `field | <key>-field`, but ui/select passes
    // the wrapper key to kit Select, which derives `<key>-trigger`.
    let mut rt = Runtime::new(Field { open: false }, field, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(rt.find("audit-order-field-trigger").is_some());
    assert!(rt.find("audit-order-field").is_none());
}

#[test]
#[ignore = "parity defect: React aria-disabled + opacity-40; this port leaves the row fully opaque and enabled"]
fn the_composite_row_looks_like_a_disabled_option() {
    // React gives the row `aria-disabled` and `opacity-40`; the kit has no
    // per-option disabled state, so the port leaves it looking enabled.
    let mut rt = Runtime::new(Field { open: true }, field, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let row = element(&rt, "option2");
    assert!(
        row.opacity < 0.9,
        "the composite primaryKeyDesc row is still fully opaque (opacity {}) and focusable={}",
        row.opacity,
        row.focusable
    );
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
