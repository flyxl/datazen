//! AppState method implementations.

#![allow(unused_imports)]
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::state_def::*;
use crate::types::{Section, TableView, Tool};
use crate::views::{next_connection, workflow_yaml};
use crate::{icons, splitter};
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
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
    /// The export formats the dialog collects, in cycle order.
    pub(crate) const EXPORT_FORMATS: [&'static str; 3] = ["csv", "json", "sql"];

    pub(crate) fn open_connection_dialog(&mut self) {
        self.connection_dialog_open = true;
        // A new open is a new body for the dialog base, so its scroll viewport
        // starts at the top (`Dialog::open_epoch`).
        self.connection_dialog_epoch = self.connection_dialog_epoch.wrapping_add(1);
        self.connection_error = None;
        self.connection_name = TextEdit::new(String::new());
        self.connection_engine = TextEdit::new(String::new());
        self.connection_address = TextEdit::new(String::new());
        self.connection_username = TextEdit::new(String::new());
        self.connection_password = TextEdit::new(String::new());
        self.connection_database = TextEdit::new(String::new());
        self.connection_ssh_enabled = false;
        self.connection_ssh_host = TextEdit::new(String::new());
        self.connection_ssh_port = TextEdit::new(String::from("22"));
        self.connection_ssh_username = TextEdit::new(String::new());
    }

    pub(crate) fn close_connection_dialog(&mut self) {
        self.connection_dialog_open = false;
        self.connection_error = None;
    }

    pub(crate) fn save_connection_form(&mut self) {
        // E5: the engines the host links are the form's whitelist — the
        // registry's own admission ("linked means offered"). A host that
        // answers none is the demo process: the memory port's own default
        // engine applies, and its rules take the save.
        let engines = self.desktop.available_engines();
        let engine = self.connection_engine.text.trim().to_owned();
        let engine = if engine.is_empty() {
            if engines.is_empty() {
                "postgres".to_owned()
            } else {
                self.connection_error = Some(format!(
                    "an engine is required — the host links {}",
                    engines.join(", ")
                ));
                return;
            }
        } else {
            engine
        };
        if !engines.is_empty() && !engines.contains(&engine) {
            self.connection_error = Some(format!(
                "the host links {} — '{engine}' is not among them",
                engines.join(", ")
            ));
            return;
        }
        let draft = datazen_ui_api::ConnectionDraft {
            name: self.connection_name.text.clone(),
            engine,
            address: self.connection_address.text.clone(),
            username: self.connection_username.text.trim().to_owned(),
            password: self.connection_password.text.clone(),
            database: self.connection_database.text.trim().to_owned(),
        };
        let saved = match self.desktop.save_connection(&draft) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_connection(&draft)
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(()) => {
                self.connection_error = None;
                self.connection_dialog_open = false;
                self.connection_name = TextEdit::new(String::new());
                self.connection_engine = TextEdit::new(String::new());
                self.connection_address = TextEdit::new(String::new());
                self.connection_username = TextEdit::new(String::new());
                self.connection_password = TextEdit::new(String::new());
                self.connection_database = TextEdit::new(String::new());
                self.connection_ssh_enabled = false;
                self.connection_ssh_host = TextEdit::new(String::new());
                self.connection_ssh_port = TextEdit::new(String::from("22"));
                self.connection_ssh_username = TextEdit::new(String::new());
            }
            Err(error) => self.connection_error = Some(error),
        }
    }
    /// Parse a picked import file (T7): the preview before anything is
    /// saved. The host is the side with filesystem rights.
    pub(crate) fn begin_connections_import(&mut self, path: &str) {
        self.import_file = Some(path.to_owned());
        let password = self.import_password.text.clone();
        match self.desktop.parse_connections_import(path, &password) {
            Ok(preview) if preview.connections.is_empty() => {
                self.import_preview = None;
                self.import_notice = Some("the import file carries no connections".into());
            }
            Ok(preview) => {
                self.import_preview = Some(preview);
                self.import_notice = None;
            }
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.import_preview = None;
                self.import_notice = Some("connection import needs the live host".into());
            }
            Err(error) => {
                self.import_preview = None;
                self.import_notice = Some(error);
            }
        }
    }

    /// Save the previewed import (T7): names already in the store are
    /// skipped; the notice counts what landed.
    pub(crate) fn confirm_connections_import(&mut self) {
        let Some(preview) = self.import_preview.take() else {
            return;
        };
        match self.desktop.confirm_import(preview.handle) {
            Ok(landed) => {
                self.import_notice = Some(format!("{landed} connections imported"));
                self.sync_desktop_connections();
            }
            Err(error) => self.import_notice = Some(error),
        }
    }

    /// Primary import action: confirm a ready preview, or (re)parse the chosen
    /// file when a password is needed/changed and no preview is available yet.
    pub(crate) fn submit_connections_import(&mut self) {
        if self.import_preview.is_some() {
            self.confirm_connections_import();
            return;
        }
        if let Some(path) = self.import_file.clone() {
            self.begin_connections_import(&path);
            if self.import_preview.is_some() {
                self.confirm_connections_import();
            }
            return;
        }
        self.import_notice = Some("请先选择要导入的连接文件".into());
    }

    /// The import modal (the React share dialog's import mode): opening it
    /// starts from a clean slate; closing drops the journey's picks.
    pub(crate) fn open_import_dialog(&mut self) {
        self.import_dialog_open = true;
        // See `open_connection_dialog`: a reopened dialog starts at the top.
        self.import_dialog_epoch = self.import_dialog_epoch.wrapping_add(1);
        self.import_file = None;
        self.import_preview = None;
        self.import_notice = None;
        self.import_password = TextEdit::new(String::new());
    }

    pub(crate) fn close_import_dialog(&mut self) {
        self.import_dialog_open = false;
        self.import_file = None;
        self.import_preview = None;
        self.import_notice = None;
        self.import_password = TextEdit::new(String::new());
    }

    /// The outbound foreign key of the focused cell's column (T5), with the
    /// column's own pairing index. E1: the key list is the structure
    /// cache's foreign-key segment — the job's answer, never a live read.
    pub(crate) fn export_format(&self) -> &'static str {
        Self::EXPORT_FORMATS[(self.export_format_index as usize) % Self::EXPORT_FORMATS.len()]
    }
    pub(crate) fn cycle_export_format(&mut self) {
        self.export_format_index =
            (self.export_format_index + 1) % Self::EXPORT_FORMATS.len() as u32;
    }

    /// Ask for an export job id (N11): the host's export first, the memory
    /// port's when the host has none. A missing range gets no job.
    /// T2: the save path is picked first; the answer in `export_answered`
    /// starts the job (or cancels, or falls back to the default directory
    /// when no dialog could show).
    pub(crate) fn begin_export_form(&mut self, tx: &mut Tx) {
        let table = self.grid.table_label().to_owned();
        let format = self.export_format().to_owned();
        let range = self.export_range.text.trim().to_owned();
        let options = FileDialogOptions {
            title: format!("Export {table} as {format}"),
            file_name: Some(format!("export-{table}.{format}")),
            ..Default::default()
        }
        .filter(
            if format == "csv" {
                "CSV files"
            } else {
                "JSON files"
            },
            &[format.as_str()],
        );
        tx.save_file(options, move |state: &mut AppState, result, _| {
            state.export_answered(table, format, range, result);
        });
    }
    pub(crate) fn export_answered(
        &mut self,
        table: String,
        format: String,
        range: String,
        result: FileDialogResult,
    ) {
        let (path, notice) = match result {
            Ok(Some(path)) => (Some(path.display().to_string()), None),
            Ok(None) => {
                self.export_job = None;
                self.export_error = Some("the export was cancelled; nothing was written".into());
                return;
            }
            Err(error) => (
                None,
                Some(format!(
                    "the path picker is unavailable ({error}); exporting to the default directory"
                )),
            ),
        };
        let started = self
            .desktop
            .start_export(&table, &format, &range, path.as_deref());
        let job = match started {
            Ok(job) => Some(job),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.start_export(&format, &range).ok()
            }
            Err(error) => {
                self.export_job = None;
                self.export_error = Some(error);
                return;
            }
        };
        match job {
            Some(job) => {
                self.export_job = Some(job);
                self.export_error = notice;
                self.export_snapshot = self.job_progress(job);
            }
            None => {
                self.export_job = None;
                self.export_error = Some("the export was not accepted".into());
            }
        }
    }

    /// One job's progress: the host's if it owns the job, the port's
    /// otherwise (N11).
    pub(crate) fn job_progress(&mut self, job: u64) -> Option<ProgressSnapshot> {
        if let Some(snapshot) = self.desktop.host_job_progress(job) {
            return Some(snapshot);
        }
        self.workspace.job_progress(job)
    }

    /// Pull one progress snapshot. No streaming, no polling.
    pub(crate) fn refresh_export_progress(&mut self) {
        if let Some(job) = self.export_job {
            self.export_snapshot = self.job_progress(job);
        }
    }
}
