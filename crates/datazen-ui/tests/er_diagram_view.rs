//! T372: `ErDiagramView` port — real branch tests.
//!
//! Pinned values come from the two React suites: schema forwarding, export
//! options and failure swallowing, plus the composed canvas keys and geometry.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::relation_metadata::{ColumnSchema, ForeignKeyInfo, IndexInfo, TableSchema};
use datazen_ui::views::er::er_diagram_view::{
    er_diagram_view, ErDataRequest, ErDiagramHost, ErDiagramState, ErEdge, EXPORT_BACKGROUND,
    EXPORT_PNG_QUALITY, EXPORT_PNG_KEY, EXPORT_SVG_KEY, FLOW_KEY, ROOT_KEY, SEARCH_KEY, STATS_KEY,
};

struct Probe {
    state: ErDiagramState,
    dark: bool,
    host: Option<Rc<RecordingHost>>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            state: ErDiagramState::new("", "", None),
            dark: true,
            host: None,
        }
    }
}

struct RecordingHost {
    calls: Rc<RefCell<Vec<String>>>,
    fail: bool,
}

impl ErDiagramHost<Probe> for RecordingHost {
    fn export_png(&self, _state: &mut Probe, filename: &str, background: &str, quality: u8) -> Result<(), String> {
        self.calls.borrow_mut().push(format!("png|{filename}|{background}|{quality}"));
        if self.fail { Err("boom".to_owned()) } else { Ok(()) }
    }
    fn export_svg(&self, _state: &mut Probe, filename: &str, background: &str) -> Result<(), String> {
        self.calls.borrow_mut().push(format!("svg|{filename}|{background}"));
        if self.fail { Err("boom".to_owned()) } else { Ok(()) }
    }
    fn export_failed(&self, _state: &mut Probe, message: &str) {
        self.calls.borrow_mut().push(format!("failed|{message}"));
    }
    fn t(&self, key: &str) -> String {
        key.to_owned()
    }
}

fn view(probe: &Probe, _cx: &mut Cx<Probe>) -> El {
    let host = probe
        .host
        .clone()
        .unwrap_or_else(|| Rc::new(RecordingHost { calls: Rc::new(RefCell::new(Vec::new())), fail: false }));
    col()
        .w_full()
        .h_full()
        .child(er_diagram_view(&probe.state, probe.dark, host))
}

fn schema(name: &str, columns: usize, fks: usize) -> TableSchema {
    TableSchema {
        table_name: name.to_owned(),
        columns: (0..columns)
            .map(|index| ColumnSchema {
                name: format!("c{index}"),
                data_type: "int4".to_owned(),
                nullable: true,
                default_value: None,
                is_primary_key: index == 0,
                is_auto_increment: false,
                comment: None,
            })
            .collect(),
        primary_keys: vec!["c0".to_owned()],
        indexes: Vec::<IndexInfo>::new(),
        foreign_keys: (0..fks)
            .map(|_| ForeignKeyInfo {
                name: "fk".to_owned(),
                columns: vec!["c0".to_owned()],
                referenced_table: "other".to_owned(),
                referenced_columns: vec!["id".to_owned()],
                on_update: None,
                on_delete: None,
                deferrability: Some(datazen_ui::relation_metadata::Deferrability::Unknown),
            })
            .collect(),
        comment: None,
    }
}

fn populated(calls: Rc<RefCell<Vec<String>>>) -> Probe {
    let mut state = ErDiagramState::new("sess-1", "db_a", Some("reporting"));
    state.set_schemas(vec![schema("users", 3, 1), schema("orders", 2, 0)]);
    state.predicted = vec![ErEdge {
        id: "pred-1".to_owned(),
        from_table: "users".to_owned(),
        to_table: "orders".to_owned(),
        predicted: true,
        ambiguous: true,
        score: 7,
    }];
    Probe {
        state,
        dark: true,
        host: Some(Rc::new(RecordingHost { calls, fail: false })),
    }
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    Runtime::new(probe, view, size, Env { system_dark: true, scale: 1.0 })
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|element| element.key.as_deref() == Some(key))
}

