//! T361: `TableNode` port — keys, columns/PK/FK rendering, handle ids,
//! expand/collapse, host callbacks and geometry at 960×640 / 1440×816 in both
//! palettes. Observations come from the runtime element tree.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::er::table_node::{
    er_column_row_center_y, er_node_body_height, er_node_height, table_node, TableColumnVm,
    TableNodeHostPort, TableNodeVm, ER_AUTO_COLLAPSE_COLUMNS, ER_COLUMN_ROW_HEIGHT,
    ER_COLLAPSED_FOOTER_HEIGHT, ER_HEADER_HEIGHT, ER_NODE_BORDER, ER_NODE_WIDTH, ROOT_KEY,
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
}

fn column(name: &str, type_name: &str, is_pk: bool, is_fk: bool) -> TableColumnVm {
    TableColumnVm {
        name: name.to_owned(),
        type_name: type_name.to_owned(),
        is_pk,
        is_fk,
    }
}

fn probe(collapsed: bool) -> Probe {
    Probe {
        vm: TableNodeVm {
            table_name: "orders".to_owned(),
            columns: vec![
                column("id", "integer", true, false),
                column("user_id", "integer", false, true),
                column("total", "numeric", false, false),
            ],
            handle_columns: vec!["id".to_owned(), "user_id".to_owned()],
            highlighted: false,
            dimmed: false,
            collapsed,
        },
        dark: true,
        events: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let host = TableNodeHostPort {
        on_toggle_collapse: Some(Rc::new(|st: &mut Probe, name: &str| {
            st.events.push(Event::Toggle(name.to_owned()))
        })),
        on_select: Some(Rc::new(|st: &mut Probe, name: &str| {
            st.events.push(Event::Select(name.to_owned()))
        })),
        on_drag: Some(Rc::new(|st: &mut Probe, name: &str| {
            st.events.push(Event::Drag(name.to_owned()))
        })),
    };
    let node = table_node(&s.vm, s.dark, host);
    col().w_full().h_full().child(node)
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
fn node_metrics_match_the_react_contract() {
    assert_eq!(ER_NODE_WIDTH, 260.0);
    assert_eq!(ER_HEADER_HEIGHT, 36.0);
    assert_eq!(ER_COLUMN_ROW_HEIGHT, 24.0);
    assert_eq!(ER_COLLAPSED_FOOTER_HEIGHT, 24.0);
    assert_eq!(ER_NODE_BORDER, 1.0);
    assert_eq!(ER_AUTO_COLLAPSE_COLUMNS, 30);
    assert_eq!(er_node_body_height(5), 120.0);
    assert_eq!(er_node_height(5, false), 36.0 + 120.0 + 2.0);
    assert_eq!(er_node_height(5, true), 36.0 + 24.0 + 2.0);
    assert_eq!(er_node_height(60, true), er_node_height(3, true));
    assert_eq!(er_column_row_center_y(0), 1.0 + 36.0 + 12.0);
    assert_eq!(er_column_row_center_y(3) - er_column_row_center_y(2), 24.0);
}

#[test]
fn expanded_node_renders_header_columns_badges_and_handles() {
    let rt = runtime(probe(false), (1440.0, 816.0));
    for key in [
        ROOT_KEY,
        "er-table-node-header",
        "er-table-node-collapse",
        "er-table-node-name",
        "er-table-node-count",
        "er-table-node-column-0",
        "er-table-node-column-1",
        "er-table-node-column-2",
        "er-table-node-column-0-pk",
        "er-table-node-column-1-fk",
        "er-table-node-column-0-name",
        "er-table-node-column-2-type",
        "er-table-node-fallback-handles",
        "node:t-l",
        "node:s-l",
        "node:s-r",
        "node:t-r",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for text in ["orders", "id", "user_id", "total", "integer", "numeric", "PK", "FK", "3"] {
        assert!(rendered.iter().any(|t| t == text), "{text} is shown");
    }
    // Only touched columns get the four per-row handles; the untouched `total`
    // row carries none.
    for key in ["id:s-r", "id:s-l", "id:t-l", "id:t-r", "user_id:s-r", "user_id:t-r"] {
        assert!(has(&rt, key), "handle {key} is mounted");
    }
    assert!(!has(&rt, "total:s-r"), "an untouched column has no per-row handles");
    assert_eq!(element(&rt, ROOT_KEY).rect[2], ER_NODE_WIDTH, "fixed packed width");
}

#[test]
fn collapsed_node_hides_columns_and_shows_the_count_footer() {
    let rt = runtime(probe(true), (960.0, 640.0));
    assert!(has(&rt, "er-table-node-footer"));
    assert!(!has(&rt, "er-table-node-column-0"), "no column rows when collapsed");
    assert!(!has(&rt, "id:s-r"), "no per-row handles when collapsed");
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == &format!("3 {}", t("erDiagram.columns"))),
        "the footer discloses the hidden column count");
    assert_eq!(element(&rt, "er-table-node-footer").rect[3], ER_COLLAPSED_FOOTER_HEIGHT);
}

#[test]
fn collapse_button_dispatches_with_title_and_table_name() {
    let mut rt = runtime(probe(false), (960.0, 640.0));
    assert_eq!(element(&rt, "er-table-node-collapse").label.as_deref(), Some(t("erDiagram.collapse")));
    click(&mut rt, "er-table-node-collapse");
    assert_eq!(rt.state.events, [Event::Toggle("orders".to_owned())]);

    let mut p = probe(true);
    p.dark = true;
    let mut collapsed = runtime(p, (960.0, 640.0));
    assert_eq!(element(&collapsed, "er-table-node-collapse").label.as_deref(), Some(t("erDiagram.expand")));
    click(&mut collapsed, "er-table-node-collapse");
    assert_eq!(collapsed.state.events, [Event::Toggle("orders".to_owned())]);
}

#[test]
fn highlighted_and_dimmed_states_paint_without_new_keys() {
    let mut p = probe(false);
    p.vm.highlighted = true;
    p.vm.dimmed = true;
    let rt = runtime(p, (1440.0, 816.0));
    assert!((element(&rt, ROOT_KEY).opacity - 0.3).abs() <= 0.01, "dimmed node is 30%");
    assert!(has(&rt, ROOT_KEY), "highlighting does not change the key");
}

#[test]
fn node_fits_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            for collapsed in [false, true] {
                let mut p = probe(collapsed);
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
                assert!(outside.is_empty(),
                    "node dark={dark} collapsed={collapsed} {size:?}: {} node(s) outside: {outside:?}",
                    outside.len());
            }
        }
    }
}
