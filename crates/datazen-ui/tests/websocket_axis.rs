//! T300: `WebSocketTunnelFields` and `AxisConfigurator` ports — keys, mode
//! validation/normalization, axis config branches, config edits and geometry at
//! 960×640 / 1440×816 in both palettes. Observations come from the runtime
//! element tree and from the host-state callbacks, never from component self
//! reports.

use std::collections::HashMap;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use datazen_ui::components::chart::axis_configurator::{AxisRecommendation, AxisConfigurator, COLOR_PALETTES};
use datazen_ui::components::chart::chart_canvas::{
    AggregationType, ChartConfig, ChartField, ChartSort, ChartType, InferredFieldType,
};
use datazen_ui::components::connection::websocket_tunnel_fields::{
    is_ws_url_valid, WebSocketTunnelFields, WebSocketValue, WsMode,
};
use datazen_ui::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Probe {
    dark: bool,
    ws: WebSocketValue,
    ws_modes: Vec<WsMode>,
    fields: Vec<ChartField>,
    config: ChartConfig,
    recommendation: Option<AxisRecommendation>,
    collapsed: bool,
    changes: Vec<ChartConfig>,
    select_open: HashMap<String, bool>,
}

fn field(name: &str, kind: InferredFieldType) -> ChartField {
    ChartField {
        name: name.to_owned(),
        data_type: String::new(),
        inferred_type: kind,
        distinct_count: None,
        sample_values: Vec::new(),
    }
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            dark: true,
            ws: WebSocketValue {
                url: TextEdit::new("wss://relay.example.com/v1".to_owned()),
                mode: WsMode::DatazenV1,
                mode_open: false,
                timeout: TextEdit::new("30".to_owned()),
                auth_token: TextEdit::new(String::new()),
            },
            ws_modes: Vec::new(),
            fields: vec![
                field("revenue", InferredFieldType::Numeric),
                field("orders", InferredFieldType::Numeric),
                field("region", InferredFieldType::Categorical),
                field("day", InferredFieldType::Datetime),
            ],
            config: ChartConfig::default(),
            recommendation: None,
            collapsed: false,
            changes: Vec::new(),
            select_open: HashMap::new(),
        }
    }
}

fn ws_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let ws = WebSocketTunnelFields::<Probe>::new(
        &s.ws,
        |st: &mut Probe| &mut st.ws.url,
        |st: &mut Probe| &mut st.ws.timeout,
        |st: &mut Probe| &mut st.ws.auth_token,
    )
    .mode_open(|st: &mut Probe| &mut st.ws.mode_open)
    .on_mode(|st: &mut Probe, mode| st.ws_modes.push(mode))
    .build(s.dark, &mut layer);
    rune_kit::root(col().w_full().h_full().child(ws), layer)
}

fn axis_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let panel = AxisConfigurator::<Probe>::new(
        &s.fields,
        &s.config,
        s.recommendation.as_ref(),
    )
    .collapsed(s.collapsed)
    .on_collapsed(|st: &mut Probe, value| st.collapsed = value)
    .on_change(|st: &mut Probe, config| st.changes.push(config))
    .select_open(s.select_open.clone(), |st: &mut Probe, key: &str| {
        st.select_open.entry(key.to_owned()).or_insert(false)
    })
    .build(s.dark, &mut layer);
    rune_kit::root(col().w_full().h_full().child(panel), layer)
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn runtime(
    view: fn(&Probe, &mut Cx<Probe>) -> El,
    p: Probe,
    size: (f32, f32),
) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, env(dark));
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

