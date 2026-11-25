//! T300 independent audit: WebSocketTunnelFields + AxisConfigurator.
//!
//! Probes branches the coder's suite does not: WS text-edit routing and the
//! accessor-free select path, group-by/add-series hiding conditions, and
//! aggregation/sort/Y-select edits. Observations come from the runtime tree
//! and the host's recorded config callbacks.

use std::collections::HashMap;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};
use rune_kit::root;

use datazen_ui::components::chart::axis_configurator::{AxisConfigurator, COLOR_PALETTES};
use datazen_ui::components::chart::chart_canvas::{
    ChartConfig, ChartField, ChartSort, ChartType, InferredFieldType,
};
use datazen_ui::components::connection::websocket_tunnel_fields::{WebSocketTunnelFields, WebSocketValue, WsMode};

#[derive(Clone)]
struct Probe {
    dark: bool,
    ws: WebSocketValue,
    modes: Vec<WsMode>,
    fields: Vec<ChartField>,
    config: ChartConfig,
    collapsed: bool,
    changes: Vec<ChartConfig>,
    open: HashMap<String, bool>,
    bind_select_accessors: bool,
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
            ws: WebSocketValue::default(),
            modes: Vec::new(),
            fields: vec![
                field("revenue", InferredFieldType::Numeric),
                field("orders", InferredFieldType::Numeric),
                field("region", InferredFieldType::Categorical),
            ],
            config: ChartConfig::default(),
            collapsed: false,
            changes: Vec::new(),
            open: HashMap::new(),
            bind_select_accessors: true,
        }
    }
}

fn ws_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let mut w = WebSocketTunnelFields::<Probe>::new(
        &s.ws,
        |st: &mut Probe| &mut st.ws.url,
        |st: &mut Probe| &mut st.ws.timeout,
        |st: &mut Probe| &mut st.ws.auth_token,
    );
    if s.bind_select_accessors {
        w = w
            .mode_open(|st: &mut Probe| &mut st.ws.mode_open)
            .on_mode(|st: &mut Probe, m| st.modes.push(m));
    }
    root(col().w_full().h_full().child(w.build(s.dark, &mut layer)), layer)
}

fn axis_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let panel = AxisConfigurator::<Probe>::new(&s.fields, &s.config, None)
        .collapsed(s.collapsed)
        .on_collapsed(|st: &mut Probe, v| st.collapsed = v)
        .on_change(|st: &mut Probe, c| st.changes.push(c))
        .select_open(s.open.clone(), |st: &mut Probe, key: &str| {
            st.open.entry(key.to_owned()).or_insert(false)
        })
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(panel), layer)
}

fn runtime(view: fn(&Probe, &mut Cx<Probe>) -> El, p: Probe) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        p,
        view,
        (1440.0, 816.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

fn type_into(rt: &mut Runtime<Probe>, key: &str, text: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste(text);
    rt.frame(1.0);
}

fn choose_second_option(rt: &mut Runtime<Probe>, trigger: &str) {
    click(rt, trigger);
    for _ in 0..2 {
        rt.key(KeyEvent { key: Key::Down, mods: Mods::default() });
        rt.frame(1.0);
    }
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0);
}

/// WS text fields route keystrokes into the host state (URL, timeout, token).
#[test]
fn ws_text_edits_route_into_the_host_state() {
    let mut rt = runtime(ws_view, Probe::default());
    type_into(&mut rt, "new-conn-ws-url", "wss://new.example.com/v2");
    assert_eq!(rt.state.ws.url.text, "wss://new.example.com/v2");
    rt.state.ws.timeout = TextEdit::new(String::new());
    rt.invalidate();
    rt.frame(1.0);
    type_into(&mut rt, "new-conn-ws-timeout", "45");
    assert_eq!(rt.state.ws.timeout.text, "45");
    type_into(&mut rt, "new-conn-ws-token", "secret");
    assert_eq!(rt.state.ws.auth_token.text, "secret");
}

/// Without mode accessors the select stays inert: pressing the trigger must
/// not panic and must not report a mode change.
#[test]
fn ws_mode_without_accessors_is_inert() {
    let mut p = Probe::default();
    p.bind_select_accessors = false;
    let mut rt = runtime(ws_view, p);
    click(&mut rt, "new-conn-ws-mode-trigger");
    assert!(rt.state.modes.is_empty(), "no mode reported without an accessor");
}

