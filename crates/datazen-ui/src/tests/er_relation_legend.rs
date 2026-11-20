//! T349: `ErRelationLegend` port — labels/counts, swatch strokes, toggle
//! behavior and geometry at 960×640 / 1440×816 in both palettes. Observations
//! come from the runtime element tree, not from the component's own constants.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::ui::kit::kit;
use crate::views::er::er_relation_legend::{
    er_relation_legend, ErRelationLegendVm, DECLARED_KEY, PREDICTED_COUNT_KEY, PREDICTED_DASH,
    PREDICTED_KEY, ROOT_KEY, TOGGLE_KEY,
};
use crate::i18n::t;

#[derive(Clone)]
struct Probe {
    vm: ErRelationLegendVm,
    dark: bool,
    toggles: Vec<bool>,
}

fn probe(enabled: bool) -> Probe {
    Probe {
        vm: ErRelationLegendVm {
            declared_count: 7,
            predicted_count: 3,
            prediction_enabled: enabled,
        },
        dark: true,
        toggles: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let legend = er_relation_legend(
        &s.vm,
        s.dark,
        Some(Rc::new(|st: &mut Probe, enabled| st.toggles.push(enabled))),
    );
    col().w_full().h_full().child(legend)
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

#[test]
fn legend_names_both_kinds_and_counts_them_separately() {
    let rt = runtime(probe(true), (1440.0, 816.0));
    for key in [
        ROOT_KEY,
        "er-legend-title-text",
        DECLARED_KEY,
        "er-legend-declared-label",
        "er-legend-declared-count",
        PREDICTED_KEY,
        "er-legend-predicted-label",
        PREDICTED_COUNT_KEY,
        TOGGLE_KEY,
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in [
        "erDiagram.legendTitle",
        "erDiagram.legendDeclared",
        "erDiagram.legendPredicted",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: exact translated label");
    }
    // The inferred count is disclosed on its own, never folded into the total.
    assert_eq!(element(&rt, "er-legend-declared-count").text.as_deref(), Some("7"));
    assert_eq!(element(&rt, PREDICTED_COUNT_KEY).text.as_deref(), Some("3"));
    assert!(!rendered.iter().any(|text| text == "10"), "no combined total is shown");
}

#[test]
fn swatches_use_the_strokes_the_canvas_draws() {
    let rt = runtime(probe(true), (1440.0, 816.0));
    let k = kit(true);
    let declared = element(&rt, "er-legend-declared-swatch-line");
    assert_eq!(declared.fill, k.c.text_accent, "declared swatch uses the accent stroke");
    assert!(!has(&rt, "er-legend-predicted-swatch-line"), "predicted swatch is not solid");

    for index in 0..4 {
        let dash = element(&rt, &format!("er-legend-predicted-dash-{index}"));
        assert_eq!(dash.fill, k.c.warning.fg, "dash {index} uses the warning stroke");
        assert_eq!(dash.rect[2], PREDICTED_DASH.0, "dash {index} is 4pt long");
    }
    let first = element(&rt, "er-legend-predicted-dash-0").rect;
    let second = element(&rt, "er-legend-predicted-dash-1").rect;
    assert_eq!(
        second[0] - (first[0] + first[2]),
        PREDICTED_DASH.1,
        "the gap between dashes is 3pt"
    );
}

#[test]
fn prediction_off_offers_to_turn_on_and_reports_true() {
    let mut rt = runtime(probe(false), (960.0, 640.0));
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == t("erDiagram.predictionOff")), "off label");
    assert!(!rendered.iter().any(|text| text == t("erDiagram.predictionOn")), "no on label");
    click(&mut rt, TOGGLE_KEY);
    assert_eq!(rt.state.toggles, [true], "the host is asked to enable prediction");
}

#[test]
fn prediction_on_reports_pressed_and_turns_off() {
    let mut rt = runtime(probe(true), (960.0, 640.0));
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == t("erDiagram.predictionOn")), "on label");
    assert!(!rendered.iter().any(|text| text == t("erDiagram.predictionOff")), "no off label");
    click(&mut rt, TOGGLE_KEY);
    assert_eq!(rt.state.toggles, [false], "the host is asked to disable prediction");
}

#[test]
fn legend_fits_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe(dark);
            p.dark = dark;
            let rt = runtime(p, size);
            let outside: Vec<String> = rt
                .elements()
                .iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(
                outside.is_empty(),
                "legend dark={dark} {size:?}: {} node(s) outside: {outside:?}",
                outside.len()
            );
        }
    }
}
