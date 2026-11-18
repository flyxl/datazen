//! T008: `ChartCanvas` dispatcher, measured viewport and honest placeholders.
//!
//! Observations come from `rt.elements()` (runtime-measured rects/text/keys),
//! not from the component's self-report. The measured-rect probe is the one
//! exception where an `on_measured` callback records into app state; that is
//! cross-checked against the independently measured element rect.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, text, Cx, El, Env, Runtime};

use crate::components::chart::chart_canvas::{
    renderer_note_key, ChartCanvas, ChartConfig, ChartDataPoint, ChartRenderContext, ChartRenderer,
    ChartType, ALL_CHART_TYPES, CANVAS_CONTAINER_KEY, CANVAS_KEY, PAD_COMPACT, PAD_FULL,
};
use crate::ui::kit::kit;

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

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_containing<S>(rt: &Runtime<S>, needle: &str) -> Option<String> {
    rt.elements().iter().find_map(|e| {
        let t = e.text.as_deref()?;
        t.contains(needle).then(|| t.to_string())
    })
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn sample_data() -> Vec<ChartDataPoint> {
    vec![
        ChartDataPoint::new()
            .with(
                "label",
                crate::components::chart::chart_canvas::ChartValue::Text("a".into()),
            )
            .with(
                "value",
                crate::components::chart::chart_canvas::ChartValue::Number(3.0),
            ),
        ChartDataPoint::new()
            .with(
                "label",
                crate::components::chart::chart_canvas::ChartValue::Text("b".into()),
            )
            .with(
                "value",
                crate::components::chart::chart_canvas::ChartValue::Number(7.0),
            ),
    ]
}

// ---- Probe view ------------------------------------------------------------

#[derive(Clone, Default)]
struct Scenario {
    config: ChartConfig,
    data: Vec<ChartDataPoint>,
    dark: bool,
    compact: bool,
    height: Option<f32>,
    seen: Option<Rc<RefCell<Vec<ChartType>>>>,
}

#[derive(Clone, Default)]
struct ChartState {
    scenario: Scenario,
    measured: Option<[f32; 4]>,
    clicks: Vec<usize>,
}

fn canvas_view(s: &ChartState, _cx: &mut Cx<ChartState>) -> El {
    let sc = &s.scenario;
    let mut canvas = ChartCanvas::<ChartState>::new(&sc.config, &sc.data)
        .dark(sc.dark)
        .compact(sc.compact)
        .on_measured(|st: &mut ChartState, r: [f32; 4]| st.measured = Some(r))
        .on_data_point_click(|st: &mut ChartState, idx: usize| st.clicks.push(idx));
    if let Some(h) = sc.height {
        canvas = canvas.height(h);
    }
    if let Some(seen) = &sc.seen {
        let seen = seen.clone();
        canvas = canvas.renderer_factory(move |t| {
            seen.borrow_mut().push(t);
            Box::new(InjectedRenderer)
        });
    }
    col().w_full().h_full().child(canvas.build())
}

/// A stand-in for a real renderer, used to prove the factory seam injects the
/// five concrete renderers later without touching `chart_canvas.rs`.
struct InjectedRenderer;
impl<S: 'static> ChartRenderer<S> for InjectedRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let k = kit(ctx.dark);
        col()
            .key("injected-renderer")
            .w_full()
            .h_full()
            .center()
            .child(
                text("injected")
                    .key("injected-renderer-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    }
}

fn runtime(sc: Scenario, size: (f32, f32)) -> Runtime<ChartState> {
    let dark = sc.dark;
    let mut rt = Runtime::new(
        ChartState {
            scenario: sc,
            ..ChartState::default()
        },
        canvas_view,
        size,
        env(dark),
    );
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn scenario(chart_type: ChartType, dark: bool) -> Scenario {
    let mut config = ChartConfig::default();
    config.chart_type = chart_type;
    Scenario {
        config,
        data: sample_data(),
        dark,
        ..Scenario::default()
    }
}

// ---- Tests -----------------------------------------------------------------

#[test]
fn each_chart_type_selects_its_own_placeholder_renderer() {
    for chart_type in ALL_CHART_TYPES {
        let rt = runtime(scenario(chart_type, true), (960.0, 640.0));
        let own = renderer_note_key(chart_type);
        assert!(has_key(&rt, own), "{chart_type:?} must select {own}");
        assert!(
            has_key(&rt, &format!("{own}-text")),
            "{own} must carry a stable marker text key"
        );
        for other in ALL_CHART_TYPES {
            if other == chart_type {
                continue;
            }
            assert!(
                !has_key(&rt, renderer_note_key(other)),
                "{chart_type:?} must not mount the renderer for {other:?}"
            );
        }
        let note = text_containing(&rt, "renderer not ported")
            .unwrap_or_else(|| panic!("{chart_type:?} renders an honest note"));
        assert!(
            note.contains(&format!(
                "{} renderer not ported",
                chart_type_label(chart_type)
            )),
            "note must name the selected engine, got {note:?}"
        );
        assert!(has_key(&rt, CANVAS_KEY), "outer container present");
        assert!(
            has_key(&rt, CANVAS_CONTAINER_KEY),
            "measured container present"
        );
    }
}

fn chart_type_label(t: ChartType) -> &'static str {
    match t {
        ChartType::Bar => "Bar",
        ChartType::Line => "Line",
        ChartType::Pie => "Pie",
        ChartType::Scatter => "Scatter",
        ChartType::Area => "Area",
    }
}

#[test]
fn empty_data_still_dispatches_without_fake_graphics() {
    for chart_type in ALL_CHART_TYPES {
        let mut sc = scenario(chart_type, true);
        sc.data = Vec::new();
        sc.config.y_axes = Vec::new();
        let rt = runtime(sc, (960.0, 640.0));
        assert!(
            has_key(&rt, renderer_note_key(chart_type)),
            "note survives empty data"
        );
        assert!(
            text_containing(&rt, "not ported").is_some(),
            "no fabricated chart marks for empty data"
        );
        assert_in_window(&rt, 960.0, 640.0, "empty data, 960x640");
    }
}

#[test]
fn the_container_respects_the_configured_height() {
    for height in [120.0, 240.0, 360.0] {
        let mut sc = scenario(ChartType::Bar, true);
        sc.height = Some(height);
        let rt = runtime(sc, (1440.0, 816.0));
        let r = element(&rt, CANVAS_CONTAINER_KEY).rect;
        assert!(
            (r[3] - height).abs() <= 0.5,
            "container height must be the configured {height}, got {:?}",
            r
        );
        assert!(r[3] > 0.0, "container has a positive measured height");
    }
}

#[test]
fn the_container_fills_the_available_width() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = runtime(scenario(ChartType::Line, true), (w, h));
        let r = element(&rt, CANVAS_CONTAINER_KEY).rect;
        let expected = w - 2.0 * PAD_FULL;
        assert!(
            (r[2] - expected).abs() <= 0.5,
            "container fills width {expected} (window {w} - 2x{PAD_FULL}), got {:?}",
            r
        );
    }
}

