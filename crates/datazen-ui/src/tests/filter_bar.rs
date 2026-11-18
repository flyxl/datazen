//! T014: the filter bar renders React's label, one pill per condition and
//! the clear-all button; remove / clear dispatch with their React payloads
//! (index / nothing); loading disables both buttons and dispatches nothing.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::filter::{FilterCondition, FilterValue};
use crate::components::filter_bar::{filter_bar_label, FilterBar};
use crate::i18n::{has, t};
use crate::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

struct Probe {
    filters: Vec<FilterCondition>,
    loading: bool,
    callbacks: bool,
    removes: Vec<usize>,
    clears: u32,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            filters: vec![
                FilterCondition::new("status", "eq", FilterValue::Single("paid".to_string())),
                FilterCondition::new("age", "gte", FilterValue::Single("18".to_string())),
            ],
            loading: false,
            callbacks: true,
            removes: Vec::new(),
            clears: 0,
        }
    }
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut page = col().key("page").w_full().h_full().padding(PAD).gap(8.0);
    let mut bar = FilterBar::new("filter-bar", &state.filters).loading(state.loading);
    if state.callbacks {
        bar = bar
            .on_remove(|state: &mut Probe, index| state.removes.push(index))
            .on_clear(|state: &mut Probe| state.clears += 1);
    }
    if let Some(bar) = bar.build(dark) {
        page = page.child(bar);
    }
    page
}

fn runtime(state: Probe, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn has_element(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press(rt: &mut Runtime<Probe>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(0.0);
}

fn focused_key(rt: &Runtime<Probe>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn find_json<'a>(node: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if node.get("key").and_then(|v| v.as_str()) == Some(key) {
        return Some(node);
    }
    node.get("children")?
        .as_array()?
        .iter()
        .find_map(|child| find_json(child, key))
}

fn node_states(rt: &Runtime<Probe>, key: &str) -> Vec<String> {
    let json = rt.accessibility_json();
    let node = find_json(&json, key).unwrap_or_else(|| panic!("{key}: is in the a11y tree"));
    node.get("states")
        .and_then(|v| v.as_array())
        .map(|states| {
            states
                .iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------- labels

#[test]
fn the_label_uses_the_raw_operator_and_the_formatted_value() {
    // React `labelFor`: `column operator value`, with the operator id as-is.
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "status",
            "eq",
            FilterValue::Single("paid".to_string())
        )),
        "status eq paid"
    );
    // An absent value drops the suffix (React `value === undefined`).
    assert_eq!(
        filter_bar_label(&FilterCondition::new("age", "isNull", FilterValue::None)),
        "age isNull"
    );
    // A null value is a value: React `formatCell(null)` is `NULL`.
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "deleted_at",
            "isNull",
            FilterValue::Single("NULL".to_string())
        )),
        "deleted_at isNull NULL"
    );
    // Set / range values go through `formatCell`, so React shows the JSON
    // array text — not the chip's human `1, 9` summary.
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "age",
            "between",
            FilterValue::Pair("1".to_string(), "9".to_string())
        )),
        "age between [\"1\",\"9\"]"
    );
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "status",
            "in",
            FilterValue::Tokens(vec!["a".to_string(), "b".to_string()])
        )),
        "status in [\"a\",\"b\"]"
    );
}

#[test]
fn empty_filters_render_nothing() {
    assert!(
        FilterBar::<Probe>::new("filter-bar", &[])
            .build(true)
            .is_none(),
        "React returns null for an empty filter list"
    );
    assert!(
        FilterBar::<Probe>::new("filter-bar", &[])
            .loading(true)
            .build(false)
            .is_none(),
        "loading does not conjure a bar out of no filters"
    );
}

// ---------------------------------------------------------------- render

