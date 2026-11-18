//! Independent audit of the DataTable `Pagination.tsx` / `TableHeader.tsx`
//! port (`02227415e`): behaviours the port's own tests do not cover, checked
//! against the React originals.
//!
//! Each test names the React behaviour it pins. Tests marked `#[ignore]` are
//! defects found on the audited commit; they are expected to be enabled (and
//! green) by the fix.

use rune::{col, el, Cx, El, Env, Runtime};

use crate::components::data_table::pagination::{Pagination, TotalRowsKind};
use crate::components::data_table::table_header::{Column, SortCondition, TableHeader};
use crate::i18n::t;

const WIDTHS: [f32; 3] = [160.0, 120.0, 200.0];

#[derive(Default)]
struct Probe {
    dark: bool,
    // pagination
    page: usize,
    page_size: usize,
    total: usize,
    kind: TotalRowsKind,
    row_count: Option<usize>,
    // header
    columns: Vec<Column>,
    sorts: Vec<SortCondition>,
    sort_calls: Vec<(String, bool)>,
    filter_calls: Vec<String>,
    resize_start: Vec<(usize, f32)>,
    resize_move: Vec<(usize, f32)>,
    reorder_start: Vec<usize>,
    reorder_over: Vec<usize>,
    reorder_drop: usize,
    reorder_drag: Option<usize>,
    reorder_spot: Option<usize>,
}

fn columns() -> Vec<Column> {
    vec![
        Column::new("name", "name").ty("text"),
        Column::new("size", "size").ty("int"),
        Column::new("created", "created").ty("datetime"),
    ]
}

fn probe() -> Probe {
    Probe {
        page_size: 25,
        total: 250,
        row_count: Some(25),
        columns: columns(),
        ..Probe::default()
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer: Vec<El> = Vec::new();
    let pagination = Pagination::<Probe>::new(state.page, state.page_size, state.total)
        .total_rows_kind(state.kind)
        .row_count(state.row_count)
        .build(state.dark, &mut layer);
    let header = TableHeader::<Probe>::new(state.columns.clone(), state.sorts.clone())
        .grid_semantics(true)
        .column_widths(WIDTHS.to_vec())
        .reorder_drag(state.reorder_drag)
        .reorder_drop(state.reorder_spot)
        .on_sort(|state: &mut Probe, column: &str, descending: bool| {
            state.sort_calls.push((column.to_owned(), descending));
        })
        .on_filter_column(|state: &mut Probe, column: &str| {
            state.filter_calls.push(column.to_owned());
        })
        .on_resize_start(|state: &mut Probe, column: usize, x: f32| {
            state.resize_start.push((column, x));
        })
        .on_resize_move(|state: &mut Probe, column: usize, travel: f32| {
            state.resize_move.push((column, travel));
        })
        .on_reorder_start(|state: &mut Probe, column: usize| {
            state.reorder_start.push(column);
            state.reorder_drag = Some(column);
        })
        .on_reorder_over(|state: &mut Probe, column: usize| {
            state.reorder_over.push(column);
            state.reorder_spot = Some(column);
        })
        .on_reorder_drop(|state: &mut Probe| {
            state.reorder_drop += 1;
            state.reorder_drag = None;
            state.reorder_spot = None;
        })
        .build(state.dark, &mut layer);
    let body = col()
        .key("audit-page")
        .w_full()
        .h_full()
        .child(header)
        .child(el().grow(1.0))
        .child(pagination);
    rune_kit::root(body, layer)
}

fn runtime(state: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = state.dark;
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

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn centre(rt: &Runtime<Probe>, key: &str) -> (f32, f32) {
    let [x, y, w, h] = element(rt, key).rect;
    (x + w / 2.0, y + h / 2.0)
}

fn texts(rt: &Runtime<Probe>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect()
}

/// React reports `onDragOver` on the column the pointer is over, so the host's
/// `dropIndex` is the target column. Rune's `on_drag` fires on the pressed
/// element only, so this probes which index `on_reorder_over` receives.
#[test]
fn a_reorder_reports_the_column_under_the_pointer() {
    let mut rt = runtime(probe(), (1440.0, 816.0));
    let (x0, y0) = centre(&rt, "data-table-header-col-0");
    let (x2, y2) = centre(&rt, "data-table-header-col-2");
    rt.pointer_down(x0, y0);
    rt.pointer_move(x2, y2);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.reorder_start,
        vec![0],
        "the drag starts on the pressed column"
    );
    assert_eq!(
        rt.state.reorder_over.last().copied(),
        Some(2),
        "the pointer is over column 2, so that is the drop target: {:?}",
        rt.state.reorder_over
    );
}

/// React's resize grip reports the pointer's window x to `onResizeStart`; the
/// host then tracks the travel. Rune's `on_resize_move` promises travel, so a
/// 40pt move must be reported as 40 no matter where the grip sits.
#[test]
fn the_resize_grip_reports_pointer_travel() {
    let mut rt = runtime(probe(), (1440.0, 816.0));
    let (x, y) = centre(&rt, "data-table-header-resize-1");
    rt.pointer_down(x, y);
    rt.pointer_move(x + 40.0, y + 3.0);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x + 40.0, y + 3.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.resize_start.len(), 1, "one press reports once");
    assert!(
        rt.state
            .resize_move
            .last()
            .is_some_and(|(column, travel)| *column == 1 && (travel - 40.0).abs() < 1.0),
        "40pt of travel is reported for column 1: {:?}",
        rt.state.resize_move
    );
}

