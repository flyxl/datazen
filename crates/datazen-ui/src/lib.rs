//! Shared DataZen UI entry point for native and WebAssembly hosts.
//!
//! The page shell is deliberately independent of DataZen's transport and storage
//! implementations. Platform adapters will be added behind typed UI ports.

mod data_grid;
mod sql_editor;

use data_grid::DataGrid;
use datazen_ui_api::{ColumnFilter, ColumnSort, MemoryWorkspace, TabKind, Tabs, WorkspaceApi};
use rune::tok::{self, type_};
use rune::{App, AppSpec, Cx, El, Ime, KeyEvent, TextEdit, Tx, col, row, text};
use sql_editor::SqlEditor;

/// Rows asked from the workspace for one table page.
pub const TABLE_PAGE_SIZE: u32 = 25;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Section {
    #[default]
    Connections,
    Workspace,
    Settings,
}

impl Section {
    const ALL: [Self; 3] = [Self::Connections, Self::Workspace, Self::Settings];

    const fn key(self) -> &'static str {
        match self {
            Self::Connections => "connections",
            Self::Workspace => "workspace",
            Self::Settings => "settings",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Connections => "Connections",
            Self::Workspace => "Workspace",
            Self::Settings => "Settings",
        }
    }
}

struct AppState {
    active: Section,
    sql: SqlEditor,
    grid: DataGrid,
    workspace: MemoryWorkspace,
    tabs: Tabs,
    /// The page the workspace last opened for the selected table tab.
    table_page: u32,
    /// The sort the table page is opened with.
    sort: Option<ColumnSort>,
    /// The filter the table page is opened with.
    filter: Option<ColumnFilter>,
    /// The column the filter bar targets, as an index into the loaded page.
    filter_column: u32,
    filter_editor: TextEdit,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active: Section::Connections,
            sql: SqlEditor::new(),
            grid: DataGrid::new(),
            workspace: MemoryWorkspace::new(),
            tabs: Tabs::new(),
            table_page: 0,
            sort: None,
            filter: None,
            filter_column: 0,
            filter_editor: TextEdit::new(String::new()),
        }
    }
}

impl AppState {
    fn select(&mut self, section: Section) {
        self.active = section;
    }

    fn open_connection(&mut self, id: &str) {
        if self.workspace.connect(id).is_err() {
            return;
        }
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        if let Ok(page) = self.workspace.open_table("customers", 0, TABLE_PAGE_SIZE, None, None) {
            self.grid.load_page(&page);
            self.grid.set_editable(true);
            self.active = Section::Workspace;
        }
    }

    fn disconnect(&mut self) {
        self.workspace.disconnect();
        self.tabs = Tabs::new();
        self.grid.set_editable(false);
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
    }