#[test]
fn the_bar_renders_the_label_pills_and_clear_in_both_themes() {
    for dark in [true, false] {
        let rt = runtime(Probe::default(), DESIGN, dark);
        let k = kit(dark);

        assert!(has("filter.filter"), "the label key exists");
        assert!(has("filter.remove"), "the remove key exists");
        assert!(has("filter.clear"), "the clear key exists");
        assert_eq!(
            element(&rt, "filter-bar-label").text.as_deref(),
            Some(t("filter.filter")),
            "React `filter.filter`"
        );
        assert_eq!(
            element(&rt, "filter-bar-filter-0-text").text.as_deref(),
            Some("status eq paid"),
            "first pill label"
        );
        assert_eq!(
            element(&rt, "filter-bar-filter-1-text").text.as_deref(),
            Some("age gte 18"),
            "second pill label"
        );
        assert_eq!(
            element(&rt, "filter-bar-filter-0-remove").label.as_deref(),
            Some(t("filter.remove")),
            "React `filter.remove` aria-label on every remove"
        );
        assert_eq!(
            element(&rt, "filter-bar-filter-1-remove").label.as_deref(),
            Some(t("filter.remove")),
            "second remove carries the same label"
        );
        assert_eq!(
            element(&rt, "filter-bar-clear").label.as_deref(),
            Some(t("filter.clear")),
            "React `filter.clear` as the button's accessible name"
        );
        // The kit Button paints its text in a child keyed `label`; the root
        // row itself carries no `text`.
        let clear_path = element(&rt, "filter-bar-clear").path.clone();
        let clear_label = rt
            .elements()
            .into_iter()
            .find(|e| e.path == format!("{clear_path}/label"))
            .unwrap_or_else(|| panic!("{clear_path}/label: the clear text is laid out"));
        assert_eq!(
            clear_label.text.as_deref(),
            Some(t("filter.clear")),
            "the clear button paints `filter.clear`"
        );

        // React: `border-b bg-surface px-3 py-2`, `gap-2` throughout.
        let root = element(&rt, "filter-bar");
        assert_eq!(root.rect[0], PAD, "the bar spans the page width");
        assert_eq!(root.rect[2], DESIGN.0 - 2.0 * PAD, "w_full inside the pad");
        assert_eq!(root.fill, k.c.surface_base, "React bg-surface");
        let label = element(&rt, "filter-bar-label");
        let chips = element(&rt, "filter-bar-filters");
        let clear = element(&rt, "filter-bar-clear");
        assert!(
            label.rect[0] >= root.rect[0] + 12.0 - 0.5,
            "px-3 before the label: {:?} in {:?}",
            label.rect,
            root.rect
        );
        assert!(
            (chips.rect[0] - (label.rect[0] + label.rect[2]) - 8.0).abs() <= 0.5,
            "gap-2 between the label and the chips: {:?} then {:?}",
            label.rect,
            chips.rect
        );
        assert!(
            clear.rect[0] > chips.rect[0] + chips.rect[2] - 0.5,
            "clear-all sits after the chips: {:?} then {:?}",
            chips.rect,
            clear.rect
        );
        for key in [
            "filter-bar-label",
            "filter-bar-filters",
            "filter-bar-filter-0",
            "filter-bar-filter-1",
            "filter-bar-clear",
        ] {
            let e = element(&rt, key);
            assert!(
                e.rect[1] >= root.rect[1] - 0.5
                    && e.rect[1] + e.rect[3] <= root.rect[1] + root.rect[3] + 0.5,
                "{key} stays inside the bar vertically: {:?} in {:?}",
                e.rect,
                root.rect
            );
        }

        // The pills: Badge geometry (9pt radius) with the accent wash,
        // the label in the accent ink, the X at text-accent/70.
        for index in [0, 1] {
            let pill = element(&rt, &format!("filter-bar-filter-{index}"));
            assert_eq!(pill.radius, 9.0, "Badge rounded-[9px]");
            assert_eq!(
                pill.fill,
                k.alpha(k.c.bg_accent, 0.10),
                "Badge bg-accent/10"
            );
            let text = element(&rt, &format!("filter-bar-filter-{index}-text"));
            assert_eq!(
                text.text_color,
                Some(k.c.text_accent),
                "Badge text-accent on the label"
            );
            let remove = element(&rt, &format!("filter-bar-filter-{index}-remove"));
            assert_eq!(remove.rect[2], 18.0, "p-0.5 around the 14pt X: width");
            assert_eq!(remove.rect[3], 18.0, "p-0.5 around the 14pt X: height");
            let icon_path = format!("{}/icon", remove.path);
            let icon = rt
                .elements()
                .into_iter()
                .find(|e| e.path == icon_path)
                .unwrap_or_else(|| panic!("{}: the X is laid out", icon_path));
            assert_eq!(icon.rect[2], 14.0, "React h-3.5 w-3.5 X width");
            assert_eq!(icon.rect[3], 14.0, "React h-3.5 w-3.5 X height");
            assert_eq!(
                icon.text_color,
                Some(k.alpha(k.c.text_accent, 0.7)),
                "React text-accent/70 on the X"
            );
        }
        assert!(
            !node_states(&rt, "filter-bar").contains(&"disabled".to_string()),
            "the idle bar is not disabled in the a11y tree"
        );
    }
}

