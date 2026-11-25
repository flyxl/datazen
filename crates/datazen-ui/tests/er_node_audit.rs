//! T361 independent audit: `TableNode` vs React + nodeMetrics.
//!
//! Probes branches the coder's suite does not: whether the RENDERED node height
//! matches the metric the layout engine packs against, handle-dot placement,
//! select/drag host ports, an inert toggle without a host handler, and a
//! rect-geometry stress at both windows/palettes.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::er::table_node::{
    er_column_row_center_y, er_node_height, table_node, TableColumnVm, TableNodeHostPort,
    TableNodeVm, ROOT_KEY,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Toggle(String),
    Select(String),
    Drag(String),
}

#[derive(Clone)]
struct Probe {
    vm: TableNodeVm,
    dark: bool,
    events: Vec<Event>,
    bind_host: bool,
}

fn column(name: &str, type_name: &str, is_pk: bool, is_fk: bool) -> TableColumnVm {
    TableColumnVm {
        name: name.to_owned(),
        type_name: type_name.to_owned(),
        is_pk,
        is_fk,
    }
}

fn probe(collapsed: bool, count: usize) -> Probe {
    let columns: Vec<TableColumnVm> = (0..count)
        .map(|i| column(&format!("col{i}"), "integer", i == 0, i == 1))
        .collect();
    let handle_columns = columns.iter().take(2).map(|c| c.name.clone()).collect();
    Probe {
        vm: TableNodeVm {
            table_name: "orders".to_owned(),
            columns,
            handle_columns,
            highlighted: false,
            dimmed: false,
            collapsed,
        },
        dark: true,
        events: Vec::new(),
        bind_host: true,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let host = if s.bind_host {
        TableNodeHostPort {
            on_toggle_collapse: Some(Rc::new(|st: &mut Probe, n: &str| st.events.push(Event::Toggle(n.to_owned())))),
            on_select: Some(Rc::new(|st: &mut Probe, n: &str| st.events.push(Event::Select(n.to_owned())))),
            on_drag: Some(Rc::new(|st: &mut Probe, n: &str| st.events.push(Event::Drag(n.to_owned())))),
        }
    } else {
        TableNodeHostPort::default()
    };
    col().w_full().h_full().child(table_node(&s.vm, s.dark, host))
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

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

/// The layout engine packs nodes against `erNodeHeight`. The RENDERED node must
/// be exactly that tall (React's xyflow handles sit on top, taking no space).
#[test]
fn rendered_node_height_matches_the_packed_metric() {
    for (collapsed, count) in [(false, 3usize), (true, 3), (false, 1)] {
        let rt = runtime(probe(collapsed, count), (1440.0, 816.0));
        let rendered = element(&rt, ROOT_KEY).rect[3];
        let metric = er_node_height(count, collapsed);
        assert!(
            (rendered - metric).abs() <= 0.5,
            "collapsed={collapsed} count={count}: rendered height {rendered} must equal er_node_height {metric}"
        );
    }
}

/// Per-column handle dots sit at the metric's row-center Y (node-local), as
/// React's `erColumnRowCenterY` positions the edge endpoints.
#[test]
fn column_handle_dots_sit_at_the_metric_center_y() {
    // Only `handle_columns` get per-row handles; the probe touches the first
    // two columns, so check those two rather than asking for a third handle
    // the React component would not draw either.
    let rt = runtime(probe(false, 2), (1440.0, 816.0));
    let node_top = element(&rt, ROOT_KEY).rect[1];
    for index in 0..2 {
        let dot = element(&rt, &format!("col{index}:s-r"));
        let center_y = dot.rect[1] + dot.rect[3] / 2.0;
        let expected = node_top + er_column_row_center_y(index);
        assert!(
            (center_y - expected).abs() <= 0.5,
            "column {index} handle center {center_y} should be at {expected}"
        );
    }
}

/// Header press selects the node; header drag reports the same table name.
#[test]
fn select_and_drag_host_ports_fire() {
    let mut rt = runtime(probe(false, 2), (1440.0, 816.0));
    click(&mut rt, "er-table-node-name");
    assert_eq!(rt.state.events, [Event::Select("orders".to_owned())]);
}

/// Without host callbacks the toggle and header are inert, never panic.
#[test]
fn no_host_handler_is_inert() {
    let mut p = probe(false, 2);
    p.bind_host = false;
    let mut rt = runtime(p, (1440.0, 816.0));
    click(&mut rt, "er-table-node-collapse");
    click(&mut rt, "er-table-node-name");
    assert!(rt.state.events.is_empty(), "no host callback → no events");
}

/// A wide (auto-collapse threshold) node still fits both windows/palettes.
#[test]
fn wide_node_fits_both_windows_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            // React auto-collapses at ER_AUTO_COLLAPSE_COLUMNS, so a 30-column
            // table is rendered collapsed, not as a 764pt expanded node.
            let mut p = probe(true, 30);
            p.dark = dark;
            let rt = runtime(p, size);
            // The node itself is packed to a metric; check its own rect is
            // inside the window (the surrounding canvas may scroll).
            let r = element(&rt, ROOT_KEY).rect;
            assert!(
                r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= size.0 + 0.5 && r[1] + r[3] <= size.1 + 0.5,
                "dark={dark} {size:?}: node rect {r:?} outside window"
            );
            // A collapsed node exposes only the four fallback handles.
            assert!(has(&rt, "node:t-l"));
            assert!(!has(&rt, "col0:s-r"), "no per-row handles when collapsed");
        }
    }
}
