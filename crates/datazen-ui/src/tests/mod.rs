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
// TRIM mod tree;
// T0-09 audit: virtual staging, page height, edges and the selected a11y state.
// TRIM mod tree_audit;
// T0-11 patch: a compact tooltip's long hint wraps inside the window.
// TRIM mod tooltip_text;
// T0-04: chrome icons come from one 24-view lucide registry.
mod icon_registry;
// T0-11 audit: independent wrap and placement probes.
// TRIM mod tooltip_text_audit;
// T0-11 row math: external selection reveal and page arithmetic.
// TRIM mod tree_rowmath_audit;

// T004: the column-statistics dialog's four branches, dismissal and bounds.
// TRIM mod column_stats_audit;
// TRIM mod column_stats_dialog;
// T002: the single DataTable cell — formatting, colour, plan and geometry.
// TRIM mod cell_renderer;
// Independent T002 audit: tree-observed colour, branch order, raw handoff, flags and geometry.
// TRIM mod cell_renderer_audit;
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
// TRIM mod shell_grid;
// Query editing/results, undo/redo/copy/cancel, generation guards.
// TRIM mod query_results;
// ER diagram, foreign-key views, column drafts.
// TRIM mod structure_er;
// Export/import, backup, value search, favorites.
// TRIM mod io_tools;
// Sync wizard, schema compare, transfer.
// TRIM mod sync_transfer;
// Workspace tabs, host wiring, forms, settings page.
// TRIM mod workspace_tabs;
// Structure/bars/inbound jobs and caches.
// TRIM mod structure_jobs;
// Host job delivery.
// TRIM mod host_jobs;
// Toolbar/status sizing, light theme, row numbers, column width.
// TRIM mod chrome_metrics;
// Dialog base: mount rules, Escape close, focus return.
// TRIM mod dialog;
// A3 query workspace (editor/results/sidebar/tabs).
// TRIM mod query_workspace;
// Rail alignment and modal dialogs.
// TRIM mod rail_modals;
// Base kit primitives: select and the switch family.
// TRIM mod components;
// T0-09 table base: geometry, sort, selection, keyboard, window fit.
// TRIM mod table;
// TRIM mod data_table_parts;
// TRIM mod data_table_parts_audit;
// TRIM mod table_audit;
// Independent review coverage: select (T0-06) and checkbox/switch/radio (T0-09).
// TRIM mod choice_extra;
// Menu and context-menu base.
// TRIM mod menus;
// TRIM mod menus_audit;
// TRIM mod select_values;
// Toast / Tooltip / HoverCard overlay base (T0-10).
// TRIM mod overlay;
// Independent audit for T0-10 overlay base.
// TRIM mod overlay_audit;
// Second-round independent recheck for T0-10 fixes.
// TRIM mod overlay_retest;
// zh-CN table wiring: the keys the rune views took over still resolve.
// TRIM mod i18n_usage;
// TRIM mod i18n_sweep;
// Independent audit of the settings/rail i18n batch.
// TRIM mod i18n_usage_audit;
// Independent audit of the batch-3 view translations.
// TRIM mod i18n_batch3_audit;

// T0-11 error display bases.
// TRIM mod banner;
// TRIM mod banner_audit;
// TRIM mod copyable_timer_audit;
// TRIM mod copyable;

// T0-02 text field base and its audit.
#[cfg(test)]
// TRIM mod field_kit;
// TRIM mod field_kit_audit;

// Slider and PathInput base controls (T0-11).
// TRIM mod widgets;
// TRIM mod widgets_audit_t0_11;

// T0-11 viewer bases.
// TRIM mod binary_viewer;
// TRIM mod json_viewer;
// Independent audit for T0-11 viewer bases.
// TRIM mod viewer_audit;

// T0-12 base geometry regression: every base inside the window, siblings apart.
// TRIM mod base_geometry;

// T0-11 chrome bases: ToolbarButton, Tabs, Spinner, Badge, Label, ToolbarShell.
// TRIM mod chrome_parts;
// TRIM mod chrome_parts_audit;
// TRIM mod chrome_fix_audit;

// T0-11 temporal value input: typing, validation and commit keys.
// TRIM mod temporal_input;

// T0-11 audit: independent probes for the temporal editor contract.
// TRIM mod temporal_input_audit;

// T029: history components (entry card + filter bar).
// TRIM mod history_components;
// TRIM mod history_components_audit;
// T005: the global query-history dialog port.
// TRIM mod global_query_history_dialog;
// TRIM mod global_query_history_audit;

// T029 fix round: CopyButton base feedback + filter bar metrics.
// TRIM mod copy_button;
// T253-T256: the .ts support layer semantics.
// TRIM mod support_layer_audit;
// T034: React lib/session/counters.ts parity (counter + handle value logic).
// TRIM mod counters;
// T034 independent audit: 2^53 gate, branch order, regex-vs-BigInt faithfulness.
// TRIM mod counters_audit;
// T032: relationMetadata/metadataCache.ts port — standard wiring so the
// `metadata_cache` test filter really runs these tests (tests::metadata_cache::*).
// TRIM mod metadata_cache;
// Fresh-tester independent re-audit of the D1/D2 fix (test-only probes).
// TRIM mod metadata_cache_retest;

