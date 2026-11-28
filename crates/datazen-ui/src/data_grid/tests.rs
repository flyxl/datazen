use super::view::status_line;
use super::{DataGrid, COL_W, COL_WINDOW};
use rune::TextEdit;

#[test]
fn a_mounted_row_builds_a_window_not_every_column() {
    let grid = DataGrid::new();
    let cols = grid.built_columns();
    assert_eq!(cols.len(), COL_WINDOW as usize);
    assert!(cols.contains(&0));
    assert!(!cols.contains(&40));
}

#[test]
fn scroll_offset_moves_the_column_window_and_keeps_the_frozen_column() {
    let mut grid = DataGrid::new();
    grid.scroll_x().set(COL_W * 50.0 + 4.0);
    assert!(grid.sync_scroll());
    assert_eq!(grid.origin_col(), 50);
    let cols = grid.built_columns();
    assert!(cols.contains(&0), "frozen column stays built");
    assert!(cols.contains(&50));
    assert!(!cols.contains(&11));
    assert!(cols.len() <= COL_WINDOW as usize + 1);
}

#[test]
fn paste_writes_the_rectangle_and_copy_reads_it_back() {
    let mut grid = DataGrid::new();
    grid.select(2, 3);
    grid.paste_tsv("A\tB\nC\tD");
    assert_eq!(grid.cell(2, 3), "A");
    assert_eq!(grid.cell(2, 4), "B");
    assert_eq!(grid.cell(3, 3), "C");
    assert_eq!(grid.cell(3, 4), "D");
    assert_eq!(grid.cell(2, 5), "r2c5");
    grid.focus = (3, 4);
    grid.anchor = (2, 3);
    assert_eq!(grid.copy_tsv(), "A\tB\nC\tD");
}

#[test]
fn a_cell_edited_back_to_its_original_value_is_not_dirty() {
    let mut grid = DataGrid::new();
    assert_eq!(grid.dirty_cells(), 0);
    grid.select(2, 2);
    grid.begin_edit();
    grid.editor.insert("x");
    grid.commit_edit();
    assert_eq!(grid.cell(2, 2), "r2c2x");
    assert_eq!(grid.dirty_cells(), 1, "an edited cell is dirty");

    grid.begin_edit();
    grid.editor = TextEdit::new(String::new());
    grid.editor.insert("r2c2");
    grid.commit_edit();
    assert_eq!(grid.cell(2, 2), "r2c2");
    assert_eq!(grid.dirty_cells(), 0, "edited back to the original value");
}

#[test]
fn the_status_line_counts_dirty_cells_not_edits() {
    let mut grid = DataGrid::new();
    grid.select(1, 1);
    grid.begin_edit();
    grid.editor.insert("z");
    grid.commit_edit();
    grid.begin_edit();
    grid.editor = TextEdit::new(String::new());
    grid.editor.insert("r1c1");
    grid.commit_edit();
    let status = status_line(&grid);
    assert!(status.contains("0 dirty"), "status says 0 dirty: {status}");
    assert!(status.contains("selection r1c1..r1c1"), "selection stays: {status}");
}

#[test]
fn loading_the_next_page_keeps_the_column_window_and_the_frozen_column() {
    let columns: Vec<String> = (0..30).map(|i| format!("c{i}")).collect();
    let rows: Vec<Vec<String>> =
        (0..50).map(|r| columns.iter().map(|c| format!("{c}-{r}")).collect()).collect();
    let page = |rows: Vec<Vec<String>>, page: u32| datazen_ui_api::TablePage {
        table: "t".into(),
        columns: columns.clone(),
        rows,
        page,
        page_size: 25,
        total_rows: 50,
        sort: None,
        filter: None,
        primary_key: Vec::new(),
        read_only: None,
    };
    let mut grid = DataGrid::new();
    grid.load_page(&page(rows[..25].to_vec(), 0));
    grid.scroll_x().set(COL_W * 20.0);
    assert!(grid.sync_scroll());
    assert_eq!(grid.origin_col(), 20);

    grid.load_page(&page(rows[25..].to_vec(), 1));
    assert_eq!(grid.page(), 1);
    assert_eq!(grid.pages(), 2);
    assert_eq!(grid.origin_col(), 0, "a fresh page starts at the first column");
    grid.scroll_x().set(COL_W * 20.0);
    assert!(grid.sync_scroll());
    let cols = grid.built_columns();
    assert!(cols.contains(&0), "the frozen column stays");
    assert!(cols.contains(&20));
}

#[test]
fn editing_commits_into_the_cell_and_survives_as_data() {
    let mut grid = DataGrid::new();
    grid.select(4, 6);
    grid.begin_edit();
    grid.editor.insert("雪");
    grid.commit_edit();
    assert!(!grid.is_editing());
    assert_eq!(grid.cell(4, 6), "r4c6雪");
}

