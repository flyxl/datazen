//! AppState method implementations.

#![allow(unused_imports)]
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::state_def::*;
use crate::types::{Section, TableView, Tool};
use crate::views::{next_connection, workflow_yaml};
use crate::{icons, splitter};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DesktopAdapter, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting,
    SettingsApi, TabKind, TablePage, Tabs, UnimplementedAdapter, Workflow, WorkflowStepResult,
    WorkspaceApi, DESKTOP_NOT_IMPLEMENTED,
};
use rune::tok::{self, type_};
use rune::{
    col, oklch, row, text, text_input, App, AppSpec, Color, Cx, El, FileDialogOptions,
    FileDialogResult, Ime, Key, KeyEvent, TextEdit, Titlebar, Tx,
};

impl AppState {
    pub fn host_snippets(&self) -> Vec<datazen_ui_api::Snippet> {
        self.desktop
            .snippets()
            .unwrap_or_else(|_| self.workspace.snippets())
    }

    /// The workflows in force (N13): the host's once its registry persists
    /// them, the memory port's while it does not. The host's read answers
    /// from its loaded files — the same memory-speed class as the settings
    /// read.
    pub fn save_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        // T3: the host's file store first; its fixed not-implemented error
        // means the demo process, and the memory port answers instead.
        let saved = match self.desktop.save_snippet(&name, &sql) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_snippet(&name, &sql)
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }
    pub fn update_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        // T3: the host's file store first, the memory port's fallback.
        let updated = match self.desktop.update_snippet(&name, &sql) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.update_snippet(&name, &sql)
            }
            Err(error) => Err(error),
        };
        match updated {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }
    pub fn delete_snippet(&mut self, name: &str) {
        // T3: the host's file store first, the memory port's fallback.
        let deleted = match self.desktop.delete_snippet(name) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => self.workspace.delete_snippet(name),
            Err(error) => Err(error),
        };
        if let Err(error) = deleted {
            self.snippet_error = Some(error);
        }
    }

    /// The favorites in force (T6): the host's root directory is the store;
    /// the demo process has none and the panel says so.
    pub fn host_favorites(&self) -> Vec<datazen_ui_api::FavoriteSpec> {
        self.desktop.favorites().unwrap_or_default()
    }

    /// Save the current query as a favorite (T6): the title from the form,
    /// the statement from the editor, the connection the live session holds.
    pub fn save_favorite_form(&mut self) {
        let title = self.favorite_title.text.trim().to_owned();
        let sql = self.sql.doc().to_owned();
        let connection = self.desktop_connection.clone().unwrap_or_default();
        let saved = match self.desktop.save_favorite(&connection, &title, &sql) {
            Ok(_) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                Err("favorites need the live host".into())
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(()) => {
                self.favorite_error = None;
                self.favorite_title = TextEdit::new(String::new());
            }
            Err(error) => self.favorite_error = Some(error),
        }
    }

    /// Open a favorite into a query tab (T6).
    pub fn open_favorite(&mut self, index: usize) {
        let Some(favorite) = self.host_favorites().get(index).cloned() else {
            return;
        };
        self.active = Section::Workspace;
        if self.open_query_tab(&favorite.title).is_some() {
            self.sql = crate::sql_editor::SqlEditor::from_doc(favorite.sql);
        }
    }

    /// Rename a favorite to the form's title (T6): the title moves, the file
    /// does not — that is the host store's own semantics.
    pub fn rename_favorite_row(&mut self, index: usize) {
        let Some(favorite) = self.host_favorites().get(index).cloned() else {
            return;
        };
        let title = self.favorite_title.text.trim().to_owned();
        if title.is_empty() {
            self.favorite_error = Some("a favorite needs a title".into());
            return;
        }
        let renamed = match self.desktop.rename_favorite(&favorite.id, &title) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                Err("favorites need the live host".into())
            }
            Err(error) => Err(error),
        };
        match renamed {
            Ok(()) => self.favorite_error = None,
            Err(error) => self.favorite_error = Some(error),
        }
    }

    /// Delete a favorite (T6).
    pub fn delete_favorite_row(&mut self, index: usize) {
        let Some(favorite) = self.host_favorites().get(index).cloned() else {
            return;
        };
        let deleted = match self.desktop.delete_favorite(&favorite.id) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                Err("favorites need the live host".into())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = deleted {
            self.favorite_error = Some(error);
        }
    }
}
