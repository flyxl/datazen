//! AppState method implementations.

#![allow(unused_imports)]
use crate::state_def::*;
use crate::views::{next_connection, workflow_yaml};
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::types::{Section, TableView, Tool};
use crate::{icons, splitter};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DESKTOP_NOT_IMPLEMENTED, DesktopAdapter, MemoryWorkspace, ProgressSnapshot,
    QueryApi, Setting, SettingsApi, TabKind, Tabs, TablePage, UnimplementedAdapter, Workflow,
    WorkflowStepResult, WorkspaceApi,
};
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
use rune::tok::{self, type_};
use rune::{
    App, AppSpec, Color, Cx, El, FileDialogOptions, FileDialogResult, Ime, Key, KeyEvent,
    TextEdit, Titlebar, Tx, col, oklch, row, text, text_input,
};

impl AppState {
    pub(crate) fn outbound_fk_of_focus(&self) -> Option<(datazen_ui_api::ForeignKeySpec, usize)> {
        let table = self.grid.table_label().to_owned();
        let column = self.grid.column_name(self.grid.focus().1)?.to_owned();
        let keys = self
            .structures
            .get(&table)
            .and_then(|entry| entry.data.as_ref())
            .map(|data| data.foreign_keys.clone())
            .unwrap_or_default();
        keys.into_iter().find_map(|key| {
            let index = key.columns.iter().position(|name| *name == column)?;
            Some((key, index))
        })
    }

    /// Open the referenced table filtered on the focused cell's value (T5).
    /// A4: pushes an FK breadcrumb so the origin can be restored.
    pub(crate) fn go_to_referenced_row(&mut self) {
        let table = self.grid.table_label().to_owned();
        let not_ready = match self.structures.get(&table) {
            None | Some(StructureState { loading: true, .. }) => {
                Some("the table's structure is still loading — try again once it lands".to_owned())
            }
            Some(StructureState { error: Some(error), .. }) => Some(error.clone()),
            Some(StructureState { data: None, .. }) => Some(
                "the table's structure is not loaded — switch the view or reopen the table"
                    .to_owned(),
            ),
            Some(StructureState { data: Some(_), .. }) => None,
        };
        if let Some(sentence) = not_ready {
            self.fk_error = Some(sentence);
            return;
        }
        self.fk_error = None;
        let Some((key, index)) = self.outbound_fk_of_focus() else { return };
        let Some(target_column) = key.target_columns.get(index).cloned() else { return };
        let value = self.grid.cell(self.grid.focus().0, self.grid.focus().1);
        let target_table = key.target_table.clone();
        let label = format!("{table} → {target_table}");
        self.push_fk_breadcrumb(label);
        self.open_table_with_filter(&target_table, &target_column, &value);
    }

