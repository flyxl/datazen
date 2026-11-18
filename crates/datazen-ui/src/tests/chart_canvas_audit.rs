//! Independent audit for T008 `ChartCanvas` (Wave 2, H-class dispatcher).
//!
//! Everything observable comes from `rt.elements()` / `rt.state`, never from a
//! value the component reports about itself. The factory-seam tests drive a
//! real press through the runtime so the `on_data_point_click` plumbing is
//! proven, not merely assumed.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, text, Cx, El, Env, Runtime};

use crate::components::chart::chart_canvas::{
    renderer_note_key, ChartCanvas, ChartConfig, ChartDataPoint, ChartRenderContext, ChartRenderer,
    ChartType, ALL_CHART_TYPES, CANVAS_CONTAINER_KEY, CANVAS_KEY, PAD_COMPACT, PAD_FULL,
};

// ---- Harness ---------------------------------------------------------------

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

fn note_text<S>(rt: &Runtime<S>, chart_type: ChartType) -> Option<String> {
    let key = format!("{}-text", renderer_note_key(chart_type));
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key.as_str()))
        .and_then(|e| e.text.clone())
}

fn keyed_set<S>(rt: &Runtime<S>) -> Vec<String> {
    let mut keys: Vec<String> = rt
        .elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .collect();
    keys.sort();
    keys
}

/// Independently check every laid-out rect against the window. This is the
/// rect-based observation channel required by the migration discipline: it
/// reads what the runtime measured, not what the component self-reported.
fn assert_no_overflow<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[derive(Clone, Default)]
struct Scenario {
    config: ChartConfig,
    dark: bool,
    compact: bool,
    height: Option<f32>,
    inject: Option<Rc<RefCell<Vec<ChartType>>>>,
    ctx_log: Option<Rc<RefCell<Vec<CtxSnapshot>>>>,
    use_click_renderer: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct CtxSnapshot {
    data_len: usize,
    y_axes: usize,
    compact: bool,
    dark: bool,
    chart_type: ChartType,
}

#[derive(Clone, Default)]
struct AuditState {
    scenario: Scenario,
    measured: Option<[f32; 4]>,
    clicks: Vec<usize>,
}

/// A stand-in renderer that records the context it was handed, so the seam is
/// proven to deliver data/config/theme rather than merely "replace a note".
struct CapturingRenderer {
    log: Rc<RefCell<Vec<CtxSnapshot>>>,
}

impl<S: 'static> ChartRenderer<S> for CapturingRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        self.log.borrow_mut().push(CtxSnapshot {
            data_len: ctx.data.len(),
            y_axes: ctx.config.y_axes.len(),
            compact: ctx.compact,
            dark: ctx.dark,
            chart_type: ctx.config.chart_type,
        });
        col()
            .key("audit-injected")
            .w_full()
            .h_full()
            .center()
            .child(text("audit injected").key("audit-injected-text"))
    }
}

/// A stand-in renderer that forwards a real press to `on_data_point_click`,
/// proving the callback seam is wired through the canvas.
struct ClickRenderer;

impl<S: 'static> ChartRenderer<S> for ClickRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let cb = ctx.on_data_point_click.clone();
        col()
            .key("audit-click-target")
            .w(120.0)
            .h(40.0)
            .on_press(move |s: &mut S| {
                if let Some(cb) = &cb {
                    cb(s, 3);
                }
            })
            .child(text("click").key("audit-click-text"))
    }
}

fn canvas_view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let sc = &s.scenario;
    let mut canvas = ChartCanvas::<AuditState>::new(&sc.config, sc.config_data_slice())
        .dark(sc.dark)
        .compact(sc.compact)
        .on_measured(|st: &mut AuditState, r: [f32; 4]| st.measured = Some(r))
        .on_data_point_click(|st: &mut AuditState, idx: usize| st.clicks.push(idx));
    if let Some(h) = sc.height {
        canvas = canvas.height(h);
    }
    if sc.use_click_renderer {
        canvas = canvas.renderer_factory(|_t| Box::new(ClickRenderer));
    } else if let (Some(seen), Some(log)) =
        (sc.inject.clone(), sc.ctx_log.clone())
    {
        canvas = canvas.renderer_factory(move |t| {
            seen.borrow_mut().push(t);
            Box::new(CapturingRenderer {
                log: log.clone(),
            })
        });
    }
    col().w_full().h_full().child(canvas.build())
}