// Independent re-audit of merged T0-07 menu base.
// TRIM mod menus_merge_reaudit;

// T0-10 merged-mainline overlay re-audit.
// TRIM mod overlay_merge_reaudit;

// React parity: DefaultOrderSelect and GridKeyboardHints ports.
// TRIM mod data_table_selects;
// T007: the DataTable selection bar port.
// TRIM mod data_table_selection_bar;
// T007 independent tester audit (coverage-driven, key/focus/geometry probes).
// TRIM mod data_table_selection_audit;
// TRIM mod data_table_ports_audit;

// T021/T023: filter editor parts (between bounds, condition chip, connector).
// TRIM mod filter_parts;

// T021/T023 audit: independent probes for the filter parts against React.
// TRIM mod filter_parts_audit;

// T021/T023 fixes: chip width, truncation, accent X, tooltips and pressed state.
// TRIM mod filter_fixes;

// T030: ExplainPlanTree port (recursive rows, expansion state, badges, bounds).
// TRIM mod explain_plan_tree;
// T030 independent audit: i18n title, registry chevrons, S-owned state, detail
// key collisions, clipped+scrollable overflow, light-theme window fit.
// TRIM mod explain_plan_audit;

// T027: SchemaDiffPanel port and its independent audit.
// TRIM mod schema_diff_panel;
// TRIM mod schema_diff_audit;

// TRIM mod connection_advanced_settings;
// TRIM mod connection_advanced_audit;
// T016: the batch-export dialog port.
// TRIM mod batch_export_dialog;
// TRIM mod batch_export_audit;
// T016 fix retest: independent D1/D2 verification.
// TRIM mod batch_export_retest;

// T068: MigrationJobFailureNotice (fail-closed refusal notice).
// TRIM mod migration_failure_notice;

// T068 independent audit: i18n no-echo, key stability, two-theme geometry.
// TRIM mod migration_failure_audit;

// T010: the controlled source/target endpoint bar port.
// TRIM mod migration_endpoints_bar;
// T010 independent audit: label-vs-id display, close-on-choice, geometry.
// TRIM mod migration_endpoints_audit;
// T010 independent retest of the D1/D2 fix: label resolution, selection, fallback, value on_choose.
// TRIM mod migration_endpoints_retest;

// T008: ChartCanvas dispatcher, measured viewport and honest placeholders.
// TRIM mod chart_canvas;
// T008 independent audit: marker honesty, factory seam, geometry, source guards.
// TRIM mod chart_canvas_audit;
// T059: the chart toolbar + empty-state ports (type switch, display toggles,
// split/expand buttons and the export dropdown seam).
// TRIM mod chart_toolbar;
// T102: the ChartView container port (config derivation, row cap, split
// tiles, the expanded overlay and the lib/chart pipeline seam).
// TRIM mod chart_view;
// T102 independent audit (T102-audit): the row-cap literal and the uncapped
// recommend count, the early-return ladder's order (and the memo calls the
// empty arm still makes), the overlay's seams and paint (95% surface, bar
// geometry, literal captions, folded config, no click, full data), the
// split grid's geometry, the live-config slot props and the toolbar wiring.
// TRIM mod chart_view_audit;

// T019: the workflow editor action bar port.
// TRIM mod workflow_action_bar;
// T019: the workflow execution result panel port.
mod workflow_execution_panel;
// T070: the workflow visual editor form port (draft model, field/variable/
// step editors, command-input schema editor, both footer variants).
// TRIM mod workflow_form;
// T113: the workflow history section port (the sidebar list, the inline
// tab's rows and its detail branch over the result panel).
// TRIM mod workflow_history_section;
// T176: the workflow step-result utils port (the StepStatusIcon ladder, the
// column extraction and the step → statement-result conversion).
// TRIM mod workflow_step_result_utils;

// T051: the App.tsx root port — the window router the host's one window is
// handed through (first-frame composition, bounds, and the wiring pin).
// TRIM mod app_root;

// T020: the main window's composition root port (branches, error screen,
// retry, and the seams that must stay honest).
// TRIM mod main_page;

// T071 (segment-2's T070): the running-execution close host port (the
// prompt behind "close a tab whose query is still running": the ask
// payload, the confirm dialog it opens, and the answer routing).
// TRIM mod running_execution_close_host;

// T022: the install-wapp dialog port (two-step pick/review flow, the
// native pickers, the honest host seams and the close/reset path).
// TRIM mod install_wapp_dialog;

// T072: the wapps management page port (header/counts, the search/filter
// chips and derivations, the grouped all view, the cards and their gates,
// the toggle/uninstall/open seams, and the geometry at both sizes).
// TRIM mod wapp_management_page;

// T136: the log-scale tooltip port (the __orig__ formatter arms, the JS
// number coercions and the contentStyle chrome tokens).
// TRIM mod log_scale_tooltip;
