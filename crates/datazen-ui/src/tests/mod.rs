#[allow(unused_imports)]
pub(crate) use crate::{
    query_selected, sql_editor, type_, view, AppState, Section, Setting, APP_FONTS,
    APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX,
    NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY,
    TABLE_PAGE_SIZE,
};
#[allow(unused_imports)]
pub(crate) use datazen_ui_api::*;
#[allow(unused_imports)]
pub(crate) use rune::{Env, Runtime};

pub(crate) mod support;
pub(crate) mod tokens;
// T0-09: the tree base (geometry, keyboard, virtualisation, selection).
mod tree;
// T0-09 audit: virtual staging, page height, edges and the selected a11y state.
mod tree_audit;
// T0-11 patch: a compact tooltip's long hint wraps inside the window.
mod tooltip_text;
// T0-04: chrome icons come from one 24-view lucide registry.
mod icon_registry;
// T0-11 audit: independent wrap and placement probes.
mod tooltip_text_audit;
// T0-11 row math: external selection reveal and page arithmetic.
mod tree_rowmath_audit;

// T004: the column-statistics dialog's four branches, dismissal and bounds.
mod column_stats_audit;
mod column_stats_dialog;
// T002: the single DataTable cell — formatting, colour, plan and geometry.
mod cell_renderer;
// Independent T002 audit: tree-observed colour, branch order, raw handoff, flags and geometry.
mod cell_renderer_audit;
// T054-S1: ColumnPresentationDialog draft model + pure functions (coder tests).
mod column_presentation_dialog;
// T054-S1 audit: independent mirror checks (IANA bounds, verbatim messages,
// predicate wiring, fallback and round-trips).
mod column_presentation_dialog_audit;
// T054-S2: ColumnPresentationDialog dialog component + state + save/enable
// wiring (coder tests: plan/preview/rules, state machine, render).
mod column_presentation_dialog_s2;
// T054-S2 audit: independent checks (U+FEFF, plan order a-e, preview,
// regex bytes semantics, save/enable outbox, render).
mod column_presentation_dialog_s2_audit;
pub(crate) use support::*;

pub(crate) fn runtime() -> Runtime<AppState> {
    let env = Env {
        system_dark: true,
        scale: 1.0,
    };
    Runtime::new(AppState::default(), view, (1440.0, 900.0), env)
}

/// T2: an Export or Backup press opens a save dialog first. Answer it the
/// way a desktop would and run the answer's action.
pub(crate) fn answer_save_dialog(rt: &mut Runtime<AppState>, path: Option<&str>) {
    rt.frame(0.0);
    let (kind, _) = rt.file_dialog().expect("the press asked for a save dialog");
    assert_eq!(kind, rune::FileDialogKind::Save);
    rt.resolve_file_dialog(path.map(std::path::PathBuf::from))
        .expect("the dialog answers");
    rt.frame(0.0);
}

/// Glyph ids of `text` shaped by `system`, in layout order.
pub(crate) fn glyph_ids(system: &mut rune::text::TextSystem, text: &str) -> Vec<u32> {
    use parley::PositionedLayoutItem;

    let layout = system.layout_rich(text, &[], type_::LABEL, 2.0, None);
    let mut ids = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(run) = item {
                ids.extend(run.glyphs().map(|glyph| glyph.id));
            }
        }
    }
    ids
}