/// React draws the sort arrow inside a 16pt box (`h-4 w-4`), with the 14pt
/// glyph centred in it; the audit lead asked for the 16pt slot.
#[test]
fn the_sort_arrow_keeps_its_sixteen_point_slot() {
    let mut state = probe();
    state.sorts = vec![SortCondition::new("name", false)];
    let rt = runtime(state, (1440.0, 816.0));
    let arrow = element(&rt, "data-table-header-sort-direction-0");
    assert_eq!(
        arrow.label.as_deref(),
        Some("升序"),
        "assistive tech hears the direction"
    );
    assert_eq!(
        arrow.rect[2], 16.0,
        "the direction slot is 16pt wide (was {:?})",
        arrow.rect
    );
    assert_eq!(
        arrow.rect[3], 16.0,
        "the direction slot is 16pt tall (was {:?})",
        arrow.rect
    );
}

/// React's multi-sort position badge carries `title="#1"`, so hover and
/// assistive tech get the rank; Rune hides it from the a11y tree and drops
/// the tooltip.
#[test]
fn the_sort_position_badge_announces_its_rank() {
    let mut state = probe();
    state.sorts = vec![
        SortCondition::new("name", false),
        SortCondition::new("size", true),
    ];
    let rt = runtime(state, (1440.0, 816.0));
    let badge = element(&rt, "data-table-header-sort-position-0");
    assert_eq!(
        badge.help.as_deref(),
        Some("#1"),
        "the badge keeps React's #1 tooltip: {badge:?}"
    );
}

/// React's estimate-overshoot warning uses `pagination.estimatedLessThanActual`
/// and the React row range (`0-75` here: `to` is `page*size + rowsOnPage`).
#[test]
fn the_estimated_overshoot_shows_the_react_warning_and_range() {
    let mut state = probe();
    state.kind = TotalRowsKind::Estimated;
    state.page = 3;
    state.row_count = Some(0);
    let rt = runtime(state, (1440.0, 816.0));
    let row_count = element(&rt, "data-table-pagination-row-count")
        .text
        .clone()
        .expect("the row count has text");
    assert_eq!(
        row_count,
        format!(
            "0-75 / {}",
            t("pagination.rowCountEstimated").replace("{count}", "250")
        ),
        "the empty estimate keeps React's from/to arithmetic"
    );
    let warning = t("pagination.estimatedLessThanActual");
    assert!(
        texts(&rt).iter().any(|text| text == &warning),
        "the overshoot warning is shown: {warning}"
    );
}

/// React hides the per-page group below the `sm` breakpoint but shows it at
/// 960; with it always mounted, it must not collide with the navigation.
#[test]
fn the_per_page_group_stays_left_of_the_navigation_at_nine_sixty() {
    let rt = runtime(probe(), (960.0, 640.0));
    let select = element(&rt, "data-table-pagination-page-size-trigger").rect;
    let prev = element(&rt, "data-table-pagination-prev").rect;
    assert!(
        select[0] + select[2] <= prev[0] + 0.5,
        "per-page select {:?} overlaps the navigation {:?}",
        select,
        prev
    );
}

/// React stops the filter button's click from reaching the column, so pressing
/// it filters and never sorts.
#[test]
fn pressing_the_filter_button_does_not_cycle_the_sort() {
    let mut rt = runtime(probe(), (1440.0, 816.0));
    let (x, y) = centre(&rt, "data-table-header-filter-0");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.filter_calls,
        vec!["name".to_owned()],
        "the filter reports its column"
    );
    assert!(
        rt.state.sort_calls.is_empty(),
        "filtering must not sort: {:?}",
        rt.state.sort_calls
    );
}