#[test]
fn compact_tiles_use_the_smaller_padding() {
    let mut sc = scenario(ChartType::Pie, true);
    sc.compact = true;
    let rt = runtime(sc, (960.0, 640.0));
    let r = element(&rt, CANVAS_CONTAINER_KEY).rect;
    let expected = 960.0 - 2.0 * PAD_COMPACT;
    assert!(
        (r[2] - expected).abs() <= 0.5,
        "compact container fills {expected}, got {:?}",
        r
    );
}

#[test]
fn the_container_reports_its_measured_rect() {
    let rt = runtime(scenario(ChartType::Scatter, true), (1440.0, 816.0));
    let measured = rt.state.measured.expect("on_measured fired during layout");
    let observed = element(&rt, CANVAS_CONTAINER_KEY).rect;
    for i in 0..4 {
        assert!(
            (measured[i] - observed[i]).abs() <= 0.5,
            "measured rect {measured:?} must match the runtime rect {observed:?}"
        );
    }
    assert!(
        measured[2] > 0.0 && measured[3] > 0.0,
        "a real, non-zero rect"
    );
}

#[test]
fn the_canvas_fits_both_window_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for chart_type in ALL_CHART_TYPES {
                let rt = runtime(scenario(chart_type, dark), (w, h));
                assert_in_window(&rt, w, h, &format!("{chart_type:?} dark={dark} {w}x{h}"));
                assert!(
                    has_key(&rt, renderer_note_key(chart_type)),
                    "renderer still selected in dark={dark}"
                );
            }
        }
    }
}

#[test]
fn a_renderer_factory_can_be_injected_later() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut sc = scenario(ChartType::Area, true);
    sc.seen = Some(seen.clone());
    let rt = runtime(sc, (960.0, 640.0));

    assert!(
        has_key(&rt, "injected-renderer"),
        "injected renderer was mounted"
    );
    assert!(
        !has_key(&rt, renderer_note_key(ChartType::Area)),
        "placeholder replaced"
    );
    let seen = seen.borrow();
    assert!(
        seen.iter().any(|t| *t == ChartType::Area),
        "factory received the selected chart type: {seen:?}"
    );
}
