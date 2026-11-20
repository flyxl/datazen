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
// T0-11 patch: a compact tooltip's long hint wraps inside the window.
mod tooltip_text;

// T004: the column-statistics dialog's four branches, dismissal and bounds.
mod column_stats_audit;
// T015: the AI context settings section — budgets, editor rejections, flows.
mod ai_context_settings_audit;
// T017: the WappIcon port (manifest icon or puzzle, the onError cycle).
mod wapp_icon_audit;
// T018: the add-to-dashboard dialog port (load gate, picker, create branch).
mod add_to_dashboard_dialog_audit;
// T021: the onboarding brand sidebar port (fixed-dark gradient, features).
mod brand_sidebar_audit;
// T025: the schema-diff deploy panel port (dialect gates, confirm journey).
mod schema_diff_deploy_panel_audit;
// T038: the useConfirmDialog port (request ids, resolve identity, preview).
mod confirm_dialog_hook_audit;
// T035: the sqlDialects support layer (profiles, strategies, exact SQL).
mod sql_dialects_audit;
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

// A3 query workspace (editor/results/sidebar/tabs).
mod query_workspace;
// Rail alignment and modal dialogs.
mod rail_modals;
mod select_values;

// T0-11 error display bases.
mod banner;
mod copyable;



// T0-11 viewer bases.
mod binary_viewer;
mod json_viewer;


mod chrome_fix_audit;




// T253-T256: the .ts support layer semantics.
mod support_layer_audit;
// T034: React lib/session/counters.ts parity (counter + handle value logic).
mod counters;
// T034 independent audit: 2^53 gate, branch order, regex-vs-BigInt faithfulness.
mod counters_audit;