impl Scenario {
    /// Placeholder data lives on the state, not the scenario; the view uses a
    /// static two-row table so the context's data_len is deterministic.
    fn config_data_slice(&self) -> &'static [ChartDataPoint] {
        // Two rows, created once, leaked for the audit process lifetime. This
        // keeps the view's borrow simple while giving the context real data.
        use crate::components::chart::chart_canvas::ChartValue;
        static DATA: std::sync::OnceLock<Vec<ChartDataPoint>> = std::sync::OnceLock::new();
        DATA.get_or_init(|| {
            vec![
                ChartDataPoint::new().with("label", ChartValue::Text("a".into())),
                ChartDataPoint::new().with("value", ChartValue::Number(3.0)),
            ]
        })
        .as_slice()
    }
}

fn runtime(sc: Scenario, size: (f32, f32)) -> Runtime<AuditState> {
    let dark = sc.dark;
    let mut rt = Runtime::new(
        AuditState {
            scenario: sc,
            ..AuditState::default()
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
    config.y_axes = vec!["value".to_string()];
    Scenario {
        config,
        dark,
        ..Scenario::default()
    }
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---- Audits ----------------------------------------------------------------

#[test]
fn marker_keys_are_unique_stable_and_the_notes_are_honest() {
    for chart_type in ALL_CHART_TYPES {
        let rt = runtime(scenario(chart_type, true), (960.0, 640.0));
        let own = renderer_note_key(chart_type);

        // Own note and its text key exist, and no other chart type's note does.
        assert!(has_key(&rt, own), "{chart_type:?} must mount {own}");
        assert!(
            has_key(&rt, &format!("{own}-text")),
            "{own} must carry a stable text key"
        );
        for other in ALL_CHART_TYPES {
            if other != chart_type {
                assert!(
                    !has_key(&rt, renderer_note_key(other)),
                    "{chart_type:?} mounted the note for {other:?}"
                );
            }
        }

        // Keys are unique across the whole tree (no silent collisions).
        let keys = keyed_set(&rt);
        let unique: std::collections::BTreeSet<_> = keys.iter().cloned().collect();
        assert_eq!(keys.len(), unique.len(), "duplicate keys: {keys:?}");

        // The only keyed nodes are the two containers and the honest note pair.
        let expected = {
            let mut v = vec![
                CANVAS_KEY.to_string(),
                CANVAS_CONTAINER_KEY.to_string(),
                own.to_string(),
                format!("{own}-text"),
            ];
            v.sort();
            v
        };
        assert_eq!(
            keys, expected,
            "no fabricated chart graphics may be mounted: {keys:?}"
        );

        // The note claims non-porting honestly; it must not pretend to draw.
        let note = note_text(&rt, chart_type)
            .unwrap_or_else(|| panic!("{chart_type:?} renders a note"));
        let label = match chart_type {
            ChartType::Bar => "Bar",
            ChartType::Line => "Line",
            ChartType::Pie => "Pie",
            ChartType::Scatter => "Scatter",
            ChartType::Area => "Area",
        };
        assert_eq!(
            note,
            format!("{label} renderer not ported"),
            "note must be honest and name the engine"
        );
    }
}

#[test]
fn chart_type_vm_maps_every_string_and_falls_back_to_bar() {
    for t in ALL_CHART_TYPES {
        assert_eq!(ChartType::from_str(t.as_str()), t);
    }
    for unknown in ["donut", "", "BAR", "unknown"] {
        assert_eq!(
            ChartType::from_str(unknown),
            ChartType::Bar,
            "unknown '{unknown}' must fall back to the React default"
        );
    }
}

#[test]
fn injection_replaces_the_placeholder_and_keeps_the_canvas_contract() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut sc = scenario(ChartType::Area, false);
    sc.inject = Some(seen.clone());
    sc.ctx_log = Some(log.clone());
    sc.compact = true;
    let rt = runtime(sc, (1440.0, 816.0));

    // Placeholder replaced, injected marker present.
    assert!(has_key(&rt, "audit-injected"), "injected renderer mounted");
    for t in ALL_CHART_TYPES {
        assert!(
            !has_key(&rt, renderer_note_key(t)),
            "no placeholder for {t:?} survives injection"
        );
    }

    // Canvas keys remain stable under injection.
    assert!(has_key(&rt, CANVAS_KEY), "outer key stable");
    assert!(has_key(&rt, CANVAS_CONTAINER_KEY), "viewport key stable");

    // State still flows: measured rect from on_layout_tx matches the
    // independently measured element rect.
    let measured = rt.state.measured.expect("on_measured still fires");
    let observed = element(&rt, CANVAS_CONTAINER_KEY).rect;
    for i in 0..4 {
        assert!(
            (measured[i] - observed[i]).abs() <= 0.5,
            "measured {measured:?} != observed {observed:?}"
        );
    }

    // Factory received the selected type; context carried real data/config.
    {
        let seen = seen.borrow();
        assert!(
            seen.iter().any(|t| *t == ChartType::Area),
            "factory saw the selected type: {seen:?}"
        );
    }
    let log = log.borrow();
    assert!(!log.is_empty(), "renderer received a context");
    for snap in log.iter() {
        assert_eq!(*snap, CtxSnapshot {
            data_len: 2,
            y_axes: 1,
            compact: true,
            dark: false,
            chart_type: ChartType::Area,
        }, "context must carry data/config/theme: {snap:?}");
    }
}

#[test]
fn injected_renderer_can_forward_a_data_point_click() {
    let rt = runtime(
        {
            let mut sc = scenario(ChartType::Bar, true);
            sc.use_click_renderer = true;
            sc
        },
        (960.0, 640.0),
    );

    assert!(has_key(&rt, "audit-click-target"));
    assert!(!has_key(&rt, renderer_note_key(ChartType::Bar)));

    let mut rt = rt;
    click(&mut rt, "audit-click-target");
    assert_eq!(
        rt.state.clicks,
        vec![3],
        "on_data_point_click must reach app state with the row index"
    );
}

#[test]
fn compact_canvas_fits_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for chart_type in ALL_CHART_TYPES {
                let mut sc = scenario(chart_type, dark);
                sc.compact = true;
                let rt = runtime(sc, (w, h));
                assert_no_overflow(&rt, w, h, &format!("compact {chart_type:?} dark={dark}"));
                // React `p-1` is 4pt; anchor to the literal, not the port's constant.
                assert_eq!(PAD_COMPACT, 4.0, "p-1 must be 4pt");
                let r = element(&rt, CANVAS_CONTAINER_KEY).rect;
                assert!(
                    (r[2] - (w - 8.0)).abs() <= 0.5,
                    "compact width must be w-8 (p-1 = 4pt each side), got {r:?}"
                );
            }
        }
    }
}

