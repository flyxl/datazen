//! Shared DataZen UI entry point for native and WebAssembly hosts.
//!
//! The page shell is deliberately independent of DataZen's transport and storage
//! implementations. Platform adapters will be added behind typed UI ports.

mod data_grid;
mod sql_editor;

use data_grid::DataGrid;
use datazen_ui_api::{ColumnDraft, ColumnFilter, ColumnSort, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting, SettingsApi, TabKind, Tabs, TablePage, WorkspaceApi};
use rune::tok::{self, type_};
use rune::{App, AppSpec, Cx, El, Ime, Key, KeyEvent, TextEdit, Tx, col, oklch, row, text, text_input};
use sql_editor::SqlEditor;

/// Rows asked from the workspace for one table page.
pub const TABLE_PAGE_SIZE: u32 = 25;

/// What the workspace shows for the selected table tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TableView {
    #[default]
    Data,
    Ddl,
    Columns,
    Indexes,
    ForeignKeys,
}

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
    /// The read-only query result grid of the selected query tab.
    result_grid: Option<DataGrid>,
    /// The query generation: bumped by every run and every cancel. A result page
    /// from a generation that is not the current one is dropped.
    query_generation: u32,
    /// Catalog nodes the tree keeps collapsed, by path: "db" or "db/schema".
    collapsed: std::collections::BTreeSet<String>,
    /// Which side of the selected table tab the workspace shows.
    table_view: TableView,
    /// The ER diagram's open schema, as (database, schema).
    er_schema: Option<(String, String)>,
    /// ER box positions per table, canvas points. UI state only, never the port.
    er_positions: std::collections::BTreeMap<String, (f32, f32)>,
    /// The box being dragged: its table and where the gesture started.
    er_drag: Option<(String, (f32, f32))>,
    /// Column-change drafts per table, kept until applied. UI state.
    column_drafts: std::collections::BTreeMap<String, Vec<ColumnDraft>>,
    draft_name: TextEdit,
    draft_type: TextEdit,
    /// The settings page's inputs and its last rejected write.
    page_size_input: TextEdit,
    max_rows_input: TextEdit,
    settings_error: Option<String>,
    /// The connection form's fields and its last rejected save.
    connection_name: TextEdit,
    connection_engine: TextEdit,
    connection_address: TextEdit,
    connection_error: Option<String>,
    /// The snippet form's fields and its last rejected write.
    snippet_name: TextEdit,
    snippet_sql: TextEdit,
    snippet_error: Option<String>,
    /// The export dialog's format, range and its job.
    export_format_index: u32,
    export_range: TextEdit,
    export_job: Option<u64>,
    export_error: Option<String>,
    /// The export job's last snapshot. Refreshed only on demand.
    export_snapshot: Option<ProgressSnapshot>,
    /// The import wizard: 0 source step, 1 target step, 2 ready.
    import_step: u32,
    import_source: TextEdit,
    import_target: TextEdit,
    import_error: Option<String>,
    /// The backup job and its last snapshot.
    backup_job: Option<u64>,
    backup_snapshot: Option<ProgressSnapshot>,
    backup_error: Option<String>,
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
            result_grid: None,
            query_generation: 0,
            collapsed: std::collections::BTreeSet::new(),
            table_view: TableView::Data,
            er_schema: None,
            er_positions: std::collections::BTreeMap::new(),
            er_drag: None,
            column_drafts: std::collections::BTreeMap::new(),
            draft_name: TextEdit::new(String::new()),
            draft_type: TextEdit::new(String::new()),
            page_size_input: TextEdit::new(String::new()),
            max_rows_input: TextEdit::new(String::new()),
            settings_error: None,
            connection_name: TextEdit::new(String::new()),
            connection_engine: TextEdit::new(String::new()),
            connection_address: TextEdit::new(String::new()),
            connection_error: None,
            snippet_name: TextEdit::new(String::new()),
            snippet_sql: TextEdit::new(String::new()),
            snippet_error: None,
            export_format_index: 0,
            export_range: TextEdit::new(String::new()),
            export_job: None,
            export_error: None,
            export_snapshot: None,
            import_step: 0,
            import_source: TextEdit::new(String::new()),
            import_target: TextEdit::new(String::new()),
            import_error: None,
            backup_job: None,
            backup_snapshot: None,
            backup_error: None,
        }
    }
}

impl AppState {
    fn select(&mut self, section: Section) {
        self.active = section;
    }

    /// Collapse or expand a catalog node. The tree starts expanded; collapsed
    /// paths are UI state and survive catalog refreshes.
    fn toggle_expand(&mut self, path: &str) {
        if !self.collapsed.insert(path.to_owned()) {
            self.collapsed.remove(path);
        }
    }

    /// Re-take the catalog from the port. Collapsed nodes stay collapsed: they
    /// live here, not in the port.
    fn refresh_catalog(&mut self) {
        let _ = self.workspace.catalog();
    }

    fn set_table_view(&mut self, view: TableView) {
        self.table_view = view;
    }