    fn open_table_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Table, title)?;
        self.sync_grid_to_selected_tab();
        Some(id)
    }

    fn open_query_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Query, title)?;
        self.sync_grid_to_selected_tab();
        Some(id)
    }

    fn select_tab(&mut self, id: u32) -> bool {
        if !self.tabs.select(id) {
            return false;
        }
        self.sync_grid_to_selected_tab();
        true
    }

    fn show_table(&mut self, title: &str) -> bool {
        match self.workspace.open_table(
            title,
            self.table_page,
            TABLE_PAGE_SIZE,
            self.sort.as_ref(),
            self.filter.as_ref(),
        ) {
            Ok(page) => {
                self.table_page = page.page;
                self.grid.load_page(&page);
                self.grid.set_editable(true);
                self.active = Section::Workspace;
                true
            }
            Err(_) => false,
        }
    }

    /// Reopen the selected table tab at the current page, sort and filter.
    fn reload_table_page(&mut self) {
        if let Some(tab) = self.tabs.selected() {
            if tab.kind == TabKind::Table {
                self.show_table(&tab.title.clone());
            }
        }
    }

    fn next_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() + 1 >= self.grid.pages() {
            return;
        }
        self.table_page += 1;
        self.reload_table_page();
    }

    fn prev_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() == 0 {
            return;
        }
        self.table_page = self.table_page.saturating_sub(1);
        self.reload_table_page();
    }

    /// A header click cycles its column: none, ascending, descending, back to none.
    fn cycle_sort(&mut self, column: u32) {
        let Some(name) = self.grid.column_name(column).map(str::to_owned) else {
            return;
        };
        let next = match &self.sort {
            Some(sort) if sort.column == name && !sort.descending => {
                Some(ColumnSort { column: name, descending: true })
            }
            Some(sort) if sort.column == name => None,
            _ => Some(ColumnSort { column: name, descending: false }),
        };
        self.sort = next;
        self.table_page = 0;
        self.reload_table_page();
    }

    /// The filter bar's Enter: read the editor, target the chosen column, return to page 1.
    fn apply_filter(&mut self) {
        let column = self.grid.column_name(self.filter_column).map(str::to_owned);
        let text = self.filter_editor.text.trim().to_owned();
        self.filter = match (column, text.is_empty()) {
            (Some(column), false) => Some(ColumnFilter { column, contains: text }),
            _ => None,
        };
        self.table_page = 0;
        self.reload_table_page();
    }

    fn cycle_filter_column(&mut self) {
        if self.grid.cols == 0 {
            return;
        }
        self.filter_column = (self.filter_column + 1) % self.grid.cols;
        if self.filter.is_some() {
            self.apply_filter();
        }
    }

    /// Commit every staged edit to the stored rows, then reload the page.
    fn commit_staged_edits(&mut self) {
        if self.workspace.commit_staged().is_ok() {
            self.reload_table_page();
        }
    }

    /// Drop every staged edit and reload the page back to the stored values.
    fn rollback_staged_edits(&mut self) {
        self.workspace.rollback_staged();
        self.reload_table_page();
    }

    /// Any tab event lands in the workspace: the selected tab decides what the
    /// workspace shows, so the tab strip and its content stay on one page.
    fn sync_grid_to_selected_tab(&mut self) {
        self.active = Section::Workspace;
        self.table_page = 0;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        let selected = self.tabs.selected().map(|tab| (tab.kind, tab.title.clone()));
        match selected {
            Some((TabKind::Table, title)) => {
                self.show_table(&title);
            }
            Some((TabKind::Query, _)) | None => {}
        }
    }

    fn close_tab(&mut self, id: u32) -> bool {
        let previous = self.tabs.selected().map(|tab| tab.id);
        if !self.tabs.close(id) {
            return false;
        }
        if self.tabs.selected().map(|tab| tab.id) != previous {
            self.sync_grid_to_selected_tab();
        }
        true
    }

    fn commit_grid_edit(&mut self) {
        if !self.grid.is_editable() {
            return;
        }
        let (row, col) = self.grid.focus();
        let column = self.grid.column_name(col).map(str::to_owned);
        self.grid.commit_edit();
        if let Some(column) = column {
            let value = self.grid.cell(row, col);
            let _ = self.workspace.stage_cell(row, &column, value);
        }
    }
}

fn navigation_item(state: &AppState, section: Section, dark: bool) -> El {
    let theme = tok::theme(dark);
    let selected = state.active == section;

    row()
        .key(format!("nav-{}", section.key()))
        .w_full()
        .h(36.0)
        .px(12.0)
        .center_y()
        .radius(7.0)
        .fill(if selected {
            tok::accent(0, dark)
        } else {
            theme.control_off
        })
        .child(
            text(section.label())
                .font(type_::LABEL)
                .color(theme.text),
        )
        .role("tab")
        .label(section.label())
        .on_press(move |state: &mut AppState| state.select(section))
}
fn query_selected(state: &AppState) -> bool {
    state.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Query)
}

fn page_body(state: &AppState, theme: tok::Theme) -> El {
    match state.active {
        Section::Connections => connection_list(state, theme),
        Section::Workspace => workspace_body(state, theme),
        Section::Settings => settings_page(theme),
    }
}

