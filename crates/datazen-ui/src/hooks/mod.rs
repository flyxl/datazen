//! Ports of `packages/frontend/src/hooks/` — view glue that keeps a state
//! machine and a small rendering half (useConfirmDialog, useAiEditorGrant,
//! then the T124 trio useAiKeyboard/useAutoScroll/useClipboardCopyFeedback;
//! T158 adds useColumnPresentationSession, T185 useColumnResize, T210
//! useColumnStats, T233 useCompactToolbar, T254 useConnectionCommand, T275
//! useFkNavigationShortcuts; T294 adds useGridClipboard with its carried
//! lib helpers; the remaining hooks land with their own tasks).

pub(crate) mod confirm_dialog;
pub(crate) mod grid_clipboard_helpers;
pub(crate) mod grid_paste_plan_helpers;
/// `lib/migrationJobHydration.ts` helpers carried for the
/// useMigrationJobHydration port (T376) until T559 lands and absorbs
/// them.
pub(crate) mod migration_job_hydration_helpers;
pub(crate) mod use_ai_editor_grant;
pub(crate) mod use_ai_keyboard;
pub(crate) mod use_auto_scroll;
pub(crate) mod use_clipboard_copy_feedback;
pub(crate) mod use_column_presentation_session;
pub(crate) mod use_column_resize;
pub(crate) mod use_column_stats;
pub(crate) mod use_compact_toolbar;
pub(crate) mod use_connection_command;
pub(crate) mod use_fk_navigation_shortcuts;
pub(crate) mod use_grid_clipboard;
/// T376: per-window job hydration for the three migration windows
/// (job-center-first read, watch re-subscribe, unsubscribe-only
/// unmount).
pub(crate) mod use_migration_job_hydration;
