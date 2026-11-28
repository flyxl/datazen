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

use crate::hooks::confirm_dialog::{ConfirmDialogHost, ConfirmDialogState, ConfirmOptionsVm};
use crate::{i18n::t, ui::confirm_dialog::ConfirmKind};

/// Transient owner, not another snippet store. The exact row the user saw
/// cannot silently become a different row with the same saved name.
#[derive(Default)]
pub(crate) struct NativeSnippetDelete {
    confirm: ConfirmDialogState,
    target: Option<datazen_ui_api::Snippet>,
}

/// An ephemeral Add draft, separate from the existing Update fields.
#[derive(Default)]
pub(crate) struct NativeSnippetAdd {
    pub(crate) open: bool,
    pub(crate) epoch: u64,
    pub(crate) name: TextEdit,
    pub(crate) sql: TextEdit,
    pub(crate) error: Option<String>,
}

pub(crate) fn mount_snippet_delete_confirmation(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    let pending = &state.native_snippet_delete;
    if !state.on_editor_settings() || pending.target.is_none() || !pending.confirm.is_open() {
        return;
    }
    let id = pending.confirm.pending_id;
    ConfirmDialogHost::<AppState>::new(&pending.confirm)
        .on_resolved(move |state, result| state.answer_snippet_delete(id, result))
        .build(dark, layer);
}

impl AppState {
    pub fn host_snippets(&self) -> Vec<datazen_ui_api::Snippet> {
        self.desktop
            .snippets()
            .unwrap_or_else(|_| self.workspace.snippets())
    }

    pub(crate) fn open_native_snippet_add(&mut self) {
        if !self.on_editor_settings() || self.native_snippet_add.open || self.native_snippet_delete.confirm.is_open() {
            return;
        }
        self.native_snippet_add = NativeSnippetAdd {
            open: true,
            epoch: self.native_snippet_add.epoch.wrapping_add(1),
            ..Default::default()
        };
    }

    pub(crate) fn close_native_snippet_add(&mut self, epoch: u64) {
        if self.native_snippet_add.epoch == epoch {
            self.retire_native_snippet_add();
        }
    }

    pub(crate) fn retire_native_snippet_add(&mut self) {
        self.native_snippet_add = NativeSnippetAdd {
            epoch: self.native_snippet_add.epoch,
            ..Default::default()
        };
    }

    pub(crate) fn submit_native_snippet_add(&mut self, epoch: u64) {
        let draft = &self.native_snippet_add;
        if !self.on_editor_settings() || !draft.open || draft.epoch != epoch {
            return;
        }
        let name = draft.name.text.trim().to_owned();
        let sql = draft.sql.text.trim().to_owned();
        // The exact same host-first persistence route, including duplicate-name
        // handling and the demo-only in-memory fallback. No React metadata is
        // invented or written into the separate settings file.
        let result = if sql.is_empty() {
            Err(crate::i18n::with_language(self.builtin_language(), || t("query.snippets.templateRequired")).to_owned())
        } else {
            match self.desktop.save_snippet(&name, &sql) {
                Err(error) if error == DESKTOP_NOT_IMPLEMENTED => self.workspace.save_snippet(&name, &sql),
                result => result,
            }
        };
        match result {
            Ok(()) => {
                self.snippet_error = None;
                self.retire_native_snippet_add();
            }
            Err(error) => self.native_snippet_add.error = Some(error),
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
    pub(crate) fn on_editor_settings(&self) -> bool {
        self.active == Section::Settings && self.settings_section() == "editor"
    }

    pub(crate) fn request_snippet_delete(&mut self, target: &datazen_ui_api::Snippet) {
        if !self.on_editor_settings()
            || self.native_snippet_add.open
            || self.native_snippet_delete.confirm.is_open()
            || !self.host_snippets().contains(target)
        {
            return;
        }
        // This is a pointer/key callback, outside the view's locale scope.
        let options = crate::i18n::with_language(self.builtin_language(), || ConfirmOptionsVm {
            title: t("query.snippets.deleteConfirmTitle").to_owned(),
            message: t("query.snippets.deleteConfirmMessage").replace("{prefix}", &target.name),
            kind: ConfirmKind::Warning,
            ..ConfirmOptionsVm::default()
        });
        self.native_snippet_delete.target = Some(target.clone());
        self.native_snippet_delete.confirm.open(options);
    }

    fn answer_snippet_delete(&mut self, id: u64, result: bool) {
        if !self.on_editor_settings() {
            return;
        }
        let Some(ok) = self.native_snippet_delete.confirm.answer(id, result) else { return; };
        let target = self.native_snippet_delete.target.take();
        if let Some(target) = target.filter(|target| ok && self.host_snippets().contains(target)) {
            self.delete_snippet(&target.name);
        }
    }

    pub(crate) fn retire_snippet_delete_confirmation(&mut self) {
        let _ = self.native_snippet_delete.confirm.answer_pending(false);
        self.native_snippet_delete.target = None;
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


#[cfg(test)]
mod snippet_delete_confirmation_tests {
    use super::*;

    #[test]
    fn stale_request_and_changed_live_snippet_cannot_delete() {
        let mut state = AppState::default();
        state.workspace.save_snippet("saved", "SELECT 1;").unwrap();
        let original = state.host_snippets().into_iter().next().unwrap();

        state.request_snippet_delete(&original);
        assert!(!state.native_snippet_delete.confirm.is_open(), "outside owner");
        state.open_settings(Some("editor"));
        state.request_snippet_delete(&original);
        let first = state.native_snippet_delete.confirm.pending_id;
        state.set_settings_section("general");
        assert!(!state.native_snippet_delete.confirm.is_open());
        state.set_settings_section("editor");
        state.request_snippet_delete(&original);
        let second = state.native_snippet_delete.confirm.pending_id;
        assert_ne!(first, second);
        state.answer_snippet_delete(first, true);
        assert!(state.native_snippet_delete.confirm.is_open());
        assert_eq!(state.host_snippets(), vec![original.clone()]);

        state.workspace.update_snippet("saved", "SELECT 2;").unwrap();
        state.answer_snippet_delete(second, true);
        assert!(!state.native_snippet_delete.confirm.is_open());
        let current = state.host_snippets();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].sql, "SELECT 2;");
        assert_ne!(current[0], original);
    }
}