fn sample_result_grid() -> DataGrid {
    let page = datazen_ui_api::TablePage {
        table: "result".into(),
        columns: vec!["id".into(), "name".into(), "note".into()],
        rows: vec![
            vec!["1".into(), "alice".into(), "hello".into()],
            vec!["2".into(), "bob".into(), "ALICE again".into()],
            vec!["3".into(), "carol".into(), "world".into()],
        ],
        page: 0,
        page_size: 50,
        total_rows: 3,
        sort: None,
        filter: None,
        primary_key: Vec::new(),
        read_only: Some("result".into()),
    };
    let mut grid = DataGrid::new();
    grid.load_page(&page);
    grid
}

#[test]
fn find_sets_hits_in_row_major_order_and_selects_first_hit() {
    let mut grid = sample_result_grid();
    grid.set_find_needle("alice");
    assert_eq!(grid.find_needle(), "alice");
    assert_eq!(grid.find_hit_count(), 2);
    assert_eq!(grid.find_current_index(), 1);
    assert_eq!(grid.current_find_hit(), Some((0, 1)));
    assert_eq!(grid.focus(), (0, 1), "focus moves to the current hit");
    assert!(grid.is_find_hit(0, 1));
    assert!(grid.is_find_hit(1, 2));
    assert!(grid.is_current_find_hit(0, 1));
    assert!(!grid.is_current_find_hit(1, 2));
}

#[test]
fn find_next_and_prev_navigate_hits_and_wrap() {
    let mut grid = sample_result_grid();
    grid.set_find_needle("alice");
    assert!(grid.find_next());
    assert_eq!(grid.current_find_hit(), Some((1, 2)));
    assert_eq!(grid.find_current_index(), 2);
    assert!(grid.find_next(), "wraps to first");
    assert_eq!(grid.current_find_hit(), Some((0, 1)));
    assert!(grid.find_prev(), "wraps to last");
    assert_eq!(grid.current_find_hit(), Some((1, 2)));
    assert!(grid.find_prev());
    assert_eq!(grid.current_find_hit(), Some((0, 1)));
}

#[test]
fn find_is_case_insensitive_and_trims_empty_needle() {
    let mut grid = sample_result_grid();
    grid.set_find_needle("ALICE");
    assert_eq!(grid.find_hit_count(), 2);
    grid.set_find_needle("   ");
    assert_eq!(grid.find_hit_count(), 0);
    assert_eq!(grid.find_current_index(), 0);
    assert_eq!(grid.current_find_hit(), None);
}

#[test]
fn find_with_no_matches_reports_zero_and_navigation_is_false() {
    let mut grid = sample_result_grid();
    grid.set_find_needle("zzz");
    assert_eq!(grid.find_hit_count(), 0);
    assert_eq!(grid.find_current_index(), 0);
    assert!(!grid.find_next());
    assert!(!grid.find_prev());
    assert!(!grid.is_find_hit(0, 0));
}

#[test]
fn clear_find_resets_hits_and_new_page_clears_find() {
    let mut grid = sample_result_grid();
    grid.set_find_needle("alice");
    assert_eq!(grid.find_hit_count(), 2);
    grid.clear_find();
    assert_eq!(grid.find_hit_count(), 0);
    assert_eq!(grid.find_needle(), "");
    grid.set_find_needle("bob");
    assert_eq!(grid.find_hit_count(), 1);
    grid.load_page(&datazen_ui_api::TablePage {
        table: "result".into(),
        columns: vec!["x".into()],
        rows: vec![vec!["bob".into()]],
        page: 0,
        page_size: 50,
        total_rows: 1,
        sort: None,
        filter: None,
        primary_key: Vec::new(),
        read_only: Some("result".into()),
    });
    assert_eq!(grid.find_hit_count(), 0, "loading a page clears find");
    assert_eq!(grid.find_needle(), "");
}

