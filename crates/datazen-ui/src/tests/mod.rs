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
// T570: the navigator flat-row builder (sections, connections, dbs,
// schemas, categories, namespace trees, search and stable keys).
mod navigator_build_flat_rows;
// T572: the navigator row ARIA equivalence gate (13 variants, level rules).
mod navigator_tree_row_aria;
// T574: the connection reorder/regroup drag state machine.
mod navigator_connection_drag;
// T576: the per-session KV db-counts cache (shared/deduped, best-effort).
mod navigator_kv_db_counts;
// T578: the navigator's seven context menus (gates, order, action payloads).
mod navigator_context_menus;
// T580: the per-database caches and the refresh ladder (pointer-first
// toggles, background-neutral loads, the cache-refresh wave).
mod navigator_db_state;
// T582: the navigator utils (namespace leaf context, the row-key contract,
// depth padding, the multi-db decision, the ghost spec).
mod navigator_utils;
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
// T349: ER relation legend port (labels/counts, swatch strokes, toggle, geometry).
mod er_relation_legend;
// T349 independent audit: row dimming, count extremes, inert toggle, constants.
mod er_legend_audit;
// T361: ER table node port (columns/badges, collapse, handles, host callbacks, geometry).
mod er_table_node;
// T361 independent audit: rendered height vs packed metric, dot placement,
// select/drag ports and wide-node geometry.
mod er_node_audit;
// T372: ErDiagramView port (schema forwarding, exports, canvas keys, geometry).
mod er_diagram_view;
// T372 independent audit: collapse seeding, zoom clamps, pins and counts.
mod er_diagram_view_audit;
// T418: the standalone HomeHero + McpPromoBar ports (keys, counts, real copy
// wake deadline and two-window/two-theme geometry).
mod home_hero_promo;
// T442: the ImportDialog wizard port (mapping plan, keys, host actions, slot
// composition and two-window/two-theme geometry).
mod import_dialog;
// T460: GlobalObjectSearch + NavigatorDialogs ports (search journey, host
// actions, confirm children and two-window/two-theme geometry).
mod global_object_search;
// T475: the navigator status pip (colour/aria mapping, idle no-op, geometry).
mod tree_status_dot;
// T489: the panel tab strip (React keys, roving focus, host actions, 48pt header).
mod panel_tab_bar;
// T498: the process-list panel (table rows, kill/refresh host actions, states).
mod process_list_view;
// T506: the query editor section (toolbar, qb-context rebind, strategy, geometry).
mod query_editor_section;
// T512: the execution-strategy toolbar select (options, check marks, host intent).
mod execution_strategy_select;
// T518: the pure query-execution gate state machine (decisions & transitions).
mod query_execution_gate;
// T524: the result table view (status bar, cells, staging, find, geometry).
mod result_table_view;
// T526: ResultWorkspace port (view resolution, branches, toolbar, geometry).
mod result_workspace;
// T526 independent audit: inference branches, config independence, branch geometry.
mod result_workspace_audit;
// T530: date-value dialog + structure column table.
mod set_value_date_dialog;
mod structure_column_table;
// T538: TableColumnFilter port (toggle/badge, rows, actions, search, geometry).
mod table_column_filter;
// T538 independent audit: helper branches, disabled state, React a11y/outside-close.
mod table_column_filter_audit;
// T532: StructureIndexTable + StructurePlanPreview ports (rows/flags/actions, states, geometry).
mod structure_index_table;
// T550: TableStructureEditor port (titles/gates, toolbar actions, states, geometry).
mod table_structure_editor;
// T556: WorkspaceModeSidebar port (mode items/actions, update reminder, geometry).
mod workspace_mode_sidebar;
// T556 independent audit: progress phases, label precedence, percent rounding.
mod workspace_mode_sidebar_audit;
// T550 independent audit: validity edges, toolbar states, enabled/export gates.
mod table_structure_editor_audit;
mod structure_plan_preview;
// T532 independent audit: helper branches, capability gates, plan-preview states.
mod structure_index_audit;
// T536: the one-line table clipboard status (paste summary, problems, deadline).
mod table_clipboard_status;
// T630: the TableView clipboard adapter decisions (intent, formatting, mapping,
// gating and host-clock feedback state).
mod table_view_clipboard;
// T618: the KV workspace slot resolver (both gates, shared props, panel-state
// atom identity, landing home, overview jump and relay lifecycle).
mod use_kv_workspace_slots;
// T544: FK error banner, load-error state and no-primary-key tip ports.
mod table_fk_error_banner;
mod table_load_error_state;
mod table_no_primary_key_tip;
// T544 independent audit: message edges, copy controls, empty guard, tooltip.
mod table_banners_audit;
// T542: the FK jump breadcrumb (segment labels, host actions, empty/truncated).
mod table_fk_breadcrumb;
// T548: read-only tip + table status banners (copy, actions, geometry).
mod table_read_only_tip;
mod table_status_banners;
// T554: the TableView decision port (grid/editor/insert/dirty/filter/quick look).
mod table_view;
// T614: pure useDirtyIndex / dirtyIndex derivation logic (dirty rows, counts, anchors).
mod use_dirty_index;
// T620: pure useNamedViewMenu decisions (scope, apply path, raw/refusal, dirty).
mod use_named_view_menu;
// T620 independent audit: scope edges, raw/filter branches, equality fields.
mod use_named_view_menu_audit;
// T614 independent audit: expression/count branches, structural sharing, identity escaping.
mod use_dirty_index_audit;
// T626: pure useTableDraftRows / draftInserts logic (drafts, dirty, wire shape).
mod table_draft_rows;
// T626 independent audit: dirty/interaction edges, Escape, paste, stale ids.
mod table_draft_rows_audit;
// T524 independent audit: any-column not-contains, replace-all splicing and
// non-contains operator planning, checked against React resultFindReplace.
mod result_table_view_audit;
// T518 independent audit: finding copy on plain mutations, Ask→entire mode,
// and Postgres quote guard restricted to the targeted statement.
mod query_execution_gate_audit;
// T512 independent audit: shared-enum order/title, menu roles, visible-rect
// geometry of the open dropdown.
mod execution_strategy_select_audit;
// T506 independent audit: rebind gate, strategy keys, toast/hint surfaces,
// visible-rect geometry and a JSON-escaping probe for the qb context key.
mod query_editor_section_audit;
// T498 independent audit: cell styling, rendered→base pid mapping, kill guard
// and visible-rect geometry.
mod process_list_view_audit;
// T489 independent audit: context menu, label edges, active underline, locale
// tablist label and visible-rect geometry of the pinned strip.
mod panel_tab_bar_audit;
// T475 independent audit: help tooltip, full-round radius, exact parsing and
// visible-rect containment of every drawn pip.
mod tree_status_dot_audit;
// T460 independent audit: result cap, sort, match reason, empty-old rename and
// visible-rect geometry.
mod global_object_search_audit;
// T442 independent audit: running footer, Start action, sample, mapped pairs,
// Select write-back and visible-rect geometry across all phases.
mod import_dialog_audit;
// T418 independent audit: host command copy, re-time, keyed marker, badge cap,
// exact 2000ms wake, visible-rect geometry and exact copy locale keys.
mod home_hero_promo_audit;

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
// T292: lib/session/tableSession.ts port (key derivation + handle rules).
mod table_session;
// T292 independent audit: absent namespace fields and a controller with no
// snapshot.
mod table_session_audit;
// T311: lib/session/targetCommand.ts port (target derivation + admission).
mod target_command;
// T311 independent audit: recover-without-view, undefined error code, rows
// shape filtering and schema columns validation.
mod target_command_audit;
// T327: lib/settingsExport.ts port + targetCommandPhase.ts X-class ruling.
mod settings_export;
// T327 independent audit: empty export, full enum sets and defensive drops.
mod settings_export_audit;


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
// T318: tunnel contract DTOs, validation rules and the clipboard-fill hook port.
mod tunnel_contracts;
// T318 independent audit: clipboard guard branches, numeric forms and purity.
mod tunnel_contracts_audit;
// T217: HttpProxyTunnelFields + IndexConnectionFields ports.
mod tunnel_fields;
// T217 independent audit: placeholders/secure password, scheme back-switch and
// dismissal, parseInt edges, Enter clamp commit, i18n and visible-rect geometry.
mod tunnel_fields_audit;
// T059: the ConnectionAiNotes port (load/save, UTF-8 byte limit, timed feedback, geometry).
mod connection_ai_notes;
// T333: useConnectionForm ported as a state slice (defaults, snapshots,
// validation, test/save ports and the inline/saved tunnel state machine).
mod use_connection_form;
// T346: useConnectionImport hook ported as an immediate-mode slice + effects.
mod use_connection_import;
// T346 independent audit: stale guard variants, dismissed picker and VM flags.
mod use_connection_import_audit;
// T358: useTunnelFormState hook ported as an immediate-mode slice + effects.
mod use_tunnel_form_state;
// T358 independent audit: port validity, WebSocket/SSH kinds and reset defaults.
mod use_tunnel_form_state_audit;
// T333 independent audit: failed commands, driver validation, file/port rules,
// derived flags, edit-mode type changes and tunnel auth configs.
mod use_form_audit;
// T103: ConnectionFormBody + ConnectionImportFields ports (keys, validation,
// disabled states, exact i18n and two-window/two-theme geometry).
mod connection_form_parts;
// T103 independent audit: observable behaviour, structural parity and coverage gaps.
mod connection_form_audit;
// T300: WebSocketTunnelFields + AxisConfigurator ports (keys, mode validation,
// axis config branches, field mapping, config edits and two-window geometry).
mod websocket_axis;
// T300 independent audit: edit routing, accessor-free path, hiding rules,
// select edits, palette data and source guards.
mod ws_axis_audit;
// T281: SshTunnelFields + StandardConnectionFields ports and the form-body swap.
mod ssh_standard_fields;
// T281 independent audit: auth mapping, toggle rule, path pick, no-username,
// error transitions, source guards and long-value geometry.
mod ssh_fields_audit;
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

