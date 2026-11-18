//! `DataTable/Pagination.tsx` and `DataTable/TableHeader.tsx` in Rune: the row
//! range and page label, the prev/next enabled states and their callbacks, the
//! page-size picker, the count action, the sort cycle and menu, the filter and
//! resize hooks, column widths, and window fit at both design sizes.

use rune::{col, el, Cx, El, Env, Runtime};

use crate::components::data_table::pagination::{
    can_page_forward, format_row_count, total_rows_view, PagingHint, Pagination, TotalRowsKind,
    TotalRowsView,
};
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
    loading: bool,
    page_size_open: bool,
    page_calls: Vec<usize>,
    size_calls: Vec<usize>,
    count_calls: usize,
    // header
    columns: Vec<Column>,
    sorts: Vec<SortCondition>,
    sort_calls: Vec<(String, bool)>,
    remove_calls: Vec<String>,
    filter_calls: Vec<String>,
    resize_calls: Vec<(usize, f32)>,
    resize_ends: usize,
    menu: Option<usize>,
}

fn columns() -> Vec<Column> {
    vec![
        Column::new("name", "name").ty("text"),
        Column::new("size", "size").ty("int"),
        Column::new("created", "created").ty("datetime"),
    ]
}

fn probe(kind: TotalRowsKind) -> Probe {
    Probe {
        page_size: 25,
        total: 250,
        kind,
        row_count: Some(25),
        columns: columns(),
        ..Probe::default()
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer: Vec<El> = Vec::new();
    let pagination = Pagination::new(state.page, state.page_size, state.total)
        .total_rows_kind(state.kind)
        .row_count(state.row_count)
        .loading(state.loading)
        .page_size_open(state.page_size_open)
        .on_page_change(|state: &mut Probe, page| {
            state.page = page;
            state.page_calls.push(page);
        })
        .on_page_size_change(|state: &mut Probe, size| {
            state.page_size = size;
            state.size_calls.push(size);
        })
        .on_page_size_toggle(|state: &mut Probe| state.page_size_open = true)
        .on_page_size_close(|state: &mut Probe| state.page_size_open = false)
        .on_count_exact(|state: &mut Probe| state.count_calls += 1)
        .build(state.dark, &mut layer);
    let header = TableHeader::new(state.columns.clone(), state.sorts.clone())
        .grid_semantics(true)
        .sort_menu(state.menu)
        .column_widths(WIDTHS.to_vec())
        .on_sort(|state: &mut Probe, column: &str, descending: bool| {
            state.sort_calls.push((column.to_owned(), descending));
            state.sorts.retain(|sort| sort.column != column);
            state
                .sorts
                .push(SortCondition::new(column.to_owned(), descending));
        })
        .on_remove_sort(|state: &mut Probe, column: &str| {
            state.remove_calls.push(column.to_owned());
            state.sorts.retain(|sort| sort.column != column);
        })
        .on_filter_column(|state: &mut Probe, column: &str| {
            state.filter_calls.push(column.to_owned());
        })
        .on_resize_start(|state: &mut Probe, column: usize, x: f32| {
            state.resize_calls.push((column, x));
        })
        .on_resize_end(|state: &mut Probe| state.resize_ends += 1)
        .on_sort_menu_toggle(|state: &mut Probe, column: usize| {
            state.menu = Some(column);
        })
        .on_sort_menu_close(|state: &mut Probe| state.menu = None)
        .build(state.dark, &mut layer);
    let body = col()
        .key("probe-page")
        .w_full()
        .h_full()
        .child(header)
        .child(el().grow(1.0))
        .child(pagination);
    rune_kit::root(body, layer)
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn runtime(state: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = state.dark;
    let mut rt = Runtime::new(state, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn text_of(rt: &Runtime<Probe>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} shows text"))
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---------------------------------------------------------------- pagination

#[test]
fn the_row_range_follows_the_total_and_the_page() {
    let mut state = probe(TotalRowsKind::Exact);
    state.page = 2;
    let rt = runtime(state, (1440.0, 816.0));
    assert_eq!(text_of(&rt, "data-table-pagination-row-count"), "51-75 / 250");

    let mut estimated = probe(TotalRowsKind::Estimated);
    estimated.page = 2;
    let rt = runtime(estimated, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, "data-table-pagination-row-count"),
        format!(
            "51-75 / {}",
            t("pagination.rowCountEstimated").replace("{count}", "250")
        ),
        "an estimate is labelled as one"
    );

    let mut unknown = probe(TotalRowsKind::Unsupported);
    unknown.page = 2;
    let rt = runtime(unknown, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, "data-table-pagination-row-count"),
        format!("51-75 / {}", t("pagination.rowCountUnknown")),
        "an unsupported total says so"
    );
}

#[test]
fn the_page_label_reads_the_page_and_the_page_count() {
    let mut state = probe(TotalRowsKind::Exact);
    state.page = 2;
    let rt = runtime(state, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, "data-table-pagination-page"),
        "第 3 / 10 页",
        "exact totals show both numbers"
    );

    let mut unknown = probe(TotalRowsKind::Unsupported);
    unknown.page = 2;
    let rt = runtime(unknown, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, "data-table-pagination-page"),
        "第 3 页",
        "no total, no page count"
    );
}