/// The workspace is driven by the tabs: a query tab shows the SQL editor above the
/// results, a loaded table page shows the grid, and anything else is an empty state.
/// The old standalone editor and demo-grid sections are no longer product pages.
fn workspace_body(state: &AppState, theme: tok::Theme) -> El {
    if query_selected(state) {
        return query_workspace(state, theme);
    }
    if state.grid.has_page() {
        return data_grid::panel(state, theme.text, theme.text_secondary, theme.control_off);
    }
    workspace_empty(theme)
}

fn workspace_empty(theme: tok::Theme) -> El {
    col()
        .key("workspace-empty")
        .w_full()
        .h(220.0)
        .px(20.0)
        .py(20.0)
        .gap(12.0)
        .radius(12.0)
        .fill(theme.control_off)
        .child(text("No tab open").font(type_::TITLE).color(theme.text))
        .child(
            text("Open a table from the session object tree, or start a new query from the sidebar.")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .w_full(),
        )
}

fn settings_page(theme: tok::Theme) -> El {
    col()
        .key("settings-empty")
        .w_full()
        .h(220.0)
        .px(20.0)
        .py(20.0)
        .gap(12.0)
        .radius(12.0)
        .fill(theme.control_off)
        .child(text("Settings are not wired into the Rune workspace yet").font(type_::TITLE).color(theme.text))
        .child(
            text("This page arrives with the settings port. The existing settings screen is unchanged.")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .w_full(),
        )
}

fn tab_strip(state: &AppState, theme: tok::Theme) -> El {
    let mut strip = row().key("tab-strip").w_full().h(32.0).gap(6.0).center_y();
    for tab in state.tabs.tabs() {
        let id = tab.id;
        let selected = state.tabs.selected().is_some_and(|open| open.id == id);
        strip = strip.child(
            row()
                .key(format!("tab-{id}"))
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(if selected { tok::accent(0, true) } else { theme.control_off })
                .child(text(tab.title.clone()).font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| {
                    state.select_tab(id);
                }),
        );
        strip = strip.child(
            row()
                .key(format!("tab-close-{id}"))
                .h(28.0)
                .px(6.0)
                .center_y()
                .child(text("×").font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| {
                    state.close_tab(id);
                }),
        );
    }
    strip
}

fn object_tree(state: &AppState, theme: tok::Theme) -> El {
    let tables = state.workspace.session().map(|session| session.tables.clone()).unwrap_or_default();
    let rows = tables.len() as f32;
    let height = if rows == 0.0 { 0.0 } else { rows * 28.0 + (rows - 1.0) * 4.0 };
    let mut tree = col().key("object-tree").w_full().h(height).gap(4.0);
    for name in tables {
        let title = name.clone();
        tree = tree.child(
            row()
                .key(format!("tree-table-{name}"))
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(name).font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| {
                    state.open_table_tab(&title);
                }),
        );
    }
    tree
}

fn query_workspace(state: &AppState, theme: tok::Theme) -> El {
    col()
        .key("query-tab")
        .w_full()
        .gap(8.0)
        .child(sql_editor::panel(&state.sql, theme.text, theme.text_secondary))
        .child(
            col()
                .key("query-results")
                .w_full()
                .h(120.0)
                .px(12.0)
                .py(8.0)
                .radius(8.0)
                .fill(theme.control_off)
                .child(text("No results").font(type_::LABEL).color(theme.text_secondary)),
        )
}

fn open_query_button(theme: tok::Theme) -> El {
    row()
        .key("open-query")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("New query").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| {
            state.open_query_tab("Query");
        })
}

fn disconnect_button(state: &AppState, theme: tok::Theme) -> El {
    if state.workspace.session().is_none() {
        return row().key("disconnect-absent").h(0.0);
    }
    row()
        .key("disconnect")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Disconnect").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.disconnect())
}

