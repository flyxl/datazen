//! T014 independent audit: fresh vectors for the filter bar.
//!
//! Deliberately different from the coder's `filter_bar.rs`: new `labelFor`
//! vectors (including empty values and a connector that must not leak into
//! the label), the literal zh-CN texts, three pills with a middle-index
//! remove, a full Tab walk, twelve pills wrapping at the minimum window,
//! and a probe of whether the loading dim is observable through the public
//! `Runtime::presented` channel.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::components::filter::{FilterCondition, FilterConnector, FilterValue};
use datazen_ui::components::filter_bar::{filter_bar_label, FilterBar};
use datazen_ui::i18n::{has, t};
use datazen_ui::ui::kit::kit;

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

impl Probe {
    fn three() -> Self {
        Self {
            filters: vec![
                FilterCondition::new("status", "eq", FilterValue::Single("paid".to_string())),
                FilterCondition::new("age", "gte", FilterValue::Single("18".to_string())),
                FilterCondition::new("name", "contains", FilterValue::Single("li".to_string())),
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
    let mut bar = FilterBar::new("audit-bar", &state.filters).loading(state.loading);
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

fn assert_inside_window(rt: &Runtime<Probe>, size: (f32, f32)) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        {
            outside.push(format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), r));
        }
    }
    assert!(outside.is_empty(), "nothing crosses the window: {outside:?}");
}

// ---------------------------------------------------------------- labels

#[test]
fn audit_label_for_fresh_vectors() {
    // A different operator id passes through verbatim (React never translates
    // the operator here — the chip's `filter.<operator>` lookup is elsewhere).
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "name",
            "contains",
            FilterValue::Single("li".to_string())
        )),
        "name contains li"
    );
    assert_eq!(
        filter_bar_label(&FilterCondition::new("score", "neq", FilterValue::None)),
        "score neq"
    );
    // Empty pair / token values still go through `formatCell`'s JSON text:
    // React `JSON.stringify(["",""])` is `["",""]`, not a dropped suffix.
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "age",
            "between",
            FilterValue::Pair(String::new(), String::new())
        )),
        "age between [\"\",\"\"]"
    );
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "status",
            "in",
            FilterValue::Tokens(Vec::new())
        )),
        "status in []"
    );
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "status",
            "in",
            FilterValue::Tokens(vec!["only".to_string()])
        )),
        "status in [\"only\"]"
    );
    // A Single value is already the `formatCell` output: JSON-looking text
    // passes through verbatim instead of being re-encoded.
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "raw",
            "eq",
            FilterValue::Single("[1,2]".to_string())
        )),
        "raw eq [1,2]"
    );
    // An empty Single is still a value: React renders `col op ` with the
    // trailing space (`` ` ${formatCell("")}` ``), not the no-suffix shape.
    assert_eq!(
        filter_bar_label(&FilterCondition::new(
            "name",
            "eq",
            FilterValue::Single(String::new())
        )),
        "name eq "
    );
    // The per-condition connector joins conditions; it never enters the label
    // (React's `labelFor` reads column/operator/value only).
    let with_connector = FilterCondition::new(
        "status",
        "eq",
        FilterValue::Single("paid".to_string()),
    )
    .connector(Some(FilterConnector::Or));
    assert_eq!(filter_bar_label(&with_connector), "status eq paid");
}

#[test]
fn audit_exact_zh_texts() {
    // Literal zh-CN values from `packages/frontend/src/locales/zh-CN/query.ts`
    // (lines 584-586), not just `has`: a wrong pack entry must fail here.
    assert!(has("filter.filter") && has("filter.remove") && has("filter.clear"));
    assert_eq!(t("filter.filter"), "筛选", "React `filter.filter`");
    assert_eq!(t("filter.remove"), "移除筛选", "React `filter.remove`");
    assert_eq!(t("filter.clear"), "清空", "React `filter.clear`");
    let rt = runtime(Probe::three(), DESIGN, true);
    assert_eq!(
        element(&rt, "audit-bar-label").text.as_deref(),
        Some("筛选"),
        "the bar label paints the resolved text"
    );
    assert_eq!(
        element(&rt, "audit-bar-filter-1-remove").label.as_deref(),
        Some("移除筛选"),
        "every remove carries the resolved aria-label"
    );
    let clear_path = element(&rt, "audit-bar-clear").path.clone();
    let clear_label = rt
        .elements()
        .into_iter()
        .find(|e| e.path == format!("{clear_path}/label"))
        .unwrap_or_else(|| panic!("{clear_path}/label: the clear text is laid out"));
    assert_eq!(
        clear_label.text.as_deref(),
        Some("清空"),
        "the clear button paints the resolved text"
    );
}