/// React falls back from descending to ascending when `onRemoveSort` is absent.
#[test]
fn a_descending_sort_without_a_remove_action_falls_back_to_ascending() {
    let mut state = probe();
    state.sorts = vec![SortCondition::new("name", true)];
    let mut rt = runtime(state, (1440.0, 816.0));
    let (x, y) = centre(&rt, "data-table-header-label-0");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.sort_calls,
        vec![("name".to_owned(), false)],
        "no remove action: descending cycles to ascending"
    );
}

/// React never clamps an unknown total, so page 99 reads 2,476-2,500 / unknown.
#[test]
fn an_unknown_total_never_clamps_the_page() {
    let mut state = probe();
    state.kind = TotalRowsKind::Unsupported;
    state.page = 99;
    let rt = runtime(state, (1440.0, 816.0));
    assert_eq!(
        element(&rt, "data-table-pagination-row-count")
            .text
            .as_deref(),
        Some(format!("2476-2500 / {}", t("pagination.rowCountUnknown")).as_str())
    );
    assert_eq!(
        element(&rt, "data-table-pagination-page").text.as_deref(),
        Some("第 100 页")
    );
}

/// React keeps the sort-menu chevron inside the cell, after the badge and the
/// direction arrow; a chevron placed in the header row would shift every column.
#[test]
fn the_sort_menu_chevron_sits_inside_its_cell() {
    let rt = runtime(probe(), (1440.0, 816.0));
    let cell = element(&rt, "data-table-header-col-0").rect;
    let chevron = element(&rt, "data-table-header-sort-0").rect;
    assert!(
        chevron[0] >= cell[0] && chevron[0] + chevron[2] <= cell[0] + cell[2],
        "the chevron {chevron:?} sits outside its cell {cell:?}"
    );
}

/// React pins the row-number gutter and the frozen columns with `sticky
/// left-{frozenLefts[i]}`; the gutter and the first frozen column must abut,
/// not overlap, and both must stay visible at the left edge.
#[test]
fn the_frozen_gutter_and_columns_stay_in_order() {
    let state = Probe {
        columns: (0..5)
            .map(|i| Column::new(format!("c{i}"), format!("c{i}")).ty("text"))
            .collect(),
        ..probe()
    };
    let mut rt = Runtime::new(
        state,
        |state: &Probe, _cx: &mut Cx<Probe>| {
            let mut layer: Vec<El> = Vec::new();
            let header = TableHeader::<Probe>::new(state.columns.clone(), state.sorts.clone())
                .show_row_numbers(true)
                .frozen_columns(2)
                .column_widths(vec![120.0; 5])
                .build(state.dark, &mut layer);
            rune_kit::root(
                col().key("audit-page").w_full().h_full().child(header),
                layer,
            )
        },
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    let gutter = element(&rt, "data-table-header-gutter").rect;
    let first = element(&rt, "data-table-header-col-0").rect;
    let second = element(&rt, "data-table-header-col-1").rect;
    assert!(
        (gutter[0] + gutter[2] - first[0]).abs() < 0.5,
        "gutter {gutter:?} and column 0 {first:?} abut"
    );
    assert!(
        (first[0] + first[2] - second[0]).abs() < 0.5,
        "columns abut: {first:?} {second:?}"
    );
}

/// The hit test must work from any pressed column, not only the first one, and
/// at the minimum window as well as the default one.
#[test]
fn a_reorder_from_the_middle_column_can_target_column_zero() {
    let mut rt = runtime(probe(), (960.0, 640.0));
    let (x1, y1) = centre(&rt, "data-table-header-label-1");
    let (x0, y0) = centre(&rt, "data-table-header-col-0");
    rt.pointer_down(x1, y1);
    rt.pointer_move(x0, y0);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.reorder_start,
        vec![1],
        "the drag starts on the pressed column"
    );
    assert_eq!(
        rt.state.reorder_over.last().copied(),
        Some(0),
        "the pointer is over column 0: {:?}",
        rt.state.reorder_over
    );
}

/// React never draws the reorder marker on the column being dragged.
#[test]
fn the_drop_marker_skips_the_dragged_column() {
    let mut state = probe();
    state.reorder_drag = Some(2);
    state.reorder_spot = Some(2);
    let rt = runtime(state, (1440.0, 816.0));
    assert!(
        element(&rt, "data-table-header-col-2").rect[2] > 0.0,
        "the dragged column is still laid out"
    );
    assert!(
        rt.elements()
            .iter()
            .all(|element| element.key.as_deref() != Some("data-table-header-drop-marker-2")),
        "no marker is drawn on the dragged column itself"
    );
}
