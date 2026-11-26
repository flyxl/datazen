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

/// Dialog-local drafts of non-secret fields. Passwords never enter this cache
/// or cross from one engine to another.
pub(crate) struct ConnectionFormSnapshot {
    name: String,
    address: String,
    database: String,
    username: String,
}

impl AppState {
    /// The export formats the dialog collects, in cycle order.
    pub const EXPORT_FORMATS: [&'static str; 3] = ["csv", "json", "sql"];

    pub fn open_connection_dialog(&mut self) {
        self.connection_dialog_open = true;
        self.invalidate_connection_probe();
        self.redis_probe_pending = false;
        self.redis_probe_result = None;
        self.redis_form_options = Default::default();
        self.redis_form_master = TextEdit::new("");
        self.redis_form_nodes = TextEdit::new("");
        self.redis_form_ca = TextEdit::new("");
        self.redis_form_cert = TextEdit::new("");
        self.redis_form_key = TextEdit::new("");
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
        self.connection_form_snapshots.clear();
        if let Some(port) = self.desktop.default_connection_port("postgresql") {
            self.connection_engine = TextEdit::new("postgresql");
            self.connection_address = TextEdit::new(format!("127.0.0.1:{port}"));
        }
        self.connection_ssh_enabled = false;
        self.connection_ssh_host = TextEdit::new(String::new());
        self.connection_ssh_port = TextEdit::new(String::from("22"));
        self.connection_ssh_username = TextEdit::new(String::new());
    }

    pub fn close_connection_dialog(&mut self) {
        self.connection_dialog_open = false;
        self.invalidate_connection_probe();
        self.connection_error = None;
        self.connection_form_snapshots.clear();
        self.connection_password = TextEdit::new("");
    }

    pub(crate) fn select_connection_form_engine(&mut self, engine: &str) {
        if self.connection_engine.text == engine {
            return;
        }
        self.invalidate_connection_probe();
        // The in-memory demo has no linked engine vocabulary; keep its old
        // immediate form behavior and let the native driver own defaults.
        if self.desktop.available_engines().is_empty() {
            self.connection_engine = TextEdit::new(engine);
            return;
        }
        let previous = self.connection_engine.text.clone();
        let carried_name = self.connection_name.text.clone();
        if !previous.is_empty() {
            self.connection_form_snapshots.insert(previous, ConnectionFormSnapshot {
                name: carried_name.clone(),
                address: self.connection_address.text.clone(),
                database: self.connection_database.text.clone(),
                username: self.connection_username.text.clone(),
            });
        }
        let draft = self.connection_form_snapshots.remove(engine).unwrap_or_else(|| ConnectionFormSnapshot {
            // An unsaved name is harmless and can follow the first switch,
            // but returning to an engine restores its own draft.
            name: carried_name,
            address: self.desktop.default_connection_port(engine)
                .map(|port| format!("127.0.0.1:{port}")).unwrap_or_default(),
            database: String::new(),
            username: String::new(),
        });
        self.connection_engine = TextEdit::new(engine);
        self.connection_name = TextEdit::new(draft.name);
        self.connection_address = TextEdit::new(draft.address);
        self.connection_database = TextEdit::new(draft.database);
        self.connection_username = TextEdit::new(draft.username);
        self.connection_password = TextEdit::new("");
        self.connection_error = None;
    }

    fn connection_form_draft(&self, engine: String) -> datazen_ui_api::ConnectionDraft {
        datazen_ui_api::ConnectionDraft {
            name: self.connection_name.text.clone(),
            engine,
            address: self.connection_address.text.clone(),
            username: self.connection_username.text.trim().to_owned(),
            password: self.connection_password.text.clone(),
            database: self.connection_database.text.trim().to_owned(),
        }
    }

    fn redis_connection_form_options(&self) -> datazen_ui_api::native_kv::RedisConnectionOptions {
        let mut options = self.redis_form_options.clone();
        options.sentinel_master = self.redis_form_master.text.clone();
        options.nodes = self.redis_form_nodes.text.clone();
        options.tls_ca = self.redis_form_ca.text.clone();
        options.tls_cert = self.redis_form_cert.text.clone();
        options.tls_key = self.redis_form_key.text.clone();
        options
    }

    pub(crate) fn test_connection_form(&mut self) {
        if !self.connection_dialog_open {
            return;
        }
        let engine = self.connection_engine.text.trim().to_owned();
        if engine == "redis" {
            self.test_redis_connection_form();
            return;
        }
        if self.connection_probe_pending {
            return;
        }
        self.invalidate_connection_probe();
        if self.connection_ssh_enabled {
            self.connection_probe_result = Some(Err(
                datazen_ui_api::ConnectionProbeError::TunnelUnavailable,
            ));
            return;
        }
        let draft = self.connection_form_draft(engine);
        match self.desktop.begin_connection_probe(self.connection_probe_id, &draft) {
            Ok(()) => self.connection_probe_pending = true,
            Err(error) => self.connection_probe_result = Some(Err(error)),
        }
    }

    pub(crate) fn invalidate_connection_probe(&mut self) {
        self.connection_probe_id = self.connection_probe_id.wrapping_add(1);
        self.connection_probe_pending = false;
        self.connection_probe_result = None;
    }

    fn test_redis_connection_form(&mut self) {
        if !self.connection_dialog_open
            || self.redis_probe_pending
            || self.connection_engine.text.trim() != "redis"
        {
            return;
        }
        let draft = self.connection_form_draft("redis".into());
        let options = self.redis_connection_form_options();
        self.redis_probe_result = None;
        match self
            .desktop
            .begin_redis_probe(self.connection_dialog_epoch, &draft, &options)
        {
            Ok(()) => self.redis_probe_pending = true,
            Err(_) => {
                self.redis_probe_result = Some(Err(
                    "Unable to test Redis. Check the address, topology and TLS fields.".into(),
                ))
            }
        }
    }

    pub fn save_connection_form(&mut self) {
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
        let draft = self.connection_form_draft(engine);
        let result = if draft.engine == "redis" && engines.iter().any(|e| e == "redis") {
            let options = self.redis_connection_form_options();
            self.desktop.save_redis_connection(&draft, &options)
        } else {
            self.desktop.save_connection(&draft)
        };
        let saved = match result {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_connection(&draft)
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(()) => {
                self.close_connection_dialog();
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
    pub fn begin_connections_import(&mut self, path: &str) {
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
    pub fn confirm_connections_import(&mut self) {
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
    pub fn submit_connections_import(&mut self) {
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
    pub fn open_import_dialog(&mut self) {
        self.import_dialog_open = true;
        // See `open_connection_dialog`: a reopened dialog starts at the top.
        self.import_dialog_epoch = self.import_dialog_epoch.wrapping_add(1);
        self.import_file = None;
        self.import_preview = None;
        self.import_notice = None;
        self.import_password = TextEdit::new(String::new());
    }

    pub fn close_import_dialog(&mut self) {
        self.import_dialog_open = false;
        self.import_file = None;
        self.import_preview = None;
        self.import_notice = None;
        self.import_password = TextEdit::new(String::new());
    }

    /// The outbound foreign key of the focused cell's column (T5), with the
    /// column's own pairing index. E1: the key list is the structure
    /// cache's foreign-key segment — the job's answer, never a live read.
    pub fn export_format(&self) -> &'static str {
        Self::EXPORT_FORMATS[(self.export_format_index as usize) % Self::EXPORT_FORMATS.len()]
    }
    pub fn cycle_export_format(&mut self) {
        self.export_format_index =
            (self.export_format_index + 1) % Self::EXPORT_FORMATS.len() as u32;
    }

    /// Ask for an export job id (N11): the host's export first, the memory
    /// port's when the host has none. A missing range gets no job.
    /// T2: the save path is picked first; the answer in `export_answered`
    /// starts the job (or cancels, or falls back to the default directory
    /// when no dialog could show).
    pub fn begin_export_form(&mut self, tx: &mut Tx) {
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
    pub fn export_answered(
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
    pub fn job_progress(&mut self, job: u64) -> Option<ProgressSnapshot> {
        if let Some(snapshot) = self.desktop.host_job_progress(job) {
            return Some(snapshot);
        }
        self.workspace.job_progress(job)
    }

    /// Pull one progress snapshot. No streaming, no polling.
    pub fn refresh_export_progress(&mut self) {
        if let Some(job) = self.export_job {
            self.export_snapshot = self.job_progress(job);
        }
    }
}
