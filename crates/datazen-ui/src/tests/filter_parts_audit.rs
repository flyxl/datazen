//! Independent audit for T021/T023: the between-bounds input, the condition
//! chip and the per-condition connector toggle, against their React sources
//! (`FilterBetweenInput.tsx`, `FilterConditionChip.tsx`, `FilterConnectorToggle.tsx`).
//!
//! The coder's `tests/filter_parts.rs` proves the actions fire and that the
//! parts fit when stacked in a column. This audit measures the React geometry
//! (pill radius, padding, icon sizes, colours), the inversion rule at its
//! boundaries, the dynamic `filter.<operator>` keys, and a single filter row
//! and a narrow filter column.

use rune::{col, row, Color, Cx, El, Runtime, TextEdit};

use crate::components::filter::{
    condition_summary, is_inverted, FilterBetweenInput, FilterCondition, FilterConditionChip,
    FilterConnector, FilterConnectorToggle, FilterValue,
};
use crate::i18n::{has, t};
use crate::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

/// Every operator the React filter editor can show a condition for.
const OPERATORS: [&str; 22] = [
    "eq", "ne", "gt", "lt", "gte", "lte", "like", "in", "isNull", "isNotNull", "notIn", "between",
    "notBetween", "contains", "notContains", "startsWith", "endsWith", "isEmpty", "isNotEmpty",
    "regex", "notRegex", "jsonContains",
];

const STATIC_KEYS: [&str; 6] = [
    "filter.and",
    "filter.or",
    "filter.betweenInverted",
    "filter.editCondition",
    "filter.remove",
    "filter.connectorInherited",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scene {
    #[default]
    Between,
    Chip,
    Toggle,
    /// The three parts on one filter row, as the filter panel stacks conditions.
    Row,
    /// The chip alone inside a narrow filter column.
    Narrow,
}

struct Probe {
    scene: Scene,
    lower: TextEdit,
    upper: TextEdit,
    condition: FilterCondition,
    connector: FilterConnector,
    explicit: bool,
    differs: bool,
    disabled: bool,
    edits: u32,
    removes: u32,
    ands: u32,
    ors: u32,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            scene: Scene::Between,
            lower: TextEdit::new("1"),
            upper: TextEdit::new("9"),
            condition: FilterCondition::new("name", "eq", FilterValue::Single("alice".to_owned())),
            connector: FilterConnector::And,
            explicit: true,
            differs: false,
            disabled: false,
            edits: 0,
            removes: 0,
            ands: 0,
            ors: 0,
        }
    }
}

fn between(state: &Probe, dark: bool) -> El {
    FilterBetweenInput::new(
        "between",
        &state.lower,
        &state.upper,
        |state: &mut Probe| &mut state.lower,
        |state: &mut Probe| &mut state.upper,
    )
    .placeholder("value")
    .build(dark)
}

fn chip(state: &Probe, dark: bool) -> El {
    FilterConditionChip::new("chip", state.condition.clone())
        .disabled(state.disabled)
        .on_edit(|state: &mut Probe| state.edits += 1)
        .on_remove(|state: &mut Probe| state.removes += 1)
        .build(dark)
}

fn toggle(state: &Probe, dark: bool) -> El {
    FilterConnectorToggle::new("connector", 0, state.connector)
        .explicit(state.explicit)
        .differs_from_global(state.differs)
        .disabled(state.disabled)
        .on_and(|state: &mut Probe| state.ands += 1)
        .on_or(|state: &mut Probe| state.ors += 1)
        .build(dark)
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut page = col().key("page").w_full().h_full().padding(PAD).gap(8.0);
    match state.scene {
        Scene::Between => page = page.child(between(state, dark)),
        Scene::Chip => page = page.child(chip(state, dark)),
        Scene::Toggle => page = page.child(toggle(state, dark)),
        Scene::Row => {
            page = page.child(
                row()
                    .key("filter-row")
                    .w_full()
                    .gap(8.0)
                    .child(chip(state, dark))
                    .child(between(state, dark))
                    .child(toggle(state, dark)),
            );
        }
        Scene::Narrow => {
            page = page.child(
                col()
                    .key("narrow")
                    .w(360.0)
                    .child(chip(state, dark)),
            );
        }
    }
    page
}