// ---------------------------------------------------------------- actions

#[test]
fn remove_and_clear_dispatch_with_the_react_payloads() {
    let mut rt = runtime(Probe::default(), DESIGN, true);
    click(&mut rt, "filter-bar-filter-0-remove");
    assert_eq!(rt.state.removes, vec![0], "onRemove fires with the index");
    click(&mut rt, "filter-bar-filter-1-remove");
    assert_eq!(
        rt.state.removes,
        vec![0, 1],
        "each pill removes its own index"
    );
    click(&mut rt, "filter-bar-clear");
    assert_eq!(rt.state.clears, 1, "onClear fires once");
    assert_eq!(rt.state.removes.len(), 2, "clear does not remove anything");
}

#[test]
fn a_bar_without_callbacks_still_renders_but_dispatches_nothing() {
    // The callbacks are optional on the builder (React requires them, the
    // probe wires them); without them the bar still renders every part,
    // and pressing its buttons is a no-op rather than a crash.
    let mut rt = runtime(
        Probe {
            callbacks: false,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert_eq!(
        element(&rt, "filter-bar-filter-0-text").text.as_deref(),
        Some("status eq paid"),
        "the pills render without callbacks"
    );
    assert!(
        has_element(&rt, "filter-bar-clear"),
        "clear-all renders without callbacks"
    );
    click(&mut rt, "filter-bar-filter-0-remove");
    click(&mut rt, "filter-bar-clear");
    press(&mut rt, Key::Tab);
    press(&mut rt, Key::Enter);
    press(&mut rt, Key::Text(" ".to_string()));
    assert_eq!(
        rt.state.removes,
        Vec::<usize>::new(),
        "no remove without a handler"
    );
    assert_eq!(rt.state.clears, 0, "no clear without a handler");
}

#[test]
fn space_and_enter_activate_the_buttons_exactly_once() {
    let mut rt = runtime(Probe::default(), DESIGN, true);
    // Tab to the first remove (the bar's first stop), then press it both ways.
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("filter-bar-filter-0-remove"),
        "the first remove is the bar's first Tab stop"
    );
    press(&mut rt, Key::Text(" ".to_string()));
    assert_eq!(rt.state.removes, vec![0], "Space presses remove once");
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.removes,
        vec![0, 0],
        "Enter presses remove once (Rune buttons only take Space)"
    );

    // Tab on to the clear button and press it both ways.
    for _ in 0..8 {
        if focused_key(&rt).as_deref() == Some("filter-bar-clear") {
            break;
        }
        press(&mut rt, Key::Tab);
    }
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("filter-bar-clear"),
        "clear is reachable by keyboard"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.clears, 1, "Enter clears exactly once");
    press(&mut rt, Key::Text(" ".to_string()));
    assert_eq!(rt.state.clears, 2, "Space clears exactly once");
}

// ---------------------------------------------------------------- loading