#[test]
fn prev_is_disabled_on_the_first_page() {
    let rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    let mut rt = rt;
    click(&mut rt, "data-table-pagination-prev");
    assert!(
        rt.state.page_calls.is_empty(),
        "a disabled previous button reports nothing: {:?}",
        rt.state.page_calls
    );
}

#[test]
fn next_is_disabled_on_the_last_exact_page() {
    let mut state = probe(TotalRowsKind::Exact);
    state.page = 9;
    let mut rt = runtime(state, (1440.0, 816.0));
    click(&mut rt, "data-table-pagination-next");
    assert!(
        rt.state.page_calls.is_empty(),
        "the exact total closes the grid: {:?}",
        rt.state.page_calls
    );
}

#[test]
fn next_reports_the_target_page_and_prev_reports_the_one_before() {
    let mut state = probe(TotalRowsKind::Exact);
    state.page = 3;
    let mut rt = runtime(state, (1440.0, 816.0));
    click(&mut rt, "data-table-pagination-next");
    assert_eq!(rt.state.page_calls, vec![4], "next asks for the following page");
    click(&mut rt, "data-table-pagination-prev");
    assert_eq!(rt.state.page_calls, vec![4, 3], "prev asks for the page before");
}

#[test]
fn the_count_button_is_offered_for_an_estimate_only() {
    let mut estimated = probe(TotalRowsKind::Estimated);
    estimated.page = 3;
    let mut rt = runtime(estimated, (1440.0, 816.0));
    assert!(
        rt.state.page_calls.is_empty(),
        "an estimate never clamps the page"
    );
    click(&mut rt, "data-table-pagination-count-exact");
    assert_eq!(rt.state.count_calls, 1);

    let exact = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    assert!(
        exact.find("data-table-pagination-count-exact").is_none(),
        "an exact total needs no count action"
    );
}

#[test]
fn the_page_size_picker_offers_the_five_sizes_and_reports_a_choice() {
    let mut state = probe(TotalRowsKind::Exact);
    state.page = 2;
    let mut rt = runtime(state, (1440.0, 816.0));
    click(&mut rt, "data-table-pagination-page-size-trigger");
    assert_eq!(rt.state.page_size_open, true, "the picker opened");
    for index in 0..5 {
        assert!(
            rt.find(&format!("option{index}")).is_some(),
            "size {} is offered",
            [25, 50, 100, 200, 500][index]
        );
    }
    click(&mut rt, "option2");
    assert_eq!(rt.state.size_calls, vec![100], "the chosen size is reported");
    assert_eq!(rt.state.page_size, 100, "and it becomes the page size");
}

#[test]
fn the_pagination_stays_inside_the_window() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = probe(TotalRowsKind::Exact);
            state.dark = dark;
            state.page = 4;
            let rt = runtime(state, (width, height));
            let theme = if dark { "dark" } else { "light" };
            let mut outside = Vec::new();
            for element in rt.elements() {
                let key = element.key.as_deref().unwrap_or(element.path.as_str());
                let [x, y, w, h] = element.rect;
                if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
                    outside.push(format!("key={key} rect={:?}", element.rect));
                }
            }
            assert!(
                outside.is_empty(),
                "pagination {width}x{height} {theme}: {} node(s) cross the window: {outside:#?}",
                outside.len()
            );
        }
    }
}

