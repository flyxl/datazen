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