fn settle(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn schema_request_forwards_the_panel_schema_or_null() {
    let request = ErDataRequest::new("sess-1", "db_a", Some("reporting"));
    assert_eq!(
        request,
        ErDataRequest {
            db_session_id: "sess-1".to_owned(),
            database: "db_a".to_owned(),
            schema: Some("reporting".to_owned()),
        }
    );
    let null = ErDataRequest::new("sess-2", "db_b", None);
    assert_eq!(null.schema, None, "never the database name");

    let mut state = ErDiagramState::new("sess-3", "db_a", Some("reporting"));
    assert_eq!(state.request().schema.as_deref(), Some("reporting"));
    state.set_schema(Some("audit"));
    assert_eq!(state.request().schema.as_deref(), Some("audit"), "re-read on change");
}

#[test]
fn loading_error_and_empty_states_are_exclusive() {
    let mut probe = Probe::default();
    probe.state.loading = true;
    let mut rt = runtime(probe, (960.0, 640.0));
    rt.frame(0.0);
    assert!(has_key(&rt, "er-diagram-loading-spinner"));
    assert!(has_key(&rt, "er-diagram-loading-label"));
    assert!(!has_key(&rt, ROOT_KEY));

    rt.state.state.loading = false;
    rt.state.state.error = Some("gone".to_owned());
    settle(&mut rt);
    assert!(has_key(&rt, "er-diagram-error-text"));
    assert!(!has_key(&rt, "er-diagram-loading-spinner"));

    rt.state.state.error = None;
    settle(&mut rt);
    assert!(has_key(&rt, "er-diagram-empty-label"));
    assert!(!has_key(&rt, ROOT_KEY));
}

#[test]
fn diagram_composes_nodes_edges_legend_controls_and_stats() {
    let rt = runtime(populated(Rc::new(RefCell::new(Vec::new()))), (1440.0, 816.0));
    let mut rt = rt;
    rt.frame(0.0);
    for key in [
        ROOT_KEY,
        FLOW_KEY,
        "er-diagram-node-users",
        "er-diagram-node-orders",
        "er-diagram-edge-pred-1",
        "er-diagram-legend",
        "er-diagram-controls",
        "er-diagram-zoom-in",
        "er-diagram-zoom-out",
        "er-diagram-relayout",
        "er-diagram-fit-view",
        SEARCH_KEY,
        STATS_KEY,
        EXPORT_PNG_KEY,
        EXPORT_SVG_KEY,
    ] {
        assert!(has_key(&rt, key), "composed key missing: {key}");
    }
    let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(texts.iter().any(|text| text.contains("2 张表".trim()) || text.contains("erDiagram.tableCount")));
    assert!(texts.iter().any(|text| text.contains("erDiagram.relationCount")));
}

#[test]
fn export_buttons_call_the_shared_exporter_with_pinned_options() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut rt = runtime(populated(calls.clone()), (1440.0, 816.0));
    click(&mut rt, EXPORT_PNG_KEY);
    click(&mut rt, EXPORT_SVG_KEY);
    assert_eq!(
        calls.borrow().as_slice(),
        [
            format!("png|er-diagram-db_a|{EXPORT_BACKGROUND}|{EXPORT_PNG_QUALITY}"),
            format!("svg|er-diagram-db_a|{EXPORT_BACKGROUND}"),
        ],
        "PNG and SVG go through the shared exporter with the ER backdrop"
    );
}

#[test]
fn absent_viewport_is_a_noop_and_failures_are_swallowed() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut probe = populated(calls.clone());
    probe.state.viewport_available = false;
    let mut rt = runtime(probe, (1440.0, 816.0));
    click(&mut rt, EXPORT_PNG_KEY);
    click(&mut rt, EXPORT_SVG_KEY);
    assert!(calls.borrow().is_empty(), "no viewport means no export");

    let mut probe = populated(calls.clone());
    probe.host = Some(Rc::new(RecordingHost { calls: calls.clone(), fail: true }));
    let mut rt = runtime(probe, (1440.0, 816.0));
    click(&mut rt, EXPORT_PNG_KEY);
    assert!(
        calls.borrow().iter().any(|call| call == "failed|boom"),
        "an export failure is reported through the host hook, never panics"
    );
}

#[test]
fn math_round_matches_js_on_negative_halves() {
    let mut state = ErDiagramState::new("sess", "db", None);
    state.set_viewport(-3.5, -2.5, 1.0);
    assert_eq!(
        (state.viewport.x, state.viewport.y),
        (-3, -2),
        "JS Math.round on negative halves; left: ({}, {}) right: (-3, -2)",
        state.viewport.x,
        state.viewport.y
    );
    state.set_viewport(3.5, 2.5, 1.0);
    assert_eq!((state.viewport.x, state.viewport.y), (4, 3), "positive halves round up");
}

#[test]
fn zoom_pan_fit_relayout_search_and_collapse_update_view_state() {
    let mut state = ErDiagramState::new("sess", "db", None);
    state.set_viewport(10.4, -3.6, 1.0);
    assert_eq!((state.viewport.x, state.viewport.y), (10, -4), "pan rounds to points");
    state.zoom_in();
    assert!(state.viewport.zoom > 1.0);
    state.zoom_out();
    assert!((state.viewport.zoom - 1.0).abs() < 0.001);
    state.fit_view();
    assert_eq!(state.viewport.zoom, 1.0);
    state.pin("users", rune_viewport(20.0, 30.0));
    state.relayout();
    assert!(state.pinned.is_empty(), "relayout drops all pins");
    state.set_search("ORD");
    assert_eq!(state.search_query, "ORD");
    state.toggle_collapse("users");
    assert!(state.collapsed.iter().any(|name| name == "users"));
    state.focus("orders");
    assert_eq!(state.active_focus.as_deref(), Some("orders"));
}

fn rune_viewport(x: f32, y: f32) -> datazen_ui::views::er::er_diagram_view::Viewport {
    datazen_ui::views::er::er_diagram_view::Viewport { x: x as i32, y: y as i32, zoom: 1.0 }
}

#[test]
fn diagram_fits_both_windows_in_both_themes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = populated(Rc::new(RefCell::new(Vec::new())));
            probe.dark = dark;
            let mut rt = runtime(probe, (width, height));
            rt.frame(0.0);
            assert!(has_key(&rt, ROOT_KEY), "diagram mounted");
            for element in rt.elements() {
                let [x, _, w, _] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows horizontally",
                    element.key,
                    element.rect
                );
            }
        }
    }
}