// Shell, navigation, connections home, data grid, fonts and rendering.
mod shell_grid;
// Query editing/results, undo/redo/copy/cancel, generation guards.
mod query_results;
// ER diagram, foreign-key views, column drafts.
mod structure_er;
// Export/import, backup, value search, favorites.
mod io_tools;
// Sync wizard, schema compare, transfer.
mod sync_transfer;
// Workspace tabs, host wiring, forms, settings page.
mod workspace_tabs;
// Structure/bars/inbound jobs and caches.
mod structure_jobs;
// Host job delivery.
mod host_jobs;
// Toolbar/status sizing, light theme, row numbers, column width.
mod chrome_metrics;
// Dialog base: mount rules, Escape close, focus return.
mod dialog;
// A3 query workspace (editor/results/sidebar/tabs).
mod query_workspace;
// Rail alignment and modal dialogs.
mod rail_modals;
// Base kit primitives: select and the switch family.
mod components;
// T0-09 table base: geometry, sort, selection, keyboard, window fit.
mod table;
mod data_table_parts;
mod data_table_parts_audit;
mod table_audit;
// Independent review coverage: select (T0-06) and checkbox/switch/radio (T0-09).
mod choice_extra;
// Menu and context-menu base.
mod menus;
mod menus_audit;
mod select_values;
// Toast / Tooltip / HoverCard overlay base (T0-10).
mod overlay;
// Independent audit for T0-10 overlay base.
mod overlay_audit;
// Second-round independent recheck for T0-10 fixes.
mod overlay_retest;
// zh-CN table wiring: the keys the rune views took over still resolve.
mod i18n_usage;
mod i18n_sweep;
// Independent audit of the settings/rail i18n batch.
mod i18n_usage_audit;
// Independent audit of the batch-3 view translations.
mod i18n_batch3_audit;

// T0-11 error display bases.
mod banner;
mod banner_audit;
mod copyable_timer_audit;
mod copyable;

// T0-02 text field base and its audit.
#[cfg(test)]
mod field_kit;
mod field_kit_audit;

// Slider and PathInput base controls (T0-11).
mod widgets;
mod widgets_audit_t0_11;

// T0-11 viewer bases.
mod binary_viewer;
mod json_viewer;
// Independent audit for T0-11 viewer bases.
mod viewer_audit;

// T0-12 base geometry regression: every base inside the window, siblings apart.
mod base_geometry;

// T0-11 chrome bases: ToolbarButton, Tabs, Spinner, Badge, Label, ToolbarShell.
mod chrome_parts;
mod chrome_parts_audit;
mod chrome_fix_audit;

// T0-11 temporal value input: typing, validation and commit keys.
mod temporal_input;

// T0-11 audit: independent probes for the temporal editor contract.
mod temporal_input_audit;

// T029: history components (entry card + filter bar).
mod history_components;
mod history_components_audit;
// T005: the global query-history dialog port.
mod global_query_history_dialog;
mod global_query_history_audit;

// T029 fix round: CopyButton base feedback + filter bar metrics.
mod copy_button;
// T253-T256: the .ts support layer semantics.
mod support_layer_audit;
// T028: lib/chart/export.ts port — PNG/SVG export options, data-URL
// splitting/decoding, save requests and dialog options.
mod chart_export;
// T028 independent audit: fresh fixtures, boundary inputs, and the two
// places the port is stricter than atob / decodeURIComponent.
mod chart_export_audit;
// T034: React lib/session/counters.ts parity (counter + handle value logic).
mod counters;
// T034 independent audit: 2^53 gate, branch order, regex-vs-BigInt faithfulness.
mod counters_audit;
// T043-S1: platform/blobTablesExporter.ts slice 1 — types, capability
// gates, DDL resolution and shared planning (planner mirror + exporter seam).
mod blob_tables_exporter;
mod export_plan;
// T043-S1 independent audit: gate-order combinations, DDL in
// data_and_structure, the S1/S2 boundary, planner error propagation.
mod blob_tables_exporter_audit;
mod export_plan_audit;
// T043-S2: platform/blobTablesExporter.ts slice 2 — data emission
// (provided + table branches) and merged delivery.
mod blob_tables_exporter_emission;
// T043-S2 independent audit: re-derived emission + merge probes,
// pinned divergences (integral floats, nested key order, the
// `isPrimaryKey` middle step, ragged provided rows) and the S1
// tester findings (merge MIME, null cursor, exponent rendering).
mod blob_tables_exporter_emission_audit;
// T039: React stores/activeConnectionStore.ts parity (active-connection store).
mod active_connection_store;
// T039 independent audit: markConnected spread semantics, timeout re-arm
// and per-entry isolation, disconnect finally ordering, reset interplay.
mod active_connection_store_audit;
// T087-S1: stores/aiStore.ts config/profile/provider slice.
mod ai_store_config;
// T087-S1 independent audit: isConfigured formula, delete-active fallback,
// save does not write settings until the reload, listener retry.
mod ai_store_config_audit;
// T087-S2: NL2SQL, diagnosis, EXPLAIN analysis, smart filter.
mod ai_store_assist;
// T087-S2 independent audit: blank generate leaves an in-flight request,
// clearNl2Sql does not touch diagnosis, blank parseFilter keeps filters.
mod ai_store_assist_audit;
// T087-S3: chat session token, clobber guard, delete, send, clear.
mod ai_store_chat;
// T087-S3 independent audit: streaming blocks restore, select invalidates
// the previous init token, delete of another id keeps the open session.
mod ai_store_chat_audit;
// T041: React stores/tableData/connectionState.ts parity (row records, cell
// values, error messages, the empty panel state, the load target).
mod table_data_connection_state;
// T041 independent audit (tester): parity re-derived from the React source,
// plus dependency-mirror completeness (keyset/grid/change enums, plan types).
mod table_data_connection_state_audit;
// T032: relationMetadata/metadataCache.ts port — standard wiring so the
// `metadata_cache` test filter really runs these tests (tests::metadata_cache::*).
mod metadata_cache;
// Fresh-tester independent re-audit of the D1/D2 fix (test-only probes).
mod metadata_cache_retest;