#[test]
fn the_row_count_helpers_port_the_typescript() {
    assert_eq!(format_row_count(999.0), "999");
    assert_eq!(format_row_count(1_234.0), "1,234");
    assert_eq!(format_row_count(1_234_567.0), "1.2M");
    assert_eq!(format_row_count(2_500_000_000.0), "2.5B");
    assert_eq!(format_row_count(f64::NAN), "0");
    assert_eq!(total_rows_view(TotalRowsKind::Estimated, 7), TotalRowsView::Estimated(7.0));
    assert!(can_page_forward(TotalRowsView::Estimated(10.0), 0, 25));
    assert!(!can_page_forward(TotalRowsView::Exact(10.0), 10, 25));
    assert!(can_page_forward(TotalRowsView::Unknown, 25, 25));
    assert!(!can_page_forward(TotalRowsView::Unknown, 24, 25));
    assert_eq!(
        crate::components::data_table::pagination::paging_hint_key(
            crate::components::data_table::pagination::KeysetUnavailableReason::NotAdjacent
        ),
        "pagination.hint.jumpReanchored"
    );
    assert!(!crate::components::data_table::pagination::show_paging_reason(
        crate::components::data_table::pagination::KeysetUnavailableReason::DriverUnsupported
    ));
    let _ = PagingHint::default();
}

// -------------------------------------------------------------------- header

#[test]
fn the_header_renders_the_gutter_and_every_column_name_and_type() {
    let rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    assert_eq!(text_of(&rt, "data-table-header-gutter-label"), "#");
    for (index, (name, ty)) in [("name", "text"), ("size", "int"), ("created", "datetime")]
        .iter()
        .enumerate()
    {
        assert_eq!(
            text_of(&rt, &format!("data-table-header-name-{index}")),
            *name
        );
        assert_eq!(text_of(&rt, &format!("data-table-header-type-{index}")), *ty);
    }
}

#[test]
fn a_header_press_cycles_none_ascending_descending_then_removes() {
    let mut rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    click(&mut rt, "data-table-header-col-0");
    assert_eq!(
        rt.state.sort_calls,
        vec![("name".to_owned(), false)],
        "the first press sorts ascending"
    );
    click(&mut rt, "data-table-header-col-0");
    assert_eq!(
        rt.state.sort_calls,
        vec![("name".to_owned(), false), ("name".to_owned(), true)],
        "the second press flips to descending"
    );
    click(&mut rt, "data-table-header-col-0");
    assert_eq!(
        rt.state.remove_calls,
        vec!["name".to_owned()],
        "the third press removes the sort"
    );
    assert!(
        rt.state.sort_calls.len() == 2,
        "removing must not also sort: {:?}",
        rt.state.sort_calls
    );
}

#[test]
fn the_sort_arrow_and_the_position_badge_follow_the_sorts() {
    let mut state = probe(TotalRowsKind::Exact);
    state.sorts = vec![
        SortCondition::new("name", false),
        SortCondition::new("created", true),
    ];
    let rt = runtime(state, (1440.0, 816.0));
    assert_eq!(
        element(&rt, "data-table-header-sort-direction-0")
            .label
            .as_deref(),
        Some(t("dataTable.sortAscending")),
        "column 0 sorts ascending"
    );
    assert_eq!(
        element(&rt, "data-table-header-sort-direction-2")
            .label
            .as_deref(),
        Some(t("dataTable.sortDescending")),
        "column 2 sorts descending"
    );
    assert!(
        rt.find("data-table-header-sort-direction-1").is_none(),
        "an unsorted column has no arrow"
    );
    assert_eq!(text_of(&rt, "data-table-header-sort-position-0"), "1");
    assert_eq!(text_of(&rt, "data-table-header-sort-position-2"), "2");
    assert!(
        rt.find("data-table-header-sort-position-1").is_none(),
        "an unsorted column has no position"
    );
}

#[test]
fn a_single_sort_shows_no_position_badge() {
    let mut state = probe(TotalRowsKind::Exact);
    state.sorts = vec![SortCondition::new("size", false)];
    let rt = runtime(state, (1440.0, 816.0));
    assert!(
        rt.find("data-table-header-sort-position-1").is_none(),
        "one sort needs no number"
    );
}