// T155: presentation plans — override keys, routing, cache keys, rules.
mod presentation_plan;
// T085: the column presentation panel's pure logic (validate/read/preview).
mod column_presentation;
// T123: the session override store and its snapshot merge.
mod column_presentation_session;
// T037: table-column override persistence (validate/apply/save).
mod column_overrides;
// T182: settings parse + snapshot builder + bundled registration.
mod settings;
// T310: the valuePresentation save path helpers (renderer / rules paths).
mod value_presentation_save;
// T0-09: the tree base (geometry, keyboard, virtualisation, selection).
mod tree;
// T0-09 audit: virtual staging, page height, edges and the selected a11y state.
mod tree_audit;
// T0-04: chrome icons come from one 24-view lucide registry.
mod icon_registry;
// T0-11 audit: independent wrap and placement probes.
mod tooltip_text_audit;
// T0-11 row math: external selection reveal and page arithmetic.
mod tree_rowmath_audit;
// T004: the column-statistics dialog's four branches, dismissal and bounds.
// T015: the AI context settings section — budgets, editor rejections, flows.
// T017: the WappIcon port (manifest icon or puzzle, the onError cycle).
// T018: the add-to-dashboard dialog port (load gate, picker, create branch).
// T021: the onboarding brand sidebar port (fixed-dark gradient, features).
// T025: the schema-diff deploy panel port (dialect gates, confirm journey).
// T038: the useConfirmDialog port (request ids, resolve identity, preview).
// T035: the sqlDialects support layer (profiles, strategies, exact SQL).
// T002: the single DataTable cell — formatting, colour, plan and geometry.
// Independent T002 audit: tree-observed colour, branch order, raw handoff, flags and geometry.
// Shell, navigation, connections home, data grid, fonts and rendering.
mod shell_grid;
// T053: React shell.ts is a pure re-export barrel; Rune's root view covers its
// four shell-facing concerns (i18n, platform, UI store, surface colour).
mod shell_parity;
// T053 independent audit: vary Cx's window-control inset and watch the titlebar.
mod shell_parity_audit;
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
mod banner_audit;
mod copyable_timer_audit;
// T0-02 text field base and its audit.
#[cfg(test)]
mod field_kit;
mod field_kit_audit;
// Slider and PathInput base controls (T0-11).
mod widgets;
mod widgets_audit_t0_11;
// Independent audit for T0-11 viewer bases.
mod viewer_audit;
// T0-12 base geometry regression: every base inside the window, siblings apart.
mod base_geometry;
// T0-11 chrome bases: ToolbarButton, Tabs, Spinner, Badge, Label, ToolbarShell.
mod chrome_parts;
mod chrome_parts_audit;
// T0-11 temporal value input: typing, validation and commit keys.
mod temporal_input;
// T0-11 audit: independent probes for the temporal editor contract.
mod temporal_input_audit;
// T029: history components (entry card + filter bar).
mod history_components;
mod history_components_audit;
// T011: SqlEditor assembly component port (props/handle/events + geometry).
mod sql_editor_component;
// T005: the global query-history dialog port.
mod global_query_history_dialog;
mod global_query_history_audit;
// T029 fix round: CopyButton base feedback + filter bar metrics.
mod copy_button;
// T253-T256: the .ts support layer semantics.
// T036: lib/structureEditor .ts helper parity (pure value logic).
mod structure_editor_helpers;
// T124: isControlEnabled / resolveCreateTableSchema / schemaToDraft parity.
mod structure_editor_create_schema;
// T124 independent audit: no-meta fallback/empty-string and branch-order gaps.
mod structure_editor_create_schema_audit;
// T036 independent audit: extra edge cases on the ported helpers.
mod structure_editor_audit;
// T028: lib/chart/export.ts port — PNG/SVG export options, data-URL
// splitting/decoding, save requests and dialog options.
mod chart_export;
// T028 independent audit: fresh fixtures, boundary inputs, and the two
// places the port is stricter than atob / decodeURIComponent.
mod chart_export_audit;
// T085: draft IDs/defaults + DDL apply warnings (pure value logic).
mod structure_editor_defaults;
// T085 independent audit: extra truth-table / de-dup / boundary coverage.
mod structure_editor_t085_audit;
// T034: React lib/session/counters.ts parity (counter + handle value logic).
// T034 independent audit: 2^53 gate, branch order, regex-vs-BigInt faithfulness.
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
// T209: the per-panel editor-selection store (empty removes, no-op
// unchanged, clear drops, the nullish read).
mod editor_selection_store;
// T209 independent audit: whitespace selections, overwrite semantics,
// panel isolation, the full global round-trip grid.
mod editor_selection_store_audit;
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
// T087-S4: workflow chat, spinner, insights, MCP, injection/skill guards.
mod ai_store_rest;
// T087-S4 independent audit: 300ms spinner deadline, MCP plan order,
// skill identity by name when id is empty.
mod ai_store_rest_audit;
// T041: React stores/tableData/connectionState.ts parity (row records, cell
// values, error messages, the empty panel state, the load target).
mod table_data_connection_state;
// T041 independent audit (tester): parity re-derived from the React source,
// plus dependency-mirror completeness (keyset/grid/change enums, plan types).
mod table_data_connection_state_audit;
// T089: stores/tableData/countStrategy.ts — catalog prior + count mode.
mod table_data_count_strategy;
// T089 independent audit: equal-to-threshold stays exact, first schema wins.
mod table_data_count_strategy_audit;
// T032: relationMetadata/metadataCache.ts port — standard wiring so the
// `metadata_cache` test filter really runs these tests (tests::metadata_cache::*).
mod metadata_cache;
// Fresh-tester independent re-audit of the D1/D2 fix (test-only probes).
mod metadata_cache_retest;
// T081: relationPrediction/fromRelationMetadata.ts + relationMetadata contract gap fill.
mod relation_prediction_ports;

// T135: components/filter/FilterSqlPreviewDialog.tsx port.
mod filter_sql_preview;
// T135 independent audit: no-count copy text, kind labels, closed mount rule,
// falsy empty error, and long-SQL rect containment at both sizes/palettes.
mod filter_sql_preview_audit;
// T083: lib/session/EditorSessionController.ts port (state machine + effects).
mod editor_session_controller;
// T083 independent audit: refresh, detach/attach feeds, open-failure queue
// isolation, close-awaiting-open, cancel resolution and observer publishing.
mod editor_session_controller_audit;
// T122: lib/session/ExecutionProjection.ts port (state machine + effects).
mod execution_projection;
// T122 independent audit: terminal truth table, stream reset resubscribe,
// error projection, count-0/exclusive fan-out, source & binding guards.
mod execution_projection_audit;
// T122 independent re-audit (D1 retest): reconnect flow, permanent binding
// invalidation, null-handle guards and dispose (previously uncovered branches).
mod execution_projection_retest;
// T154: lib/session/pageUnload.ts port (host-owned fallback registry).
mod page_unload;
// T154 independent audit: multi-failure accumulation, re-registration and id
// monotonicity, per-release error reset, unknown-token unregister.
mod page_unload_audit;