// Independent re-audit of merged T0-07 menu base.
mod menus_merge_reaudit;

// T0-10 merged-mainline overlay re-audit.
mod overlay_merge_reaudit;

// React parity: DefaultOrderSelect and GridKeyboardHints ports.
mod data_table_selects;
// T007: the DataTable selection bar port.
mod data_table_selection_bar;
// T007 independent tester audit (coverage-driven, key/focus/geometry probes).
mod data_table_selection_audit;
mod data_table_ports_audit;

// T021/T023: filter editor parts (between bounds, condition chip, connector).
mod filter_parts;

// T021/T023 audit: independent probes for the filter parts against React.
mod filter_parts_audit;

// T021/T023 fixes: chip width, truncation, accent X, tooltips and pressed state.
mod filter_fixes;

// T030: ExplainPlanTree port (recursive rows, expansion state, badges, bounds).
mod explain_plan_tree;
// T030 independent audit: i18n title, registry chevrons, S-owned state, detail
// key collisions, clipped+scrollable overflow, light-theme window fit.
mod explain_plan_audit;

// T063: DbTypeBadge port (meta table, parent map, sizing, mounted tile).
mod db_type_badge;
// T063: ErrorBoundary port (state machine, fallbacks, mounted fallback).
mod error_boundary;
// T063: ExportColumnPicker port (order, selection, toggles, bulk actions).
mod export_column_picker;
// T064: AdminSchemaFields port (commandSchema parse, visibility, labels, mounted list).
mod admin_schema_fields;
// T064: PrivilegeSelector port (x-datazen parse, flattening, grid, toggles).
mod privilege_selector;
// T064 audit: parse order, non-string type, parse rejection arms, mounted
// placeholder, group spacing, same-label key collision.
mod t064_audit;
// T063 audit: backup re-export path, light-theme fills, half-up rounding,
// capture-over-capture, PK-chip/type-label no-toggle.
mod t063_audit;

// T057: QueryArtifactExport port (dedup, visibility, labels, state machine,
// mounted bar with the in-flight guard and status lines).
mod query_artifact_export;
// T057 audit: guard no-ops, outcome marks React only sets, stale-press guard.
mod query_artifact_export_audit;

// T060: AiCodeBlock port (SQL detection, dialect, labels, state machine,
// mounted inline + fullscreen block).
mod ai_code_block;
// T060 audit: long-line overflow, detection edges, dialect survival,
// mounted stale-expiry guard, fullscreen-toggle text deviation pin.
mod ai_code_block_audit;