fn connection_list(state: &AppState, theme: tok::Theme) -> El {
    let mut list = col().key("connection-list").w_full().gap(8.0);
    for profile in state.workspace.connections() {
        let id = profile.id.clone();
        let label = format!("{} · {}", profile.name, profile.engine);
        list = list.child(
            row()
                .key(format!("connection-{id}"))
                .w_full()
                .h(40.0)
                .px(12.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .child(text(label).font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| state.open_connection(&id)),
        );
    }
    list
}

fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);

    let mut navigation = col().w_full().gap(4.0);
    for section in Section::ALL {
        navigation = navigation.child(navigation_item(state, section, dark));
    }

    let sidebar = col()
        .key("datazen-sidebar")
        .w(224.0)
        .h_full()
        .px(16.0)
        .py(20.0)
        .gap(18.0)
        .fill(theme.bg)
        .child(
            text("DataZen")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            text("DATABASE WORKSPACE")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(navigation)
        .child(object_tree(state, theme))
        .child(open_query_button(theme))
        .child(disconnect_button(state, theme))
        .child(
            col()
                .w_full()
                .h(1.0)
                .fill(theme.control_off),
        )
        .child(
            text("Rune native UI preview")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );

    let header = row()
        .key("datazen-header")
        .w_full()
        .h(48.0)
        .center_y()
        .gap(12.0)
        .child(
            text(state.active.label())
                .font(type_::TITLE)
                .color(theme.text)
                .w_full(),
        )
        .child(
            text(if dark { "DARK" } else { "LIGHT" })
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );

    let page = col()
        .key("datazen-page")
        .w_full()
        .h_full()
        .px(24.0)
        .py(24.0)
        .gap(16.0)
        .fill(theme.bg)
        .child(header)
        .child(if state.active == Section::Workspace {
            tab_strip(state, theme)
        } else {
            row().key("tab-strip-absent").h(0.0)
        })
        .child(page_body(state, theme))
        .child(
            row()
                .w_full()
                .h(28.0)
                .center_y()
                .child(
                    text("Migration preview · existing React and Tauri entrypoints are unchanged")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );

    row()
        .key("datazen-root")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(sidebar)
        .child(page)
}

/// Launch the same Rune view on native desktop and WebAssembly.
pub fn run() {
    App::new(AppSpec {
        on_key: Some(app_key),
        poll: Some(app_poll),
        ..AppSpec::new("DataZen", (1440.0, 900.0), AppState::default, view)
    })
    .on_ime(app_ime)
    .run();
}

fn app_key(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if state.active != Section::Workspace {
        return false;
    }
    if query_selected(state) {
        return sql_editor::on_key(&mut state.sql, ev);
    }
    if state.grid.has_page() {
        return data_grid::on_key(&mut state.grid, ev, tx);
    }
    false
}

fn app_poll(state: &mut AppState, _: f64, _: &mut Tx) -> bool {
    state.active == Section::Workspace
        && !query_selected(state)
        && state.grid.has_page()
        && state.grid.sync_scroll()
}

fn app_ime(state: &mut AppState, ev: &Ime, _: &mut Tx) -> bool {
    state.active == Section::Workspace && query_selected(state) && sql_editor::on_ime(&mut state.sql, ev)
}

/// WebAssembly entrypoint consumed by wasm-bindgen.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    run();
}

#[cfg(test)]
mod tests {
    use super::{AppState, Section, TABLE_PAGE_SIZE, query_selected, view};
    use rune::{Env, Runtime};

    fn runtime() -> Runtime<AppState> {
        let env = Env {
            system_dark: true,
            scale: 1.0,
        };
        Runtime::new(AppState::default(), view, (1440.0, 900.0), env)
    }