    /// The table page size from the settings port (fallback: the built-in 25).
    fn table_page_size(&self) -> u32 {
        self.workspace
            .read_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::TablePageSize { rows } => Some(*rows),
                _ => None,
            })
            .unwrap_or(TABLE_PAGE_SIZE)
    }

    /// Set the table page size from the settings input; an invalid value keeps
    /// the old one and shows the port's error.
    fn set_table_page_size(&mut self) {
        let rows = self.page_size_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.workspace.write_setting(Setting::TablePageSize { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("table page size must be a number".into()),
        }
    }

    fn set_query_max_rows(&mut self) {
        let rows = self.max_rows_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.workspace.write_setting(Setting::QueryMaxRows { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("query max rows must be a number".into()),
        }
    }

    /// Save the connection form through the port. A missing address is refused
    /// and nothing is saved.
    fn save_connection_form(&mut self) {
        let name = self.connection_name.text.clone();
        let engine = self.connection_engine.text.clone();
        let address = self.connection_address.text.clone();
        match self.workspace.save_connection(&name, &engine, &address) {
            Ok(()) => {
                self.connection_error = None;
                self.connection_name = TextEdit::new(String::new());
                self.connection_engine = TextEdit::new(String::new());
                self.connection_address = TextEdit::new(String::new());
            }
            Err(error) => self.connection_error = Some(error),
        }
    }

    fn save_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        match self.workspace.save_snippet(&name, &sql) {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }

    fn update_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        match self.workspace.update_snippet(&name, &sql) {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }

    fn delete_snippet(&mut self, name: &str) {
        if let Err(error) = self.workspace.delete_snippet(name) {
            self.snippet_error = Some(error);
        }
    }

    /// The export formats the dialog collects, in cycle order.
    const EXPORT_FORMATS: [&'static str; 3] = ["csv", "json", "sql"];

    fn export_format(&self) -> &'static str {
        Self::EXPORT_FORMATS[(self.export_format_index as usize) % Self::EXPORT_FORMATS.len()]
    }

    fn cycle_export_format(&mut self) {
        self.export_format_index =
            (self.export_format_index + 1) % Self::EXPORT_FORMATS.len() as u32;
    }

    /// Ask the port for an export job id. A missing range gets no job.
    fn start_export_form(&mut self) {
        let format = self.export_format().to_owned();
        let range = self.export_range.text.trim().to_owned();
        match self.workspace.start_export(&format, &range) {
            Ok(job) => {
                self.export_job = Some(job);
                self.export_error = None;
                self.export_snapshot = self.workspace.job_progress(job);
            }
            Err(error) => {
                self.export_job = None;
                self.export_error = Some(error);
            }
        }
    }

    /// Pull one progress snapshot from the port. No streaming, no polling.
    fn refresh_export_progress(&mut self) {
        if let Some(job) = self.export_job {
            self.export_snapshot = self.workspace.job_progress(job);
        }
    }

    /// The import wizard's step 1: a source is required to move on.
    fn import_next(&mut self) {
        if self.import_source.text.trim().is_empty() {
            self.import_error = Some("a source is required".into());
            return;
        }
        self.import_error = None;
        self.import_step = 1;
    }

    /// Step 2: a target table is required to finish.
    fn import_finish(&mut self) {
        if self.import_target.text.trim().is_empty() {
            self.import_error = Some("a target table is required".into());
            return;
        }
        self.import_error = None;
        self.import_step = 2;
    }

    fn import_back(&mut self) {
        self.import_step = 0;
        self.import_error = None;
    }

    fn start_backup_form(&mut self) {
        match self.workspace.start_backup() {
            Ok(job) => {
                self.backup_job = Some(job);
                self.backup_error = None;
                self.backup_snapshot = self.workspace.job_progress(job);
            }
            Err(error) => self.backup_error = Some(error),
        }
    }

    /// Stop the backup and pull the stopped snapshot.
    fn stop_backup(&mut self) {
        let Some(job) = self.backup_job else { return };
        match self.workspace.stop_job(job) {
            Ok(()) => {
                self.backup_error = None;
                self.backup_snapshot = self.workspace.job_progress(job);
            }
            Err(error) => self.backup_error = Some(error),
        }
    }

    fn refresh_backup_progress(&mut self) {
        if let Some(job) = self.backup_job {
            self.backup_snapshot = self.workspace.job_progress(job);
        }
    }

    /// Flip the theme. The only write goes to the settings port.
    fn toggle_theme(&mut self) {
        let dark = self
            .workspace
            .read_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::Theme { dark } => Some(*dark),
                _ => None,
            })
            .unwrap_or(true);
        let _ = self.workspace.write_setting(Setting::Theme { dark: !dark });
    }

    /// Open the ER diagram for the current schema: the selected table's schema,
    /// else the first.
    fn open_er(&mut self) {
        let catalog = self.workspace.catalog();
        let Some(database) = catalog.databases.first() else {
            return;
        };
        let selected = self.tabs.selected().map(|tab| tab.title.clone());
        let schema = selected
            .and_then(|table| {
                database
                    .schemas
                    .iter()
                    .find(|schema| schema.tables.contains(&table))
                    .map(|schema| schema.name.clone())
            })
            .or_else(|| database.schemas.first().map(|schema| schema.name.clone()));
        if let Some(schema) = schema {
            self.er_schema = Some((database.name.clone(), schema));
            self.active = Section::Workspace;
        }
    }

    fn close_er(&mut self) {
        self.er_schema = None;
    }

    /// The selected table tab's title, if any.
    fn selected_table(&self) -> Option<String> {
        let tab = self.tabs.selected()?;
        (tab.kind == TabKind::Table).then(|| tab.title.clone())
    }

    /// Queue an add-column draft from the draft inputs.
    fn add_column_draft(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let name = self.draft_name.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let sql_type = if self.draft_type.text.trim().is_empty() {
            "text".to_owned()
        } else {
            self.draft_type.text.trim().to_owned()
        };
        self.column_drafts.entry(table).or_default().push(ColumnDraft::AddColumn { name, sql_type });
        self.draft_name = TextEdit::new(String::new());
        self.draft_type = TextEdit::new(String::new());
    }

    /// Apply the table's drafts through the port. A failure keeps the drafts.
    fn apply_column_drafts(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let Some(drafts) = self.column_drafts.get(&table) else { return };
        if drafts.is_empty() {
            return;
        }
        let sql = drafts.iter().map(|draft| draft.sql(&table)).collect::<Vec<_>>().join("\n");
        if self.workspace.apply_ddl(&table, &sql).is_ok() {
            self.column_drafts.remove(&table);
        }
    }

    /// Queue a retype draft for the column named in the draft inputs.
    fn add_retype_draft(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let column = self.draft_name.text.trim().to_owned();
        if column.is_empty() {
            return;
        }
        let sql_type = if self.draft_type.text.trim().is_empty() {
            "text".to_owned()
        } else {
            self.draft_type.text.trim().to_owned()
        };
        self.column_drafts
            .entry(table)
            .or_default()
            .push(ColumnDraft::ChangeType { column, sql_type });
        self.draft_name = TextEdit::new(String::new());
        self.draft_type = TextEdit::new(String::new());
    }

    fn open_connection(&mut self, id: &str) {
        if self.workspace.connect(id).is_err() {
            return;
        }
        self.collapsed.clear();
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        if let Ok(page) = self.workspace.open_table("customers", 0, self.table_page_size(), None, None) {
            self.grid.load_page(&page);
            self.grid.set_editable(page.read_only.is_none());
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
        self.result_grid = None;
        self.collapsed.clear();
        self.er_schema = None;
        self.er_positions.clear();
        self.er_drag = None;
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
            self.table_page_size(),
            self.sort.as_ref(),
            self.filter.as_ref(),
        ) {
            Ok(page) => {
                self.table_page = page.page;
                self.grid.load_page(&page);
                self.grid.set_editable(page.read_only.is_none());
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

    /// Load a query result into the shared grid, read-only, below the editor.
    fn load_result_page(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) {
        let page = self.workspace.open_result(columns, rows);
        let mut grid = DataGrid::new();
        grid.load_page(&page);
        grid.set_editable(false);
        self.result_grid = Some(grid);
    }

    /// Bump and return the next query generation. Every run and every cancel
    /// starts one.
    fn next_query_generation(&mut self) -> u32 {
        self.query_generation += 1;
        self.query_generation
    }

    /// A result page arrives for a generation. Stale generations — superseded by
    /// a newer run or invalidated by a cancel — are dropped on the floor.
    fn deliver_query_result(&mut self, generation: u32, page: TablePage) {
        if generation != self.query_generation {
            return;
        }
        self.load_result_page(page.columns, page.rows);
    }

    /// Cancel the in-flight query: bumping the generation invalidates every
    /// outstanding one, so their results are dropped when they arrive.
    fn cancel_query(&mut self) {
        self.next_query_generation();
    }

    /// Run the statement at the cursor. A failed execution changes nothing; a
    /// successful one lands in the result grid only if still current.
    fn run_query(&mut self) {
        let generation = self.next_query_generation();
        let Some(sql) = self.sql.statement_at_cursor() else {
            return;
        };
        match self.workspace.execute(&sql) {
            Ok(page) => self.deliver_query_result(generation, page),
            Err(_) => {}
        }
    }

    /// Any tab event lands in the workspace: the selected tab decides what the
    /// workspace shows, so the tab strip and its content stay on one page.
    fn sync_grid_to_selected_tab(&mut self) {
        self.active = Section::Workspace;
        self.er_schema = None;
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
        Section::Settings => settings_page(state, theme),
    }
}

/// The workspace is driven by the tabs: a query tab shows the SQL editor above the
/// results, a loaded table page shows the grid, and anything else is an empty state.
/// The old standalone editor and demo-grid sections are no longer product pages.
fn workspace_body(state: &AppState, theme: tok::Theme) -> El {
    if query_selected(state) {
        return query_workspace(state, theme);
    }
    if let Some((database, schema)) = &state.er_schema {
        return er_view(state, database, schema, theme);
    }
    if state.grid.has_page() {
        return table_workspace(state, theme);
    }
    workspace_empty(theme)
}

/// The ER page: one box per table of the open schema. Wide tables (more than 12
/// columns) draw collapsed, showing the column count. Boxes drag; their
/// positions live in UI state.
fn er_view(state: &AppState, database: &str, schema: &str, theme: tok::Theme) -> El {
    let catalog = state.workspace.catalog();
    let tables: Vec<String> = catalog
        .databases
        .iter()
        .find(|db| db.name == database)
        .and_then(|db| db.schemas.iter().find(|node| node.name == schema))
        .map(|node| node.tables.clone())
        .unwrap_or_default();
    let mut canvas = col()
        .key("er-canvas")
        .w_full()
        .h(420.0)
        .radius(8.0)
        .fill(theme.control_off);
    for (index, table) in tables.iter().enumerate() {
        let columns = state.workspace.table_columns(table).unwrap_or_default();
        let default = ((index % 3) as f32 * 230.0, (index / 3) as f32 * 180.0);
        let (x, y) = state.er_positions.get(table).copied().unwrap_or(default);
        canvas = canvas.child(er_box(table, &columns, x, y, theme));
    }
    col()
        .key("er-page")
        .w_full()
        .gap(8.0)
        .child(
            row()
                .key("er-toolbar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text(format!("ER · {database}/{schema}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    row()
                        .key("er-close")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Close").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.close_er()),
                ),
        )
        .child(canvas)
}

/// One table box. Collapsed boxes show the column count instead of every column.
fn er_box(
    table: &str,
    columns: &[datazen_ui_api::ColumnSpec],
    x: f32,
    y: f32,
    theme: tok::Theme,
) -> El {
    let collapsed = columns.len() > 12;
    let table = table.to_owned();
    let drag_table = table.clone();
    let mut class_box = col()
        .key(format!("er-box-{table}"))
        .absolute(x, y)
        .w(200.0)
        .radius(8.0)
        .fill(oklch(0.24, 0.03, 250.0))
        .px(8.0)
        .py(6.0)
        .gap(2.0)
        .child(
            text(table.to_owned())
                .key(format!("er-title-{table}"))
                .font(type_::TITLE)
                .color(theme.text),
        )
        .on_drag(move |state: &mut AppState, drag: &mut rune::Drag, _: &mut Tx| {
            match drag.phase {
                rune::Phase::Down => {
                    state.er_drag = Some((drag_table.clone(), (x, y)));
                }
                rune::Phase::Move => {
                    if let Some((dragged, (start_x, start_y))) = &state.er_drag {
                        if *dragged == drag_table {
                            state
                                .er_positions
                                .insert(drag_table.clone(), ((start_x + drag.dx).max(0.0), (start_y + drag.dy).max(0.0)));
                        }
                    }
                }
                rune::Phase::Up => {
                    state.er_drag = None;
                }
            }
        });
    if collapsed {
        class_box = class_box.child(
            text(format!("{} columns", columns.len()))
                .key(format!("er-collapsed-{table}"))
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
    } else {
        for (index, column) in columns.iter().enumerate() {
            class_box = class_box.child(
                text(column.name.clone())
                    .key(format!("er-col-{table}-{index}"))
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
        }
    }
    class_box
}

/// A table tab shows the view switcher above either the data grid or the DDL.
fn table_workspace(state: &AppState, theme: tok::Theme) -> El {
    let title = state.tabs.selected().map(|tab| tab.title.clone()).unwrap_or_default();
    let _ = &title;
    let body = match state.table_view {
        TableView::Data => data_grid::panel(state, theme.text, theme.text_secondary, theme.control_off),
        TableView::Ddl => ddl_view(&state.workspace, &title, theme),
        TableView::Columns => columns_view(state, &title, theme),
        TableView::Indexes => indexes_view(&state.workspace, &title, theme),
        TableView::ForeignKeys => foreign_keys_view(&state.workspace, &title, theme),
    };
    col()
        .key("table-workspace")
        .w_full()
        .gap(8.0)
        .child(view_switcher(state, theme))
        .child(body)
}

fn view_switcher(state: &AppState, theme: tok::Theme) -> El {
    let entry = |key: &str, label: &str, view: TableView, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active { tok::accent(0, true) } else { theme.control_off })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_table_view(view))
    };
    row()
        .key("table-view-switcher")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(entry("view-data", "Data", TableView::Data, state.table_view == TableView::Data))
        .child(entry("view-columns", "Columns", TableView::Columns, state.table_view == TableView::Columns))
        .child(entry("view-indexes", "Indexes", TableView::Indexes, state.table_view == TableView::Indexes))
        .child(entry("view-fks", "Foreign keys", TableView::ForeignKeys, state.table_view == TableView::ForeignKeys))
        .child(entry("view-ddl", "DDL", TableView::Ddl, state.table_view == TableView::Ddl))
}

/// The column list: name, type, nullability — one row per column, plus the
/// drafts section. The official list is read-only; drafts are separate.
fn columns_view(state: &AppState, title: &str, theme: tok::Theme) -> El {
    let columns = state.workspace.table_columns(title).unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("columns-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("columns-header")
                .w_full()
                .h(24.0)
                .child(header("Column", 240.0))
                .child(header("Type", 140.0))
                .child(header("Nullable", 100.0)),
        );
    for (index, column) in columns.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        list = list.child(
            row()
                .key(format!("column-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&column.name, 240.0))
                .child(cell(&column.sql_type, 140.0))
                .child(cell(if column.nullable { "yes" } else { "no" }, 100.0)),
        );
    }
    let mut list = list.child(column_drafts_section(state, title, theme));
    list
}

/// The drafts area under the column list: inputs, the queued drafts, separate
/// from the official columns.
fn column_drafts_section(state: &AppState, title: &str, theme: tok::Theme) -> El {
    let drafts = state.column_drafts.get(title).cloned().unwrap_or_default();
    let mut section = col()
        .key("column-drafts")
        .w_full()
        .gap(6.0)
        .child(
            row()
                .key("draft-inputs")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&state.draft_name, type_::LABEL)
                        .key("draft-name-input")
                        .w(140.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.draft_name),
                )
                .child(
                    text_input(&state.draft_type, type_::LABEL)
                        .key("draft-type-input")
                        .w(110.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.draft_type),
                )
                .child(
                    row()
                        .key("draft-add")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("+ Column").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.add_column_draft()),
                )
                .child(
                    row()
                        .key("draft-retype")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Retype").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.add_retype_draft()),
                ),
        );
    for (index, draft) in drafts.iter().enumerate() {
        let label = match draft {
            ColumnDraft::AddColumn { name, sql_type } => format!("add {name} {sql_type}"),
            ColumnDraft::ChangeType { column, sql_type } => format!("retype {column} → {sql_type}"),
        };
        section = section.child(
            row()
                .key(format!("draft-row-{index}"))
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(label).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if !drafts.is_empty() {
        section = section.child(
            row()
                .key("draft-apply")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text("Apply").font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.apply_column_drafts()),
        );
    }
    if !drafts.is_empty() {
        let preview = drafts
            .iter()
            .map(|draft| draft.sql(title))
            .collect::<Vec<_>>()
            .join("\n");
        let mut sql_block = col()
            .key("draft-sql")
            .w_full()
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off);
        for line in preview.lines() {
            sql_block = sql_block.child(
                text(line.to_owned())
                    .font(sql_editor::editor_font())
                    .color(theme.text),
            );
        }
        section = section.child(sql_block);
    }
    section
}

