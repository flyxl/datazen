//! T026-S2: the Backup window's run-state machine.
//!
//! The pure halves of `handleBackup` / `handleRestore`
//! (`BackupWindow.tsx`) and of `runSqlFileExecution`
//! (`lib/sqlFileExecution.ts`): each function is one React step —
//! the head that arms the run, a progress event, the finish, or the
//! catch — applied to the Vm and the [`BackupLogPump`]. The Vm never
//! performs IPC: the host drives the async middle (the
//! `backup-progress` / `restore-progress` listeners, the
//! `backup_database` / `restore_sql_file` invokes, the save/file
//! dialogs, the restore confirm dialog, and the
//! `datazen:refresh-connection` emit) and feeds the events back
//! through these functions, in the order React runs them.
//!
//! ## Host contract (the async middle, per step)
//!
//! `handleBackup` — the host computes the file name from
//! [`BackupWindowVm::default_file_name`] /
//! [`BackupWindowVm::file_extension`], calls [`begin_backup`],
//! listens for `backup-progress` and feeds each payload to
//! [`apply_backup_progress`], invokes `backup_database` with
//! `defaultFileName`, `filterExtension`, the enabled options (the
//! Vm's `BTreeSet` order — sorted, where React passes toggle-insertion
//! order; the backend reads them as a set) and `compress`, then
//! calls [`finish_backup`] with the invoke's boolean; any throw
//! goes to [`fail_run`]. The listener is removed in the `finally`
//! the host runs after the finish/fail call.
//!
//! `handleRestore` — the host calls [`begin_restore`], then runs
//! `runSqlFileExecution`: `listCatalog` and, when the database is
//! non-empty, the confirm dialog; a declined confirm ends the run
//! through [`abort_restore_before_execute`] (only `backing` clears —
//! React returns from `runSqlFileExecution` before its log reset,
//! so the `restoring` status and log stay); otherwise the host
//! listens for `restore-progress` and feeds each payload/line to
//! [`apply_restore_progress`], invokes `restore_sql_file`, and
//! calls [`finish_restore`] with the invoke's boolean; on `true`
//! the host emits `datazen:refresh-connection` with the session
//! id. Any throw goes to [`fail_run`].

// The production consumer is the Backup window host (not yet
// written); until then the component tests are the only callers, so
// the dead-code lint is silenced here rather than in a crate-wide
// allow — the same note as `backup_window.rs`.
#![allow(dead_code)]

use crate::i18n::t;
use crate::views::{BackupProgress, BackupProgressPhase, BackupWindowVm};

use super::backup_progress::{
    format_backup_progress, format_restore_progress, BackupLogPump,
};

/// The `done` payload both runs finish with
/// (`{ current: 0, total: 0, objectName: '', phase: 'done' }`).
fn done_progress() -> BackupProgress {
    BackupProgress {
        current: 0,
        total: 0,
        object_name: String::new(),
        phase: BackupProgressPhase::Done,
    }
}

fn apply_pump(vm: &mut BackupWindowVm, pump: &BackupLogPump) {
    vm.progress_log = pump.lines().to_vec();
}

/// The `handleBackup` head: arm the run, clear the last progress,
/// reset the log to the preparing line, and set the status to it.
pub(crate) fn begin_backup(vm: &mut BackupWindowVm, pump: &mut BackupLogPump) {
    vm.backing = true;
    vm.progress = None;
    pump.reset(vec![t("backup.progressPreparing").to_owned()]);
    apply_pump(vm, pump);
    vm.status_message = t("backup.progressPreparing").to_owned();
}

/// One `backup-progress` event: the formatted line becomes the
/// status and is pushed to the log, and the payload becomes the
/// progress bar's state.
pub(crate) fn apply_backup_progress(
    vm: &mut BackupWindowVm,
    pump: &mut BackupLogPump,
    payload: &BackupProgress,
) {
    let line = format_backup_progress(payload);
    vm.progress = Some(payload.clone());
    vm.status_message = line.clone();
    pump.push(&line);
    apply_pump(vm, pump);
}