// T435: ShortcutFooter port (keymap presets/custom, display format, wrapping geometry).
mod shortcut_footer;
// T435 independent audit: value unions, F-key display, per-preset kbd, geometry.
mod shortcut_audit;
// T409: ConnectionCardList port (cards, sorting/filter, paging, host ports, geometry).
mod connection_card_list;
// T409 independent audit: engine chip, connecting button, keyboard, filter, geometry.
mod card_list_audit;
// T427: RecentQueriesList port (rows, sources, relative time, actions, geometry).
mod recent_queries_list;
// T427 independent audit: inert ports, relative-time boundaries and row role.
mod recent_queries_list_audit;
// T454: DatabaseValueSearchDialog port (keys, scope/type rules, answers, host
// actions, two-window/two-palette geometry).
mod database_value_search_dialog;
// T454 independent audit: Enter-to-start, rounded seconds, resume, picker, busy.
mod value_search_audit;
// T470: NavigatorTreeRow port (13 variants, ARIA equivalence, host actions, geometry).
mod navigator_tree_row;
// T470 independent audit: Space activation, KV click, context menus, chevron isolation, ARIA label, rect geometry.
mod navigator_tree_row_audit;
// T485: PanelContentRenderer port (slot routing, KV relay, states, actions, geometry).
mod panel_content_renderer;
// T495: PrivilegeView port (grant trees, modes, host actions, empty/loading, geometry).
mod privilege_view;
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
// T465: NavigatorToolbar port (React testids/i18n, host actions, optional
// buttons, controlled search input and two-size two-theme geometry).
mod navigator_toolbar;
// T465 independent audit: structural heights/keys and partial port wiring.
mod navigator_toolbar_audit;

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