#[test]
fn loading_disables_both_buttons_and_dispatches_nothing() {
    // Mirrors the React test: aria-busy/aria-disabled on the root, both
    // buttons disabled, clicks fire neither callback.
    let mut rt = runtime(
        Probe {
            loading: true,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert!(
        node_states(&rt, "filter-bar").contains(&"disabled".to_string()),
        "the loading bar reports disabled in the a11y tree"
    );
    assert!(
        node_states(&rt, "filter-bar-filter-0-remove").contains(&"disabled".to_string()),
        "the loading remove reports disabled in the a11y tree"
    );
    assert!(
        !element(&rt, "filter-bar-filter-0-remove").focusable,
        "the loading remove leaves the tab order"
    );
    assert!(
        !element(&rt, "filter-bar-clear").focusable,
        "the loading clear leaves the tab order"
    );
    // No opacity assertion: the remove also dims to `layer_opacity(0.5)`
    // (the chip's shape), but neither `ElementInfo.opacity` nor
    // `Runtime::presented` includes the layer opacity, and the paint probe
    // that does (`paints()`) is `pub(crate)` in rune — there is no public
    // channel for it in a unit test, so the dim is documented, not asserted.
    for _ in 0..8 {
        press(&mut rt, Key::Tab);
        assert_ne!(
            focused_key(&rt).as_deref(),
            Some("filter-bar-filter-0-remove"),
            "the loading remove never takes focus"
        );
        assert_ne!(
            focused_key(&rt).as_deref(),
            Some("filter-bar-clear"),
            "the loading clear never takes focus"
        );
    }
    click(&mut rt, "filter-bar-filter-0-remove");
    click(&mut rt, "filter-bar-clear");
    press(&mut rt, Key::Enter);
    press(&mut rt, Key::Text(" ".to_string()));
    assert_eq!(
        rt.state.removes,
        Vec::<usize>::new(),
        "no remove while loading"
    );
    assert_eq!(rt.state.clears, 0, "no clear while loading");
}

// ---------------------------------------------------------------- geometry

#[test]
fn a_long_condition_stays_inside_the_minimum_window() {
    let long = "a".repeat(200);
    let rt = runtime(
        Probe {
            filters: vec![FilterCondition::new(
                "notes",
                "contains",
                FilterValue::Single(long),
            )],
            ..Probe::default()
        },
        MIN,
        true,
    );
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > MIN.0 + 0.5 || r[1] + r[3] > MIN.1 + 0.5 {
            outside.push(format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "nothing crosses the 960x640 window: {outside:?}"
    );
    assert!(
        has_element(&rt, "filter-bar-filter-0-remove"),
        "the remove button survives a long label"
    );
    assert!(
        has_element(&rt, "filter-bar-clear"),
        "clear-all survives a long label"
    );
    // The pill shrinks to the chips row and the label truncates (React
    // `max-w-full truncate` on the Badge), instead of pushing the remove
    // button out of the window. The `…` glyph itself has no public
    // observation channel — `ElementInfo.text` deliberately carries the
    // whole string, as rune's own truncate test asserts — so the proof is
    // geometric: the same label laid out in a 2400pt window is wider, and
    // here it ends before its remove button.
    let text = element(&rt, "filter-bar-filter-0-text");
    let remove = element(&rt, "filter-bar-filter-0-remove");
    assert!(
        text.rect[0] + text.rect[2] <= remove.rect[0] + 0.5,
        "the label ends before its remove button: {:?} then {:?}",
        text.rect,
        remove.rect
    );
    let wide = runtime(
        Probe {
            filters: vec![FilterCondition::new(
                "notes",
                "contains",
                FilterValue::Single("a".repeat(200)),
            )],
            ..Probe::default()
        },
        (2400.0, 640.0),
        true,
    );
    let wide_text = element(&wide, "filter-bar-filter-0-text");
    assert!(
        text.rect[2] < wide_text.rect[2] - 1.0,
        "the narrow label is cut short of its full width: {:?} vs {:?}",
        text.rect,
        wide_text.rect
    );
}