// T073: DataTransferAttachedJobs port (counter reading, active-job filter,
// cancel disposition + state machine, mounted strip).
mod data_transfer_attached_jobs;

// T073 audit: number-branch safe-integer gate (React has none on numbers).
mod data_transfer_attached_jobs_audit;

// T027: SchemaDiffPanel port and its independent audit.
mod schema_diff_panel;
mod schema_diff_audit;

mod connection_advanced_settings;
mod connection_advanced_audit;
// T016: the batch-export dialog port.
mod batch_export_dialog;
mod batch_export_audit;
// T016 fix retest: independent D1/D2 verification.
mod batch_export_retest;

// T068: MigrationJobFailureNotice (fail-closed refusal notice).
mod migration_failure_notice;

// T068 independent audit: i18n no-echo, key stability, two-theme geometry.
mod migration_failure_audit;

// T010: the controlled source/target endpoint bar port.
mod migration_endpoints_bar;
// T010 independent audit: label-vs-id display, close-on-choice, geometry.
mod migration_endpoints_audit;
// T010 independent retest of the D1/D2 fix: label resolution, selection, fallback, value on_choose.
mod migration_endpoints_retest;

// T023-S1: ColumnMappingRow leaf (row model, target options, skip/select/text commits, disabled).
mod column_mapping_row;
// T023-S1 independent audit: fresh vectors (target_options boundaries, patch/skip interaction, disabled, key scope, placeholder label).
mod column_mapping_row_audit;
// T023-S2: ColumnMappingEditor shell (target table + create-new toggle, auto-match/clear, warning, header, row assembly).
mod column_mapping_editor;
// T023-S2 independent audit: fresh vectors against React + transferMappingView.ts.
mod column_mapping_editor_audit;
// T023-S3: RecordsetEditor (enable switch, single-bound + limit, tuple editor).
mod recordset_editor;
// T023-S3 independent audit (tester).
mod recordset_editor_audit;

// T024: DataSyncCompareStep (comparing / compared / prompt branches, cancel,
// page-key/cursor derivations, empty child mounts for T288/T308/T383).
mod data_sync_compare_step;
// T024 independent audit: full branch matrix, NUL key, cursor semantics,
// empty child mounts, light theme.
mod data_sync_compare_step_audit;

// T074: DataSyncDialogs (error / delete-confirm / explain / execute-confirm
// dialogs, footer wiring, explain loading/result branches, open epochs).
mod data_sync_dialogs;
// T074 independent audit: empty explain text, topmost-dialog click
// capture when all four are open, non-error epoch restamp, header close
// label, light theme all-open, 576pt panel width.
mod data_sync_dialogs_audit;

// T026-S1: BackupWindow state model + selection logic (option tables,
// select/session journey, grouping + search ranking, prefill, close
// listener, empty ProgressLog mount).
mod backup_window_state;
// T026-S1 independent audit: full label table, dialect aliases,
// untouched-field journey, React grouping quirks, timestamp edges.
mod backup_window_state_audit;
// T026-S2: BackupWindow execution actions + three-column rendering
// (progress formatting, log pump, run-state machine, chrome,
// columns, options, backup/restore journeys, N4 host contract).
mod backup_window;
// T026-S2 independent audit: multi-byte log truncation, format-custom
// extension, restore None-payload, re-arm, badge tones, confirm
// dialog, log-copy visibility.
mod backup_window_audit;

// T076-S1: ProgressLog model — virtual-window math, auto-scroll
// decision, copy text and copy-feedback state (coder tests).
mod progress_log_s1;

// T076-S1 independent audit: TanStack cross-check (independent
// re-implementation + sweep) and copy-feedback ordering.
mod progress_log_s1_audit;

