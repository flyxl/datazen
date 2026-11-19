//! Rune ports of `packages/frontend/src/windows/backup/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `BackupWindow.tsx` (state model + selection logic) | `backup_window_state` + `backup_db_meta` + `backup_grouping` | T026-S1 |
//! | `BackupWindow.tsx` (three columns, chrome, start button, backup/restore runs) | `backup_window` + `backup_progress` + `backup_run` | T026-S2 |
//! | `ProgressLog.tsx` | — | own task |

pub(crate) mod backup_db_meta;
pub(crate) mod backup_grouping;
pub(crate) mod backup_progress;
pub(crate) mod backup_run;
pub(crate) mod backup_window;
pub(crate) mod backup_window_state;

#[allow(unused_imports)] // the Backup window host is the production consumer
pub(crate) use backup_progress::{
    append_progress_log, backup_progress_ratio, format_backup_progress, format_restore_progress,
    BackupLogPump, MAX_PROGRESS_LOG_CHARS, MAX_PROGRESS_LOG_LINES,
};
#[allow(unused_imports)] // the Backup window host is the production consumer
pub(crate) use backup_run::{
    abort_restore_before_execute, apply_backup_progress, apply_restore_progress, begin_backup,
    begin_restore, fail_run, finish_backup, finish_restore, restore_progress_line,
};
#[allow(unused_imports)] // the Backup window host is the production consumer
pub(crate) use backup_window::{
    db_badge_meta, db_type_badge, BackupWindow, BackupWindowEdits, RestoreConfirm,
};

#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub(crate) use backup_db_meta::{backup_options_for, db_label, db_supports_backup, BackupOption};
#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub(crate) use backup_grouping::{
    connection_match, format_group_label, group_connections, group_display_label,
    initial_expanded_groups, rank_connections, ConnectionGroup, ConnectionMatch, MatchReason,
};
#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub(crate) use backup_window_state::{
    progress_log_mount, BackupConnection, BackupProgress, BackupProgressPhase, BackupWindowVm,
    DatabaseInfo, PrefillTarget, SelectConnectionOutcome, SessionConnected,
};