fn refresh(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(1.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {:?}: {outside:?}", outside.len(), size);
}

// ---- WebSocket tunnel fields ------------------------------------------------

#[test]
fn ws_fields_pin_testids_labels_values_and_hint() {
    let rt = runtime(ws_view, Probe::default(), DESIGN);
    for key in [
        "new-conn-ws-fields",
        "new-conn-ws-url-block",
        "new-conn-ws-mode-block",
        "new-conn-ws-mode-trigger",
        "new-conn-ws-timeout-block",
        "new-conn-ws-token-block",
        "new-conn-ws-hint",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in [
        "newConn.wsUrl",
        "newConn.wsMode",
        "newConn.wsTimeout",
        "newConn.wsAuthToken",
        "newConn.wsHint",
        "newConn.wsModeDatazen",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: exact translated text");
    }
    assert!(rendered.iter().any(|text| text == "wss://relay.example.com/v1"), "URL value is shown");
    assert!(rendered.iter().any(|text| text == "30"), "timeout value is shown");
    // React renders the auth token as a password input: no readable text node.
    assert!(element(&rt, "new-conn-ws-token").text.is_none(), "token field is secure");
}

#[test]
fn ws_mode_normalizes_unknown_values_to_datazen_v1() {
    assert_eq!(WsMode::from_value("raw_binary"), WsMode::RawBinary);
    assert_eq!(WsMode::from_value("datazen_v1"), WsMode::DatazenV1);
    assert_eq!(WsMode::from_value("bogus"), WsMode::DatazenV1);
    assert_eq!(WsMode::default().value(), "datazen_v1");
    assert_eq!(WsMode::from_value("raw_binary").value(), "raw_binary");
}

#[test]
fn ws_url_validation_follows_the_form_required_rule() {
    assert!(!is_ws_url_valid(""), "empty URL is invalid");
    assert!(!is_ws_url_valid("   "), "whitespace-only URL is invalid");
    assert!(is_ws_url_valid("wss://relay.example.com/v1"), "trimmed URL is valid");
}

#[test]
fn ws_mode_select_opens_and_chooses_raw_binary() {
    let mut rt = runtime(ws_view, Probe::default(), DESIGN);
    click(&mut rt, "new-conn-ws-mode-trigger");
    assert!(rt.state.ws.mode_open, "trigger opens the select");
    click(&mut rt, "option1");
    assert_eq!(rt.state.ws_modes.last(), Some(&WsMode::RawBinary), "raw binary is chosen");
    assert!(!rt.state.ws.mode_open, "choosing closes the select");
}

// ---- Axis configurator ------------------------------------------------------

#[test]
fn axis_default_panel_pins_labels_and_controls() {
    let rt = runtime(axis_view, Probe::default(), DESIGN);
    for key in [
        "chart-axis-configurator",
        "chart-axis-config-header",
        "chart-axis-config-title",
        "chart-axis-x-label",
        "chart-axis-x-select-trigger",
        "chart-axis-y-label",
        "chart-axis-add-series",
        "chart-axis-group-label",
        "chart-axis-group-select-trigger",
        "chart-axis-agg-label",
        "chart-axis-agg-select-trigger",
        "chart-axis-sort-label",
        "chart-axis-sort-select-trigger",
        "chart-axis-color-label",
        "chart-axis-color-options",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in [
        "chart.configPanel",
        "chart.xAxis",
        "chart.yAxis",
        "chart.addSeries",
        "chart.groupBy",
        "chart.aggregation",
        "chart.sortBy",
        "chart.colorScheme",
        "chart.autoIndex",
        "chart.noGroupBy",
        "chart.agg.none",
        "chart.sort.none",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: exact translated text");
    }
    assert_eq!(
        COLOR_PALETTES.len(),
        8,
        "all React palettes are present"
    );
    for (name, _) in COLOR_PALETTES {
        assert!(has(&rt, &format!("chart-axis-color-{name}")), "palette {name} is mounted");
        assert!(has(&rt, &format!("chart-axis-color-{name}-swatch-0")), "palette {name} has a swatch");
    }
    assert!(!has(&rt, "chart-axis-recommendation"), "no recommendation by default");
}

#[test]
fn axis_field_mapping_uses_auto_index_and_numeric_options() {
    let mut p = Probe::default();
    p.config.x_axis = Some("revenue".to_owned());
    p.config.y_axes = vec!["orders".to_owned()];
    let rt = runtime(axis_view, p, DESIGN);
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == "revenue (numeric)"), "X shows the field+type label");
    assert!(rendered.iter().any(|text| text == "orders"), "Y shows the chosen field");
    assert!(!has(&rt, "chart-axis-y-remove-0"), "single Y series has no remove button");
}

#[test]
fn axis_add_and_remove_series_emit_a_whole_config() {
    let mut rt = runtime(axis_view, Probe::default(), DESIGN);
    click(&mut rt, "chart-axis-add-series");
    assert_eq!(
        rt.state.changes.last().map(|c| c.y_axes.clone()),
        Some(vec!["revenue".to_owned()]),
        "add series appends the first unused numeric field"
    );

    rt.state.config.y_axes = vec!["revenue".to_owned(), "orders".to_owned()];
    refresh(&mut rt);
    assert!(has(&rt, "chart-axis-y-remove-0"), "two series expose remove controls");
    click(&mut rt, "chart-axis-y-remove-0");
    assert_eq!(
        rt.state.changes.last().map(|c| c.y_axes.clone()),
        Some(vec!["orders".to_owned()]),
        "remove drops the chosen index"
    );
}

#[test]
fn axis_pie_and_scatter_hide_group_by_and_add_series() {
    for chart_type in [ChartType::Pie, ChartType::Scatter] {
        let mut p = Probe::default();
        p.config.chart_type = chart_type;
        let rt = runtime(axis_view, p, DESIGN);
        assert!(!has(&rt, "chart-axis-group-block"), "{chart_type:?}: group by is hidden");
        assert!(!has(&rt, "chart-axis-add-series"), "{chart_type:?}: add series is hidden");
        assert!(has(&rt, "chart-axis-agg-block"), "{chart_type:?}: aggregation still shows");
    }
}

#[test]
fn axis_collapse_and_expand_toggle_the_panel() {
    let mut rt = runtime(axis_view, Probe::default(), DESIGN);
    click(&mut rt, "chart-axis-config-collapse");
    assert!(rt.state.collapsed, "collapse reports to the host");
    rt.state.collapsed = true;
    refresh(&mut rt);
    assert!(has(&rt, "chart-axis-config-toggle"), "collapsed rail is mounted");
    assert!(!has(&rt, "chart-axis-configurator"), "expanded panel is gone");
    click(&mut rt, "chart-axis-config-toggle");
    assert!(!rt.state.collapsed, "expand reports to the host");
}

#[test]
fn axis_select_edits_and_palette_press_emit_config() {
    let mut rt = runtime(axis_view, Probe::default(), DESIGN);
    click(&mut rt, "chart-axis-x-select-trigger");
    assert!(
        rt.state.select_open.get("chart-axis-x-select").copied().unwrap_or(false),
        "only the X select opens"
    );
    // Pointer-open leaves the highlight invisible; two Downs reveal then move
    // it to the second option (`revenue`), then Enter commits the value.
    for _ in 0..2 {
        rt.key(KeyEvent { key: Key::Down, mods: Mods::default() });
        rt.frame(1.0);
    }
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0);
    assert_eq!(
        rt.state.changes.last().map(|c| c.x_axis.clone()),
        Some(Some("revenue".to_owned())),
        "X mapping edit reaches the host as a config"
    );

    click(&mut rt, "chart-axis-color-navy");
    assert_eq!(
        rt.state.changes.last().map(|c| c.color_scheme.clone()),
        Some("navy".to_owned()),
        "palette press reaches the host as a config"
    );
}