// T091: platform/capabilities.ts — desktop and web capability tables.
mod platform_capabilities;
// T160: platform/env.ts — the compile-time form determination.
mod platform_env;
// T160 independent audit: the fold agrees with the capability tables,
// and the consumer branch shape.
mod platform_env_audit;
// T091 independent audit: web keeps behavior, drops logging, desktop ends
// at extensions, neither table uses the http version source.
mod platform_capabilities_audit;

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
// T059: the ConnectionAiNotes port (load/save, UTF-8 byte limit, timed feedback, geometry).
mod connection_ai_notes;
// T103: ConnectionFormBody + ConnectionImportFields ports (keys, validation,
// disabled states, exact i18n and two-window/two-theme geometry).
mod connection_form_parts;
// T103 independent audit: observable behaviour, structural parity and coverage gaps.
mod connection_form_audit;
// T016: the batch-export dialog port.
mod batch_export_dialog;
mod batch_export_audit;
// T016 fix retest: independent D1/D2 verification.
mod batch_export_retest;

// T144: the React ConnectionWorkspaceHome port (hero counts, card keys,
// filter, MCP copy feedback with a real wake, KV-slot takeover, geometry).
mod connection_workspace_home;
// T144 independent audit: React testid clicks, filter engine/id, pinned pill
// text, copy deadline retargeting and rect containment.
mod connection_home_audit;
// T110: the React windows/connection/ConnectionPage.tsx container port
// (composed shell, section reachability, tabs/toolbar, two-size two-theme fit).
mod connection_page;
// T110 independent audit: contract key families, root paste glue and a
// stronger drawn-right geometry probe.
mod connection_page_audit;
// T067: the React ConnectionNavigatorTree port (groups, rows, keys, keyboard,
// reveal and two-size two-theme geometry).
mod connection_navigator_tree;
// Independent T067 audit: isolated keyboard branches, active-descendant
// mounted-row contract, filter trim/case, key preservation and object geometry.
mod connection_navigator_tree_audit;

// T068: MigrationJobFailureNotice (fail-closed refusal notice).
mod migration_failure_notice;
// T140: MigrationRunHistoryDialog (history rows, states, rollback, reconcile).
mod migration_run_history;
// T140 independent audit: untested rollback outcomes, optional lines, callback
// guard and a rect-based on-screen/sized probe.
mod migration_run_history_audit;

// T068 independent audit: i18n no-echo, key stability, two-theme geometry.
mod migration_failure_audit;

// T049: the AI grant lifecycle state machine port and its branch coverage.
mod grant_state;
// T049 independent audit: receipt-id races, exact timing boundaries, observers.
mod grant_state_audit;
// T097: the AI session-cancel state machine port, its branches and its real 8s wake.
mod session_cancel;
// T097 independent audit: mark gate, literal 8000ms, settled-only listener,
// per-request deadlines, disposed no-ops, vanished entry, real 8s runtime wake.
mod session_cancel_audit;
// T133: the ai/streamRegistry.ts port (lifecycle, generations, cancel gating,
// subscriber events and the no-pruning invariant).
mod stream_registry;
// T133 independent audit: supersede events, terminal re-begin, precedence and
// multi-subscriber delivery order.
mod stream_registry_audit;
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

// T045: platform/web/index.ts is a pure re-export barrel; document Rune's
// existing cross-platform assembly + unwired-port equivalent (no shell to port).
mod platform_web;
// T045 independent audit: source-guard the barrel ruling + prove adapter injection.
mod platform_web_audit;
// T094: platform/web/stub.ts is web assembly code; pin the per-method parity
// mapping (browser APIs live in the web shell, Rune has one cross-platform app).
mod platform_web_stub;
// T094 independent audit: measure the mapping claim against the real React
// source, prove Rune has no browser ports, and guard the mapping table rows.
mod platform_web_stub_audit;

// T059: the chart toolbar + empty-state ports (type switch, display toggles,
// split/expand buttons and the export dropdown seam).
mod chart_toolbar;

// T019: the workflow editor action bar port.
mod workflow_action_bar;
// T019: the workflow execution result panel port.
mod workflow_execution_panel;
// T070: the workflow visual editor form port (draft model, field/variable/
// step editors, command-input schema editor, both footer variants).
mod workflow_form;
// T113: the workflow history section port (the sidebar list, the inline
// tab's rows and its detail branch over the result panel).
mod workflow_history_section;

// T051: the App.tsx root port — the window router the host's one window is
// handed through (first-frame composition, bounds, and the wiring pin).
mod app_root;

// T020: the main window's composition root port (branches, error screen,
// retry, and the seams that must stay honest).
mod main_page;

