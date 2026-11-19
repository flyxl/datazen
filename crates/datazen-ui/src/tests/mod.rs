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
// T036: lib/structureEditor .ts helper parity (pure value logic).
mod structure_editor_helpers;
// T124: isControlEnabled / resolveCreateTableSchema / schemaToDraft parity.
mod structure_editor_create_schema;
// T124 independent audit: no-meta fallback/empty-string and branch-order gaps.
mod structure_editor_create_schema_audit;
// T036 independent audit: extra edge cases on the ported helpers.
mod structure_editor_audit;
// T085: draft IDs/defaults + DDL apply warnings (pure value logic).
mod structure_editor_defaults;
// T085 independent audit: extra truth-table / de-dup / boundary coverage.
mod structure_editor_t085_audit;
// T034: React lib/session/counters.ts parity (counter + handle value logic).
mod counters;
// T034 independent audit: 2^53 gate, branch order, regex-vs-BigInt faithfulness.
mod counters_audit;
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
// T183: lib/session/QueryPanelSession.ts port (registry + two-phase switch).
mod query_panel_session;
// T183 independent audit: transaction-state matrix, error mapping, conflict
// refresh fallback and result status strings.
mod query_panel_session_audit;
// T208: lib/session/queryPanelSessionRoot.ts port (root singleton + identity).
mod query_panel_session_root;
// T208 independent audit: ambient exports, independent namespace defaults,
// exact fail-loud messages.
mod query_panel_session_root_audit;
// T231: lib/session/runEditorExplain.ts port (effect flow + plan extraction).
mod run_editor_explain;
// T231 independent audit: missing-plan failure, DTO string views, envelope
// field filters and typed execute-error mapping.
mod run_editor_explain_audit;
// T252: lib/session/runEditorQuery.ts port (artifact events + terminal rules).
mod run_editor_query;
// T252 independent audit: disconnect exhaustion, statement chunk kinds and
// envelope limit gating.
mod run_editor_query_audit;
// T273: lib/session/sessionPrompts.ts port (prompt keys + typed errors).
mod session_prompts;
// T273 independent audit: literal React key strings, disconnect regex branches
// and unknown-code fallbacks.
mod session_prompts_audit;


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
// T217: HttpProxyTunnelFields + IndexConnectionFields ports.
mod tunnel_fields;
// T217 independent audit: placeholders/secure password, scheme back-switch and
// dismissal, parseInt edges, Enter clamp commit, i18n and visible-rect geometry.
mod tunnel_fields_audit;
// T059: the ConnectionAiNotes port (load/save, UTF-8 byte limit, timed feedback, geometry).
mod connection_ai_notes;
// T103: ConnectionFormBody + ConnectionImportFields ports (keys, validation,
// disabled states, exact i18n and two-window/two-theme geometry).
mod connection_form_parts;
// T103 independent audit: observable behaviour, structural parity and coverage gaps.
mod connection_form_audit;
// T239: the React NewConnectionDialog port (picker, form wiring, actions, geometry).
mod new_connection_dialog;
// T239 independent audit: availability filter, group wiring/labels, edit AI
// notes, host-port contract and group-heavy geometry.
mod new_dialog_audit;
// T260: ObjectFilterDialog + SaveTunnelDialog + shared ports.
mod object_filter_save_tunnel;
// T260 independent audit: Escape, reseed, busy, error actionability, no IPC,
// long-value geometry.
mod object_filter_audit;

// T193: FileConnectionFields port (path field, ADB flow, keys, geometry).
mod file_connection_fields;
// T193 independent audit: empty/no-match filter, load failures, cancelled pull,
// i18n key resolution and long-message visible-rect geometry.
mod file_connection_fields_audit;
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
// T164: the ai/targetClaim.ts contract port (claim, attribution, receipt reuse).
mod target_claim;
// T164 independent audit: Pick<grantId> semantics and verbatim string flow.
mod target_claim_audit;
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

// T192: the React NamedViewMenu port (six-state machine, flows, geometry).
mod named_view_menu;
// T192 independent audit: overwrite id, apply-close, default exclusivity,
// import refusal, keyboard commits and many-view geometry.
mod named_view_menu_audit;
// T216: the real filterValues / FilterColumn / filterLabels contracts.
mod filter_types_labels;
// T216 independent audit: boundary semantics, formatCell path, source guards.
mod filter_contracts_audit;
// T056: the expanded filter condition editor port.
mod filter_condition_editor;
// T056 independent audit: ported contracts, dispatch paths, i18n, token events.
mod filter_condition_editor_audit;
// T166: the React FilterTokenInput + FilterValueInput ports.
mod filter_token_value_input;
// T166 independent audit: commit edge cases, position-based removal, editor
// de-duplication and many-token geometry.
mod filter_token_value_audit;

// T106: MigrationJobVerdictPanel (verdict states, counters, copy feedback, geometry).
mod migration_job_verdict;
// T106 independent audit: i18n key resolution, boundary tone/gap polarity,
// resume gating and adversarial two-size two-theme rect geometry.
mod migration_job_verdict_audit;

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
// T167: ConnectionShareDialogHost (state, callback wiring, keys, geometry).
mod connection_share_host;
// T167 independent audit: independent i18n formatter pin, exact 1500ms wake +
// copied label, long-message visible-rect geometry, export failure fallback.
mod connection_share_host_audit;

// T173: ContentStatusBar + ContentToolbar controlled ports.
mod content_bar_ports;
// T173 independent audit: preserved inner ER testid, React detail/AI titles,
// KV slot branches and adversarial two-size two-theme geometry.
mod content_bar_audit;

// T199: ContentView composition (gates, drawers, split panes, geometry).
mod content_view;
// T199 independent audit: no-panel React gates, AI drawer independence,
// host pinned keys and the 800-line / no-bare-panic source contract.
mod content_view_audit;

// T223: ContentViewDialogs host (keys, open gates, callbacks, geometry).
mod content_view_dialogs;
// T223 independent audit: cancel/header routing, database fallback, slot
// integration geometry and the source-contract guard.
mod content_view_dialogs_audit;
// T266: CreateDatabaseDialog + CreateSchemaDialog ports.
mod create_db_schema_dialogs;
// T266 independent audit: blank-name guard, failure retry, schema spec words,
// running guard, Enter commit, visible-rect geometry and slot composition.
mod create_db_schema_dialogs_audit;
// T286: CreateUserDialog port (two steps, validation, host actions, slot mount).
mod create_user_dialog;
// T286 independent audit: select-all toggle, failed grant retry, success text,
// reset host facts, guard errors, no-grant-step branch, visible-rect geometry,
// database Select default/openability.
mod create_user_dialog_audit;
// T245: ContentViewDrawers port (drawer keys, split-pane source, auto-context,
// result-readonly verdicts, KV slot and geometry).
mod content_view_drawers;
// T245 independent audit: reason parsing, width bounds, auto-context fallback,
// object cells, table detail row, resolving banner and visible-rect geometry.
mod content_view_drawers_audit;

