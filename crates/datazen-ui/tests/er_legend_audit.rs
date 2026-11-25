//! T349 independent audit: `ErRelationLegend` vs React.
//!
//! Probes branches the coder's suite does not: the dimmed predicted row when
//! prediction is off, zero/large counts, an absent toggle handler, shared
//! constant values, and a rect-based geometry stress at both windows/palettes.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::er::er_relation_legend::{
    er_relation_legend, ErRelationLegendVm, PREDICTED_COUNT_KEY, PREDICTED_DASH,
    PREDICTED_KEY, ROOT_KEY, TOGGLE_KEY,
};

#[derive(Clone)]
struct Probe {
    vm: ErRelationLegendVm,
    dark: bool,
    toggles: Vec<bool>,
    bind_toggle: bool,
}

fn probe(enabled: bool, declared: usize, predicted: usize) -> Probe {
    Probe {
        vm: ErRelationLegendVm {
            declared_count: declared,
            predicted_count: predicted,
            prediction_enabled: enabled,
        },
        dark: true,
        toggles: Vec::new(),
        bind_toggle: true,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let on_toggle = if s.bind_toggle {
        Some(Rc::new(|st: &mut Probe, v: bool| st.toggles.push(v)) as Rc<dyn Fn(&mut Probe, bool)>)
    } else {
        None
    };
    col().w_full().h_full().child(er_relation_legend(&s.vm, s.dark, on_toggle))
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

/// React dims the whole predicted row (`opacity-60`) when prediction is off.
/// The Rune port must dim the row's elements, not only its count.
#[test]
fn predicted_row_is_dimmed_when_prediction_is_off() {
    let rt = runtime(probe(false, 1, 1), (1440.0, 816.0));
    let row_opacity = element(&rt, PREDICTED_KEY).opacity;
    let swatch_opacity = element(&rt, "er-legend-predicted-dash-0").opacity;
    let label_opacity = element(&rt, "er-legend-predicted-label").opacity;
    let count_opacity = element(&rt, PREDICTED_COUNT_KEY).opacity;
    assert!((row_opacity - 0.6).abs() <= 0.01, "the predicted row itself is dimmed to 60%");
    assert!((swatch_opacity - 0.6).abs() <= 0.01, "the predicted swatch inherits the dim");
    assert!((label_opacity - 0.6).abs() <= 0.01, "the predicted label inherits the dim");
    assert!((count_opacity - 0.6).abs() <= 0.01, "the predicted count is dimmed");
}

/// React keeps the row fully opaque when prediction is on.
#[test]
fn predicted_row_is_opaque_when_prediction_is_on() {
    let rt = runtime(probe(true, 1, 1), (1440.0, 816.0));
    for key in [PREDICTED_KEY, "er-legend-predicted-dash-0", "er-legend-predicted-label", PREDICTED_COUNT_KEY] {
        assert!((element(&rt, key).opacity - 1.0).abs() <= 0.01, "{key} is full opacity when on");
    }
}

/// Zero and large counts are rendered verbatim as separate numbers.
#[test]
fn zero_and_large_counts_render_verbatim() {
    let rt = runtime(probe(true, 0, 0), (960.0, 640.0));
    assert_eq!(element(&rt, "er-legend-declared-count").text.as_deref(), Some("0"));
    assert_eq!(element(&rt, PREDICTED_COUNT_KEY).text.as_deref(), Some("0"));

    let rt = runtime(probe(true, 123456, 98765), (960.0, 640.0));
    assert_eq!(element(&rt, "er-legend-declared-count").text.as_deref(), Some("123456"));
    assert_eq!(element(&rt, PREDICTED_COUNT_KEY).text.as_deref(), Some("98765"));
}

/// Without a host callback the toggle is inert but never panics.
#[test]
fn no_toggle_handler_is_inert() {
    let mut p = probe(false, 1, 0);
    p.bind_toggle = false;
    let mut rt = runtime(p, (960.0, 640.0));
    click(&mut rt, TOGGLE_KEY);
    assert!(rt.state.toggles.is_empty(), "no request without a handler");
}

/// The shared canvas constants are the exact React values.
#[test]
fn shared_constants_match_react_relation_style() {
    assert_eq!(PREDICTED_DASH, (4.0, 3.0));
    assert_eq!(datazen_ui::views::er::er_relation_legend::PANEL_WIDTH, 224.0);
    assert_eq!(ROOT_KEY, "er-diagram-legend");
    assert_eq!(TOGGLE_KEY, "er-diagram-toggle-prediction");
}

/// Large counts still fit both windows in both palettes (rect observation).
#[test]
fn legend_with_large_counts_fits_both_windows_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe(true, 123456, 987654);
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
            assert!(outside.is_empty(), "dark={dark} {size:?}: {outside:?}");
        }
    }
}