/// The `handleBackup` tail, after the `backup_database` invoke:
/// `saved == false` is the user cancelling the save dialog — the
/// status and progress clear and the log resets to empty;
/// `saved == true` is success — the status becomes
/// `t('backup.success')`, the progress becomes `done`, and the
/// success line is pushed and flushed. Either way the run ends
/// (`backing` clears, React's `finally`).
pub(crate) fn finish_backup(
    vm: &mut BackupWindowVm,
    pump: &mut BackupLogPump,
    saved: bool,
) {
    if !saved {
        vm.status_message.clear();
        vm.progress = None;
        pump.reset(Vec::new());
        apply_pump(vm, pump);
    } else {
        vm.status_message = t("backup.success").to_owned();
        vm.progress = Some(done_progress());
        pump.push(&t("backup.success").to_owned());
        pump.flush();
        apply_pump(vm, pump);
    }
    vm.backing = false;
}

/// The `handleRestore` head (and the head `runSqlFileExecution`
/// repeats after its confirm gate): arm the run, clear the last
/// progress, reset the log to the restoring line, and set the
/// status to it.
pub(crate) fn begin_restore(vm: &mut BackupWindowVm, pump: &mut BackupLogPump) {
    vm.backing = true;
    vm.progress = None;
    pump.reset(vec![t("backup.restoring").to_owned()]);
    apply_pump(vm, pump);
    vm.status_message = t("backup.restoring").to_owned();
}

/// One `restore-progress` event, as `runSqlFileExecution`'s
/// `onProgress` receives it: a payload (when present) becomes the
/// progress bar's state, the formatted line becomes the status and
/// is pushed to the log. (`format_restore_progress` is the line
/// `runSqlFileExecution` formats before calling `onProgress`.)
pub(crate) fn apply_restore_progress(
    vm: &mut BackupWindowVm,
    pump: &mut BackupLogPump,
    payload: Option<&BackupProgress>,
    line: &str,
) {
    if let Some(payload) = payload {
        vm.progress = Some(payload.clone());
    }
    vm.status_message = line.to_owned();
    pump.push(line);
    apply_pump(vm, pump);
}

/// The `handleRestore` tail, after the `restore_sql_file` invoke:
/// `executed == false` is the user cancelling the file dialog —
/// the log resets to empty and the status and progress clear;
/// `executed == true` is success — the restore-success line is
/// pushed and flushed, the progress becomes `done`, and the status
/// becomes that line. Either way the run ends. The host emits
/// `datazen:refresh-connection` after this returns, only when
/// `executed` is true.
pub(crate) fn finish_restore(
    vm: &mut BackupWindowVm,
    pump: &mut BackupLogPump,
    executed: bool,
) {
    if !executed {
        pump.reset(Vec::new());
        apply_pump(vm, pump);
        vm.status_message.clear();
        vm.progress = None;
    } else {
        let done = t("backup.restoreSuccess").to_owned();
        pump.push(&done);
        pump.flush();
        apply_pump(vm, pump);
        vm.progress = Some(done_progress());
        vm.status_message = done;
    }
    vm.backing = false;
}

/// A declined restore confirm: `runSqlFileExecution` returns `false`
/// before its log reset, so the log, status, and progress stay as
/// [`begin_restore`] left them; only the `finally`'s
/// `setBacking(false)` runs. (This is distinct from
/// [`finish_restore`] with `executed == false`, which is the
/// cancelled file dialog *after* the confirm.)
pub(crate) fn abort_restore_before_execute(vm: &mut BackupWindowVm) {
    vm.backing = false;
}

/// The `catch` of either run: the error becomes the status, the
/// progress clears, and the message is pushed to the log and
/// flushed; the run ends. (`runSqlFileExecution`'s own catch pushes
/// first, so a restore error reaches the pump twice — the pump
/// dedupes the consecutive identical line.)
pub(crate) fn fail_run(
    vm: &mut BackupWindowVm,
    pump: &mut BackupLogPump,
    message: &str,
) {
    vm.status_message = message.to_owned();
    vm.progress = None;
    pump.push(message);
    pump.flush();
    apply_pump(vm, pump);
    vm.backing = false;
}

/// The restore progress line for a payload, for the host's
/// `restore-progress` listener (the line `runSqlFileExecution`
/// formats before its `onProgress` call).
pub(crate) fn restore_progress_line(payload: &BackupProgress) -> String {
    format_restore_progress(payload)
}