// ---------------------------------------------------------------- actions

#[test]
fn audit_three_pills_remove_the_middle_index_and_clear() {
    let mut rt = runtime(Probe::three(), DESIGN, true);
    assert_eq!(
        element(&rt, "audit-bar-filter-2-text").text.as_deref(),
        Some("name contains li"),
        "third pill label"
    );
    click(&mut rt, "audit-bar-filter-1-remove");
    assert_eq!(rt.state.removes, vec![1], "onRemove fires with the middle index");
    click(&mut rt, "audit-bar-filter-2-remove");
    assert_eq!(rt.state.removes, vec![1, 2], "each pill removes its own index");
    click(&mut rt, "audit-bar-clear");
    assert_eq!(rt.state.clears, 1, "onClear fires once");
    assert_eq!(rt.state.removes.len(), 2, "clear does not remove anything");
}

#[test]
fn audit_tab_order_walks_every_remove_then_clear() {
    let mut rt = runtime(Probe::three(), DESIGN, true);
    let mut stops = Vec::new();
    for _ in 0..3 {
        press(&mut rt, Key::Tab);
        stops.push(focused_key(&rt));
    }
    assert_eq!(
        stops[0].as_deref(),
        Some("audit-bar-filter-0-remove"),
        "first Tab stop is the first remove: {stops:?}"
    );
    assert_eq!(
        stops[1].as_deref(),
        Some("audit-bar-filter-1-remove"),
        "second Tab stop is the second remove: {stops:?}"
    );
    assert_eq!(
        stops[2].as_deref(),
        Some("audit-bar-filter-2-remove"),
        "third Tab stop is the third remove: {stops:?}"
    );
    // Focus is on the third remove: Space fires it exactly once, index 2.
    press(&mut rt, Key::Text(" ".to_string()));
    assert_eq!(rt.state.removes, vec![2], "Space on the third remove fires once");
    press(&mut rt, Key::Tab);
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("audit-bar-clear"),
        "Tab from the third remove reaches clear"
    );
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.clears, 1, "Enter on clear fires exactly once");
}

// ---------------------------------------------------------------- loading

#[test]
fn audit_loading_dim_and_disabled_state_cover_every_pill() {
    let idle = runtime(Probe::three(), DESIGN, true);
    let loading = runtime(
        Probe {
            loading: true,
            ..Probe::three()
        },
        DESIGN,
        true,
    );
    // The coder's module docs claim the `layer_opacity(0.5)` dim has no
    // public observation channel. Verified here instead of taken on trust:
    // `Runtime::presented` reports the effective opacity, and it is 1.0 in
    // BOTH states — the layer opacity genuinely does not leak into the
    // presented channel (nor into `ElementInfo.opacity`, same source), and
    // the paint probe that does carry it (`paints()`) is `pub(crate)` in
    // the rune crate, so a datazen-ui test cannot reach it. The loading
    // proof therefore rests on the a11y disabled states and no-dispatch
    // below, exactly as the coder documented.
    let idle_opacity = idle
        .presented("audit-bar-filter-0-remove")
        .map(|(_, opacity)| opacity);
    let loading_opacity = loading
        .presented("audit-bar-filter-0-remove")
        .map(|(_, opacity)| opacity);
    assert_eq!(
        idle_opacity,
        Some(1.0),
        "the idle remove paints fully opaque"
    );
    assert_eq!(
        loading_opacity,
        Some(1.0),
        "the loading dim does not surface through presented() — no public channel, as documented"
    );
    // Every pill — not just the first — reports disabled while loading.
    for index in 0..3 {
        let key = format!("audit-bar-filter-{index}-remove");
        assert!(
            node_states(&loading, &key).contains(&"disabled".to_string()),
            "{key} reports disabled in the a11y tree"
        );
        assert!(
            node_states(&loading, &format!("audit-bar-filter-{index}"))
                .contains(&"disabled".to_string()),
            "pill {index} reports disabled in the a11y tree"
        );
        assert!(
            !element(&loading, &key).focusable,
            "{key} leaves the tab order"
        );
    }
    assert!(
        node_states(&loading, "audit-bar").contains(&"disabled".to_string()),
        "the loading bar reports disabled in the a11y tree"
    );
    let mut rt = loading;
    click(&mut rt, "audit-bar-filter-1-remove");
    click(&mut rt, "audit-bar-clear");
    press(&mut rt, Key::Enter);
    press(&mut rt, Key::Text(" ".to_string()));
    assert_eq!(
        rt.state.removes,
        Vec::<usize>::new(),
        "no remove while loading, middle pill included"
    );
    assert_eq!(rt.state.clears, 0, "no clear while loading");
}

