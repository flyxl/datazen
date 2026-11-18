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
// T015: the AI context settings section — budgets, editor rejections, flows.
mod ai_context_settings_audit;
// T017: the WappIcon port (manifest icon or puzzle, the onError cycle).
mod wapp_icon_audit;
// T018: the add-to-dashboard dialog port (load gate, picker, create branch).
mod add_to_dashboard_dialog_audit;
mod column_stats_dialog;
// T002: the single DataTable cell — formatting, colour, plan and geometry.
mod cell_renderer;
// Independent T002 audit: tree-observed colour, branch order, raw handoff, flags and geometry.
mod cell_renderer_audit;
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
// T011: SqlEditor assembly component port (props/handle/events + geometry).
mod sql_editor_component;
// T005: the global query-history dialog port.
mod global_query_history_dialog;
mod global_query_history_audit;

// T029 fix round: CopyButton base feedback + filter bar metrics.
mod copy_button;
// T253-T256: the .ts support layer semantics.
mod support_layer_audit;
// T034: React lib/session/counters.ts parity (counter + handle value logic).
mod counters;
// T034 independent audit: 2^53 gate, branch order, regex-vs-BigInt faithfulness.
mod counters_audit;
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

// T008: ChartCanvas dispatcher, measured viewport and honest placeholders.
mod chart_canvas;
// T008 independent audit: marker honesty, factory seam, geometry, source guards.
mod chart_canvas_audit;
