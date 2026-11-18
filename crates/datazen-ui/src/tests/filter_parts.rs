//! T021/T023: the filter editor parts (`between` bounds, condition chip and
//! per-condition connector) render their React structure, commit the same
//! actions, and stay inside the two window sizes.

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::components::filter::{
    condition_summary, is_inverted, FilterBetweenInput, FilterCondition, FilterConditionChip,
    FilterConnector, FilterConnectorToggle, FilterValue,
};
use crate::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scene {
    #[default]
    Between,
    Chip,
    Toggle,
    All,
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

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let dark = _cx.system_dark();
    let mut page = col().key("page").w_full().h_full().padding(PAD).gap(8.0);
    match state.scene {
        Scene::Between => page = page.child(between(state, dark)),
        Scene::Chip => page = page.child(chip(state, dark)),
        Scene::Toggle => page = page.child(toggle(state, dark)),
        Scene::All => {
            page = page
                .child(between(state, dark))
                .child(chip(state, dark))
                .child(toggle(state, dark));
        }
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element_text<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
}

#[test]
fn between_renders_both_bounds_and_the_connector() {
    let rt = runtime(Probe::default(), DESIGN, true);
    assert!(
        rt.find("between-lower").is_some(),
        "lower bound is laid out"
    );
    assert!(
        rt.find("between-upper").is_some(),
        "upper bound is laid out"
    );
    assert_eq!(
        element_text(&rt, "between-connector").as_deref(),
        Some(t("filter.and")),
        "the connector text is the filter.and label"
    );
    assert!(
        rt.find("between-inverted").is_none(),
        "1..9 is not inverted, so no warning is drawn"
    );
}

#[test]
fn between_warns_when_the_lower_bound_is_above_the_upper_bound() {
    let rt = runtime(
        Probe {
            lower: TextEdit::new("9"),
            upper: TextEdit::new("2"),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert_eq!(
        element_text(&rt, "between-inverted").as_deref(),
        Some(t("filter.betweenInverted")),
        "9..2 shows the inverted warning"
    );
}

#[test]
fn between_leaves_non_comparable_bounds_alone() {
    let rt = runtime(
        Probe {
            lower: TextEdit::new("alice"),
            upper: TextEdit::new("bob"),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert!(
        rt.find("between-inverted").is_none(),
        "two non-comparable strings must not warn"
    );
}

#[test]
fn inverted_compares_numbers_and_timestamps_only() {
    assert!(is_inverted("9", "2"));
    assert!(!is_inverted("1", "9"));
    assert!(!is_inverted("", "9"), "an empty bound is not comparable");
    assert!(is_inverted("2026-05-02", "2026-05-01"));
    assert!(is_inverted("2026-05-01 12:00", "2026-05-01 11:00"));
    assert!(!is_inverted("2026-05-02T10:00", "2026-05-02 10:00"));
    assert!(!is_inverted("not a date", "2026-05-01"));
}

#[test]
fn chip_shows_the_condition_summary_and_fires_both_actions() {
    let mut rt = runtime(
        Probe {
            scene: Scene::Chip,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    assert!(
        texts
            .iter()
            .any(|label| label.contains("name") && label.contains("alice")),
        "the chip summary reads the condition: {texts:?}"
    );
    click(&mut rt, "chip-edit");
    assert_eq!(
        rt.state.edits, 1,
        "pressing the summary edits the condition"
    );
    click(&mut rt, "chip-remove");
    assert_eq!(rt.state.removes, 1, "pressing the X removes the condition");
}

#[test]
fn chip_summary_shapes_match_the_react_operator_kinds() {
    let null = FilterCondition::new("deleted_at", "isNull", FilterValue::None);
    assert_eq!(
        condition_summary(&null),
        format!("deleted_at {}", t("filter.isNull"))
    );
    let pair = FilterCondition::new(
        "age",
        "between",
        FilterValue::Pair("18".to_owned(), "30".to_owned()),
    );
    assert_eq!(
        condition_summary(&pair),
        format!("age {} 18, 30", t("filter.between"))
    );
    let tokens = FilterCondition::new(
        "status",
        "in",
        FilterValue::Tokens(vec!["paid".to_owned(), "sent".to_owned()]),
    );
    assert_eq!(
        condition_summary(&tokens),
        format!("status {} paid, sent", t("filter.in"))
    );
    let empty = FilterCondition::new("name", "eq", FilterValue::Single(String::new()));
    assert!(condition_summary(&empty).ends_with('…'));
}

#[test]
fn disabled_chip_drops_both_actions() {
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
    assert_eq!(rt.state.edits, 0, "a disabled chip does not edit");
    assert_eq!(rt.state.removes, 0, "a disabled chip does not remove");
}

#[test]
fn connector_toggle_reports_which_half_was_pressed() {
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
    assert_eq!(rt.state.ors, 1, "pressing OR reports OR");
    assert_eq!(rt.state.ands, 0);
    click(&mut rt, "connector-and");
    assert_eq!(rt.state.ands, 1, "pressing AND reports AND");
}

#[test]
fn connector_hides_its_actions_when_disabled() {
    let mut rt = runtime(
        Probe {
            scene: Scene::Toggle,
            disabled: true,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    click(&mut rt, "connector-and");
    click(&mut rt, "connector-or");
    assert_eq!(rt.state.ands + rt.state.ors, 0);
}

#[test]
fn every_part_stays_inside_both_window_sizes_and_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(
                Probe {
                    scene: Scene::All,
                    ..Probe::default()
                },
                size,
                dark,
            );
            let (width, height) = size;
            for element in rt.elements() {
                let [x, y, w, h] = element.rect;
                let key = element
                    .key
                    .clone()
                    .unwrap_or_else(|| "<unnamed>".to_owned());
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                    "{key} rect [{x}, {y}, {w}, {h}] leaves {width}x{height} (dark={dark})"
                );
            }
        }
    }
}