// ---------------------------------------------------------------- geometry

#[test]
fn audit_single_filter_geometry_order_and_kit_metrics() {
    for dark in [true, false] {
        let rt = runtime(
            Probe {
                filters: vec![FilterCondition::new(
                    "status",
                    "eq",
                    FilterValue::Single("paid".to_string()),
                )],
                ..Probe::three()
            },
            DESIGN,
            dark,
        );
        let k = kit(dark);
        let root = element(&rt, "audit-bar");
        let label = element(&rt, "audit-bar-label");
        let chips = element(&rt, "audit-bar-filters");
        let clear = element(&rt, "audit-bar-clear");
        assert_eq!(root.rect[0], PAD, "the bar spans the page width");
        assert_eq!(root.rect[2], DESIGN.0 - 2.0 * PAD, "w_full inside the pad");
        assert_eq!(root.fill, k.c.surface_base, "React bg-surface");
        // Strict left-to-right order with no horizontal overlap: React lays
        // the label, the chips row and clear-all out in one flex row.
        assert!(
            label.rect[0] + label.rect[2] <= chips.rect[0] + 0.5,
            "label ends before the chips: {:?} then {:?}",
            label.rect,
            chips.rect
        );
        assert!(
            chips.rect[0] + chips.rect[2] <= clear.rect[0] + 0.5,
            "chips end before clear-all: {:?} then {:?}",
            chips.rect,
            clear.rect
        );
        assert_eq!(
            label.text_color,
            Some(k.c.text_muted),
            "React text-fg-muted on the label"
        );
        let pill = element(&rt, "audit-bar-filter-0");
        assert_eq!(pill.radius, 9.0, "Badge rounded-[9px]");
        assert_eq!(pill.fill, k.alpha(k.c.bg_accent, 0.10), "Badge bg-accent/10");
        assert_eq!(
            element(&rt, "audit-bar-filter-0-text").text_color,
            Some(k.c.text_accent),
            "Badge text-accent on the label"
        );
        // The kit Ghost Small clear button measures 20pt high (push metrics),
        // not the 13pt the module docs claim for its font: the Small control
        // sets its label in 11pt `small_medium`.
        assert_eq!(clear.rect[3], 20.0, "kit Ghost Small height");
        assert_inside_window(&rt, DESIGN);
    }
}

#[test]
fn audit_twelve_pills_wrap_inside_the_min_window_light() {
    let many: Vec<FilterCondition> = (0..12)
        .map(|i| {
            FilterCondition::new(
                format!("column_{i:02}"),
                "eq",
                FilterValue::Single(format!("value_{i:02}")),
            )
        })
        .collect();
    let rt = runtime(
        Probe {
            filters: many,
            ..Probe::three()
        },
        MIN,
        false,
    );
    let root = element(&rt, "audit-bar");
    assert!(
        has_element(&rt, "audit-bar-filter-11-remove"),
        "the last pill's remove button survives twelve pills"
    );
    assert!(has_element(&rt, "audit-bar-clear"), "clear-all survives twelve pills");
    assert_inside_window(&rt, MIN);
    // Wrapping is real: twelve pills need more than one row, so the bar is
    // taller than the single-pill bar at the same window.
    let single = runtime(
        Probe {
            filters: vec![FilterCondition::new(
                "status",
                "eq",
                FilterValue::Single("paid".to_string()),
            )],
            ..Probe::three()
        },
        MIN,
        false,
    );
    let single_root = element(&single, "audit-bar");
    assert!(
        root.rect[3] > single_root.rect[3] + 1.0,
        "twelve pills wrap to extra rows: {:?} vs single {:?}",
        root.rect,
        single_root.rect
    );
    // Every pill stays inside the bar vertically-or-wrapped: no pill hangs
    // below the bar's bottom edge.
    for index in 0..12 {
        let pill = element(&rt, &format!("audit-bar-filter-{index}"));
        assert!(
            pill.rect[1] + pill.rect[3] <= root.rect[1] + root.rect[3] + 0.5,
            "pill {index} stays inside the bar: {:?} in {:?}",
            pill.rect,
            root.rect
        );
    }
}