// T071 (segment-2's T070): the running-execution close host port (the
// prompt behind "close a tab whose query is still running": the ask
// payload, the confirm dialog it opens, and the answer routing).
mod running_execution_close_host;

// T022: the install-wapp dialog port (two-step pick/review flow, the
// native pickers, the honest host seams and the close/reset path).
mod install_wapp_dialog;
// T078: chart field inference and number/axis formatting.
mod chart_field_inference;
mod chart_format;
// T117: the post-query table-vs-chart decision.
mod chart_post_query_view;
// T117 independent audit: fresh vectors (float8/numeric/timestamptz
// columns, rowless corners, the four-way fallback grid).
mod chart_post_query_view_audit;
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
// T106: the FilterEditor shell (summary row, mode row, Any Column / Raw SQL
// panes, logic row, chip list; dirty/incomplete/badge/summary model).
mod filter_editor;
// T140: the LocaleDomainLoading skeleton (page/section variants, status
// role, hardcoded fallback, geometry).
mod locale_domain_loading;
// T140 independent audit: fresh vectors (a different testId, the narrow
// window, channels measured apart).
mod locale_domain_loading_audit;
// T106 independent audit: fresh vectors (uuid/unknown columns, no-value
// operators, verbatim predicates, one-channel badge, geometry).
mod filter_editor_audit;
// T014 independent audit: fresh vectors for the filter bar.
mod filter_bar_audit;
// T207: valuePresentation temporal math — parsers, DST refusal, LMT offsets.
mod temporal_math;
// T272: valuePresentation timestamp output formats (iso / iso-tz / locale).
mod timestamp_format;
// T230: valuePresentation timestamp options validation (pathed problems).
mod timestamp_config;
// T056: the expanded filter condition editor port.
mod filter_condition_editor;
// T056 independent audit: ported contracts, dispatch paths, i18n, token events.
mod filter_condition_editor_audit;

// T072: the wapps management page port (header/counts, the search/filter
// chips and derivations, the grouped all view, the cards and their gates,
// the toggle/uninstall/open seams, and the geometry at both sizes).
mod wapp_management_page;
// T049: the `@datazen/frontend/app` barrel — crate-root launch surface,
// host-entry construction, first-frame adapter consult.
mod app_entry;
// T049 audit: the host half — a host-built adapter's list (and the
// unwired adapter's error) through the assign + sync the launch closure
// performs, which the coder's demo-path test does not exercise.
mod app_entry_audit;
// T291: valuePresentation rules — restricted regex, validation, matching.
mod value_presentation_rules;
// T251: the datazen.timestamp contribution + contracts/registry slice.
mod timestamp_contribution;
// T046: the shared bootstrap chain (adapter + bundled registration + sync).
mod bootstrap;
// T047: the ADB driver-command wrappers (unified execute entry).
mod commands_adb;
// T106: MigrationJobVerdictPanel (verdict states, counters, copy feedback, geometry).
mod migration_job_verdict;
// T106 independent audit: i18n key resolution, boundary tone/gap polarity,
// resume gating and adversarial two-size two-theme rect geometry.
mod migration_job_verdict_audit;