fn runtime(state: Probe, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        state,
        view,
        size,
        rune::Env {
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

fn assert_subtree_inside(rt: &Runtime<Probe>, root_key: &str, what: &str) {
    let root = element(rt, root_key);
    let prefix = format!("{}/", root.path);
    for e in rt.elements() {
        if e.path != root.path && !e.path.starts_with(&prefix) {
            continue;
        }
        let r = e.rect;
        assert!(
            r[0] >= root.rect[0] - 0.5
                && r[1] >= root.rect[1] - 0.5
                && r[0] + r[2] <= root.rect[0] + root.rect[2] + 0.5
                && r[1] + r[3] <= root.rect[1] + root.rect[3] + 0.5,
            "{what}: {} at {r:?} crosses {:?}",
            e.key.as_deref().unwrap_or(&e.path),
            root.rect
        );
    }
}

// ---------------------------------------------------------------- between

#[test]
fn the_between_bounds_keep_the_react_geometry_and_connector() {
    let rt = runtime(
        Probe {
            scene: Scene::Between,
            lower: TextEdit::new(""),
            upper: TextEdit::new(""),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let lower = element(&rt, "between-lower");
    let connector = element(&rt, "between-connector");
    let upper = element(&rt, "between-upper");

    // React: `!h-7 w-20` inputs and `gap-1`, the word between them.
    assert_eq!(lower.rect[2], 80.0, "lower bound keeps React's w-20");
    assert_eq!(lower.rect[3], 28.0, "lower bound keeps React's h-7");
    assert_eq!(upper.rect[2], 80.0, "upper bound keeps React's w-20");
    assert_eq!(upper.rect[3], 28.0, "upper bound keeps React's h-7");
    assert!(
        (connector.rect[0] - (lower.rect[0] + lower.rect[2]) - 4.0).abs() <= 0.5,
        "gap-1 between the lower bound and the word: {:?} then {:?}",
        lower.rect,
        connector.rect
    );
    assert!(
        (upper.rect[0] - (connector.rect[0] + connector.rect[2]) - 4.0).abs() <= 0.5,
        "gap-1 between the word and the upper bound: {:?} then {:?}",
        connector.rect,
        upper.rect
    );
    assert_eq!(
        connector.text.as_deref(),
        Some(t("filter.and")),
        "the connector text comes from the locale"
    );
    assert!(
        !has_element(&rt, "between-inverted"),
        "no warning when both bounds are empty"
    );
}

#[test]
fn the_between_inversion_rule_matches_the_react_comparisons() {
    // Equal values are not inverted; a real order is not inverted either.
    assert!(!is_inverted("2024-05-01", "2024-05-01"));
    assert!(!is_inverted("2024-12-31", "2025-01-01"));
    assert!(is_inverted("2025-01-01", "2024-12-31"));
    // Numbers compare numerically before dates.
    assert!(is_inverted("9", "2"));
    assert!(!is_inverted("2", "9"));
    assert!(!is_inverted("9", "9"));
    assert!(is_inverted("9.5", "9.49"));
    // Timestamps accept `T` and a space, with optional seconds.
    assert!(is_inverted("2024-05-01T12:00", "2024-05-01 11:00"));
    assert!(!is_inverted("2024-05-01 10:00", "2024-05-01T10:00"));
    assert!(is_inverted("2024-05-01 10:00:30", "2024-05-01T10:00:29"));
    // Unparseable and empty bounds never warn.
    assert!(!is_inverted("2024-13-01", "2024-12-01"));
    assert!(!is_inverted("not a date", "2024-05-01"));
    assert!(!is_inverted("", "2024-05-01"));
}

#[test]
fn an_inverted_between_shows_only_the_react_warning_with_its_text() {
    let rt = runtime(
        Probe {
            scene: Scene::Between,
            lower: TextEdit::new("2025-01-01"),
            upper: TextEdit::new("2024-12-31"),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let warning = element(&rt, "between-inverted");
    assert_eq!(
        warning.text.as_deref(),
        Some(t("filter.betweenInverted")),
        "the warning shows the locale copy"
    );
    // React also sets `title=`; Rune `El::description` is the a11y description, not
    // a hover tooltip, so the visible label is the only carrier in Rune. Annotated,
    // not asserted: the copy itself is on screen either way.
    // The bound order is left as typed; no swap.
    let lower = element(&rt, "between-lower");
    let upper = element(&rt, "between-upper");
    assert!(
        lower.rect[0] < upper.rect[0],
        "lower stays left of upper: {:?} / {:?}",
        lower.rect,
        upper.rect
    );

    // The same bounds in order show no warning.
    let rt = runtime(
        Probe {
            scene: Scene::Between,
            lower: TextEdit::new("2024-12-31"),
            upper: TextEdit::new("2025-01-01"),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert!(!has_element(&rt, "between-inverted"), "no warning in order");
}

#[test]
fn the_between_parser_documents_its_react_subset() {
    // `parse_instant` documents that it covers the forms a filter bound usually
    // holds and leaves the rest alone, so these stay quiet instead of warning:
    // a browser `Date.parse` would date them.
    for (low, high) in [
        ("2024/05/01", "2024-04-01"),
        ("2024-05-01T12:00:00Z", "2024-05-02"),
        ("2024-05-01T12:00:00.500", "2024-05-02"),
        ("0x10", "9"),
    ] {
        assert!(
            !is_inverted(low, high),
            "{low} / {high}: outside the documented parse subset, no warning"
        );
    }
}

// ---------------------------------------------------------------- chip

#[test]
fn the_chip_pill_keeps_the_react_geometry() {
    let rt = runtime(
        Probe {
            scene: Scene::Chip,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let k = kit(true);
    let chip = element(&rt, "chip");
    let edit = element(&rt, "chip-edit");
    let remove = element(&rt, "chip-remove");

    assert_eq!(chip.radius, 999.0, "rounded-full pill");
    assert_eq!(
        chip.fill,
        k.alpha(k.c.bg_accent, 0.10),
        "React bg-accent/10"
    );
    // `pl-2 pr-0.5 py-0.5`: 8pt before the summary, 2pt around the X.
    assert!(
        (edit.rect[0] - chip.rect[0] - 8.0).abs() <= 0.5,
        "pl-2: chip {:?}, edit {:?}",
        chip.rect,
        edit.rect
    );
    assert!(
        remove.rect[0] + remove.rect[2] <= chip.rect[0] + chip.rect[2] + 0.5,
        "the X stays inside the pill: chip {:?}, remove {:?}",
        chip.rect,
        remove.rect
    );
    // The summary is the accent Ghost button with truncation and the edit hint.
    let summary_ink = rt
        .elements()
        .into_iter()
        .find(|e| e.text.as_deref().is_some_and(|t| t.contains("alice")))
        .and_then(|e| e.text_color);
    assert_eq!(summary_ink, Some(k.c.text_accent), "summary is accent");
    assert_eq!(
        edit.help.as_deref(),
        Some(t("filter.editCondition")),
        "React title={{filter.editCondition}}"
    );
    // The X is a 16pt icon button (React p-0.5 + h-3 w-3 = 16pt).
    assert_eq!(
        (remove.rect[2], remove.rect[3]),
        (16.0, 16.0),
        "the remove button keeps React's 16pt hit area"
    );
    assert_eq!(
        remove.label.as_deref(),
        Some(t("filter.remove")),
        "React aria-label={{filter.remove}}"
    );
}

#[test]
#[ignore = "DEFECT 23b973021: the chip remove is a bare row with role=button but focusable=false; React's button is keyboard reachable"]
fn the_chip_remove_is_keyboard_reachable() {
    let rt = runtime(
        Probe {
            scene: Scene::Chip,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let remove = element(&rt, "chip-remove");
    assert!(
        remove.focusable,
        "the remove button must be a keyboard stop: role={:?} label={:?}",
        remove.role, remove.label
    );
}

#[test]
fn the_chip_hugs_a_short_condition_like_an_inline_pill() {
    let rt = runtime(
        Probe {
            scene: Scene::Chip,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let chip = element(&rt, "chip");
    assert!(
        chip.rect[2] < 240.0,
        "a short condition keeps a short pill, got {:?}",
        chip.rect
    );
}

#[test]
fn the_chip_remove_icon_keeps_the_react_accent_ink() {
    let rt = runtime(
        Probe {
            scene: Scene::Chip,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let k = kit(true);
    let remove = element(&rt, "chip-remove");
    let icon_path = format!("{}/icon", remove.path);
    let icon = rt
        .elements()
        .into_iter()
        .find(|e| e.path == icon_path)
        .unwrap_or_else(|| panic!("{}: the X icon is laid out", icon_path));
    assert_eq!(
        icon.text_color,
        Some(k.alpha(k.c.text_accent, 0.8)),
        "React text-accent/80 on the X"
    );
}

#[test]
fn a_long_chip_truncates_inside_a_narrow_filter_column() {
    let long = "a".repeat(120);
    let rt = runtime(
        Probe {
            scene: Scene::Narrow,
            condition: FilterCondition::new("name", "eq", FilterValue::Single(long)),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert_subtree_inside(&rt, "narrow", "narrow filter column");
}

#[test]
fn a_disabled_chip_drops_both_actions() {
    let mut rt = runtime(
        Probe {
            scene: Scene::Chip,
            disabled: true,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "chip-edit");
    click(&mut rt, "chip-remove");
    assert_eq!(rt.state.edits, 0, "a disabled chip does not open the editor");
    assert_eq!(rt.state.removes, 0, "a disabled chip does not remove");

    let mut rt = runtime(
        Probe {
            scene: Scene::Chip,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "chip-edit");
    click(&mut rt, "chip-remove");
    assert_eq!(rt.state.edits, 1, "an enabled chip opens the editor");
    assert_eq!(rt.state.removes, 1, "an enabled chip removes");
}

// ---------------------------------------------------------------- connector

#[test]
fn the_connector_follows_the_react_state_conditions() {
    let k = kit(true);
    // Inherited (not explicit): dimmed, tooltip names the inheritance.
    let rt = runtime(
        Probe {
            scene: Scene::Toggle,
            explicit: false,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let inherited = element(&rt, "connector");
    assert!(
        (inherited.opacity - 0.7).abs() <= 0.01,
        "React opacity-70 when inherited: {}",
        inherited.opacity
    );
    assert_eq!(
        inherited.help.as_deref(),
        Some(t("filter.connectorInherited")),
        "React title={{filter.connectorInherited}}"
    );
    assert_eq!(inherited.fill, Color::TRANSPARENT, "no accent wash");

    // Explicit and different from the global connector: accent border/fill and
    // the pressed half painted in accent.
    let rt = runtime(
        Probe {
            scene: Scene::Toggle,
            explicit: true,
            differs: true,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let root = element(&rt, "connector");
    assert_eq!(root.opacity, 1.0, "explicit is fully opaque");
    assert_eq!(root.help, None, "React drops the title once explicit");
    assert_eq!(
        root.fill,
        k.alpha(k.c.bg_accent, 0.10),
        "React bg-accent/10 when it differs"
    );
    let and = element(&rt, "connector-and");
    let or = element(&rt, "connector-or");
    assert_eq!(
        and.fill,
        k.alpha(k.c.text_accent, 0.15),
        "the active half gets bg-accent/15"
    );
    assert_eq!(or.fill, Color::TRANSPARENT, "the inactive half has no wash");
    let label_color = |label: &str| {
        rt.elements()
            .into_iter()
            .find(|e| e.text.as_deref() == Some(label))
            .and_then(|e| e.text_color)
    };
    assert_eq!(
        label_color(t("filter.and")),
        Some(k.c.text_accent),
        "active text is accent"
    );
    assert_eq!(
        label_color(t("filter.or")),
        Some(k.c.text_muted),
        "inactive text is muted"
    );
    assert_eq!(root.radius, k.radius.sm, "rounded = radius.sm (4pt)");
}

#[test]
fn the_connector_reports_the_pressed_half() {
    let mut rt = runtime(
        Probe {
            scene: Scene::Toggle,
            connector: FilterConnector::And,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "connector-or");
    click(&mut rt, "connector-and");
    assert_eq!(rt.state.ors, 1, "OR reports once");
    assert_eq!(rt.state.ands, 1, "AND reports once");

    let mut rt = runtime(
        Probe {
            scene: Scene::Toggle,
            disabled: true,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "connector-or");
    click(&mut rt, "connector-and");
    assert_eq!(rt.state.ors, 0, "a disabled toggle does not report OR");
    assert_eq!(rt.state.ands, 0, "a disabled toggle does not report AND");
}

// ---------------------------------------------------------------- i18n

#[test]
fn every_operator_key_the_summary_can_ask_for_exists() {
    for op in OPERATORS {
        let key = format!("filter.{op}");
        assert!(has(&key), "{key}: is defined by the zh-CN packs");
        assert_ne!(t(&key), key, "{key}: resolves to Chinese copy");
        let condition =
            FilterCondition::new("name", op, FilterValue::Single("alice".to_owned()));
        let summary = condition_summary(&condition);
        assert!(
            summary.contains(t(&key)),
            "{op}: the summary uses {key}, got {summary}"
        );
    }
    for key in STATIC_KEYS {
        assert!(has(key), "{key}: is defined by the zh-CN packs");
        assert_ne!(t(key), key, "{key}: resolves to Chinese copy");
    }
}

// ---------------------------------------------------------------- geometry

#[test]
fn one_filter_row_stays_inside_both_windows_and_themes() {
    for (size, label) in [(MIN, "960x640"), (DESIGN, "1440x816")] {
        for dark in [true, false] {
            let rt = runtime(
                Probe {
                    scene: Scene::Row,
                    condition: FilterCondition::new(
                        "customer_name",
                        "contains",
                        FilterValue::Single("acme corporation".to_owned()),
                    ),
                    differs: true,
                    ..Probe::default()
                },
                size,
                dark,
            );
            let theme = if dark { "dark" } else { "light" };
            for e in rt.elements() {
                let r = e.rect;
                assert!(
                    r[0] >= -0.5
                        && r[1] >= -0.5
                        && r[0] + r[2] <= size.0 + 0.5
                        && r[1] + r[3] <= size.1 + 0.5,
                    "{label} {theme}: {} at {r:?} crosses the window",
                    e.key.as_deref().unwrap_or(&e.path)
                );
            }
            // The three parts share one line, in React's order: chip, between, toggle.
            let chip = element(&rt, "chip");
            let between = element(&rt, "between");
            let toggle = element(&rt, "connector");
            assert!(
                chip.rect[0] < between.rect[0] && between.rect[0] < toggle.rect[0],
                "{label} {theme}: one row order chip<between<connector: {:?} {:?} {:?}",
                chip.rect,
                between.rect,
                toggle.rect
            );
        }
    }
}

#[test]
fn a_narrow_filter_column_keeps_a_short_chip_inside() {
    let rt = runtime(
        Probe {
            scene: Scene::Narrow,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let chip = element(&rt, "chip");
    assert!(
        chip.rect[2] < 200.0,
        "a short condition keeps the pill hugged even in a narrow column, got {:?} inside {:?}",
        chip.rect,
        element(&rt, "narrow").rect
    );
}
