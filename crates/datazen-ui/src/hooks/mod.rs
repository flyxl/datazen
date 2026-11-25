//! Ports of `packages/frontend/src/hooks/` — view glue that keeps a state
//! machine and a small rendering half (useConfirmDialog, useAiEditorGrant,
//! then the T124 trio useAiKeyboard/useAutoScroll/useClipboardCopyFeedback;
//! T158 adds useColumnPresentationSession, T185 useColumnResize, T210
//! useColumnStats, T233 useCompactToolbar, T254 useConnectionCommand, T275
//! useFkNavigationShortcuts; T294 adds useGridClipboard with its carried
//! lib helpers; the remaining hooks land with their own tasks).

pub mod confirm_dialog;
pub mod grid_clipboard_helpers;
pub mod grid_paste_plan_helpers;
/// `lib/migrationJobHydration.ts` helpers carried for the
/// useMigrationJobHydration port (T376) until T559 lands and absorbs
/// them.
pub mod migration_job_hydration_helpers;
pub mod use_ai_editor_grant;
pub mod use_ai_keyboard;
pub mod use_auto_scroll;
pub mod use_clipboard_copy_feedback;
pub mod use_column_presentation_session;
pub mod use_column_resize;
pub mod use_column_stats;
pub mod use_compact_toolbar;
pub mod use_connection_command;
pub mod use_fk_navigation_shortcuts;
pub mod use_grid_clipboard;
/// T376: per-window job hydration for the three migration windows
/// (job-center-first read, watch re-subscribe, unsubscribe-only
/// unmount).
pub mod use_migration_job_hydration;
/// T422: the foreign keys of one table as the grid needs them (DB-07:
/// the schema-cache read, the column→group index, degradation).
pub mod use_table_foreign_keys;
/// T341a: the host-side alias for the single i18n hook (the table
/// face + `{param}` interpolation).
pub mod use_i18n;
/// T341b: the keyboard-shortcut registry (spec grammar, scope gates,
/// first-match-wins resolution).
pub mod use_keyboard_shortcuts;
/// T353: the leaf-side read of a KV panel's selected key (the
/// host-owned relay's scalar, the AI button / assistant drawer's input).
pub mod use_kv_slot_selected_key;
/// T363: ensure the lazy i18n domain packs are loaded for the active
/// language (the stable-key ready gate; the packs themselves are
/// compiled in under the T018 ruling).
pub mod use_locale_domains;
/// T384: the OS detection hook (the user-agent parse, the probe
/// cache, the synchronous first frame; absorbed `Platform` from the
/// sql_editor_context_menu carry).
pub mod use_platform;



// T525/T527 seam: the grid keyboard semantics + the composing guard,
// carried for the grid hooks until their own ledger tasks land (the
// T041 mirror convention).
pub mod grid_keyboard_helpers;
// T313: the grid keyboard wiring (DB-02).
pub mod use_grid_keyboard_nav;
// T328: the grid's pointer wiring (DB-01: the drag session, the
// gutter hand-off, Escape's cancel-drag-first clearing).
pub mod use_grid_selection;
// T395: the per-pane query-execution read (the whole-entry and
// single-field faces over the pane-key rule).
mod use_query_exec;
// T404: the resizable-panel drag machine (the persisted clamp, the
// measured-start adoption, the reverse axis).
mod use_resizable;
// T404: the per-window settings composition (the settings sync
// mount + T430's theme sync machine).
mod use_settings;
// T413: the cross-window settings sync (the T11 notification rule:
// a re-read, never a trusted snapshot).
mod use_settings_sync;
// T430: the reusable theme sync (the menu's change + the other
// windows' theme/theme-pack toggles over the T413 triple).
mod use_theme_sync;
// T438's carried half: the failure classification, the read-model
// projections and the lib predicates (T561's until it lands).
pub mod transfer_job_run_helpers;
// T445: the host presentation pipeline wiring hook (the registry
// revision + the cache-key-only plan memo over T157's builder).
mod use_value_presentation;
// T451: the snapshot provider hook (the store's stored value + O-5
// revision counter + language over T184's parse/build).
mod use_value_presentation_snapshot;
// T438: the Data Transfer Job execution state machine (the prepare →
// apply → verdict lifecycle over the T424 command rail).
mod use_transfer_job_run;
// T457's split half: the virtual table's layout vocabulary (the
// VirtualRow projection, the default overscan, the max-offset clamp).
pub mod virtual_table_helpers;
// T457: the virtual table hook (the viewport window with overscan on
// both edges, the absolute row offsets, the clamped scroll ask).
pub mod use_virtual_table;