    #[test]
    fn shell_renders_the_datazen_navigation() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        assert!(runtime.find("datazen-root").is_some());
        for section in Section::ALL {
            let id = format!("nav-{}", section.key());
            assert!(runtime.find(&id).is_some(), "missing navigation node: {id}");
        }
    }

    #[test]
    fn the_sidebar_lists_only_connections_workspace_and_settings() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        for key in ["connections", "workspace", "settings"] {
            let id = format!("nav-{key}");
            assert!(runtime.find(&id).is_some(), "missing navigation node: {id}");
        }
        for key in ["sql-editor", "data-browser", "schema", "workflows"] {
            let id = format!("nav-{key}");
            assert!(runtime.find(&id).is_none(), "{id} is not a product page anymore");
        }
    }

    #[test]
    fn clicking_navigation_changes_the_active_section() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime
            .find("nav-workspace")
            .expect("Workspace navigation item should exist");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);

        assert_eq!(runtime.state.active, Section::Workspace);
    }

    #[test]
    fn a_workspace_without_a_tab_or_a_loaded_page_is_not_the_demo_grid() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("nav-workspace").expect("workspace navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Workspace);
        assert!(runtime.find("workspace-empty").is_some(), "the workspace has an empty state");
        assert!(runtime.find("data-grid").is_none(), "the POC demo grid is not a product page");
        assert!(runtime.find("grid-row-0").is_none(), "no demo row is mounted");
        assert!(runtime.find("datazen-preview-card").is_none(), "the placeholder card is gone");
    }

    #[test]
    fn sql_editor_scrolls_a_surface_wider_than_the_viewport() {
        let mut runtime = runtime();
        runtime.state.open_query_tab("Query");
        runtime.frame(0.0);

        let field = runtime.presented("sql-editor-field").expect("editor field");
        let scroll = runtime.presented("sql-editor-scroll").expect("editor scroll");
        assert!(field.0[2] > scroll.0[2] + 100.0, "field {} viewport {}", field.0[2], scroll.0[2]);
    }

    #[test]
    fn chinese_ime_is_not_part_of_the_document_until_commit() {
        use super::sql_editor::on_key;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(|state, ev, _| {
            state.active == Section::Workspace && query_selected(state) && on_key(&mut state.sql, ev)
        });
        runtime.state.open_query_tab("Query");
        runtime.state.sql.focus();
        runtime.frame(0.0);
        assert!(runtime.presented("sql-caret").is_some(), "main caret is the input-method cursor");

        let before = runtime.state.sql.doc().to_owned();
        runtime.state.sql.preedit("中");
        runtime.frame(0.0);
        assert_eq!(runtime.state.sql.doc(), before);
        assert_eq!(runtime.state.sql.composition(), Some("中"));

        runtime.state.sql.commit("中文");
        runtime.frame(0.0);
        assert!(runtime.state.sql.composition().is_none());
        assert!(runtime.state.sql.doc().starts_with("中文"), "{}", runtime.state.sql.doc());

        runtime.key(KeyEvent { key: Key::Text("Q".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with("中文Q"), "{}", runtime.state.sql.doc());
    }

    #[test]
    fn data_grid_mounts_a_row_window_and_a_column_window() {
        use super::app_poll;
        use datazen_ui_api::TablePage;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        let columns: Vec<String> = (0..200).map(|index| format!("c{index}")).collect();
        let rows: Vec<Vec<String>> = (0..6000)
            .map(|row| columns.iter().map(|col| format!("{col}-{row}")).collect())
            .collect();
        runtime.state.grid.load_page(&TablePage {
            table: "wide".into(),
            columns,
            rows,
            page: 0,
            page_size: 6000,
            total_rows: 6000,
            sort: None,
            filter: None,
        });
        runtime.state.active = Section::Workspace;
        runtime.frame(0.0);

        assert!(runtime.find("grid-row-0").is_some());
        assert!(runtime.find("cell-0-0").is_some());
        assert!(runtime.find("cell-0-11").is_some());
        assert!(runtime.find("grid-row-5000").is_none(), "rows far outside the viewport are not built");
        assert!(runtime.find("cell-0-40").is_none(), "columns outside the window are not built");

        let mut mounted = 0;
        for index in 0..200 {
            if runtime.find(&format!("grid-row-{index}")).is_some() {
                mounted += 1;
            }
        }
        assert!(mounted > 0 && mounted < 80, "mounted rows {mounted}");

        runtime.state.grid.scroll_x().set(super::data_grid::COL_W * 50.0);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.origin_col(), 50);
        assert!(runtime.find("cell-0-0").is_some(), "frozen column");
        assert!(runtime.find("cell-0-50").is_some());
        assert!(runtime.find("cell-0-11").is_none());
    }

    #[test]
    fn data_grid_edits_a_cell_and_copies_the_selection() {
        use super::data_grid::on_key;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(|state, ev, tx| {
            state.active == Section::Workspace && !query_selected(state) && on_key(&mut state.grid, ev, tx)
        });
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("cell-0-1").expect("cell");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.focus(), (0, 1));

        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.grid.is_editing());
        // The second Enter is the production commit: the focused input's on_submit
        // commits the cell and blurs, so later Command keys still reach the grid.
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert!(!runtime.state.grid.is_editing());

        runtime.state.grid.paste_tsv("A\tB");
        assert_eq!(runtime.state.grid.cell(0, 1), "A");
        assert_eq!(runtime.state.grid.cell(0, 2), "B");
        runtime.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        runtime.frame(0.0);
        let copied = runtime.copy_request.as_ref().map(|copy| copy.content().text().to_owned());
        assert_eq!(copied.as_deref(), Some("A"));
    }

    #[test]
    fn opening_a_connection_loads_the_table_page_into_the_grid() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Workspace);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert_eq!(runtime.state.grid.column_name(1), Some("name"));
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor.insert("-edited");
        runtime.state.commit_grid_edit();
        assert_eq!(
            runtime.state.workspace.open_table("customers", 0, TABLE_PAGE_SIZE, None, None).unwrap().rows[0][1],
            "customer-0-edited"
        );
    }

    #[test]
    fn opening_a_table_loads_the_first_page_of_the_table() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let grid = &runtime.state.grid;
        assert_eq!(grid.page(), 0);
        assert_eq!(grid.page_size(), TABLE_PAGE_SIZE);
        assert_eq!(grid.total_rows(), 40);
        assert_eq!(grid.rows, 25, "only the page is loaded, not the whole table");
        assert_eq!(grid.cell(24, 1), "customer-24");
        assert_eq!(grid.cell(0, 1), "customer-0");
    }

    #[test]
    fn the_footer_pages_through_the_table_and_stops_at_the_edges() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        assert!(runtime.find("grid-footer").is_some());
        assert!(runtime.find("page-prev").is_some());
        assert!(runtime.find("page-next").is_some());

        let (x, y) = runtime.find("page-prev").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0, "prev on the first page is a no-op");

        let (x, y) = runtime.find("page-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);
        assert_eq!(runtime.state.grid.rows, 15);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-25");

        let (x, y) = runtime.find("page-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1, "next on the last page is a no-op");

        let (x, y) = runtime.find("page-prev").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert!(runtime.find("cell-0-1").is_some(), "the frozen column is still built");
    }

    #[test]
    fn clicking_a_header_cycles_none_ascending_descending() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        // Page to the second page first: a sort must return to the first page.
        runtime.state.next_table_page();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);

        let (x, y) = runtime.find("head-1").expect("name header");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0, "sorting returns to the first page");
        assert_eq!(runtime.state.grid.sort(), Some((1, false)));
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert_eq!(runtime.state.grid.cell(2, 1), "customer-10", "lexicographic ascending order");

        let (x, y) = runtime.find("head-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.sort(), Some((1, true)));
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-9", "descending starts at the max");
        assert_eq!(runtime.state.grid.cell(6, 1), "customer-39");

        let (x, y) = runtime.find("head-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.sort(), None);
        assert_eq!(runtime.state.grid.cell(2, 1), "customer-2", "storage order returns");
    }

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
    fn tab_strip_selects_and_closes_table_tabs() {
        let mut runtime = runtime();
        let first = runtime.state.open_table_tab("customers").unwrap();
        let second = runtime.state.open_table_tab("orders").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find(&format!("tab-{first}")).expect("first tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.tabs.selected().unwrap().id, first);

        let (x, y) = runtime.find(&format!("tab-close-{second}")).expect("close second");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.tabs.tabs().iter().all(|tab| tab.id != second));
        assert_eq!(runtime.state.tabs.selected().unwrap().id, first);
    }

    #[test]
    fn clicking_a_session_table_opens_a_table_tab() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        assert!(runtime.find("object-tree").is_some());
        assert!(runtime.find("tree-table-customers").is_none());

        let (x, y) = runtime.find("connection-local").expect("connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("tree-table-customers").expect("table in the tree");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.tabs.tabs().len(), 1);
        assert_eq!(runtime.state.tabs.selected().unwrap().title, "customers");
        assert_eq!(runtime.state.tabs.selected().unwrap().kind, datazen_ui_api::TabKind::Table);
    }

    #[test]
    fn the_open_table_tab_is_the_table_in_the_grid() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("tree-table-orders").expect("orders");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let orders = runtime.state.tabs.selected().unwrap().id;
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");

        let (x, y) = runtime.find("tree-table-customers").expect("customers");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let customers = runtime.state.tabs.selected().unwrap().id;
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");

        runtime.frame(1.0);
        let (x, y) = runtime.find(&format!("tab-{orders}")).expect("orders tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.tabs.selected().unwrap().id, orders);
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
        assert_ne!(customers, orders);
    }

    #[test]
    fn closing_the_selected_table_tab_follows_the_remaining_table() {
        use datazen_ui_api::WorkspaceApi;
        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        let orders = runtime.state.open_table_tab("orders").unwrap();
        let customers = runtime.state.open_table_tab("customers").unwrap();
        assert!(runtime.state.close_tab(customers));
        assert_eq!(runtime.state.tabs.selected().unwrap().id, orders);
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
    }

    #[test]
    fn a_padded_table_title_loads_the_trimmed_table() {
        use datazen_ui_api::WorkspaceApi;
        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.state.open_table_tab(" orders ").unwrap();
        assert_eq!(runtime.state.tabs.selected().unwrap().title, "orders");
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
    }

    #[test]
    fn a_query_tab_keeps_the_editor_above_an_empty_result() {
        use rune::{Key, KeyEvent, Mods};
        use super::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.frame(0.0);
        let (x, y) = runtime.find("open-query").expect("new query");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        assert!(runtime.find("query-tab").is_some());
        assert!(runtime.find("sql-editor-field").is_some());
        assert!(runtime.find("query-results").is_some());
        assert_eq!(runtime.state.tabs.selected().unwrap().kind, datazen_ui_api::TabKind::Query);

        runtime.state.sql.focus();
        runtime.key(KeyEvent { key: Key::Text("select 1".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with("select 1"), "{}", runtime.state.sql.doc());
        assert!(runtime.find("query-results").is_some());
    }

    #[test]
    fn a_query_tab_stays_in_the_workspace_and_off_the_connections_page() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("open-query").expect("new query");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("sql-editor-field").is_some());
        assert!(runtime.find("tab-strip").is_some());

        let (x, y) = runtime.find("nav-connections").expect("connections navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Connections);
        assert!(runtime.find("connection-local").is_some());
        assert!(runtime.find("sql-editor-field").is_none(), "the editor belongs to the workspace");
        assert!(runtime.find("query-results").is_none(), "the results area belongs to the workspace");
        assert!(runtime.find("tab-strip").is_none(), "tabs are a workspace control");
        assert_eq!(runtime.state.tabs.tabs().len(), 1, "the query tab itself is kept");
    }

    #[test]
    fn the_settings_page_is_an_empty_state_not_the_old_preview_card() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").expect("settings navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Settings);
        assert!(runtime.find("settings-empty").is_some());
        assert!(runtime.find("datazen-preview-card").is_none(), "the placeholder card is gone");
        assert!(runtime.find("connection-local").is_none(), "the connection list is its own page");
        assert!(runtime.find("workspace-empty").is_none(), "the workspace empty state is not here");
    }

    #[test]
    fn disconnect_clears_the_tree_and_blocks_editing() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        assert!(runtime.find("disconnect").is_none());
        let (x, y) = runtime.find("connection-local").expect("connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.open_table_tab("customers").unwrap();
        assert!(runtime.state.grid.is_editable());

        runtime.frame(1.0);
        let (x, y) = runtime.find("disconnect").expect("disconnect");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.state.workspace.session().is_none());
        assert!(runtime.state.tabs.tabs().is_empty());
        assert!(runtime.find("tree-table-customers").is_none());
        assert!(!runtime.state.grid.is_editable());
        runtime.state.grid.begin_edit();
        assert!(!runtime.state.grid.is_editing());
    }
}