// Independent segment-3 retests (tester-owned). Do not treat the *_audit
// modules as this evidence.
mod t076_tester;
mod t078_tester;
mod t087_tester;
mod t089_tester;
mod t091_tester;
// T051: the `@datazen/frontend` light entry — the barrel that dissolved
// into the crate split (api crate = light surface, crate root = launch
// surface): one import point, the separation invariant, capability bits
// as the linked assembly, the light consumer's exercise.
mod index_entry;
// T051 audit: the publicity tripwire (the mutation round proved pub vs
// pub(crate) invisible to in-crate tests), the no-second-import-point
// scan, the native host's two-surface manifest, and the contracts
// family's ported members through the api crate alone.
mod index_entry_audit;
// T062-S1: schema completion's pure core — dialect facade (quote/fold/
// must-quote), the three quoting policies, qualifier filtering with
// boosts, the all-columns dedup, SELECT-column extraction, and the
// relations-by-columns filter.
mod schema_completion;
// T137: ConnectionShareDialog (export validation, import titles/actions, geometry).
mod connection_share_dialog;
// T137 independent audit: full error table, stale epochs, footer disable,
// React-faithful field/error placement and adversarial geometry.
mod connection_share_audit;
// T156: structureEditor/types.ts contract verification and gap-fill.
mod structure_editor_types;
// T156 independent audit: full 20-field capability surface, nested UiConfig
// defaults, duplicate-definition and bare-panic source guards.
mod structure_editor_types_audit;
// T096: WebContextMenu — contract, position math, store, panels, presses.
mod web_context_menu;
// T157: the contextMenuStore singleton — normalized opens, hide, reopen.
mod context_menu_store;
// T157 independent audit: fresh vectors (empty submenus, disabled rows,
// predefined separators, cursor overwrite, idempotent hide).
mod context_menu_store_audit;
// T096 independent audit: fresh vectors, id'd separators, Copy command.
mod web_context_menu_audit;
// T100: QueryContextSelectors + QueryErrorPanel.
mod query_context_and_error;
// T100 independent audit: fresh vectors for both T100 components.
mod query_context_and_error_audit;
// T103: AiEgressNotice — the amber strip above the chat input.
mod ai_egress_notice;
// T103 independent audit: fresh vectors for the egress notice.
mod ai_egress_notice_audit;
// T185: the filter-draft helpers (filterUtils).
mod table_data_filter_utils;
// T210: grid-selection geometry and state transitions (DB-01).
mod table_data_grid_selection;
// T210 independent audit: fresh vectors (render order disagreeing with
// lexicographic everywhere, boundary walls, combined deltas, reconcile).
mod table_data_grid_selection_audit;
// T185 independent audit: fresh vectors for the filter-draft helpers.
mod table_data_filter_utils_audit;
// T293: the tab-closed notification (panelCloseNotifier).
mod panel_close_notifier;
// T293 independent audit: fresh vectors (multi-close diffs, re-add in the
// same change, stranger unsubscribes, per-panel independence).
mod panel_close_notifier_audit;
// T385: the query-run request bridge (immediate accept, ordered queue,
// flush-on-mount, unregister drops both).
mod query_run_request_store;
// T385 independent audit: fresh vectors (runner replacement, foreign
// mounts, the acceptance contract, duplicate requests).
mod query_run_request_store_audit;
// T421: the schema request tracker (session/slot barriers).
mod schema_request_tracker;
// T421 independent audit: fresh vectors (parallel sessions, the bind
// chain, cross-session slot names, reset scoping).
mod schema_request_tracker_audit;
// T301: the window chrome fallback (the chrome slot, the announcing
// spinner, center geometry).
mod window_chrome_fallback;
// T301 independent audit: fresh vectors (a 64pt chrome fixture, the
// narrow window, the no-bar centering, the ring/label channels).
mod window_chrome_fallback_audit;

// T113: the Data Transfer terminal-results strip (the restored verdict:
// uncompleted surfaces, verdict fold, load reducer, mounted strip).
mod data_transfer_terminal_results;

// T113 independent audit: fresh vectors (mixed kinds, the in-flight latest
// gate, the fixed uncompleted attribute set, stale identities, copy path).
mod data_transfer_terminal_results_audit;

// T176: the mapping gate notice (the message chain visible / gateLost /
// derived reason, the nothing-renders branches, the alert role, geometry).
mod mapping_gate_notice;

// T202: the source filter editor (the update chain, the empty state, the
// row mounts with the null-pair value rule, the logic switch threshold,
// the disabled inertness, geometry).
mod source_filter_editor;

// T225: the transfer execute-confirm dialog (the write-mode intro, the
// plan list, the footer buttons, the close/confirm callbacks, geometry).
mod data_transfer_execute_confirm_dialog;

// T202 independent audit: the Enter commit chain, the disabled inert
// remove, the middle-of-three removal, double add, the and default
// display, the short-drafts defence.
mod source_filter_editor_audit;

// T176 independent audit: whitespace vectors, the disabled create-new,
// the empty row list, receipt-over-reason order, banner/text nesting.
mod mapping_gate_notice_audit;

// T114: the Data Sync endpoint step (scope reconfirmation, profile
// controls: name input, profile select, save/load/delete gates).
mod data_sync_endpoint_step;

// T114 independent audit: fresh vectors (empty profile list, pre-selected
// id, pre-held name text, the dismissal close path, narrow-window notice).
mod data_sync_endpoint_step_audit;

// T148: the Data Sync job status panel (loose Number() semantics, verdict
// label branches, committed fallback chain, banner/list truthiness,
// button mounts + disabled logic + callbacks, window-width geometry).
mod data_sync_job_status_panel;

// T360: the Data Sync recordset editor (the pure update chain: toggle,
// bounds, order, tuple components, the limit validation; the mounted
// branches; geometry).
mod data_sync_recordset_editor;