#[test]
fn native_table_pager_distinguishes_exact_estimated_and_unknown_counts() {
    use super::view::{table_count_text, table_page_text};
    use crate::i18n::t;
    use datazen_ui_api::{NativeTablePage, NativeTableRowCount as Count, TablePage};

    let mut grid = DataGrid::new();
    let mut read = NativeTablePage {
        column_types: vec!["INTEGER".into()],
        page: TablePage {
            table: "scoped".into(),
            columns: vec!["id".into()],
            rows: vec![vec!["a".into()], vec!["b".into()]],
            page: 0,
            page_size: 2,
            total_rows: 0,
            sort: None,
            filter: None,
            primary_key: vec!["id".into()],
            read_only: Some("read only fixture".into()),
        },
        row_count: Count::Estimated(1),
        bounded_offset: true,
    };
    grid.load_native_table_page(&read);
    assert_eq!(grid.column_type(0), Some("INTEGER"));
    assert!(grid.can_next_page(), "a small estimate is not a paging bound");
    assert_eq!(table_page_text(&grid), "Page 1");
    assert_eq!(
        table_count_text(&grid),
        format!(
            "{} {}",
            t("pagination.rowCountEstimated").replace("{count}", "1"),
            t("connWin.rowCount"),
        ),
    );
    assert_eq!(grid.read_only_reason(), Some("read only fixture"));

    read.page.page = 1;
    read.row_count = Count::Unknown;
    grid.load_native_table_page(&read);
    assert!(grid.can_next_page(), "a full unknown-total offset page can advance");
    assert_eq!(table_page_text(&grid), "Page 2");
    assert_eq!(table_count_text(&grid), t("pagination.rowsUnknown"));

    read.bounded_offset = false;
    grid.load_native_table_page(&read);
    assert!(!grid.can_next_page(), "do not guess after a cursor or unpaged read");
    read.bounded_offset = true;
    read.page.rows.pop();
    grid.load_native_table_page(&read);
    assert!(!grid.can_next_page(), "a short offset page stops even with no count");
    read.page.rows.clear();
    read.row_count = Count::Estimated(100);
    grid.load_native_table_page(&read);
    assert!(!grid.can_next_page(), "a large estimate cannot advance an empty page");

    read.page.page = 0;
    read.row_count = Count::Exact(0);
    grid.load_native_table_page(&read);
    assert!(!grid.can_next_page());
    assert_eq!(table_page_text(&grid), "Page 1 / 0");
    assert_eq!(table_count_text(&grid), "0 rows");

    read.page.total_rows = 12;
    grid.load_page(&read.page);
    assert!(grid.can_next_page(), "legacy/query load resets nonexact metadata");
    assert_eq!(grid.row_count(), Count::Exact(12));
    assert_eq!(table_page_text(&grid), "Page 1 / 6");
    assert_eq!(table_count_text(&grid), "12 rows");
}


#[test]
fn native_table_column_types_cannot_outlive_their_page() {
    use datazen_ui_api::{NativeTablePage, NativeTableRowCount, TablePage};

    let mut grid = DataGrid::new();
    let mut read = NativeTablePage {
        page: TablePage {
            table: "first".into(),
            columns: vec!["same_name".into(), "undeclared".into()],
            rows: Vec::new(),
            page: 0,
            page_size: 50,
            total_rows: 0,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        },
        column_types: vec!["INTEGER".into(), "".into()],
        row_count: NativeTableRowCount::Exact(0),
        bounded_offset: true,
    };
    grid.load_native_table_page(&read);
    assert_eq!((grid.column_type(0), grid.column_type(1)), (Some("INTEGER"), None));
    assert!(grid.is_declared_numeric(0));
    assert!(!grid.is_declared_numeric(1));

    read.page.table = "second".into();
    read.column_types = vec!["TEXT".into(), "   ".into()];
    grid.load_native_table_page(&read);
    assert_eq!((grid.column_type(0), grid.column_type(1)), (Some("TEXT"), None));
    assert!(!grid.is_declared_numeric(0));

    read.column_types.pop();
    grid.load_native_table_page(&read);
    assert_eq!(grid.column_type(0), None, "unmatched metadata is not projected");

    read.column_types.push("BLOB".into());
    grid.load_native_table_page(&read);
    assert_eq!(grid.column_type(1), Some("BLOB"));
    grid.load_page(&read.page);
    assert_eq!((grid.column_type(0), grid.column_type(1)), (None, None));
}

#[test]
fn a_measured_live_table_window_covers_narrow_columns_and_keeps_raw_pin_order() {
    let mut grid = DataGrid::new();
    grid.set_column_widths(vec![40; grid.cols as usize]);
    grid.scroll_x().set(804.0);
    assert!(grid.sync_scroll());
    assert_eq!(grid.origin_col(), 20);
    let unpinned = grid.built_columns_with_pin(0, Some(1000.0));
    assert!(!unpinned.contains(&0));
    assert!(unpinned.contains(&45), "far partially visible column");
    let pinned = grid.built_columns_with_pin(3, Some(1000.0));
    assert_eq!(&pinned[..3], &[0, 1, 2]);
    assert_eq!(pinned[3..], unpinned);
    let overlapping = grid.built_columns_with_pin(25, Some(1000.0));
    assert!(overlapping.windows(2).all(|pair| pair[0] + 1 == pair[1]));
}