#[test]
fn the_sort_menu_opens_from_the_chevron_and_sorts_from_its_items() {
    let mut rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    assert!(
        rt.find("data-table-header-sort-menu-1-item0").is_none(),
        "the menu starts closed"
    );
    click(&mut rt, "data-table-header-sort-1");
    assert_eq!(rt.state.menu, Some(1), "the chevron opened that column's menu");
    for (index, key) in [
        "dataTable.sortColumnAscending",
        "dataTable.sortColumnDescending",
        "dataTable.removeSort",
    ]
    .iter()
    .enumerate()
    {
        let item = element(&rt, &format!("data-table-header-sort-menu-1-item{index}"));
        assert_eq!(item.label.as_deref(), Some(t(key)), "menu item {index}");
    }
    click(&mut rt, "data-table-header-sort-menu-1-item1");
    assert_eq!(
        rt.state.sort_calls,
        vec![("size".to_owned(), true)],
        "the descending item reports the column"
    );
    assert_eq!(rt.state.menu, None, "choosing an item closes the menu");
}

#[test]
fn the_menu_remove_item_is_disabled_until_the_column_is_sorted() {
    let mut state = probe(TotalRowsKind::Exact);
    state.menu = Some(0);
    let mut rt = runtime(state, (1440.0, 816.0));
    click(&mut rt, "data-table-header-sort-menu-0-item2");
    assert!(
        rt.state.remove_calls.is_empty(),
        "an unsorted column has nothing to remove: {:?}",
        rt.state.remove_calls
    );

    let mut sorted = probe(TotalRowsKind::Exact);
    sorted.menu = Some(0);
    sorted.sorts = vec![SortCondition::new("name", false)];
    let mut rt = runtime(sorted, (1440.0, 816.0));
    click(&mut rt, "data-table-header-sort-menu-0-item2");
    assert_eq!(rt.state.remove_calls, vec!["name".to_owned()]);
}

#[test]
fn the_filter_button_reports_its_column() {
    let mut rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    click(&mut rt, "data-table-header-filter-2");
    assert_eq!(
        rt.state.filter_calls,
        vec!["created".to_owned()],
        "the filter button is per column"
    );
    assert!(
        rt.state.sort_calls.is_empty(),
        "filtering must not sort the column: {:?}",
        rt.state.sort_calls
    );
}

#[test]
fn the_resize_grip_reports_its_column_and_the_press_point() {
    let mut rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    let (x, y) = rt.find("data-table-header-resize-1").expect("the grip lays out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.resize_calls.len(), 1, "one press, one report");
    assert_eq!(rt.state.resize_calls[0].0, 1, "the second column's grip");
    assert!(
        (rt.state.resize_calls[0].1 - x).abs() < 1.0,
        "the press x is the pointer's: {:?} vs {x}",
        rt.state.resize_calls[0]
    );
    assert_eq!(rt.state.resize_ends, 1, "the release ends the resize");
    assert!(
        rt.state.sort_calls.is_empty(),
        "resizing must not sort the column: {:?}",
        rt.state.sort_calls
    );
}

#[test]
fn the_column_widths_come_from_the_props() {
    let rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    for (index, width) in WIDTHS.iter().enumerate() {
        let rect = element(&rt, &format!("data-table-header-col-{index}")).rect;
        assert!(
            (rect[2] - width).abs() < 0.5,
            "column {index} is {width}pt wide, got {:?}",
            rect
        );
    }
    let gutter = element(&rt, "data-table-header-gutter").rect;
    assert!((gutter[2] - 40.0).abs() < 0.5, "the gutter is 40pt: {gutter:?}");
}

#[test]
fn the_header_stays_inside_the_window() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = probe(TotalRowsKind::Exact);
            state.dark = dark;
            state.sorts = vec![
                SortCondition::new("name", false),
                SortCondition::new("size", true),
            ];
            let rt = runtime(state, (width, height));
            let theme = if dark { "dark" } else { "light" };
            let mut outside = Vec::new();
            for element in rt.elements() {
                let key = element.key.as_deref().unwrap_or(element.path.as_str());
                let [x, y, w, h] = element.rect;
                if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
                    outside.push(format!("key={key} rect={:?}", element.rect));
                }
            }
            assert!(
                outside.is_empty(),
                "header {width}x{height} {theme}: {} node(s) cross the window: {outside:#?}",
                outside.len()
            );
        }
    }
}

#[test]
fn the_strip_and_the_header_share_one_page_column() {
    let rt = runtime(probe(TotalRowsKind::Exact), (1440.0, 816.0));
    let header = element(&rt, "data-table-header").rect;
    let strip = element(&rt, "data-table-pagination").rect;
    assert!(
        (header[1] - 0.0).abs() < 0.5,
        "the header starts the page: {header:?}"
    );
    assert!(
        (strip[1] + strip[3] - 816.0).abs() < 0.5,
        "the strip ends it: {strip:?}"
    );
}