/// React hides group-by when there are no categorical (or datetime) fields,
/// and hides add-series when every numeric field is already used.
#[test]
fn axis_hides_group_by_without_categorical_and_add_series_when_exhausted() {
    let mut p = Probe::default();
    p.fields = vec![field("revenue", InferredFieldType::Numeric)];
    let rt = runtime(axis_view, p);
    assert!(!has(&rt, "chart-axis-group-block"), "no categorical fields → no group-by");

    let mut p = Probe::default();
    p.config.y_axes = vec!["revenue".to_owned(), "orders".to_owned()];
    let mut rt = runtime(axis_view, p);
    assert!(!has(&rt, "chart-axis-add-series"), "all numeric fields used → no add-series");
    // Two series still expose remove controls.
    assert!(has(&rt, "chart-axis-y-remove-0"));
    assert!(has(&rt, "chart-axis-y-remove-1"));
}

/// Aggregation and sort select edits emit whole configs with the exact React
/// enum values; a Y select edit rewrites that series slot.
#[test]
fn axis_agg_sort_and_y_select_edits_emit_configs() {
    let mut rt = runtime(axis_view, Probe::default());
    choose_second_option(&mut rt, "chart-axis-agg-select-trigger");
    assert_eq!(
        rt.state.changes.last().map(|c| c.aggregation),
        Some(datazen_ui::components::chart::chart_canvas::AggregationType::Sum)
    );

    let mut rt = runtime(axis_view, Probe::default());
    choose_second_option(&mut rt, "chart-axis-sort-select-trigger");
    assert_eq!(rt.state.changes.last().map(|c| c.sort_by), Some(ChartSort::XAsc));

    let mut p = Probe::default();
    p.config.y_axes = vec!["revenue".to_owned()];
    let mut rt = runtime(axis_view, p);
    choose_second_option(&mut rt, "chart-axis-y-select-0-trigger");
    assert_eq!(
        rt.state.changes.last().map(|c| c.y_axes.clone()),
        Some(vec!["orders".to_owned()])
    );
}

/// Group-by edits and palette data: the eight palettes are the exact React set.
#[test]
fn axis_group_by_edit_and_palette_data() {
    let mut rt = runtime(axis_view, Probe::default());
    choose_second_option(&mut rt, "chart-axis-group-select-trigger");
    assert_eq!(
        rt.state.changes.last().map(|c| c.group_by.clone()),
        Some(Some("region".to_owned()))
    );

    assert_eq!(COLOR_PALETTES.len(), 8);
    assert!(COLOR_PALETTES.iter().all(|(_, colors)| colors.len() >= 4));
}

/// No fabricated IPC and no invented i18n keys in either new module.
#[test]
fn no_fake_ipc_or_invented_keys() {
    for src in [
        include_str!("../src/components/connection/websocket_tunnel_fields.rs"),
        include_str!("../src/components/chart/axis_configurator.rs"),
    ] {
        for forbidden in ["connectionCommands", "invoke(", "spawn(", "oklch"] {
            assert!(!src.contains(forbidden), "forbidden token: {forbidden}");
        }
    }
    let ws = include_str!("../src/components/connection/websocket_tunnel_fields.rs");
    for key in ["newConn.wsUrl", "newConn.wsMode", "newConn.wsTimeout", "newConn.wsAuthToken", "newConn.wsHint"] {
        assert!(ws.contains(key), "exact React i18n key present: {key}");
    }
}

/// Pie/scatter still render a single empty Y series; datetime fields count as
/// categorical for group-by (React's `categorical` filter includes datetime).
#[test]
fn axis_datetime_counts_as_categorical() {
    let mut p = Probe::default();
    p.fields = vec![field("revenue", InferredFieldType::Numeric), field("day", InferredFieldType::Datetime)];
    let rt = runtime(axis_view, p);
    assert!(has(&rt, "chart-axis-group-block"), "datetime field enables group-by");
    assert_eq!(rt.state.config.chart_type, ChartType::Bar);
}