/// The index list: name, columns in index order, uniqueness.
fn indexes_view(workspace: &MemoryWorkspace, title: &str, theme: tok::Theme) -> El {
    let indexes = workspace.table_indexes(title).unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("indexes-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("indexes-header")
                .w_full()
                .h(24.0)
                .child(header("Index", 260.0))
                .child(header("Columns", 320.0))
                .child(header("Unique", 100.0)),
        );
    for (index, spec) in indexes.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        list = list.child(
            row()
                .key(format!("index-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&spec.name, 260.0))
                .child(cell(&spec.columns.join(", "), 320.0))
                .child(cell(if spec.unique { "yes" } else { "no" }, 100.0)),
        );
    }
    list
}

/// The foreign key list: this table's columns, the target table, its columns.
fn foreign_keys_view(workspace: &MemoryWorkspace, title: &str, theme: tok::Theme) -> El {
    let keys = workspace.table_foreign_keys(title).unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("fks-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("fks-header")
                .w_full()
                .h(24.0)
                .child(header("Name", 260.0))
                .child(header("Columns", 240.0))
                .child(header("References", 320.0)),
        );
    for (index, key) in keys.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        let references = format!(
            "{} ({})",
            key.target_table,
            key.target_columns.join(", ")
        );
        list = list.child(
            row()
                .key(format!("fk-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&key.name, 260.0))
                .child(cell(&key.columns.join(", "), 240.0))
                .child(cell(&references, 320.0)),
        );
    }
    list
}

/// The DDL page: the port's string, verbatim, one line per row. An empty string
/// is its own empty state.
fn ddl_view(workspace: &MemoryWorkspace, title: &str, theme: tok::Theme) -> El {
    let ddl = workspace.table_ddl(title).ok().filter(|ddl| !ddl.is_empty());
    match ddl {
        Some(ddl) => {
            let mut page = col()
                .key("table-ddl")
                .w_full()
                .px(12.0)
                .py(8.0)
                .radius(8.0)
                .fill(theme.control_off);
            for line in ddl.lines() {
                page = page.child(text(line.to_owned()).font(sql_editor::editor_font()).color(theme.text));
            }
            page
        }
        None => col()
            .key("table-ddl-empty")
            .w_full()
            .h(160.0)
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(text("No DDL available for this table").font(type_::TITLE).color(theme.text))
            .child(
                text("The port returned an empty DDL string")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            ),
    }
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

fn settings_page(state: &AppState, theme: tok::Theme) -> El {
    let dark = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::Theme { dark } => Some(*dark),
            _ => None,
        })
        .unwrap_or(true);
    let page_size = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::TablePageSize { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(TABLE_PAGE_SIZE);
    let max_rows = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::QueryMaxRows { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(100);
    let mut page = col()
        .key("settings-page")
        .w_full()
        .gap(12.0)
        .child(
            row()
                .key("settings-appearance")
                .w_full()
                .h(32.0)
                .gap(12.0)
                .center_y()
                .child(
                    text("Appearance".to_owned())
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    row()
                        .key("settings-theme")
                        .h(28.0)
                        .px(10.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(if dark { "Theme: Dark" } else { "Theme: Light" }.to_owned())
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.toggle_theme()),
                ),
        )
        .child(
            row()
                .key("settings-data")
                .w_full()
                .h(32.0)
                .gap(8.0)
                .center_y()
                .child(text("Data".to_owned()).font(type_::TITLE).color(theme.text))
                .child(
                    text(format!("Rows per page: {page_size}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text_input(&state.page_size_input, type_::LABEL)
                        .key("settings-page-size-input")
                        .w(90.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.page_size_input),
                )
                .child(
                    row()
                        .key("settings-page-size-set")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Set").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.set_table_page_size()),
                )
                .child(
                    text(format!("Query max rows: {max_rows}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text_input(&state.max_rows_input, type_::LABEL)
                        .key("settings-max-rows-input")
                        .w(90.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.max_rows_input),
                )
                .child(
                    row()
                        .key("settings-max-rows-set")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Set").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.set_query_max_rows()),
                ),
        );
    if let Some(error) = &state.settings_error {
        page = page.child(
            row()
                .key("settings-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }

    // SQL snippets: add, update, delete.
    let mut snippets = col()
        .key("settings-snippets")
        .w_full()
        .gap(6.0)
        .child(text("SQL snippets".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("snippet-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&state.snippet_name, type_::LABEL)
                        .key("snippet-name-input")
                        .w(140.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.snippet_name),
                )
                .child(
                    text_input(&state.snippet_sql, type_::LABEL)
                        .key("snippet-sql-input")
                        .w(280.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.snippet_sql),
                )
                .child(
                    row()
                        .key("snippet-save")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Save").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.save_snippet_form()),
                )
                .child(
                    row()
                        .key("snippet-update")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Update").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.update_snippet_form()),
                ),
        );
    for (index, snippet) in state.workspace.snippets().iter().enumerate() {
        let name = snippet.name.clone();
        snippets = snippets.child(
            row()
                .key(format!("snippet-row-{index}"))
                .w_full()
                .h(24.0)
                .gap(8.0)
                .center_y()
                .px(8.0)
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(snippet.name.clone()).font(type_::LABEL).color(theme.text))
                .child(text(snippet.sql.clone()).font(type_::LABEL).color(theme.text_secondary))
                .child(
                    row()
                        .key(format!("snippet-delete-{index}"))
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Delete").font(type_::LABEL).color(theme.text_secondary))
                        .on_press(move |state: &mut AppState| state.delete_snippet(&name)),
                ),
        );
    }
    if let Some(error) = &state.snippet_error {
        snippets = snippets.child(
            row()
                .key("snippet-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    page.child(snippets)
        .child(export_section(state, theme))
        .child(import_section(state, theme))
        .child(backup_section(state, theme))
}

/// The backup page: start, stop, and the same progress panel as exports.
fn backup_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("backup-section")
        .w_full()
        .gap(6.0)
        .child(text("Backup".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("backup-controls")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("backup-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Start backup").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.start_backup_form()),
                )
                .child(
                    row()
                        .key("backup-stop")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Stop").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.stop_backup()),
                )
                .child(
                    row()
                        .key("backup-refresh")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_backup_progress()),
                ),
        );
    if let Some(error) = &state.backup_error {
        section = section.child(
            row()
                .key("backup-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if let Some(snapshot) = &state.backup_snapshot {
        section = section.child(progress_panel(snapshot, "backup", theme));
    }
    section
}

/// The import wizard's first two steps: source, then target table. No target,
/// no next step.
fn import_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("import-section")
        .w_full()
        .gap(6.0)
        .child(text("Import".to_owned()).font(type_::TITLE).color(theme.text));
    match state.import_step {
        0 => section = section.child(
            row()
                .key("import-step-1")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text("Step 1 · Source".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text_input(&state.import_source, type_::LABEL)
                        .key("import-source-input")
                        .w(240.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.import_source),
                )
                .child(
                    row()
                        .key("import-next")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Next").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.import_next()),
                ),
        ),
        1 => section = section
            .child(
                row()
                    .key("import-step-2")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        text("Step 2 · Target table".to_owned())
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    )
                    .child(
                        text_input(&state.import_target, type_::LABEL)
                            .key("import-target-input")
                            .w(180.0)
                            .h(24.0)
                            .on_edit(|state: &mut AppState| &mut state.import_target),
                    )
                    .child(
                        row()
                            .key("import-finish")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Finish").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.import_finish()),
                    )
                    .child(
                        row()
                            .key("import-back")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Back").font(type_::LABEL).color(theme.text_secondary))
                            .on_press(|state: &mut AppState| state.import_back()),
                    ),
            ),
        _ => section = section.child(
            row()
                .key("import-ready")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!(
                        "Import ready: {} into {}",
                        state.import_source.text.trim(),
                        state.import_target.text.trim()
                    ))
                    .font(type_::LABEL)
                    .color(theme.text),
                ),
        ),
    }
    if let Some(error) = &state.import_error {
        section = section.child(
            row()
                .key("import-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    section
}

/// The export dialog collects a format and a range, then asks the port for a
/// job id. No range, no job.
fn export_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("export-section")
        .w_full()
        .gap(6.0)
        .child(text("Export".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("export-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("export-format")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("format: {}", state.export_format()))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_export_format()),
                )
                .child(
                    text_input(&state.export_range, type_::LABEL)
                        .key("export-range-input")
                        .w(220.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.export_range),
                )
                .child(
                    row()
                        .key("export-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Export").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.start_export_form()),
                ),
        );
    if let Some(error) = &state.export_error {
        section = section.child(
            row()
                .key("export-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if let Some(job) = state.export_job {
        section = section.child(
            row()
                .key("export-job")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(format!("job #{job}")).font(type_::LABEL).color(theme.text_secondary))
                .child(
                    row()
                        .key("export-refresh")
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_export_progress()),
                ),
        );
    }
    if let Some(snapshot) = &state.export_snapshot {
        section = section.child(progress_panel(snapshot, "export", theme));
    }
    section
}

/// One job's progress drawn from a single snapshot: status, percent, the log.
/// Export and backup share this component.
fn progress_panel(snapshot: &ProgressSnapshot, prefix: &str, theme: tok::Theme) -> El {
    let mut panel = col()
        .key(format!("{prefix}-progress"))
        .w_full()
        .gap(4.0)
        .px(8.0)
        .py(6.0)
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key(format!("{prefix}-status"))
                .w_full()
                .h(24.0)
                .center_y()
                .child(
                    text(format!("{} · {}%", snapshot.status, snapshot.percent))
                        .font(type_::LABEL)
                        .color(theme.text),
                ),
        );
    for (index, line) in snapshot.log.iter().enumerate() {
        panel = panel.child(
            row()
                .key(format!("{prefix}-log-{index}"))
                .w_full()
                .h(18.0)
                .center_y()
                .child(
                    text(line.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    panel
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

/// The session object tree: databases, schemas and tables from the port's
/// catalog, with the tree's collapsed nodes kept in UI state.
fn object_tree(state: &AppState, theme: tok::Theme) -> El {
    let catalog = state.workspace.catalog();
    let mut rows: Vec<El> = Vec::new();
    for database in &catalog.databases {
        let db_path = database.name.clone();
        rows.push(
            row()
                .key(format!("tree-db-{}", database.name))
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(format!("{} · database", database.name)).font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| state.toggle_expand(&db_path)),
        );
        if state.collapsed.contains(&database.name) {
            continue;
        }
        for schema in &database.schemas {
            let schema_path = format!("{}/{}", database.name, schema.name);
            let schema_key = format!("tree-schema-{}-{}", database.name, schema.name);
            let collapsed = state.collapsed.contains(&schema_path);
            rows.push(
                row()
                    .key(schema_key)
                    .w_full()
                    .h(28.0)
                    .px(20.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text(format!("{} · schema", schema.name)).font(type_::LABEL).color(theme.text_secondary))
                    .on_press(move |state: &mut AppState| state.toggle_expand(&schema_path)),
            );
            if collapsed {
                continue;
            }
            for name in &schema.tables {
                let title = name.clone();
                rows.push(
                    row()
                        .key(format!("tree-table-{name}"))
                        .w_full()
                        .h(28.0)
                        .px(32.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(name.clone()).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| {
                            state.open_table_tab(&title);
                        }),
                );
            }
        }
    }
    let count = rows.len() as f32;
    let height = if count == 0.0 { 0.0 } else { count * 28.0 + (count - 1.0) * 4.0 };
    let mut tree = col().key("object-tree").w_full().h(height).gap(4.0);
    for row in rows {
        tree = tree.child(row);
    }
    tree
}

fn query_workspace(state: &AppState, theme: tok::Theme) -> El {
    let results = match &state.result_grid {
        Some(grid) => data_grid::result_panel(grid, theme.text, theme.text_secondary, theme.control_off),
        None => col()
            .key("query-results")
            .w_full()
            .h(120.0)
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(text("No results").font(type_::LABEL).color(theme.text_secondary)),
    };
    col()
        .key("query-tab")
        .w_full()
        .gap(8.0)
        .child(sql_editor::panel(&state.sql, theme.text, theme.text_secondary))
        .child(
            row()
                .key("query-toolbar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("sql-run")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Run").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.run_query()),
                )
                .child(
                    row()
                        .key("sql-cancel")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Cancel").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.cancel_query()),
                )
                .child(
                    text("Command+Enter runs the statement at the cursor")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .child(results)
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

fn er_button(state: &AppState, theme: tok::Theme) -> El {
    if state.workspace.session().is_none() {
        return row().key("er-button-absent").h(0.0);
    }
    row()
        .key("er-button")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("ER diagram").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.open_er())
}

fn refresh_button(state: &AppState, theme: tok::Theme) -> El {
    if state.workspace.session().is_none() {
        return row().key("tree-refresh-absent").h(0.0);
    }
    row()
        .key("tree-refresh")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Refresh catalog").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.refresh_catalog())
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
    let mut form = col().key("connection-form").w_full().gap(8.0).child(
        row()
            .key("connection-form-fields")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                text_input(&state.connection_name, type_::LABEL)
                    .key("connection-name-input")
                    .w(150.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_name),
            )
            .child(
                text_input(&state.connection_engine, type_::LABEL)
                    .key("connection-engine-input")
                    .w(110.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_engine),
            )
            .child(
                text_input(&state.connection_address, type_::LABEL)
                    .key("connection-address-input")
                    .w(170.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_address),
            )
            .child(
                row()
                    .key("connection-save")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text("Save connection").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.save_connection_form()),
            ),
    );
    if let Some(error) = &state.connection_error {
        form = form.child(
            row()
                .key("connection-form-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    list.child(form)
}

fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::Theme { dark } => Some(*dark),
            _ => None,
        })
        .unwrap_or_else(|| cx.system_dark());
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
        .child(refresh_button(state, theme))
        .child(er_button(state, theme))
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
        if matches!(ev.key, Key::Enter) && ev.mods.cmd {
            state.run_query();
            return true;
        }
        return sql_editor::on_key(&mut state.sql, ev);
    }
    if state.grid.has_page() {
        return data_grid::on_key(&mut state.grid, ev, tx);
    }
    false
}

fn app_poll(state: &mut AppState, _: f64, _: &mut Tx) -> bool {
    if state.active != Section::Workspace {
        return false;
    }
    if query_selected(state) {
        return state.result_grid.as_mut().is_some_and(|grid| grid.sync_scroll());
    }
    state.grid.has_page() && state.grid.sync_scroll()
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
    use super::{AppState, Section, TABLE_PAGE_SIZE, query_selected, sql_editor, view};
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
            read_only: None,
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
        use super::app_key;

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
        use super::app_key;

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
        use super::app_key;

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
        runtime.state.set_table_view(super::TableView::Ddl);
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

    #[test]
    fn the_foreign_keys_view_keeps_compound_keys_together() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("orders").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("view-fks").expect("foreign keys button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("fks-list").is_some());
        assert!(runtime.find("fks-header").is_some());
        assert!(runtime.find("fk-row-0").is_some(), "orders has one foreign key");
        assert!(runtime.find("fk-row-1").is_none());
        let keys = runtime.state.workspace.table_foreign_keys("orders").unwrap();
        assert_eq!(keys[0].columns, vec!["id", "name"]);
        assert_eq!(keys[0].target_columns, vec!["id", "name"]);
    }

    #[test]
    fn the_er_page_draws_the_current_schema_and_collapses_wide_tables() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("er-button").expect("er button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("er-page").is_some());
        assert!(runtime.find("er-canvas").is_some());
        assert!(runtime.find("er-box-customers").is_some(), "the current schema's tables");
        assert!(runtime.find("er-box-orders").is_some());
        assert!(runtime.find("er-col-customers-0").is_some(), "narrow tables list their columns");
        assert!(runtime.find("er-col-customers-7").is_some());
        assert!(runtime.find("er-box-catalog").is_none(), "analytics is another schema");

        // A wide table in its own schema collapses to the count.
        runtime.state.open_table_tab("catalog").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        let (x, y) = runtime.find("er-button").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("er-box-catalog").is_some());
        assert!(runtime.find("er-collapsed-catalog").is_some(), "30 columns collapse to the count");
        assert!(runtime.find("er-col-catalog-12").is_none(), "not every column is drawn");
    }

    #[test]
    fn er_boxes_drag_and_the_position_survives_rerenders() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("er-button").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let before = runtime.presented("er-box-customers").expect("the box is drawn").0;
        let (x, y) = runtime.find("er-box-customers").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_move(x + 40.0, y + 20.0);
        runtime.pointer_move(x + 80.0, y + 50.0);
        runtime.pointer_up(x + 80.0, y + 50.0);
        runtime.frame(0.0);

        assert_eq!(
            runtime.state.er_positions.get("customers"),
            Some(&(80.0, 50.0)),
            "the drag moved the box by its travel"
        );
        let after = runtime.presented("er-box-customers").expect("still drawn").0;
        assert!(
            after[0] >= before[0] + 70.0 && after[1] >= before[1] + 40.0,
            "before {before:?} after {after:?}"
        );

        // A plain rerender keeps the position.
        runtime.frame(1.0);
        let kept = runtime.presented("er-box-customers").unwrap().0;
        assert!((kept[0] - after[0]).abs() < 1.0 && (kept[1] - after[1]).abs() < 1.0, "kept {kept:?}");
        assert!(runtime.state.er_drag.is_none(), "the gesture ended");
    }

    #[test]
    fn column_drafts_stay_separate_from_the_official_columns() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("view-columns").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("column-drafts").is_some());
        assert!(runtime.find("column-row-8").is_none(), "8 official columns only");

        runtime.state.draft_name = rune::TextEdit::new("score");
        runtime.state.draft_type = rune::TextEdit::new("int");
        let (x, y) = runtime.find("draft-add").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("draft-row-0").is_some(), "the draft is queued");
        assert!(runtime.find("column-row-8").is_none(), "the official list is untouched");
        assert_eq!(
            runtime.state.workspace.table_columns("customers").unwrap().len(),
            8,
            "the port's columns are unchanged"
        );

        runtime.state.draft_name = rune::TextEdit::new("name");
        runtime.state.draft_type = rune::TextEdit::new("varchar(80)");
        let (x, y) = runtime.find("draft-retype").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("draft-row-1").is_some(), "the retype draft is queued too");
    }

    #[test]
    fn the_drafts_preview_as_sql_without_executing() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("view-columns").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("draft-sql").is_none(), "no drafts, no preview");

        runtime.state.draft_name = rune::TextEdit::new("name");
        runtime.state.draft_type = rune::TextEdit::new("varchar(80)");
        let (x, y) = runtime.find("draft-retype").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("draft-sql").is_some(), "the preview appears");
        let drafts = runtime.state.column_drafts.get("customers").unwrap();
        assert_eq!(
            drafts[0].sql("customers"),
            "ALTER TABLE customers ALTER COLUMN name TYPE varchar(80);",
            "a retype draft shows its SQL"
        );
        assert_eq!(
            runtime.state.workspace.table_columns("customers").unwrap().len(),
            8,
            "the preview executed nothing"
        );
    }

    #[test]
    fn applying_the_drafts_lands_and_a_failure_keeps_them() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("view-columns").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        runtime.state.draft_name = rune::TextEdit::new("score");
        runtime.state.draft_type = rune::TextEdit::new("int");
        let (x, y) = runtime.find("draft-add").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("draft-apply").expect("apply button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("draft-row-0").is_none(), "the drafts are gone");
        assert!(runtime.find("draft-sql").is_none());
        assert!(runtime.find("column-row-8").is_some(), "the official list grew");
        assert_eq!(
            runtime.state.workspace.table_columns("customers").unwrap()[8].name,
            "score"
        );
        assert_eq!(
            runtime.state.workspace.applied_ddl().len(),
            1,
            "the port recorded the apply"
        );

        // A read-only table refuses: the drafts stay, the official list is untouched.
        runtime.state.open_table_tab("orders").unwrap();
        runtime.settle_all();
        runtime.frame(1.0);
        runtime.state.draft_name = rune::TextEdit::new("x");
        runtime.state.draft_type = rune::TextEdit::new("int");
        let (x, y) = runtime.find("draft-add").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("draft-apply").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("draft-row-0").is_some(), "the failed apply keeps the drafts");
        assert!(runtime.find("column-row-2").is_none(), "the official columns are unchanged");
        assert_eq!(runtime.state.workspace.table_columns("orders").unwrap().len(), 2);
    }

    #[test]
    fn the_theme_toggles_through_the_port_and_survives_reopening() {
        use datazen_ui_api::{Setting, SettingsApi};

        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("settings-page").is_some());
        assert!(runtime.find("settings-theme").is_some());

        runtime
            .state
            .workspace
            .write_setting(Setting::Theme { dark: true })
            .unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("settings-theme").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime
                .state
                .workspace
                .read_settings()
                .contains(&Setting::Theme { dark: false }),
            "the toggle wrote the port"
        );

        // Leave and come back: the setting is where the port keeps it.
        let (x, y) = runtime.find("nav-connections").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime
                .state
                .workspace
                .read_settings()
                .contains(&Setting::Theme { dark: false }),
            "reopening the page keeps the theme"
        );

        let (x, y) = runtime.find("settings-theme").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime
                .state
                .workspace
                .read_settings()
                .contains(&Setting::Theme { dark: true })
        );
    }

    #[test]
    fn the_page_size_setting_drives_the_grid_paging() {
        use datazen_ui_api::{Setting, SettingsApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page_size(), 25, "the default");

        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.page_size_input = rune::TextEdit::new("10");
        let (x, y) = runtime.find("settings-page-size-set").expect("set button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.state.workspace.read_settings().contains(&Setting::TablePageSize { rows: 10 }),
            "the write went through the port"
        );

        // The next page now turns at the new size.
        runtime.state.active = Section::Workspace;
        runtime.state.next_table_page();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);
        assert_eq!(runtime.state.grid.rows, 10, "the second page has 10 rows");
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-10");
        assert_eq!(runtime.state.grid.cell(9, 1), "customer-19");

        // An invalid value keeps the old setting and shows the error.
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.page_size_input = rune::TextEdit::new("0");
        let (x, y) = runtime.find("settings-page-size-set").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.state.workspace.read_settings().contains(&Setting::TablePageSize { rows: 10 }),
            "the rejected write changed nothing"
        );
        assert!(runtime.find("settings-error").is_some(), "the rejection is visible");
    }

    #[test]
    fn a_connection_form_save_needs_an_address() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        assert!(runtime.find("connection-local").is_some());
        assert!(runtime.find("connection-form").is_some());

        let (x, y) = runtime.find("connection-save").expect("save button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("connection-remote-postgres").is_none(), "nothing was saved");
        assert!(runtime.find("connection-form-error").is_some(), "the refusal is visible");

        runtime.state.connection_name = rune::TextEdit::new("Remote Postgres");
        runtime.state.connection_engine = rune::TextEdit::new("postgres");
        runtime.state.connection_address = rune::TextEdit::new("db.example.com:5432");
        let (x, y) = runtime.find("connection-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.find("connection-remote-postgres").is_some(),
            "the saved connection joins the list"
        );
        assert!(runtime.find("connection-form-error").is_none(), "the error clears");
    }

    #[test]
    fn snippets_add_update_delete_and_duplicate_names_are_refused() {
        use datazen_ui_api::{SettingsApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("settings-snippets").is_some());

        runtime.state.snippet_name = rune::TextEdit::new("top");
        runtime.state.snippet_sql = rune::TextEdit::new("SELECT * FROM customers LIMIT 10");
        let (x, y) = runtime.find("snippet-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("snippet-row-0").is_some(), "the snippet is listed");

        runtime.state.snippet_name = rune::TextEdit::new("top");
        runtime.state.snippet_sql = rune::TextEdit::new("SELECT 1");
        let (x, y) = runtime.find("snippet-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("snippet-error").is_some(), "the duplicate name is refused");
        assert!(runtime.find("snippet-row-1").is_none(), "still one snippet");

        runtime.state.snippet_name = rune::TextEdit::new("top");
        runtime.state.snippet_sql = rune::TextEdit::new("SELECT 1");
        let (x, y) = runtime.find("snippet-update").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.workspace.snippets()[0].sql,
            "SELECT 1",
            "update changed the sql"
        );

        runtime.frame(1.0);
        let (x, y) = runtime.find("snippet-delete-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("snippet-row-0").is_none(), "the snippet is gone");
        assert!(runtime.state.workspace.snippets().is_empty());
    }

    #[test]
    fn an_export_without_a_range_gets_no_job() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("export-section").is_some());

        let (x, y) = runtime.find("export-start").expect("export button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.export_job.is_none(), "no range, no job");
        assert!(runtime.find("export-error").is_some(), "the refusal is visible");
        assert!(runtime.find("export-job").is_none());

        runtime.state.export_range = rune::TextEdit::new("customers");
        let (x, y) = runtime.find("export-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.export_job, Some(1), "the port handed out a job id");
        assert!(runtime.find("export-job").is_some());
        assert!(runtime.find("export-error").is_none());

        // The format chip cycles.
        let (x, y) = runtime.find("export-format").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.export_format(), "json");
    }

    #[test]
    fn a_failed_export_shows_its_error_line_from_the_snapshot() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        // A range naming no table fails on the third snapshot.
        runtime.state.export_range = rune::TextEdit::new("no-such-table");
        let (x, y) = runtime.find("export-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let snapshot = runtime.state.export_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "running");
        assert_eq!(snapshot.percent, 0);
        assert!(runtime.find("export-progress").is_some());
        assert!(runtime.find("export-log-0").is_some());

        let (x, y) = runtime.find("export-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.export_snapshot.as_ref().unwrap().percent, 50);

        runtime.frame(1.0);
        let (x, y) = runtime.find("export-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let snapshot = runtime.state.export_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.log.last().map(String::as_str),
            Some("no such table no-such-table"),
            "the failure's error sentence is in the snapshot"
        );
        assert!(
            runtime.find(&format!("export-log-{}", snapshot.log.len() - 1)).is_some(),
            "the error line is drawn"
        );
        assert!(runtime.find("export-status").is_some());
    }

    #[test]
    fn the_import_wizard_needs_a_target_to_move_on() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("import-step-1").is_some());
        assert!(runtime.find("import-step-2").is_none());

        // Step 1 needs a source.
        let (x, y) = runtime.find("import-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("import-step-1").is_some(), "still on step 1");
        assert!(runtime.find("import-error").is_some());

        runtime.state.import_source = rune::TextEdit::new("dump.csv");
        let (x, y) = runtime.find("import-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("import-step-2").is_some(), "step 2 arrived");
        assert!(runtime.find("import-step-1").is_none());

        // Step 2 needs a target table: no target, no next step.
        let (x, y) = runtime.find("import-finish").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("import-step-2").is_some(), "still on step 2");
        assert!(runtime.find("import-ready").is_none());
        assert!(runtime.find("import-error").is_some(), "the refusal is visible");

        runtime.state.import_target = rune::TextEdit::new("customers");
        let (x, y) = runtime.find("import-finish").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("import-ready").is_some(), "both steps done");
    }

    #[test]
    fn a_backup_page_stops_and_shows_stopped() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("backup-section").is_some());

        let (x, y) = runtime.find("backup-start").expect("start button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let snapshot = runtime.state.backup_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "running");
        assert!(runtime.find("backup-progress").is_some());
        assert!(runtime.find("backup-log-1").is_some());

        let (x, y) = runtime.find("backup-stop").expect("stop button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let snapshot = runtime.state.backup_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "stopped", "after stopping the status is stopped");
        assert!(snapshot.log.last() == Some(&"stopped by the user".to_owned()));
        assert!(
            runtime.find(&format!("backup-log-{}", snapshot.log.len() - 1)).is_some(),
            "the stop line is drawn"
        );

        // A second stop is refused and visible.
        let (x, y) = runtime.find("backup-stop").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("backup-error").is_some());
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
    fn the_settings_page_is_its_own_page_not_the_old_preview_card() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").expect("settings navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Settings);
        assert!(runtime.find("settings-page").is_some(), "the settings page is real since P2");
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