#[test]
fn a_configured_height_still_fits_both_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for chart_type in ALL_CHART_TYPES {
                let mut sc = scenario(chart_type, dark);
                sc.height = Some(240.0);
                let rt = runtime(sc, (w, h));
                assert_no_overflow(&rt, w, h, &format!("height {chart_type:?} dark={dark}"));
                // React `p-4` is 16pt; anchor to the literal, not the port's constant.
                assert_eq!(PAD_FULL, 16.0, "p-4 must be 16pt");
                let r = element(&rt, CANVAS_CONTAINER_KEY).rect;
                assert!((r[3] - 240.0).abs() <= 0.5, "height must be 240, got {r:?}");
                assert!(
                    (r[2] - (w - 32.0)).abs() <= 0.5,
                    "width must still fill w-32 (p-4 = 16pt each side) with a height override, got {r:?}"
                );
            }
        }
    }
}

#[test]
fn product_source_has_no_recharts_dependency_and_no_raw_oklch() {
    let src = include_str!("../components/chart/chart_canvas.rs");
    // Doc comments mention recharts/oklch to explain the port; only actual code
    // matters here.
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("recharts::") && !code.contains("use recharts"),
        "recharts must not be used in product code"
    );
    assert!(!code.contains("oklch("), "no raw oklch in product code");

    let manifest = include_str!("../../Cargo.toml");
    assert!(
        !manifest.lines().any(|l| l.contains("recharts")),
        "no recharts crate dependency"
    );
}