// T382: ExecuteSqlFileDialog port (keys, validation, host ports, overlay geometry).
mod execute_sql_file_dialog;
// T382 independent audit: running close guard, empty banner, log, real slot.
mod exec_sql_audit;
// T223: ContentViewDialogs host (keys, open gates, callbacks, geometry).
mod content_view_dialogs;
// T223 independent audit: cancel/header routing, database fallback, slot
// integration geometry and the source-contract guard.
mod content_view_dialogs_audit;
// T391: ExportDialog port (option tables, scope correction, columns, states, slot).
mod export_dialog;
// T391 independent audit: export guards, entire-table fallback, visible-rect
// geometry and Select label/openability probes.
mod export_dialog_audit;
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
// T305: DatabaseObjectView port (object detail, host actions, slot, geometry).
mod database_object_view;
// T305 independent audit: load/message, DDL retry, execute guards, empty schema,
// null-schema action, visible-rect geometry and real connection-page composition.
mod database_object_view_audit;
// T337: DocumentConnectionView port (tabs, sidebar, result/editor, query, geometry).
mod document_connection_view;
// T337 independent audit: initial DB choice, query index clamp, show_editor,
// real slot composition, visible-rect geometry, palette underline and id probes.
mod document_connection_view_audit;
// T322: DDLView port (branch keys, copy/wake, state machine, slot, geometry).
mod ddl_view;
// T322 independent audit: exact 2000ms wake, copied deadline, visible-rect
// geometry and a whitespace-only DDL probe.
mod ddl_view_audit;
// T400: ForeignKeysView port (states, FK rows/chips, navigation, geometry).
mod foreign_keys_view;
// T400 independent audit: callback-free composition, header titles, dense rows.
mod foreign_keys_view_audit;
// T448: IndexesView port (keys, rows/flags, create/drop actions, states, geometry).
mod indexes_view;
// T448 independent audit: dialect method sets, name helpers and load fallback.
mod indexes_view_audit;
// T480: ObjectBrowser port (tabs, rows/identity, context menus, DDL flow, states, geometry).
mod object_browser;
// T492: PendingPlanDialog port (once-only counts, statement preview, close action, geometry).
mod pending_plan_dialog;
// T501: ExecutionStrategyAskModal port (strategy options, actions, geometry).
mod execution_strategy_ask_modal;
// T508: QuerySidebarSection port (panels, lists, scope/search, menus, geometry).
mod query_sidebar_section;
// T520: QueryPanel port (pane routing, execute/cancel, dialogs, sessions, geometry).
mod query_panel;
// T520 independent audit: uncovered branches, all-dialog geometry, React parity.
mod query_panel_audit;
// T514: RefreshCompletionButton port (label/tooltip, disabled states, action, geometry).
mod refresh_completion_button;
// T514 independent audit: VM guards, error answer, full-label text, independent rect.
mod refresh_completion_button_audit;
// T508 independent audit: unknown-db grouping, find_group and context-level edges.
mod query_sidebar_section_audit;
// T501 independent audit: preview boundaries and missing-current card.
mod execution_strategy_ask_modal_audit;
// T492 independent audit: no-insert description, statement order, empty fields.
mod pending_plan_dialog_audit;
// T480 independent audit: DDL/execute answers, running gate and menu teardown.
mod object_browser_audit;
// T504: QueryBuilderHostAdapter port (catalog, context/rebind, slot mount, host actions, geometry).
mod query_builder_host_adapter;
// T510: QueryTransactionModals port (dialog copy, modal actions, busy/blank gating, geometry).
mod query_transaction_modals;
// T516: SnippetMenuButton port (builtins, menu items, insert actions, disabled, geometry).
mod snippet_menu_button;
// T522: ExecutionSummaryCard + ResultFindBar ports (summary values, find actions, geometry).
mod execution_summary_card;
mod result_find_bar;
// T528: ServerStatusView port (cards/charts, tabs, header actions, states, geometry).
mod server_status_view;
// T534: StructureView port (column rows, badges, highlight, edit action, states, geometry).
mod structure_view;
// T568: nodeMetrics + relationStyle ports (size contract, stroke contract,
// legend drift guard, rendering-variant pinning).
mod er_node_metrics;
// T564: interactionState port (pins, hover neighbourhood/styling, label
// fitting) over the shared ER graph model.
mod er_interaction_state;
// T566: layoutErGraph port (layered ranks/order/coordinates, grid fallback,
// parallel-edge collapse, seeded fuzz, 500-table budget).
mod er_layout_er_graph;
// T562: buildErGraph port (nodes/edges from schemas, focus mode, inferred
// relations, collapse, handle routing, seeded collapse set).
mod er_build_er_graph;
// T540: TableFilterToolbar port (E2E keys, filter states, toggle/quick actions, geometry).
mod table_filter_toolbar;
// T546: TablePendingChangesBar + TableTransactionControls (counts, actions, busy, states, geometry).
mod table_pending_changes_bar;
// T552: TableToolbar port (E2E anchors, refresh/insert actions, gating, filter threading, geometry).
mod table_toolbar;
// T576: useKvDbCounts logic (row derivation, fetch dedup, refresh/invalidate, alive guard).
mod use_kv_db_counts;
// T628: useTableForeignKeyNavigation logic (FK index/navigability, jump, breadcrumb, history).
mod table_foreign_key_navigation;
// T616: useKvSlotActions logic (wired/unwired branches, warnings, dangerous gate).
mod use_kv_slot_actions;
// T622: usePanelHandlers logic (query binding, ER schema, table identity, close/menu/refresh).
mod use_panel_handlers;