    /// Ask the host for the loaded table's inbound references (T5) as a job
    /// (E3) — the scan walks the whole catalog and never belongs on the
    /// frame loop. A host that declines keeps the synchronous read: the
    /// demo process has none, and the honest refusal names the live host.
    pub(crate) fn load_inbound_refs(&mut self) {
        let table = self.grid.table_label().to_owned();
        let generation = self.next_inbound_generation();
        let job = AdapterJob::InboundRefs { generation, table: table.clone() };
        if self.desktop.begin_job(job) {
            self.inbound_loading = Some(generation);
            self.inbound_refs = None;
            self.fk_error = None;
            return;
        }
        match self.desktop.table_inbound_references(&table) {
            Ok(refs) => {
                self.inbound_refs = Some(refs);
                self.fk_error = None;
            }
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.inbound_refs = None;
                self.fk_error = Some("the live host lists the references".into());
            }
            Err(error) => {
                self.inbound_refs = None;
                self.fk_error = Some(error);
            }
        }
    }

    /// Bump and return the next inbound generation (E3). Every ask starts
    /// one, so a delivery from a scan the state has moved past is dropped.
    pub(crate) fn next_inbound_generation(&mut self) -> u32 {
        self.inbound_generation += 1;
        self.inbound_generation
    }

    /// Hide the inbound list.
    pub(crate) fn close_inbound_refs(&mut self) {
        self.inbound_refs = None;
        self.fk_error = None;
    }

    /// Open a referencing table filtered on the focused row's value of the
    /// referenced column (T5). A4: pushes an FK breadcrumb.
    pub(crate) fn locate_inbound_ref(&mut self, index: usize) {
        let Some(list) = self.inbound_refs.as_ref() else { return };
        let Some(reference) = list.get(index).cloned() else { return };
        let Some(referenced_index) = (0..self.grid.cols)
            .find(|col| self.grid.column_name(*col) == Some(reference.referenced_column.as_str()))
        else {
            self.fk_error = Some(format!(
                "the loaded page has no column {}",
                reference.referenced_column
            ));
            return;
        };
        let value = self.grid.cell(self.grid.focus().0, referenced_index);
        let from = self.grid.table_label().to_owned();
        let label = format!("{from} ← {}", reference.table);
        self.push_fk_breadcrumb(label);
        self.open_table_with_filter(&reference.table, &reference.column, &value);
    }

    /// A4: capture the current table context as an FK breadcrumb step.
    pub(crate) fn push_fk_breadcrumb(&mut self, label: String) {
        if !self.grid.has_page() {
            return;
        }
        let crumb = FkBreadcrumb {
            label,
            table: self.grid.table_label().to_owned(),
            page: self.grid.page(),
            sort: self.sort.clone(),
            filter: self.filter.clone(),
        };
        self.fk_breadcrumbs.push(crumb);
    }

    /// A4: navigate back to a breadcrumb step, restoring its table/filter context.
    pub(crate) fn navigate_fk_breadcrumb(&mut self, index: usize) {
        if index >= self.fk_breadcrumbs.len() {
            return;
        }
        let crumb = self.fk_breadcrumbs[index].clone();
        self.fk_breadcrumbs.truncate(index);
        self.active = Section::Workspace;
        self.open_table_tab(&crumb.table);
        self.filter = crumb.filter;
        self.sort = crumb.sort;
        self.table_page = crumb.page;
        self.reload_table_page();
    }

    /// Open a table with a filter preset (T4/T5 share this): the tab's own
    /// open resets the filter, so the filter lands after it and the page
    /// re-reads.
    pub(crate) fn open_table_with_filter(&mut self, table: &str, column: &str, value: &str) {
        self.active = Section::Workspace;
        self.open_table_tab(table);
        self.filter = Some(ColumnFilter { column: column.to_owned(), contains: value.to_owned() });
        self.sort = None;
        self.table_page = 0;
        self.reload_table_page();
    }

    /// The value search's five match modes, in cycle order (T4). The ids
    /// are the React vocabulary's (`ValueSearchMode`).
    const VALUE_SEARCH_MODES: [&'static str; 5] =
        ["contains", "notContains", "exact", "prefix", "suffix"];
    pub(crate) fn value_search_mode(&self) -> &'static str {
        Self::VALUE_SEARCH_MODES
            [(self.value_search_mode as usize) % Self::VALUE_SEARCH_MODES.len()]
    }
    pub(crate) fn cycle_value_search_mode(&mut self) {
        self.value_search_mode =
            (self.value_search_mode + 1) % Self::VALUE_SEARCH_MODES.len() as u32;
    }

    /// Start a value search over the live session (T4): one job for the
    /// whole scan. The demo process has no live session and the memory port
    /// has no scan — the honest refusal names both.
    pub(crate) fn begin_value_search(&mut self) {
        let value = self.value_search_value.text.trim().to_owned();
        if value.is_empty() {
            self.value_search_error = Some("a value is required".into());
            return;
        }
        if !self.session_open() {
            self.value_search_error =
                Some("value search needs an open connection".into());
            return;
        }
        let generation = self.next_value_search_generation();
        let job = AdapterJob::ValueSearch {
            generation,
            value,
            mode: self.value_search_mode().to_owned(),
        };
        if self.desktop.begin_job(job) {
            self.value_search_loading = Some(generation);
            self.value_search_error = None;
            self.value_search_hits.clear();
            self.value_search_query = self.value_search_value.text.trim().to_owned();
        } else {
            self.value_search_error =
                Some("value search needs the live host".into());
        }
    }

    /// Cancel the running search: the generation moves past it, so the
    /// scan's delivery lands nowhere.
    pub(crate) fn cancel_value_search(&mut self) {
        if self.value_search_loading.is_some() {
            let _ = self.next_value_search_generation();
            self.value_search_loading = None;
            self.value_search_error = Some("the search was cancelled".into());
        }
    }
    pub(crate) fn next_value_search_generation(&mut self) -> u32 {
        self.value_search_generation += 1;
        self.value_search_generation
    }

    /// Open the hit's table with a filter on the hit's column (T4): the
    /// locate intent the React dialog answers with.
    pub(crate) fn locate_value_search_hit(&mut self, index: usize) {
        let Some(hit) = self.value_search_hits.get(index).cloned() else { return };
        let value = self.value_search_query.clone();
        self.open_table_with_filter(&hit.table, &hit.column, &value);
    }

}