// T348: the sync options bar (the operation checkboxes with the
// delete confirm-first split, the conflict-policy select and warnings,
// the hint, geometry).
mod data_sync_options_bar;

// T203: the Data Sync options step (the card chrome, the title block,
// the mounted OptionsBar, the callback pass-through, geometry).
mod data_sync_options_step;

// T203 independent audit: the title-above-bar order, every pass-through
// callback in one journey, the live skip warning inside the step.
mod data_sync_options_step_audit;

// T348 independent audit: the delete checkbox's both directions under a
// live model, an all-off mount, the warning beside the select, the
// unset-policy abort display.
mod data_sync_options_bar_audit;

// T360 independent audit: limit boundary integers, the toggle's editor
// unmount, the sibling-preserving all-empty deletion, the scalar→tuple
// switch on the first tuple edit, the limit error paragraph.
mod data_sync_recordset_editor_audit;

// T148 independent audit (tester): the partial banner matrix, the
// empty-string hydration error, `Number()` edge spellings, the NaN
// counter printing, the raw wire-state table, row part order by x
// geometry, five-row stacking, light theme.
mod data_sync_job_status_panel_audit;

// T125: the persisted-connections store (display helpers over the T403
// placeholder subset, load/refetch chains, group edits, per-connection moves).
mod connection_store;

// T125 independent audit: fresh vectors (the registry label branch, the
// rank ladder, duplicate ids, invalid timestamps, collapse renames).
mod connection_store_audit;

// T127: the dirty-state index (identity subset, anchor priority, the
// ambiguity guard, counts, off-page keys, structural sharing).
mod table_data_dirty_index;

// T127 independent audit: fresh vectors (composite keys, nested identity
// values, anchor under ambiguity, expressions, the delete-mark flip).
mod table_data_dirty_index_audit;

// T158: the new-row draft transitions + the enum-candidate cache
// (draft id minting, read helpers, every transition's effect and no-op
// branch, the plan-trio reset, the pending_changes invariant).
mod table_data_draft_inserts;

// T158 independent audit: base-36 boundaries, order authority under a
// manual order, duplicate-id appends, self-duplication, sibling order
// preservation, typed JSON writes, drafts surviving the reset, empty
// enum-candidate list semantics.
mod table_data_draft_inserts_audit;

// T129: the platform port contracts (updater wire types, window routing,
// the exporter port, the §7.4 binding registry).
mod platform_contracts;

// T129 independent audit: fresh vectors (a web capability table, an
// exporter offering the optional arms, the registry's reader isolation).
mod platform_contracts_audit;

// T132 independent audit: fresh vectors (the scope/format gates and
// fallbacks, the export-button gate, the mounted modal and its request).
mod data_export_dialog_audit;

// T134: the query execution status strip (coder tests).
mod query_execution_status;

// T134 independent audit: fresh vectors (the a11y surface, the winding-down run, the inert button tooltip, three counted presses, both windows).
mod query_execution_status_audit;

// T137: the AI input (coder tests: the @ trigger, the token pop, the pick
// rewrite, the placeholder ternary, the send/stop gate).
mod ai_input;

// T137 independent audit: fresh vectors (unicode @ queries, the mid-word
// caret pick, the Backspace key path, stop presses, the inert wire-less field).
mod ai_input_audit;

// T151 independent audit: fresh vectors (boundary rows/counts, cascade
// precedence, overlap cliques, pattern wraparound, JS coercion corners).
mod chart_recommend_audit;

// T147-S1: the Data Transfer window's wizard state model (coder tests).
mod data_transfer_window_state;

// T147-S1 independent audit: fresh vectors (driver-meta table, pairing
// matrix, fail-open options, sql-file round-trip, plan max count, gate
// normalization).
mod data_transfer_window_state_audit;

// T147-S2: the Data Transfer Job run machine (coder tests).
mod transfer_job_run;

// T147-S2 independent audit: fresh vectors (classification order, the
// replayed carve-out, downgrade spellings, stale-id refusal, cancel
// address holes, unbacked-row verdicts).
mod transfer_job_run_audit;

// T147-S3: the Data Transfer window's rendering (coder tests).
mod data_transfer_window;

// T147-S3 independent audit: fresh tree-level vectors (experimental pair
// note, rail/body geometry, destructive arm, DDL key suffixes, sql-file
// mode swap, execute gate, 40-row fit).
mod data_transfer_window_audit;
