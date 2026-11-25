//! Rune ports of `packages/frontend/src/windows/backup/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `BackupWindow.tsx` (state model + selection logic) | `backup_window_state` + `backup_db_meta` + `backup_grouping` | T026-S1 |
//! | `BackupWindow.tsx` (three columns, chrome, start button, backup/restore runs) | `backup_window` + `backup_progress` + `backup_run` | T026-S2 |
//! | `ProgressLog.tsx` (model + virtual window + copy feedback) | `progress_log_state` | T076-S1 |
//! | `ProgressLog.tsx` (header, copy button, virtualized body) | `progress_log` | T076-S2 |

pub mod backup_db_meta;
pub mod backup_grouping;
pub mod backup_progress;
pub mod backup_run;
pub mod backup_window;
pub mod backup_window_state;
pub mod progress_log;
pub mod progress_log_state;

#[allow(unused_imports)] // the Backup window host is the production consumer
pub use backup_progress::{
    append_progress_log, backup_progress_ratio, format_backup_progress, format_restore_progress,
    BackupLogPump, MAX_PROGRESS_LOG_CHARS, MAX_PROGRESS_LOG_LINES,
};
#[allow(unused_imports)] // the Backup window host is the production consumer
pub use backup_run::{
    abort_restore_before_execute, apply_backup_progress, apply_restore_progress, begin_backup,
    begin_restore, fail_run, finish_backup, finish_restore, restore_progress_line,
};
#[allow(unused_imports)] // the Backup window host is the production consumer
pub use backup_window::{
    db_badge_meta, db_type_badge, BackupWindow, BackupWindowEdits, RestoreConfirm,
};

#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub use backup_db_meta::{backup_options_for, db_label, db_supports_backup, BackupOption};
#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub use backup_grouping::{
    connection_match, format_group_label, group_connections, group_display_label,
    initial_expanded_groups, rank_connections, ConnectionGroup, ConnectionMatch, MatchReason,
};
#[allow(unused_imports)] // the Backup window (T026-S2) is the production consumer
pub use backup_window_state::{
    progress_log_mount, BackupConnection, BackupProgress, BackupProgressPhase, BackupWindowVm,
    DatabaseInfo, PrefillTarget, SelectConnectionOutcome, SessionConnected,
};
#[allow(unused_imports)] // the Backup window (T076-S2) is the production consumer
pub use progress_log::ProgressLog;
#[allow(unused_imports)] // the ProgressLog component (T076-S2) is the production consumer
pub use progress_log_state::{
    copy_text, is_near_bottom, row_offset, should_auto_scroll, total_size, visible_range,
    ProgressLogState, COPIED_FEEDBACK_MS, OVERSCAN, ROW_HEIGHT,
};
