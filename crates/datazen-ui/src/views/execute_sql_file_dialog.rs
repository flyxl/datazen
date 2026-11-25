//! Port of `packages/frontend/src/windows/connection/ExecuteSqlFileDialog.tsx`.
//!
//! React's dialog is a status/log surface around a host-owned file-picker +
//! execution flow (`runSqlFileExecution`). Rune has no async bridge here, so the
//! picker and the SQL run are an injected [`ExecuteSqlFileHostPort`]; this view
//! owns only the React state machine (validation, running, status, log) and the
//! keys. No IPC, thread or future is created here.
//!
//! ## Keys
//!
//! React `data-testid`s are preserved (`sql-file-execute`, `sql-file-status`),
//! as are T223's slot keys (`content-view-sql-file-dialog` with `-title`,
//! `-body`, `-cancel`, `-confirm`). `data-status-kind` becomes the status
//! container's accessible label; the status icons are the registry's check/x
//! glyphs (no dedicated check-circle/x-circle exists, and no new glyph is made).
//!
//! ## Leftover L1 (removal stage)
//!
//! The confirm step (`useConfirmDialog`) and the progress-log pump are host
//! concerns: the host resolves `on_confirm` and feeds [`ExecuteSqlFileState`]
//! progress lines. Replace those stand-ins with the shared confirm/log ports
//! when they land, then drop the per-item allows.

use std::rc::Rc;

use rune::{col, icon, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_X};
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

pub const DIALOG_KEY: &str = "content-view-sql-file-dialog";
pub const EXECUTE_KEY: &str = "sql-file-execute";
pub const STATUS_KEY: &str = "sql-file-status";

/// React `StatusKind`.
#[allow(dead_code)] // view VM; consumer: the host execution port (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SqlFileStatusKind {
    #[default]
    Idle,
    Running,
    Success,
    Error,
}

impl SqlFileStatusKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Success => "success",
            Self::Error => "error",
        }
    }
}

/// The three `useState` values around the host execution.
#[allow(dead_code)] // view VM; consumer: the host execution port (not wired).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecuteSqlFileState {
    pub running: bool,
    pub status_message: String,
    pub status_kind: SqlFileStatusKind,
    pub progress_log: Vec<String>,
}

impl ExecuteSqlFileState {
    /// React's `open === false` reset effect.
    #[allow(dead_code)] // host API; consumer: the dialog host (not wired).
    pub fn reset(&mut self) {
        self.running = false;
        self.status_message.clear();
        self.status_kind = SqlFileStatusKind::Idle;
        self.progress_log.clear();
    }

    /// React `handleExecute` prologue: no database means no command at all.
    #[allow(dead_code)] // validation; consumed by the execute button.
    pub fn begin(&mut self, database: Option<&str>) -> bool {
        if database.map_or(true, str::is_empty) {
            return false;
        }
        self.running = true;
        self.status_kind = SqlFileStatusKind::Running;
        self.status_message = t("backup.restoring").to_owned();
        self.progress_log = vec![self.status_message.clone()];
        true
    }

    /// React `onProgress` — a `done` phase settles success, any other line keeps
    /// the running state.
    #[allow(dead_code)] // host API; consumer: the execution port (not wired).
    pub fn apply_progress(&mut self, line: String, done: bool) {
        self.status_message = line.clone();
        if done {
            self.status_kind = SqlFileStatusKind::Success;
        } else if !line.is_empty() {
            self.status_kind = SqlFileStatusKind::Running;
        }
        self.progress_log.push(line);
    }

    /// React `onError`.
    #[allow(dead_code)] // host API; consumer: the execution port (not wired).
    pub fn apply_error(&mut self, message: String) {
        self.status_message = message.clone();
        self.status_kind = SqlFileStatusKind::Error;
        self.progress_log.push(message);
    }

    /// React `finally { setRunning(false) }`.
    #[allow(dead_code)] // host API; consumer: the execution port (not wired).
    pub fn settle(&mut self) {
        self.running = false;
    }
}

/// The request the host executes: it owns the native file picker and session.
#[allow(dead_code)] // host port; consumer: the dialog host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlFileRequest {
    pub db_session_id: String,
    pub database: String,
}

/// Host ports behind the dialog. `on_confirm` gates execution like React's
/// `confirmBeforeExecute`; a missing port means "confirmed".
#[allow(dead_code)] // host port; consumer: the dialog host (not wired).
pub struct ExecuteSqlFileHostPort<S> {
    pub on_close: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_execute: Option<Rc<dyn Fn(&mut S, SqlFileRequest) + 'static>>,
    pub on_executed: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_confirm: Option<Rc<dyn Fn(&mut S) -> bool + 'static>>,
}

impl<S> Default for ExecuteSqlFileHostPort<S> {
    fn default() -> Self {
        Self {
            on_close: None,
            on_execute: None,
            on_executed: None,
            on_confirm: None,
        }
    }
}