#[test]
fn axis_option_lists_match_the_react_sets() {
    // The option lists are the exact React six and five; the defaults are the
    // React `DEFAULT_CHART_CONFIG` values. Option labels only exist while the
    // listbox is open, so each select is opened in its own runtime.
    let config = ChartConfig::default();
    assert_eq!(config.aggregation, AggregationType::None);
    assert_eq!(config.sort_by, ChartSort::None);

    let mut agg = runtime(axis_view, Probe::default(), DESIGN);
    click(&mut agg, "chart-axis-agg-select-trigger");
    let rendered = texts(&agg);
    for key in [
        "chart.agg.none",
        "chart.agg.sum",
        "chart.agg.avg",
        "chart.agg.count",
        "chart.agg.min",
        "chart.agg.max",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: aggregation option exists");
    }
    assert!(has(&agg, "option5"), "six aggregation rows are mounted");
    assert!(!has(&agg, "option6"), "no seventh aggregation option");

    let mut sort = runtime(axis_view, Probe::default(), DESIGN);
    click(&mut sort, "chart-axis-sort-select-trigger");
    let rendered = texts(&sort);
    for key in [
        "chart.sort.none",
        "chart.sort.xAsc",
        "chart.sort.xDesc",
        "chart.sort.yAsc",
        "chart.sort.yDesc",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: sort option exists");
    }
    assert!(has(&sort, "option4"), "five sort rows are mounted");
    assert!(!has(&sort, "option5"), "no sixth sort option");
}

#[test]
fn axis_recommendation_renders_its_reason() {
    let mut p = Probe::default();
    p.recommendation = Some(AxisRecommendation {
        reason: "chart.recommended".to_owned(),
    });
    let rt = runtime(axis_view, p, DESIGN);
    assert!(has(&rt, "chart-axis-recommendation"));
    assert!(has(&rt, "chart-axis-recommendation-label"));
    assert!(has(&rt, "chart-axis-recommendation-reason"));
    let rendered = texts(&rt);
    assert!(rendered.iter().filter(|text| text.as_str() == t("chart.recommended")).count() >= 2,
        "header and recommendation label both render");
}

// ---- Geometry ---------------------------------------------------------------

#[test]
fn both_panels_fit_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            let mut p = Probe::default();
            p.dark = dark;
            p.config.x_axis = Some("region".to_owned());
            p.config.y_axes = vec!["revenue".to_owned(), "orders".to_owned()];
            p.config.group_by = Some("region".to_owned());
            p.config.aggregation = AggregationType::Sum;
            p.config.sort_by = ChartSort::XAsc;
            p.config.color_scheme = "ocean".to_owned();
            p.recommendation = Some(AxisRecommendation {
                reason: "chart.recommended".to_owned(),
            });
            let label = format!("axis dark={dark} {size:?}");
            let axis = runtime(axis_view, p.clone(), size);
            assert_inside(&axis, size, &label);

            let ws = runtime(ws_view, p, size);
            assert_inside(&ws, size, &format!("ws dark={dark} {size:?}"));
        }
    }
}
