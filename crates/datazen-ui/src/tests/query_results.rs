use super::*;
#[allow(unused_imports)]
use crate::{
    query_selected, sql_editor, type_, view, AppState, Section, Setting, APP_FONTS,
    APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX,
    NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY,
    TABLE_PAGE_SIZE,
};
#[allow(unused_imports)]
use rune::{Env, Runtime};

    #[test]
    fn the_filter_bar_narrows_the_page_and_resets_to_the_first_page() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        assert!(runtime.find("grid-filter").is_some());
        assert!(runtime.find("filter-column").is_some());
        assert!(runtime.find("filter-input").is_some());

        // Page to the second page first: applying a filter returns to the first page.
        runtime.state.next_table_page();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);

        // The column chip cycles from id (column 0) to name (column 1).
        let (x, y) = runtime.find("filter-column").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.filter_column, 1);

        let (x, y) = runtime.find("filter-input").expect("filter input");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Text("customer-3".into()), mods: Mods::default() });
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);

        assert_eq!(runtime.state.grid.page(), 0, "filtering returns to the first page");
        assert_eq!(runtime.state.grid.total_rows(), 11, "customer-3 and customer-30..39");
        assert_eq!(runtime.state.grid.rows, 11);
        assert!(runtime.state.grid.cell(0, 1).contains("customer-3"));
        assert!(runtime.state.grid.cell(10, 1).contains("customer-3"));
        assert_eq!(runtime.state.filter_editor.focused, false, "submit blurs the filter input");

        // An empty editor clears the filter and brings the whole table back.
        runtime.state.filter_editor = rune::TextEdit::new(String::new());
        runtime.state.apply_filter();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.total_rows(), 40);
        assert_eq!(runtime.state.grid.rows, 25);
    }

    #[test]
    fn commit_and_rollback_buttons_drive_the_staged_edits() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let edit = |runtime: &mut rune::Runtime<AppState>| {
            runtime.state.grid.select(0, 1);
            runtime.state.grid.begin_edit();
            runtime.state.grid.editor = rune::TextEdit::new(String::new());
            runtime.state.grid.editor.insert("renamed");
            runtime.state.commit_grid_edit();
            runtime.frame(0.0);
        };

        edit(&mut runtime);
        assert_eq!(runtime.state.grid.cell(0, 1), "renamed");
        assert_eq!(runtime.state.grid.dirty_cells(), 1);

        let (x, y) = runtime.find("rollback-staged").expect("rollback button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0", "rollback restores the original");
        assert_eq!(runtime.state.grid.dirty_cells(), 0);

        edit(&mut runtime);
        let (x, y) = runtime.find("commit-staged").expect("commit button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.cell(0, 1), "renamed");
        assert_eq!(
            runtime
                .state
                .workspace
                .open_table("customers", 0, TABLE_PAGE_SIZE, None, None)
                .unwrap()
                .rows[0][1],
            "renamed",
            "committed values survive reopening the table"
        );
    }

    #[test]
    fn a_read_only_table_blocks_editing_and_shows_its_reason() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("orders").unwrap();
        runtime.frame(0.0);

        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
        assert!(!runtime.state.grid.is_editable(), "a read-only page is not editable");
        runtime.state.grid.begin_edit();
        assert!(!runtime.state.grid.is_editing(), "a read-only page cannot enter the editor");
        assert!(runtime.find("grid-readonly").is_some(), "the reason banner is on the page");
        assert!(
            runtime.state.workspace.stage_cell(0, "name", "x".into()).is_err(),
            "staging through the port is rejected too"
        );

        // Back on a writable table the editor opens again.
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.state.grid.is_editable());
        assert!(runtime.find("grid-readonly").is_none(), "the banner leaves with the read-only page");
    }

    #[test]
    fn a_query_result_renders_in_the_shared_grid_and_cannot_stage() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);
        assert!(runtime.find("query-results").is_some(), "no result yet");

        runtime.state.load_result_page(
            vec!["id".into(), "label".into()],
            vec![
                vec!["1".into(), "one".into()],
                vec!["2".into(), "two".into()],
                vec!["3".into(), "three".into()],
            ],
        );
        runtime.settle_all();
        runtime.frame(0.0);

        assert!(runtime.find("result-grid").is_some());
        assert!(runtime.find("result-cell-0-0").is_some());
        assert!(runtime.find("result-cell-2-1").is_some());
        assert!(runtime.find("query-results").is_none(), "the placeholder leaves");
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 1), "one");
        assert!(!runtime.state.result_grid.as_ref().unwrap().is_editable());
        assert!(
            runtime.state.workspace.stage_cell(0, "name", "x".into()).is_err(),
            "the port rejects staging while the result is open"
        );

        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("cell-0-1").is_some(), "the table grid keeps its own keys");
        assert!(
            runtime.state.workspace.stage_cell(0, "name", "ok".into()).is_ok(),
            "reopening a table releases the result lock"
        );
    }

    #[test]
    fn command_z_on_a_query_tab_undoes_and_redoes_typing() {
        use rune::{Key, KeyEvent, Mods};
        use crate::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);
        runtime.state.sql.focus();

        let before = runtime.state.sql.doc().to_owned();
        runtime.key(KeyEvent { key: Key::Text("Q".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with('Q'), "{}", runtime.state.sql.doc());

        runtime.key(KeyEvent { key: Key::Command('z'), mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(runtime.state.sql.doc(), before, "undo returns the document");

        runtime.key(KeyEvent { key: Key::Command('z'), mods: Mods { shift: true, ..Mods::default() } });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with('Q'), "shift-command-z redoes");
    }

    #[test]
    fn a_result_from_an_old_generation_is_dropped() {
        use datazen_ui_api::{QueryApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);

        let first = runtime.state.next_query_generation();
        let second = runtime.state.next_query_generation();
        let stale = runtime.state.workspace.execute("SELECT * FROM CUSTOMERS").unwrap();
        runtime.state.deliver_query_result(first, stale);
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(
            runtime.state.result_grid.is_none(),
            "generation 1 arriving after generation 2 is ignored"
        );

        let fresh = runtime.state.workspace.execute("select 1").unwrap();
        runtime.state.deliver_query_result(second, fresh);
        runtime.settle_all();
        runtime.frame(0.0);
        let grid = runtime.state.result_grid.as_ref().expect("the current generation lands");
        assert_eq!(grid.cell(0, 0), "1");
        assert!(runtime.find("result-grid").is_some());
    }

    #[test]
    fn a_cancelled_generation_never_reaches_the_grid() {
        use datazen_ui_api::{QueryApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);

        let generation = runtime.state.next_query_generation();
        runtime.state.cancel_query();
        let cancelled = runtime.state.workspace.execute("select 1").unwrap();
        runtime.state.deliver_query_result(generation, cancelled);
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(
            runtime.state.result_grid.is_none(),
            "the cancelled result does not enter the grid"
        );

        // A fresh run after the cancel still lands.
        let fresh = runtime.state.next_query_generation();
        let page = runtime.state.workspace.execute("select 1").unwrap();
        runtime.state.deliver_query_result(fresh, page);
        runtime.settle_all();
        runtime.frame(0.0);
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");
    }

    #[test]
    fn the_run_button_fills_the_lower_half_with_the_result_grid() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.frame(0.0);
        assert!(runtime.find("query-results").is_some(), "no result yet");

        let (x, y) = runtime.find("sql-run").expect("run button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("result-grid").is_some(), "the result lands in the lower half");
        assert!(runtime.find("result-cell-0-0").is_some(), "the returned cell is visible");
        assert!(runtime.find("query-results").is_none(), "the placeholder leaves");
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");
    }

    #[test]
    fn command_enter_runs_the_statement_at_the_cursor() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use crate::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql =
            sql_editor::SqlEditor::from_doc("SELECT * FROM CUSTOMERS; select 1");
        runtime.state.sql.focus();
        runtime.frame(0.0);

        // The caret sits at 0: the first statement runs.
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods { cmd: true, ..Mods::default() } });
        runtime.frame(0.0);
        let grid = runtime.state.result_grid.as_ref().expect("first statement ran");
        assert_eq!(grid.rows, 40, "SELECT * FROM CUSTOMERS returns up to the query cap");
        assert_eq!(grid.cell(0, 1), "customer-0");

        // Move the caret into the second statement: only that one runs.
        let doc = runtime.state.sql.doc().to_owned();
        let second = doc.find("select 1").unwrap();
        runtime.state.sql.set_main_caret(second + 2);
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods { cmd: true, ..Mods::default() } });
        runtime.frame(0.0);
        let grid = runtime.state.result_grid.as_ref().expect("second statement ran");
        assert_eq!(grid.rows, 1);
        assert_eq!(grid.cell(0, 0), "1");
    }

    #[test]
    fn the_result_grid_takes_the_keyboard_and_copies_tsv() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use crate::app_key;

        let clipboard = rune::clipboard::memory();
        let mut runtime = runtime();
        runtime.use_clipboard(&clipboard);
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("SELECT * FROM CUSTOMERS");
        runtime.state.sql.focus();
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods { cmd: true, ..Mods::default() } });
        runtime.frame(1.0);
        assert!(runtime.state.result_grid.is_some(), "the statement ran");

        // Pressing a result cell takes the selection and the keyboard.
        let (x, y) = runtime.find("result-cell-0-1").expect("result cell");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().focus(), (0, 1));
        assert!(runtime.state.result_focused, "the result grid holds the keyboard");

        // Shift-Down and Shift-Right grow the rectangle to 2x2, and Command-C
        // puts the rectangle on the system clipboard as TSV.
        let shift = Mods { shift: true, ..Mods::default() };
        runtime.key(KeyEvent { key: Key::Down, mods: shift });
        runtime.key(KeyEvent { key: Key::Right, mods: shift });
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        runtime.frame(0.0);
        let grid = runtime.state.result_grid.as_ref().unwrap();
        let expected = format!(
            "{}\t{}\n{}\t{}",
            grid.cell(0, 1),
            grid.cell(0, 2),
            grid.cell(1, 1),
            grid.cell(1, 2),
        );
        assert_eq!(clipboard.text().as_deref().map(String::as_str), Some(expected.as_str()), "2x2 TSV lands on the clipboard");

        // Typing hands the keyboard back to the editor, and Command-V pastes
        // the clipboard text into it through the app root's paste handler.
        clipboard.set_text(" pasted");
        runtime.state.sql.set_main_caret(runtime.state.sql.doc().len());
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Text("x".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(!runtime.state.result_focused, "typing goes back to the editor");
        assert!(runtime.state.sql.doc().ends_with('x'));
        runtime.key(KeyEvent { key: Key::Command('v'), mods: Mods::default() });
        // The paste request round-trips through the action queue: one frame
        // hands it to the clipboard, the next lands the text in the editor.
        runtime.frame(0.0);
        runtime.frame(0.0);
        assert!(
            runtime.state.sql.doc().ends_with("x pasted"),
            "the editor receives the pasted text: {}",
            runtime.state.sql.doc()
        );
    }

    #[test]
    fn the_table_grid_copies_a_2x2_selection_as_tsv() {
        use rune::{Key, KeyEvent, Mods};
        use crate::app_key;

        let clipboard = rune::clipboard::memory();
        let mut runtime = runtime();
        runtime.use_clipboard(&clipboard);
        runtime.on_key = Some(app_key);
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.grid.has_page(), "the table page loaded");

        let (x, y) = runtime.find("cell-0-0").expect("first cell");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let shift = Mods { shift: true, ..Mods::default() };
        runtime.key(KeyEvent { key: Key::Down, mods: shift });
        runtime.key(KeyEvent { key: Key::Right, mods: shift });
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        runtime.frame(0.0);
        let expected = format!(
            "{}\t{}\n{}\t{}",
            runtime.state.grid.cell(0, 0),
            runtime.state.grid.cell(0, 1),
            runtime.state.grid.cell(1, 0),
            runtime.state.grid.cell(1, 1),
        );
        assert_eq!(clipboard.text().as_deref().map(String::as_str), Some(expected.as_str()), "2x2 TSV lands on the clipboard");
    }

    #[test]
    fn command_c_copies_the_editor_selection_and_command_x_cuts_it() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use crate::app_key;

        let clipboard = rune::clipboard::memory();
        let mut runtime = runtime();
        runtime.use_clipboard(&clipboard);
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("SELECT one FROM t");
        runtime.state.sql.focus();
        runtime.frame(0.0);

        // A find selects its match: Command-C copies exactly that text.
        assert!(runtime.state.sql.find_next("SELECT"));
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(clipboard.text().as_deref().map(String::as_str), Some("SELECT"));

        // Command-C with a bare caret copies nothing and leaves the clipboard.
        runtime.state.sql.set_main_caret(0);
        runtime.frame(0.0);
        clipboard.set_text("kept");
        runtime.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(clipboard.text().as_deref().map(String::as_str), Some("kept"), "no selection, no write");

        // Command-X copies and removes the selection in one history step.
        assert!(runtime.state.sql.find_next("FROM"));
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Command('x'), mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(clipboard.text().as_deref().map(String::as_str), Some("FROM"));
        assert_eq!(runtime.state.sql.doc(), "SELECT one  t");
    }

    #[test]
    fn a_failed_run_keeps_the_previous_result() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.frame(0.0);

        let (x, y) = runtime.find("sql-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.result_grid.is_some());

        runtime.state.sql = sql_editor::SqlEditor::from_doc("drop table customers");
        let (x, y) = runtime.find("sql-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.result_grid.as_ref().unwrap().cell(0, 0),
            "1",
            "a failed execution leaves the interface as it was"
        );
    }

    #[test]
    fn the_cancel_button_drops_a_result_that_arrives_after_it() {
        use datazen_ui_api::{QueryApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.frame(0.0);

        let (x, y) = runtime.find("sql-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");

        let generation = runtime.state.query_generation;
        let (x, y) = runtime.find("sql-cancel").expect("cancel button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        let late = runtime.state.workspace.execute("SELECT * FROM CUSTOMERS").unwrap();
        runtime.state.deliver_query_result(generation, late);
        runtime.settle_all();
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.result_grid.as_ref().unwrap().cell(0, 0),
            "1",
            "the cancelled delivery is dropped, the previous result stays"
        );
    }

    #[test]
    fn the_completion_list_is_drawn_in_the_canvas() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use crate::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("");
        runtime.state.sql.focus();
        runtime.frame(0.0);

        for ch in ["S", "E", "L"] {
            runtime.key(KeyEvent { key: Key::Text(ch.into()), mods: Mods::default() });
        }
        runtime.frame(0.0);
        assert!(runtime.find("sql-completion").is_some(), "the list is in the canvas");
        assert!(runtime.find("sql-completion-item-0").is_some());
        assert!(runtime.find("sql-editor-field").is_some());

        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(runtime.state.sql.doc(), "SELECT");
        assert!(runtime.find("sql-completion").is_none(), "the list leaves after the insert");
    }

    #[test]
    fn the_tree_collapses_and_a_refresh_keeps_it_collapsed() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        assert!(runtime.find("tree-db-local").is_some());
        assert!(runtime.find("tree-schema-local-public").is_some());
        assert!(runtime.find("tree-schema-local-analytics").is_some());
        assert!(runtime.find("tree-table-customers").is_some());
        assert!(runtime.find("tree-table-catalog").is_some());

        let (x, y) = runtime.find("tree-schema-local-public").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-table-customers").is_none(), "public is collapsed");
        assert!(runtime.find("tree-table-orders").is_none());
        assert!(runtime.find("tree-table-catalog").is_some(), "analytics stays open");

        let (x, y) = runtime.find("tree-refresh").expect("refresh button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.find("tree-table-customers").is_none(),
            "still collapsed after the catalog refresh"
        );

        let (x, y) = runtime.find("tree-schema-local-public").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-table-customers").is_some(), "expand brings them back");
    }

    #[test]
    fn collapsing_the_database_hides_the_whole_tree() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("tree-db-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-schema-local-public").is_none());
        assert!(runtime.find("tree-table-customers").is_none());
        assert!(runtime.find("tree-db-local").is_some(), "the database row stays");

        let (x, y) = runtime.find("tree-db-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-table-customers").is_some());
    }

    #[test]
    fn the_ddl_view_shows_the_port_string_and_an_empty_state() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        assert!(runtime.find("view-data").is_some());
        assert!(runtime.find("view-ddl").is_some());
        assert!(runtime.find("data-grid").is_some(), "data is the default view");

        let (x, y) = runtime.find("view-ddl").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("table-ddl").is_some(), "the DDL string has its own page");
        assert!(runtime.find("data-grid").is_none(), "the grid leaves");

        let (x, y) = runtime.find("view-data").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("data-grid").is_some());

        // The analytics table has no DDL: its own empty state.
        runtime.state.set_table_view(crate::TableView::Ddl);
        runtime.state.open_table_tab("catalog").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("table-ddl-empty").is_some(), "an empty DDL string is an empty state");
        assert!(runtime.find("table-ddl").is_none());
    }

    #[test]
    fn the_columns_view_lists_one_row_per_column() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("view-columns").expect("columns button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("columns-list").is_some());
        assert!(runtime.find("columns-header").is_some());
        for index in 0..8 {
            assert!(
                runtime.find(&format!("column-row-{index}")).is_some(),
                "one row per column, row {index}"
            );
        }
        assert!(runtime.find("column-row-8").is_none(), "customers has 8 columns");
        assert!(runtime.find("data-grid").is_none(), "the grid leaves for the list");
    }

    #[test]
    fn the_indexes_view_lists_the_indexes_with_ordered_columns() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("view-indexes").expect("indexes button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("indexes-list").is_some());
        assert!(runtime.find("indexes-header").is_some());
        assert!(runtime.find("index-row-0").is_some());
        assert!(runtime.find("index-row-1").is_some());
        assert!(runtime.find("index-row-2").is_none(), "customers has two indexes");
        let indexes = runtime.state.workspace.table_indexes("customers").unwrap();
        assert_eq!(indexes[1].columns, vec!["name", "city"], "the compound order is in the row");
    }