/// React `ExecuteSqlFileDialogProps` plus the state slice the host owns.
#[allow(dead_code)] // view VM; consumer: the dialog host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecuteSqlFileVm {
    pub open: bool,
    pub connection_name: String,
    pub database: Option<String>,
    pub db_session_id: String,
    pub state: ExecuteSqlFileState,
}

impl Default for ExecuteSqlFileVm {
    fn default() -> Self {
        Self {
            open: false,
            connection_name: String::new(),
            database: None,
            db_session_id: String::new(),
            state: ExecuteSqlFileState::default(),
        }
    }
}

/// Build the dialog into `layer`. A closed dialog mounts nothing, as in React.
#[allow(dead_code)] // view entry point; consumer: the content-view dialogs slot.
pub fn execute_sql_file_dialog<S: 'static>(
    vm: &ExecuteSqlFileVm,
    dark: bool,
    layer: &mut Vec<El>,
    host: ExecuteSqlFileHostPort<S>,
) {
    let k = kit(dark);
    let target_label = match vm.database.as_deref() {
        Some(database) => t("sqlFile.targetDatabase").replace("{database}", database),
        None => t("sqlFile.noDatabase").to_owned(),
    };
    let description = format!("{} · {}", vm.connection_name, target_label);

    let mut dialog = Dialog::new(DIALOG_KEY, t("common.executeSqlFile"))
        .open(vm.open)
        .description(description)
        // React `max-w-2xl`; the Dialog base fills the remaining window height
        // with a scrim behind it, so the overlay geometry is full-window.
        .width(672.0)
        .on_close({
            let on_close = host.on_close.clone();
            // React guards `onClose` with `if (!running)`: the header X, Escape
            // and backdrop dismissal are inert while a file is executing.
            let running = vm.state.running;
            move |s: &mut S| {
                if running {
                    return;
                }
                if let Some(on_close) = &on_close {
                    on_close(s);
                }
            }
        });

    // React renders the banner only when `statusMessage` is truthy.
    if !vm.state.status_message.is_empty() {
        let kind = vm.state.status_kind;
        let mut status = col()
            .key(STATUS_KEY)
            .role("status")
            .label(kind.as_str())
            .w_full()
            .gap(4.0);
        let icon_el = match kind {
            SqlFileStatusKind::Error => Some(icon(&ICON_X, 14.0).color(k.c.danger.fg)),
            SqlFileStatusKind::Success => Some(icon(&ICON_CHECK, 14.0).color(k.c.success.fg)),
            // The kit registry has no spinner glyph; the message carries the
            // running state instead of inventing one.
            _ => None,
        };
        if let Some(glyph) = icon_el {
            status = status.child(glyph);
        }
        status = status.child(
            text(vm.state.status_message.clone())
                .key(format!("{STATUS_KEY}-text"))
                .font(k.ty.text_xsmall)
                .color(match kind {
                    SqlFileStatusKind::Error => k.c.danger.fg,
                    SqlFileStatusKind::Success => k.c.success.fg,
                    _ => k.c.text_muted,
                })
                .wrap(),
        );
        for (index, line) in vm.state.progress_log.iter().enumerate() {
            status = status.child(
                text(line.clone())
                    .key(format!("{STATUS_KEY}-log-{index}"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }
        dialog = dialog.child(status);
    }

    let mut cancel = Button::new(t("common.close"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::ExtraLarge)
        .disabled(vm.state.running);
    if let Some(on_close) = host.on_close.clone() {
        cancel = cancel.on_press(move |s: &mut S| on_close(s));
    }
    dialog = dialog.footer(cancel.build(k).key(format!("{DIALOG_KEY}-cancel")));

    let execute_label = if vm.state.running {
        t("backup.restoring")
    } else {
        t("sqlFile.selectAndRun")
    };
    let disabled = vm.state.running || vm.database.as_deref().map_or(true, str::is_empty);
    let mut execute = Button::new(execute_label)
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::ExtraLarge)
        .disabled(disabled);
    let request = vm.database.as_deref().map(|database| SqlFileRequest {
        db_session_id: vm.db_session_id.clone(),
        database: database.to_owned(),
    });
    if let (Some(on_execute), Some(req)) = (host.on_execute.clone(), request) {
        let on_confirm = host.on_confirm.clone();
        let on_executed = host.on_executed.clone();
        execute = execute.on_press(move |s: &mut S| {
            let confirmed = on_confirm.as_ref().map_or(true, |cb| cb(s));
            if !confirmed {
                return;
            }
            on_execute(s, req.clone());
            if let Some(on_executed) = &on_executed {
                on_executed(s);
            }
        });
    }
    let execute_button = execute.build(k).key(EXECUTE_KEY);
    // The T223 slot also exposes a `-confirm` handle for the execute action;
    // wrap the same button so both keys stay stable for existing callers.
    dialog = dialog.footer(row().key(format!("{DIALOG_KEY}-confirm")).child(execute_button));

    dialog.build(k, layer);
}
