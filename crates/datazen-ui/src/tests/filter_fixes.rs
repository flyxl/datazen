//! T021/T023 fixes from the filter-parts audit: the chip hugs its content,
//! truncates inside a narrow filter column, and keeps React's accent-ink X.
//! Also pins the low-risk items: the literal `NULL` value, the inverted
//! warning's tooltip, and the connector's pressed state.

use rune::{col, Cx, El, ElementInfo, Env, Runtime, TextEdit};

use crate::components::filter::{
    FilterBetweenInput, FilterCondition, FilterConditionChip, FilterConnector,
    FilterConnectorToggle, FilterValue,
};
use crate::i18n::t;
use crate::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scene {
    #[default]
    Wide,
    Narrow,
    Between,
    Toggle,
}

struct Probe {
    scene: Scene,
    condition: FilterCondition,
    connector: FilterConnector,
    lower: TextEdit,
    upper: TextEdit,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            scene: Scene::Wide,
            condition: FilterCondition::new("name", "eq", FilterValue::Single("alice".to_owned())),
            connector: FilterConnector::And,
            lower: TextEdit::new("2025-01-01"),
            upper: TextEdit::new("2024-12-31"),
        }
    }
}

fn chip(state: &Probe, dark: bool) -> El {
    FilterConditionChip::new("chip", state.condition.clone()).build(dark)
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut page = col().key("page").w_full().h_full().padding(PAD).gap(8.0);
    match state.scene {
        Scene::Wide => {
            page = page.child(col().key("wide").w(900.0).gap(8.0).child(chip(state, dark)));
        }
        Scene::Narrow => {
            page = page.child(
                col()
                    .key("narrow")
                    .w(360.0)
                    .gap(8.0)
                    .child(chip(state, dark)),
            );
        }
        Scene::Between => {
            page = page.child(
                FilterBetweenInput::new(
                    "between",
                    &state.lower,
                    &state.upper,
                    |state: &mut Probe| &mut state.lower,
                    |state: &mut Probe| &mut state.upper,
                )
                .build(dark),
            );
        }
        Scene::Toggle => {
            page = page.child(
                FilterConnectorToggle::new("connector", 0, state.connector)
                    .explicit(true)
                    .build(dark),
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
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn info<'a>(elements: &'a [ElementInfo], key: &str) -> &'a ElementInfo {
    elements
        .iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn rect(rt: &Runtime<Probe>, key: &str) -> [f32; 4] {
    let elements = rt.elements();
    if let Some(found) = elements.iter().find(|e| e.key.as_deref() == Some(key)) {
        return found.rect;
    }
    let keys: Vec<Option<String>> = elements.iter().map(|e| e.key.clone()).collect();
    panic!("{key} is laid out; keys={keys:?}");
}

fn long_condition() -> Probe {
    Probe {
        condition: FilterCondition::new("name", "eq", FilterValue::Single("a".repeat(120))),
        ..Probe::default()
    }
}

#[test]
fn a_short_chip_hugs_its_content_in_a_wide_column() {
    let rt = runtime(Probe::default(), DESIGN, true);
    let chip = rect(&rt, "chip");
    assert!(
        chip[2] < 300.0,
        "a short chip is content-sized, not max_w(560): {chip:?}"
    );
    assert!(chip[2] > 60.0, "the pill still holds its summary: {chip:?}");
}

#[test]
fn a_long_chip_stays_under_the_pill_limit() {
    let rt = runtime(long_condition(), DESIGN, true);
    let chip = rect(&rt, "chip");
    let edit = rect(&rt, "chip-edit");
    assert!(
        chip[2] <= 560.5,
        "the pill keeps React's max width: {chip:?}"
    );
    assert!(
        edit[2] <= 300.5,
        "the summary truncates below the column budget: {edit:?}"
    );
}

#[test]
fn a_long_chip_stays_inside_a_narrow_filter_column() {
    let rt = runtime(
        Probe {
            scene: Scene::Narrow,
            ..long_condition()
        },
        MIN,
        true,
    );
    let narrow = rect(&rt, "narrow");
    let chip = rect(&rt, "chip");
    let right = narrow[0] + narrow[2] + 0.5;
    assert!(
        chip[0] + chip[2] <= right,
        "the chip stays inside the 360pt column: chip {chip:?}, column {narrow:?}"
    );
    let edit = rect(&rt, "chip-edit");
    assert!(
        edit[2] < 300.5,
        "the summary is truncated, not overflowing: {edit:?}"
    );
}

#[test]
fn a_short_chip_stays_inside_a_narrow_filter_column() {
    let rt = runtime(
        Probe {
            scene: Scene::Narrow,
            ..Probe::default()
        },
        MIN,
        true,
    );
    let narrow = rect(&rt, "narrow");
    let chip = rect(&rt, "chip");
    assert!(
        chip[0] + chip[2] <= narrow[0] + narrow[2] + 0.5,
        "chip {chip:?} inside column {narrow:?}"
    );
}

#[test]
fn the_remove_icon_uses_the_react_accent_ink() {
    let rt = runtime(Probe::default(), DESIGN, true);
    let k = kit(true);
    let elements = rt.elements();
    let icon = elements
        .iter()
        .find(|e| e.path.ends_with("chip-remove/icon"))
        .expect("the X icon child is laid out");
    assert_eq!(
        icon.text_color,
        Some(k.alpha(k.c.text_accent, 0.8)),
        "React's text-accent/80, not GhostMuted"
    );
    let remove = info(&elements, "chip-remove");
    assert_eq!((remove.rect[2], remove.rect[3]), (16.0, 16.0));
    assert_eq!(remove.label.as_deref(), Some(t("filter.remove")));
}

#[test]
fn a_null_literal_is_shown_as_typed() {
    let state = Probe {
        condition: FilterCondition::new("name", "eq", FilterValue::Single("NULL".to_owned())),
        ..Probe::default()
    };
    let rt = runtime(state, DESIGN, true);
    let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    assert!(
        texts.iter().any(|text| text.trim_end().ends_with("NULL")),
        "the typed NULL is a value, not the empty placeholder: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains('…')),
        "no empty-value ellipsis for a literal NULL: {texts:?}"
    );
}

#[test]
fn an_inverted_between_warning_carries_a_tooltip() {
    let rt = runtime(
        Probe {
            scene: Scene::Between,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let elements = rt.elements();
    let warning = info(&elements, "between-inverted");
    assert_eq!(warning.text.as_deref(), Some(t("filter.betweenInverted")));
    assert_eq!(
        warning.help.as_deref(),
        Some(t("filter.betweenInverted")),
        "React's title= becomes Rune's help tooltip"
    );
}

#[test]
fn the_connector_exposes_its_pressed_state() {
    let rt = runtime(
        Probe {
            scene: Scene::Toggle,
            connector: FilterConnector::And,
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    let elements = rt.elements();
    assert!(
        info(&elements, "connector-and").item_selected,
        "AND is the pressed segment"
    );
    assert!(
        !info(&elements, "connector-or").item_selected,
        "OR is not pressed"
    );
}

#[test]
fn the_fixed_chip_stays_inside_both_windows() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(long_condition(), size, dark);
            let chip = rect(&rt, "chip");
            assert!(
                chip[0] >= -0.5
                    && chip[1] >= -0.5
                    && chip[0] + chip[2] <= size.0 + 0.5
                    && chip[1] + chip[3] <= size.1 + 0.5,
                "chip {chip:?} leaves {size:?} (dark={dark})"
            );
        }
    }
}
