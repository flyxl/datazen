//! Rune ports of `packages/frontend/src/windows/backup/**`.
//!
//! | React source | Rune module | task |
//! |---|---|---|
//! | `BackupWindow.tsx` (state model + selection logic) | `backup_window_state` + `backup_db_meta` + `backup_grouping` | T026-S1 |
//! | `BackupWindow.tsx` (three columns, chrome, start button, backup/restore runs) | — | T026-S2 |
//! | `ProgressLog.tsx` | — | own task |

pub(crate) mod backup_db_meta;
pub(crate) mod backup_grouping;
pub(crate) mod backup_window_state;

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