// T076-S2: ProgressLog component (header, copy button, virtualized rows).
mod progress_log_s2;
// T076-S2 independent audit: row pitch from rects, host-driven copied
// label, pinned vs unpinned windows.
mod progress_log_s2_audit;

// T008: ChartCanvas dispatcher, measured viewport and honest placeholders.
mod chart_canvas;
// T008 independent audit: marker honesty, factory seam, geometry, source guards.
mod chart_canvas_audit;

// T078: chart field inference and number/axis formatting.
mod chart_field_inference;
mod chart_format;
// T078 independent audit: type-name substrings, Number/Date.parse
// deviations, Intl fraction rounding, the 1e4 compact cutoff.
mod chart_field_inference_audit;
mod chart_format_audit;

// T009-S1: AiChatPanel leaf subcomponents (format/hasSnapshotContent/chip/QuestionBlock).
mod ai_chat_panel;
// T009-S1 independent audit: fresh vectors, dual-channel selection, geometry, source guards.
mod ai_chat_panel_audit;
// T009-S2: AiChatPanel ChatBubble (tool/reasoning/content/memory/remember/streaming/cancelled/questions).
mod ai_chat_panel_bubble;
// T009-S2 independent audit: fresh vectors, dual-channel geometry, settle_all reproduction.
mod ai_chat_panel_bubble_audit;
// T009-S3: AiChatPanel header (tabs, migration banners, agent-mode row + template select, profile row, share-editor row).
mod ai_chat_panel_header;
// T009-S3 independent audit: fresh vectors against the React source and zh-CN packs.
mod ai_chat_panel_header_audit;
// T009-S4: AiChatPanel message list (memory toasts, messages, streaming placeholder, jump-to-bottom).
mod ai_chat_panel_message_list;
// T009-S4 independent audit: fresh vectors against the React source and zh-CN packs.
mod ai_chat_panel_message_list_audit;
// T009-S5: AiChatPanel input area (egress notice, skills, auto-context / injection / KV chips, conflict bar, AiInput wiring).
mod ai_chat_panel_input_area;
// T009-S5: `AiChatInputAreaState` pure logic (send gate, submit, conflict, snapshot removal, show gates).
mod ai_chat_panel_input_area_state;
// T009-S5 independent audit: fresh vectors against the React source and zh-CN packs.
mod ai_chat_panel_input_area_audit;
// T009-S6: AiChatPanel shell + session wiring (state machine + S3/S4/S5 assembly).
mod ai_chat_panel_shell;
// T009-S6 independent audit: fresh vectors against the React source and zh-CN packs.
mod ai_chat_panel_shell_audit;
// T009-S7: AiChatPanel Section wiring (mount, toolbar toggle, host sync, geometry).
mod ai_chat_panel_section;
// T009-S7 independent audit: fresh vectors against the React source and zh-CN packs.
mod ai_chat_panel_section_audit;
// T012: AiWriteProposalHost (state machine + dialog / receipt / audit-error render).
mod ai_write_proposal_host;
// T012 independent audit: fresh vectors against the React source and zh-CN packs.
mod ai_write_proposal_host_audit;
// T013: AdminCreateDialog (state machine + dialog / name field / schema fields / error render).
mod admin_create_dialog;
// T013 independent audit: fresh vectors against the React source and zh-CN packs.
mod admin_create_dialog_audit;
// T014: ExportSuccessPanel (centred green check over the caller's message).
mod export_success_panel;
// T014 independent audit: fresh vectors for the export success panel.
mod export_success_panel_audit;
// T014: FilterBar (label, one pill per condition, clear-all; loading disables).
mod filter_bar;
// T014 independent audit: fresh vectors for the filter bar.
mod filter_bar_audit;
// T049: the `@datazen/frontend/app` barrel — crate-root launch surface,
// host-entry construction, first-frame adapter consult.
mod app_entry;
// T049 audit: the host half — a host-built adapter's list (and the
// unwired adapter's error) through the assign + sync the launch closure
// performs, which the coder's demo-path test does not exercise.
mod app_entry_audit;
