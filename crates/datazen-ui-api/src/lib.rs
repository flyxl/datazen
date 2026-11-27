//! Ports the Rune UI is allowed to call.
//!
//! Hosts provide the [`DesktopAdapter`] implementation. The demo uses
//! [`MemoryWorkspace`] for its in-memory data.

mod ai_memory_correction;
mod app_root_surface;
mod bootstrap_icon_resolver;
mod build_connection_url;
mod build_editor_schema;
mod cell_editor_candidates;
mod cell_editors;
mod chart_colors;
mod chart_empty_state;
mod chart_toolbar;
mod connection_editor;
mod connection_share;
mod cross_window_bus;
mod data_fetcher;
mod default_sort;
mod demo_adapter;
mod docs_urls;
mod document_scroll_lock;
mod ensure_namespace;
mod er_node_context_menu;
mod export_dialog_shared;
mod extract_sql;
mod filter_expression;
mod fk_navigation_actions;
mod grid_keyboard;
mod html_sanitizer;
mod import_mapping;
mod keymap;
mod language_pack_runtime;
mod mcp_agent_config;
mod migration_verification;
mod migration_window_prefill;
mod path_picker;
mod query_context_path;
mod result_find_replace;
mod row_count_strategy;
mod schema_diff_confirm;
mod schema_resource_cache;
// T228: the memory-settings normalizers and the message-block parser —
// pure functions over the persistence JSON and chat markdown.
mod ai_memory_settings;
// T228: the fenced-block splitter and the SQL probe.
mod ai_memory_patterns;
mod ai_message_blocks;
// T251: the provider preset table, its PRD order and the backend normalizer.
mod ai_providers;
// T291: the three safety-gate presets and their identity/locality predicates.
mod ai_safety_presets;
// T291: the React `lib/aiSkills.ts` surface — its two normalizers already
// live in `ai_types` (T228/T230); this module re-exports them and pins the
// file's contract without forking a second implementation.
mod ai_skills;
// T291: the Any Column filter expansion (DBR-04) and its narrow cell-type
// projection (`cellTypes.ts`/`valueSearch.ts` are not ported into this crate).
mod any_column_filter;
// T272: the query-error AI action adapters — pure descriptors over raw JSON.
mod ai_query_actions;
mod ai_sessions;
mod ai_store_types;
mod ai_streaming;
mod ai_types;
mod backup_progress;
mod base64;
mod batch_export_job;
mod chart_types;
mod command_schema;
mod compare_report;
mod connection_clipboard;
mod connection_config;
mod connection_form_model;
mod connection_groups;
mod connection_locator;
mod connection_share_error;
mod connection_tab_context_menu;
mod connection_types;
mod connection_view_registry;
mod connection_views;
mod context_items;
mod context_menu_position;
mod dangerous_sql;
mod data_sync_bars;
mod data_sync_comparison;
mod data_sync_endpoints;
mod data_sync_execution;
mod data_sync_explain;
mod data_sync_job_lifecycle;
mod data_sync_options;
mod data_sync_profiles;
mod data_sync_refs;
mod data_sync_run_state;
mod data_sync_selection;
mod data_sync_selection_state;
mod data_sync_wizard_navigation;
mod data_table_bits;
mod data_table_clipboard_dom;
mod data_table_context_menu;
mod diff_detail;
mod grid_cell;
mod quick_look_dialog;
mod tunnel_store;
mod workflow_chat_panel;
// `pub mod` — the crate's only public module: the T159 dashboard facade
// keeps its display `DashboardWidget` here, but that root name is held
// by the N13 workspace stub below (whose `datazen-ui` consumers are out
// of that task's reach), and the module-local `ExecutionTarget` wire
// mirror's root name belongs to `metadata`'s serde-less family — so those
// names reach the outside world through the module path while everything
// else re-exports at the root (see `dashboard_types`' docs).
mod column_layout_prefs;
pub mod dashboard_types;
mod database_object_identity;
mod database_types;
mod dedicated_db_session;
mod dismiss_prefs;
mod driver_capabilities;
mod driver_command_ids;
mod endpoint_identity;
mod error_message;
mod export_capability;
mod export_data;
mod export_format;
mod export_table_structure;
mod extract_questions;
mod fetch_relation_ddl;
mod filter_db_types;
mod filter_operators;
mod filter_types;
mod fk_navigation;
mod fk_referenced_by;
mod fk_selector;
mod global_text_selection;
mod grid_clipboard;
mod grid_errors;
mod grid_event_guards;
mod grid_paste_plan;
mod history_groups;
mod host_error_message;
mod host_lucide_map;
mod icon_ids;
mod icon_resolver_api;
mod import_connection_file;
mod import_data;
mod import_execution;
mod keyset_paging;
mod keyset_types;
mod kv_ai_context;
mod kv_slot_state;
mod kv_workspace;
mod load_batch_export_table;
mod locale_sync;
mod main_window_context_menu;
mod metadata;
mod migration_job_hydration;
mod migration_job_verdict;
mod named_views;
mod native_context_menu;
pub mod native_csv_import;
pub mod native_kv;
pub mod native_transfer;
mod object_browser_context_menu;
mod object_filter;
mod panel_context_path;
mod panel_exec_cleanup;
mod panel_query_context;
mod panel_result_edit;
mod panel_store;
mod panel_types;
mod platform_shared;
mod process_list_result;
mod product_features;
mod query_exec_actions;
mod query_execution_view_model;
mod query_sidebar_context_menu;
mod query_stream;
mod relation_identity;
mod relation_prediction;
mod relative_time;
mod resolve_ui_language;
mod result_editability;
mod row_to_record;
mod run_predefined_edit;
mod run_to_chart;
mod run_to_result;
mod running_execution_close;
mod scalar_cell_editors;
mod schema_cache;
mod schema_changing_sql;
mod schema_column_loading;
mod schema_diff_limitations_prefs;
mod schema_store;
mod schema_store_selectors;
mod schema_tree_context_menu;
mod selection_actions;
mod selection_scope;
mod settings_store;
mod splash;
mod sql_bind_params;
mod sql_completion_context;
mod sql_completions;
mod sql_file_execution;
mod sql_format;
mod sql_function_registry;
mod sql_generator;
mod sql_namespace_ops;
mod sql_path_prefix;
mod sql_schema_drop_targets;
mod sql_statement_range;
mod sql_transaction_guard;
mod stage_cell_value;
mod stage_edit_actions;
mod stage_paste_writes;
mod startup_timer;
mod surface_bg_cache;
mod sync_pairing;
mod sync_taxonomy;
mod table_data_state;
mod table_data_store;
mod table_list_panel;
mod table_schema_for_sql;
mod table_sort_preferences;
mod table_sql_actions;
mod tabs;
mod theme_editor_colors;
mod theme_tokens;
mod total_rows_display;
mod transfer_limitations_prefs;
mod transfer_pairing;
mod tunnel_deletion;
mod tunnel_draft;
mod update_reminder_schedule;
mod update_reminder_store;
mod updater;
mod value_search;
mod virtual_body;
mod wapp_asset_origin;
mod wapp_bridge;
mod wapp_i18n;
mod wapp_store;
mod window_kind;
mod window_manager;
mod workflow_draft_yaml;
mod workflow_list_context_menu;
mod workflow_panel;
mod workflow_yaml;
// `pub mod` — the second public module: the T211 types aggregate (the
// `types/index.ts` port) is the family's ONE import surface, so its module
// path `datazen_ui_api::types::…` is the Rust twin of React's `'../types'`
// import path. Its own re-export surface mirrors the React file's export
// list exactly; the new vocabulary also rides the crate root through the
// `pub use types::{…}` block below, the family convention (see the module's
// docs for the full ownership map and the surface law).
mod ai_draft_bridge;
mod connection_page_utils;
mod connection_shared;
mod content_view_helpers;
mod expanded_db_cache_refresh;
mod open_history_query;
mod query_contracts;
mod query_drop_handler;
mod query_error_chat_prompt;
mod query_execution_risk;
mod query_execution_target;
mod query_toolbar_width;
mod result_workspace;
mod result_workspace_index;
pub mod schema_object_search;
mod schema_tree;
pub mod server_status_trends;
pub mod sql_editor_defaults;
pub mod table_changes;
pub mod theme_pack_apply;
pub mod types;
mod use_query_builder_contribution;
mod validate_bind_params;
mod workspace_mode_sidebar;

use std::collections::HashMap;

pub use demo_adapter::DemoAdapter;

pub use ai_memory_correction::{
    mark_stale_for_schema_drops, submit_memory_correction_if_needed, AiMemoryCommands,
    MemoryCorrectionSubmission, SchemaStaleSubmission,
};
pub use ai_memory_patterns::{
    extract_memory_patterns_if_needed, extract_once, HistorySort, MemoryPatternCommands,
    MemoryPatternExtractionInput, MemoryPatternScheduler, QueryHistoryPage, QueryHistoryPageParams,
    SubmitDecision, MEMORY_PATTERN_MIN_OCCURRENCES, MEMORY_PATTERN_WINDOW_DAYS,
};
pub use app_root_surface::{
    app_module_mark, resolve_window_component, should_hide_splash, AppWindowRoute,
    APP_CHUNK_LOAD_MARKS, APP_DEFAULT_COMPONENT, APP_DEFAULT_WINDOW_KIND, APP_DOM_GAP,
    APP_GLOBAL_HOSTS, APP_MOUNT_MARK, APP_SELECTION_INSTALLERS, APP_SHELL_WRAPPERS,
    APP_WINDOW_ROUTES,
};
pub use bootstrap_icon_resolver::{
    default_icon_resolver_seed, resolve_icon, DbIcon, IconResolverSeed, ResolvedIcon,
};
pub use build_connection_url::{build_connection_url, ConnectionMode, ConnectionUrlDriverMeta};
pub use build_editor_schema::{
    build_editor_schema, flat_from_tables, hoist_namespace_child, hoist_namespace_path,
    hoist_nested_table_leaves, is_leaf, is_namespace_empty, merge_unqualified_tables,
    overlay_column_map, BuildEditorSchemaInput,
};
pub use cell_editor_candidates::{
    build_distinct_candidates_sql, build_row_limited_select, normalized_limit, CandidateDialect,
    DistinctCandidatesArgs, IdentifierQuote, RowLimitPlacement, RowLimitShape,
    CANDIDATE_VALUE_LIMIT, FETCH_FIRST_ROW_LIMIT, LIMIT_ROW_LIMIT, TOP_ROW_LIMIT,
};
pub use cell_editors::{
    build_enum_view, build_json_view, build_long_text_view, enum_key_decision, json_indent_of,
    reformat_json, text_area, EnumEditorInput, EnumEditorMode, JsonAction, JsonEditorInput,
    JsonEditorView, KeyDecision, LoadButtonView, LongTextEditorInput, SelectOption, TextAreaView,
    CELL_EDITORS_GAP, CELL_EDITORS_SURFACE,
};
pub use chart_colors::{
    built_in_palette, get_color_palette, read_css_color, COLOR_PALETTES, STROKE_DASH_PATTERNS,
};
pub use chart_empty_state::{
    build_chart_empty_state, ChartEmptyReason, ChartEmptyStateView, EMPTY_CONTAINER_CLASS,
    EMPTY_ICON_CLASS, EMPTY_KEY_NO_CONFIG, EMPTY_KEY_NO_DATA, EMPTY_KEY_NO_NUMERIC_FIELD,
    EMPTY_TEXT_CLASS,
};
pub use chart_toolbar::{
    chart_type_class, chart_type_label_key, chart_type_test_id, reduce_toolbar,
    resolve_toolbar_labels, split_available, split_class, visible_controls, ChartExportFormat,
    ChartToolbarState, ToolbarContext, ToolbarControl, ToolbarEffect, ToolbarLabels,
    ToolbarOutcome, ACTIVE_CLASS, CHART_TYPES, INACTIVE_CLASS, KEY_EXPAND, KEY_EXPORT, KEY_GRID,
    KEY_LEGEND, KEY_SPLIT_VIEW, KEY_VALUES, TOOLBAR_SHELL_CLASS,
};
pub use connection_editor::{
    close_new_connection_dialog, open_new_connection_dialog, ConnectionEditorState,
};
pub use connection_share::{
    close_connection_share_dialog, open_connection_share_dialog, ConnectionImportSource,
    ConnectionShareMode, ConnectionShareState,
};
pub use cross_window_bus::{
    cross_window_rail, make_message, matches_event, payload_of, CrossWindowMessage,
    CrossWindowRail, CHANNEL_NAME,
};
pub use data_fetcher::{
    resolve_fetcher, DataFetcherKind, ExportContext, ExportProgress, ExportScope, FetchColumn,
    FetchOutcome, FetchRequest, FetchResult, FetchTableInput, FetchTableSource, ProvidedFormatPlan,
};
pub use docs_urls::{
    build_docs_url, is_chinese_ui_language, DocsSectionId, DOCS_BASE_EN, DOCS_BASE_ZH,
};
pub use document_scroll_lock::{
    apply_snap, scroll_lock_rail, snap_targets, ScrollAxis, ScrollLockRail, ScrollLockState,
    ScrollPosition,
};
pub use ensure_namespace::{
    build_slash_fetch_path, inflight_key, is_path_nav, namespace_ensure_pending,
    namespace_has_child, namespace_path_loaded, path_key, path_nav_segment, plan_default_sql,
    plan_path_hierarchy, plan_postgresql, resolve_ensure_strategy, resolve_for_ensure,
    resolve_path_root, run_ensure_plan, table_names, uses_current_database_root, CatalogResult,
    EnsureDeps, EnsureStrategy, MergeCall, MergeKind,
};
pub use er_node_context_menu::{
    build_er_node_context_menu_items, ErMenuAction, ErMenuItem, ErNodeContextMenuHandlers,
    ErNodeContextMenuLabels,
};
pub use export_dialog_shared::{
    build_export_format_options, build_export_scope_options, BuildExportFormatOptions,
    BuildExportScopeOptions, DisabledReasonKey, EntireTableAvailability, ExportFormatOption,
    ExportScopeOption, FormatOptionSpec, ScopeLabels, ARRAY_EXPORT_FORMAT_OPTIONS,
    OBJECT_EXPORT_FORMAT_OPTIONS, SHARED_EXPORT_FORMAT_OPTIONS,
};
pub use extract_sql::{
    extract_sql_from_response, extract_sql_streaming, looks_like_sql, starts_sql_statement,
};
pub use filter_expression::{
    filter_expression_to_conditions, flatten_parts, flatten_to_connectors, has_or_group_under_and,
    parse_filter_expression, parse_filter_for_apply, token_positions, FilterExpression,
    FilterExpressionLogic, FilterExpressionOperator, FilterExpressionValue,
    FlattenedFilterCondition, ParseFailure, ParsedFilterApplication,
};
pub use fk_navigation_actions::{
    host_error_message, open_foreign_key_target, FkActionDeps, FkActionEffects, FkJumpError,
    FkJumpOutcome, FkNavEntry, FkNavOrigin, FkNavState, OpenForeignKeyTargetInput, PanelKind,
    TableChangeContext, TableIdentity,
};
pub use grid_keyboard::{
    cell_coord_equals, clamp_cell_coord, clamp_grid_cursor, coord_at, grid_cursor_equals,
    is_ime_composing_key, move_cell_focus, read_grid_key_intent, resolve_grid_key_command,
    resolve_keyboard_cursor, resolve_page_delta, selection_from_anchor_focus,
    to_grid_key_event_like, GridEdge, GridKeyCommand, GridKeyEventLike, GridKeyIntent,
    GridKeyboardContext, GridNativeKeyEventLike, GridPageDirection, EMPTY_ROW_MODE_SELECTION,
};
pub use html_sanitizer::{
    decode_entities, get_attr, is_event_attr, is_javascript_href, is_strip_tag, is_void_tag,
    parse_html, regex_sanitize, sanitize_html, serialize_html, set_attr, HtmlNode, STRIP_TAGS,
    VOID_TAGS,
};
pub use import_mapping::{
    mapping_is_executable, plan_import_mapping, ImportMappingConflict, ImportMappingEntry,
    ImportMappingMode, ImportMappingPlan, ImportMatchKind, PlanImportMappingInput,
};
pub use keymap::{
    format_mod_key, format_shortcut_for_display, get_action_shortcut, normalize_shortcut_input,
    to_code_mirror_key_format, to_shortcut_hook_format, KeymapAction, KeymapPreset, KEYMAP_ACTIONS,
    KEYMAP_PRESETS,
};
pub use language_pack_runtime::{
    ensure_language_pack_messages, is_builtin_locale, reset_language_pack_runtime_for_tests,
    unload_language_pack, EnsureOutcome, LanguagePackDeps, LanguagePackMessages,
    LanguagePackRuntime, BUILTIN_LOCALES,
};
pub use mcp_agent_config::{
    build_mcp_agent_snippet, build_mcp_agent_snippet_default, build_mcp_server_entry,
    format_mcp_cli_command, initial_app_executable_path, AppExecutablePathResolver,
    McpAgentSnippet, McpAgentTarget, McpServerEntry,
};
pub use migration_window_prefill::{
    apply_migration_endpoint_prefill, build_migration_window_url_params,
    read_migration_prefill_from_url, EndpointPrefillEffect, MigrationEndpointConnection,
    MigrationWindowKind, MigrationWindowPrefill, PREFILL_KEYS,
};
pub use path_picker::{
    pick_path, web_path_picker, PathPicker, PathPickerFileFilter, PickPathInput,
};
pub use query_context_path::{
    auto_complete_path_hierarchy_path, build_path_hierarchy_database_pin,
    build_path_hierarchy_selector_segments, infer_sql_relation_path, namespace_roots_from,
    path_hierarchy_connection_root, path_hierarchy_relative_namespace_path,
    path_hierarchy_selector_segments_for_ui, paths_equal, resolve_query_context_path,
    split_path_hierarchy_database_pin, unquote_ident, NamespaceNode, PathHierarchySelectorSegment,
    QueryContextResolveOpts, SplitDatabasePin, PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS,
};
pub use result_find_replace::{
    cell_matches, cell_text, filter_result_rows, next_plan_item, plan_result_replacements,
    replace_text, row_matches, ResultFindOperator, ResultFindQuery, ResultReplacePlanItem,
    EMPTY_RESULT_FIND_QUERY,
};
pub use row_count_strategy::{resolve_count_strategy, CountStrategyInput};
pub use schema_diff_confirm::{
    can_confirm_destructive_deploy, can_run_deploy, CanRunDeployOptions, DESTRUCTIVE_CONFIRM_TOKEN,
};
pub use schema_resource_cache::{LoadStart, SchemaResourceCache, ERROR_TTL_MS, VALUE_TTL_MS};
// T228's two lib halves ride the crate root the family way.
pub use ai_memory_settings::{
    memory_mode_for, normalize_ai_settings_config, normalize_memory_settings,
};
pub use ai_message_blocks::{
    is_sql_code_block, parse_message_segments, CodeBlockSegment, MessageSegment, TextSegment,
};
// T251: the React lib/aiProviders.ts surface — the lookup, the two ordered
// lists, the const aliases and the three pure helpers.
pub use ai_providers::{
    is_known_provider_type, is_local_cli_provider, normalize_ai_providers, provider_preset,
    DEEP_SEEK_PROVIDER, LOCAL_CLI_PROVIDER_TYPES, OLLAMA_PROVIDER, PROVIDER_ORDER,
};
// T291: the React `lib/aiSafetyPresets.ts` surface — the three named gates,
// the default/detection helpers and the local-endpoint predicate.
pub use ai_safety_presets::{
    detect_safety_preset, get_default_safety_gate, is_local_endpoint, preset_from_str,
    safety_preset, AiSafetyPresetType, CLOUD_STRICT_PRESET, ENTERPRISE_BALANCED_PRESET,
    LOCAL_TRUST_PRESET,
};
// T291: the React `lib/aiSkills.ts` surface. `normalize_loaded_skills` is
// already rooted through `ai_sessions` (T230), so only the settings
// normalizer is added here — no duplicate root name.
pub use ai_skills::normalize_ai_skills_settings;
// T291: the React `lib/anyColumnFilter.ts` surface — the expansion, its
// options/result and the type-resolution vocabulary it speaks.
pub use any_column_filter::{
    expand_any_column_filter, resolve_cell_type, AnyColumnExpansion, AnyColumnFilterOptions,
    CellType, CellTypeHints, CellTypeMatch, CellTypeRule, ValueSearchTypeGroup,
    ValueSearchTypeScope, DEFAULT_CELL_TYPE_RULES,
};
// T272: the React `lib/aiQueryActions.ts` surface — the guarded context
// builder, the three action descriptors and their param helpers.
pub use ai_query_actions::{
    bound_params_from_json, build_explain_action, build_fix_sql_action,
    build_query_diagnosis_context, build_retry_action, equal_bound_params, BoundParamValue,
    BoundParams, ExplainActionDescriptor, FixSqlActionDescriptor, PromptConnectionContext,
    QueryActionBuildFailure, QueryActionBuildResult, QueryActionBuildSuccess, QueryActionError,
    QueryActionErrorCode, QueryDiagnosisContext, QueryDiagnosisContextLike, QueryDiagnosisParams,
    QueryPromptContext, RetryActionDescriptor, RetryRequest, RetryValidation,
    RetryValidationFailure, RetryValidationInput, RetryValidationSuccess,
    SanitizedConnectionContext, SanitizedJson, SqlDiff, SqlEditorDraft,
};
pub use ai_sessions::{
    chat_session_from_detail, chat_session_from_stored, chat_session_to_upsert_input,
    compute_session_key, export_filename, iso_from_epoch_ms, local_detail, markdown_for,
    message_from_stored, message_metadata, message_status, new_chat_session,
    normalize_loaded_skills, split_legacy_key, title_for, truncate_bytes, ActiveConnectionEntry,
    ActiveConnections, AiSessionBackend, AiSessionStore, ExportFormat, MigrationOutcome, NewId,
    SessionClock, SessionDeleteResult, SessionExport, SessionKey, SessionListParams,
    SessionLocalStorage, SessionMap, SessionPersistResult, SessionRestoreResult, SessionScope,
    StoredSession, MAX_MESSAGES_PER_SESSION, MAX_MESSAGE_BYTES, MAX_SESSIONS, MIGRATION_MARKER_KEY,
    STORAGE_KEY,
};
// The T126 AiStore contract: the zustand store's state struct and action
// trait (the aiStore port, T128, is the implementer), the NL2SQL slice with
// its initial literal, and the field-complete lean mirrors of the
// `types/index.ts` shapes the store holds (T213 owns the real ports).
pub use ai_store_types::{
    initial_nl2sql, AiMemoryMode, AiMemoryScope, AiStoreActions, AiStoreState,
    AnalyzeExplainParams, AnalyzeQueriesParams, CallMcpToolParams, ConnectionDiagnosis,
    ConnectionSolution, DiagnoseConnectionParams, DiagnoseErrorParams, EventListenerTeardown,
    ExecuteWorkflowParams, GenerateSchemaDocParams, GenerateSqlParams, McpClientStatus,
    McpServerConfig, McpServerTransport, McpToolInfo, MemoryCandidateNotice, Nl2SqlState,
    ParseFilterParams, QueryAnalysis, QueryCategory, SendChatMessageParams,
    SendWorkflowChatMessageParams, StepExecutionResult, StepStatus, WorkflowExecutionResult,
    WorkflowListItem, WorkflowVariable,
};
// The T088 streaming adapter: the module's own types plus the seam
// vocabulary the future sibling tasks (streamRegistry/sessionCancel/
// targetClaim/commands-ai, T503/T505, T126/T128) will implement or wire.
pub use ai_streaming::{
    find_mcp_tool_name, AiCancelCommand, AiStoreView, AiStreaming, AiTargetClaim, ApplyChunkGate,
    ApplyGateResult, CancelReceipt, ChatSessionPersister, ExtractQuestionsOutcome, ExtractionSeams,
    Nl2sqlStreamingSlice, SessionCancellerOps, StreamRegistryOps, StreamingClock,
};
pub use ai_types::{
    AiAgentMode, AiChatAttribution, AiChatContextSnapshot, AiChatInjectionEvent,
    AiChatInjectionSnapshot, AiChatMessage, AiChatResultPreview, AiChatSession, AiDataEgressLevel,
    AiLoadedSkill, AiMemoryCandidatesEvent, AiMemoryCloudPolicy, AiMemoryCorrectionInput,
    AiMemoryCreateInput, AiMemoryEntry, AiMemoryExtractionInput, AiMemoryKind, AiMemoryListParams,
    AiMemoryPage, AiMemoryReference, AiMemoryScopeKind, AiMemorySensitivity, AiMemorySettings,
    AiMemorySource, AiMemorySqlSample, AiMemoryStaleInput, AiMemoryStats, AiMemoryStatus,
    AiMemoryUpdateInput, AiMessageRole, AiMessageStatus, AiModelProfile, AiPreviewCell,
    AiProviderConfig, AiProviderType, AiQuestion, AiQuestionOption, AiSafetyGateConfig,
    AiSessionDetail, AiSessionMessage, AiSessionMessageInput, AiSessionSummary,
    AiSessionUpsertInput, AiSettingsConfig, AiSkillContent, AiSkillSource, AiSkillSummary,
    AiSkillsSettings, AiToolCall, AiToolPermissionPolicy, AiToolResult, AiWriteApplyReceipt,
    AiWriteAuditDecision, AiWriteAuditDecisionKind, AiWriteAuditRecord, AiWriteImpact,
    AiWriteProposal, AiWriteStatement, Bottleneck, BottleneckSeverity, DiagnosisResult,
    ExplainAnalysis, ExplainSuggestion, ModelInfo, PromptTemplate, ProviderListItem,
    StreamChunkPayload, StreamChunkUsage, StreamErrorPayload, DEFAULT_AI_MEMORY_SETTINGS,
    DEFAULT_AI_SKILLS_SETTINGS,
};
pub use backup_progress::{
    append_progress_log, backup_progress_ratio, format_backup_progress, format_restore_progress,
    BackupDumpPhase, BackupProgressPayload, ProgressLogPump, Translate, KEY_LOG_OMITTED,
    KEY_PROGRESS_OBJECT, KEY_PROGRESS_PREPARING, KEY_PROGRESS_WRITING, KEY_RESTORE_PREPARING,
    KEY_RESTORE_PROGRESS, KEY_RESTORE_SUCCESS, KEY_RESTORING, KEY_SUCCESS, MAX_PROGRESS_LOG_CHARS,
    MAX_PROGRESS_LOG_LINES,
};
pub use base64::{base64_to_bytes, Base64DecodeError};
pub use batch_export_job::{
    export_single_table_structure_via_rust, export_single_table_via_rust, run_batch_export_job,
    BatchExportColumn, BatchExportDataFormat, BatchExportMode, BatchExportOutputMode,
    BatchExportProgress, BatchExportTableInput, CancelHandle, ExportTableInput,
    ExportTablesRequest, ExportTablesResult, LoadedTableExportData, RunBatchExportJobOptions,
    RunBatchExportJobResult, SingleFetchOutcome, SingleTableExportOptions, SingleTableExportResult,
    SingleTableFetchConfig, SingleTableFetchScope, SingleTableStructureExportOptions,
    SingleTableStructureExportResult,
};
pub use chart_types::{
    AggregationType, ChartConfig, ChartDataPoint, ChartDataPointValue, ChartField,
    ChartRecommendation, ChartSortBy, ChartType, InferredFieldType,
};
pub use command_schema::{
    all_privileges, field_label, field_placeholder, get_schema_field, has_command,
    has_schema_field, privilege_groups, schema_properties, PrivilegeGroup, SchemaField,
};
pub use compare_report::{
    build_compare_report_json, build_compare_report_text, summarize_compare, CompareReportRow,
    CompareSummaryStats,
};
pub use connection_clipboard::{
    apply_connection_clipboard_fill, apply_matched_clipboard, match_connection_clipboard,
    parse_generic_connection_url, ClipboardDriverMeta, ConnectionClipboardFill,
    ConnectionClipboardParser, ConnectionFormOps, MatchedConnectionClipboard,
};
pub use connection_config::{
    clone_connection_config_for_ipc, to_ipc_connection_config, PlainConfigAssertion,
};
pub use connection_form_model::{
    assert_plain_connection_config, build_connection_config, clone_plain_json,
    coerce_connection_group, normalize_index_database_field, plain_config_assertion,
    sanitize_connection_options, BuildConnectionConfigInput, ConnectionFormDriverMeta,
    ConnectionFormSnapshot, DatabaseFieldType,
};
pub use connection_groups::{
    format_group_label, KEY_GROUP_DEV, KEY_GROUP_PROD, KEY_GROUP_TEST, PRESET_DEVELOPMENT,
    PRESET_GROUP_OPTIONS, PRESET_PRODUCTION, PRESET_TESTING,
};
pub use connection_locator::{
    connection_expand_key, group_connections_with_recent_sections, normalize,
    order_connections_for_display, parse_connection_expand_key, rank_connections,
    ActiveConnectionSlice, ActiveStatus, Candidate, ConnectionLocatorResult,
    ConnectionLocatorUsageState, ConnectionMatch, ConnectionMatchReason, ConnectionSection,
    ExpandKey, SchemaSlice, PINNED_GROUP_KEY, RECENT_CONNECTION_LIMIT, RECENT_GROUP_KEY,
};
pub use connection_share_error::{
    format_connection_import_success, ipc_connection_share_error, translate_connection_share_error,
    ConnectionImportResult, ShareErrorSource,
};
pub use connection_tab_context_menu::{
    build_connection_tab_context_menu_items, BuildConnectionTabContextMenuArgs,
    ConnectionTabAction, ConnectionTabContextMenuHandlers, ConnectionTabContextMenuLabels,
    ConnectionTabMenuItem,
};
pub use connection_types::{
    ConnectionConfig, ContextEntry, DatabaseObject, DatabaseObjectKind, DatabaseType,
    DriverCapabilities, ExplainPlanDetail, ExplainPlanNode, ExplainResult, ForeignKeyDeferrability,
    ForeignKeyInfo, ForeignKeyTargetRef, ForeignKeyTargetResolution, HttpProxyScheme,
    HttpProxyTunnelConfig, IndexInfo, PrivilegeGrant, QueryHistoryEntry,
    ResolveForeignKeyTargetsRequest, SavedTunnel, SavedTunnelKind, ServerInfo, SshAuthMethod,
    SshTunnelConfig, SslMode, TableInfo, TableSchema, TableType, TunnelKind, Value, WebSocketMode,
    WebSocketTunnelConfig,
};
pub use connection_view_registry::{
    get_connection_view, is_builtin_document_mode, ConnectionViewSelection, BUILTIN_DOCUMENT_MODE,
};
pub use connection_views::{
    ConnectionOpenTarget, ConnectionViewActions, ConnectionViewProps, NewQueryContext,
    NodeContextMenuPayload, ObjectKind, TableContextInput, TableSqlActionKind,
};
pub use context_items::{split_context_items, ContextItem, ContextKind, SplitContextItems};
pub use context_menu_position::{
    position_root_menu, position_root_menu_with_pad, position_submenu, position_submenu_with_pad,
    Point, Rect, RootMenuPosition, Size, SubmenuPosition, SubmenuSide, Viewport, CONTEXT_MENU_PAD,
};
pub use diff_detail::{
    cell_view, clear_all_op, diff_rows, footer_visible, format_cell, is_changed, is_empty_state,
    key_label, max_cols, next_disabled, operation_allowed, operation_badge_class, page_count,
    page_rows, prev_disabled, row_key_string, row_selectable, safe_page, select_all_op,
    server_paged, toggle_row, CellView, DataSyncOperation, DataSyncRowChange, DataSyncTableResult,
    SyncOptions, DIFF_DETAIL_DOM_GAP, PAGE_SIZE,
};
pub use workflow_chat_panel::{
    answer_questions_params, build_send_request, connection_options, copied_block_index,
    extract_workflow_yaml, handle_new_chat_reset, handle_stop, is_configured_gate,
    is_last_assistant, is_user, questions_visible, save_yaml, selected_database,
    selected_db_session_id, streaming_bubble_visible, thinking_visible, validation_connections,
    yaml_actions_visible, ActiveSessionSnapshot, MultiDbProvider, NewChatReset,
    ParsedWorkflowDefinition, SaveSuccess, SendRequest, WorkflowChatPanelPorts, WorkflowChatState,
    WorkflowPanelConnection, WorkflowSaveError, WorkflowValidationConnection, COPIED_FEEDBACK_MS,
    SAVE_OK_RESET_MS, WORKFLOW_CHAT_PANEL_GAP,
};
// The T159 dashboard vocabulary (see `dashboard_types`' docs): every
// name the React facade re-exports rides the crate root, but the display
// `DashboardWidget` (the N13 stub above owns the root name) and the
// module-local serde `ExecutionTarget` family stay module-path only —
// the mounting note there rules the split.
pub use dangerous_sql::{
    classify_risk, is_dangerous_write_statement, normalize_fullwidth, scan_sql,
    sql_contains_dangerous_write, SigKind, SigTok, SqlRiskAssessment, SqlRiskClassification,
    SqlRiskFinding, SqlRiskFindingType, SqlStatementRange, SqlStatementRisk, SqlToken,
    SqlTokenKind, DESTRUCTIVE_VERBS, FALSE_SQL_VALUES, MAIN_VERBS, READ_VERBS, WHERE_NEEDS_VERBS,
    WRITE_VERBS,
};
pub use dashboard_types::{
    clamp_refresh_sec, normalize_refresh_policy, should_warn_refresh_sec, AlertChannel,
    AlertMetric, AlertMetricAgg, AlertMetricKind, AlertOperator, AlertRule,
    CreateWidgetFromSqlParams, CreateWidgetFromWorkflowParams, CreateWidgetResult, Dashboard,
    DashboardLayout, DashboardWorkflowRef, MonitorEmailSettings, MonitorGrant,
    MonitorScheduleGrantSummary, MonitorScope, MonitorSettings, RefreshMode, RefreshPolicy,
    RunIndexEntry, UpdateHiddenWidgetSqlParams, ViewMode, WidgetLayout, WidgetRun, WidgetRunStatus,
    DEFAULT_MONITOR_SETTINGS, DEFAULT_REFRESH, MIN_REFRESH_SEC, REFRESH_WARN_BELOW_SEC,
};
pub use data_sync_bars::{
    build_endpoints_bar_config, build_execute_bar_view, can_fire, resolve_action_note, ActionNote,
    ActivePairing, CancelVariant, EndpointsBarConfig, EndpointsLayout, ExecuteBarEvent,
    ExecuteBarInput, ExecuteBarView, RunVariant, SyncPairingPath, TranslateFn, ACTION_NOTE_CLASS,
    CANCEL_TEST_ID, COMPARE_LABEL_KEY, CONTAINER_CLASS, DELETE_CLASS, ENDPOINTS_I18N_PREFIX,
    ENDPOINTS_TEST_ID_PREFIX, EXECUTE_TEST_ID, EXPERIMENTAL_HINT_KEY, EXPERIMENTAL_PAIR_KEY,
    KEY_CANCEL, KEY_DELETE_WILL_APPLY, KEY_EXECUTE, KEY_EXECUTE_UNAVAILABLE, KEY_EXECUTING,
    KEY_SELECTED_ROWS, KEY_TARGET_READ_ONLY, PATH_DIRECT_KEY, PATH_IR_KEY, PATH_TEST_ID,
    READ_ONLY_CLASS, START_DISABLED_TEST_ID, START_TEST_ID, SUPPORTED_CLASS, UNSUPPORTED_CLASS,
    USE_TRANSFER_HINT_KEY,
};
pub use data_sync_comparison::{
    apply_options_to_rows, cancel_actions, clear_job_refs, first_diff_key, is_compared_sync_state,
    is_current_generation, is_paged_comparison, mark_disabled_tables, merge_compare_into_mappings,
    next_generation, selected_rows_from_merged, should_keep_cancelled_result, tables_for_compare,
    toggle_disabled_status, update_source_filter, validate_endpoints, CancelActions,
    ClearJobRefsInput, ComparisonRow, ComparisonTableRow, ComparisonTableStatus, DataSyncJobKind,
    DataSyncSyncState, EndpointInput, MappedRow, SelectedComparisonRow, ValidationFailure,
    DATA_SYNC_COMPARISON_GAP, DATA_SYNC_COMPARISON_SURFACE, DATA_SYNC_DB_REQUIRED_KEY,
    DATA_SYNC_SAME_DB_KEY, DATA_SYNC_SELECT_BOTH_KEY, DATA_SYNC_TRANSFER_HINT_KEY,
    SYNC_CANCELLING_EXECUTION_KEY, SYNC_COMPARE_CANCELLED_KEY,
};
pub use data_sync_endpoints::{
    apply_endpoint_change, build_conn_options, build_target_options, can_refresh_sessions,
    clear_endpoint_selection, connect_failed_message, connection_label, is_experimental,
    is_same_endpoint_identity, is_target_read_only, is_verified_migration_pair,
    load_databases_failed_message, pick_default_schema, pick_prefill_database, pick_prefill_schema,
    resolve_default_database, schema_load_allowed, session_failed_message, swap_scope,
    unique_schemas_from_tables, ChangeEffect, ConnectionLike, ConnectionOption,
    DedicatedSideSession, EndpointField, EndpointIdentity, EndpointPrefill, EndpointScope,
    PrefillSide, TargetOption, CONNECTIONS_CHANGED_EVENT, CONNECTIONS_CLOSED_EVENT,
    CONNECT_FAILED_KEY, DATA_SYNC_ENDPOINTS_GAP, DATA_SYNC_ENDPOINTS_SURFACE,
    EXPERIMENTAL_PAIR_KEY as ENDPOINTS_EXPERIMENTAL_PAIR_KEY, LOAD_DATABASES_FAILED_KEY,
    SESSION_FAILED_KEY, UNSUPPORTED_PAIR_KEY, VERIFIED_MIGRATION_TYPES,
};
pub use data_sync_execution::{
    can_execute, handle_execute, plan_execution, plan_finalize, run_execute_guard, CompareResult,
    ExecuteGuard, ExecuteResult, ExecutionEffect, ExecutionInput, ExecutionPlan,
    ExecutionResponses, FinalizeInput, FinalizePlan, GuardOutcome, SessionRefresh, TableSelection,
    DATA_SYNC_EXECUTION_GAP, DATA_SYNC_EXECUTION_SURFACE,
};
pub use data_sync_explain::{
    begin_explain, build_explain_prompt, build_explain_report, explain_gate, explain_request,
    failure_explain, success_explain, ExplainEffects, ExplainInput, ExplainRequest,
    DATA_SYNC_EXPLAIN_GAP, DATA_SYNC_EXPLAIN_SURFACE, EXPLAIN_DIFF_FAILED_KEY,
    EXPLAIN_DIFF_NO_AI_KEY, EXPLAIN_DIFF_PROMPT_KEY,
};
pub use data_sync_job_lifecycle::{
    CancelOutcome, DataSyncJobDetails, DataSyncJobLifecycle, EndpointSession, Endpoints,
    FeedbackSink, JobLifecycleHost, JobRecoveryVerdict, LifecycleState, RecoveryTarget,
    VerifyOutcome, VerifyRequest, VerifyResponse, DATA_SYNC_JOB_LIFECYCLE_GAP,
    DATA_SYNC_JOB_LIFECYCLE_SURFACE, JOB_POLL_INTERVAL_MS, RECOVERY_ENDPOINT_MISMATCH_KEY,
    RECOVERY_MANUAL_REVIEW_KEY, RECOVERY_RECORDED_KEY, RECOVERY_SELECT_ENDPOINTS_KEY,
};
pub use data_sync_options::{
    apply_options_change, clear_mapping_rows, confirm_delete, invalidates_comparison,
    scopes_after_options, selected_rows_after_options, OptionsChangeEffects, OptionsChangeInput,
    ScopeExclusion, TableScopeWithExclusions, DATA_SYNC_OPTIONS_GAP, DATA_SYNC_OPTIONS_SURFACE,
    DEFAULT_CONFLICT_POLICY, SYNC_STEP_PREVIEW, SYNC_STEP_RESULT, SYNC_STEP_SETUP,
};
pub use data_sync_profiles::{
    current_profile_tables, delete_selected_profile, load_selected_profile, load_sync_profiles,
    parse_timestamp, reconcile_unknown_run, save_current_profile, DataSyncTableMapping,
    MappingRowLike, MigrationRunRecordLike, ProfileConnection, ProfileEffect, ProfileEndpointState,
    ProfilesHost, ProfilesState, ReconcileInput, SyncProfile, DATA_SYNC_PROFILES_GAP,
    DATA_SYNC_PROFILES_SURFACE, PROFILE_DELETED_KEY, PROFILE_LOADED_KEY,
    PROFILE_MISSING_CONNECTION_KEY, PROFILE_MISSING_FIELDS_KEY, PROFILE_SAVED_KEY,
    RECONCILE_BUSY_KEY, RECONCILE_MISSING_CONNECTION_KEY, RECONCILE_NEEDS_SCOPE_KEY,
    RECONCILE_PROFILE_CHANGED_KEY, RECONCILE_PROFILE_READY_KEY, RECONCILE_SCOPE_UNAVAILABLE_KEY,
};
pub use data_sync_refs::{
    create_data_sync_refs, DataSyncSharedRefs, DATA_SYNC_REFS_GAP, DATA_SYNC_REFS_SURFACE,
};
pub use data_sync_run_state::{
    DataSyncConflict, DataSyncExecutionOutcome, DataSyncExecutionResult, DataSyncRunState,
    SetAction, DATA_SYNC_RUN_STATE_GAP, DATA_SYNC_RUN_STATE_SURFACE,
};
pub use data_sync_selection::{
    clear_table_operation_scope, has_selected_deletes, is_table_operation_scoped,
    loaded_page_token, next_page_cursors, next_page_index, page_selection_decision,
    reconcile_selected_rows, resolve_cursor, row_counts, set_table_operation_scope, table_page_key,
    total_selected_rows, update_exclusions, CursorOutcome, PageCursor, PageDirection, RowChange,
    RowCounts, SelectionStateView, SelectionTable, TableSelectionScope, DATA_SYNC_SELECTION_GAP,
    DATA_SYNC_SELECTION_SURFACE, PAGE_DIRECTION_INITIAL, PAGE_DIRECTION_NEXT,
    PAGE_DIRECTION_PREVIOUS,
};
pub use data_sync_selection_state::{
    DataSyncPageCursor, DataSyncSelectionState, SelectionRefs, SelectionScopeKey, SelectionState,
    DATA_SYNC_SELECTION_STATE_GAP, DATA_SYNC_SELECTION_STATE_SURFACE,
};
pub use data_sync_wizard_navigation::{
    can_go_next, compare_disabled, is_busy, next_step, plan_go_back, plan_go_next, previous_step,
    step_index, NavigationEffect, NavigationInput, WizardStep, DATA_SYNC_WIZARD_NAVIGATION_GAP,
    WIZARD_STEPS,
};
pub use data_table_clipboard_dom::{
    bind_clipboard_dom, handle_copy, handle_paste, is_editable_target, resolve_copy_target,
    resolve_paste_anchor, ClipboardBinding, ClipboardBridge, ClipboardDomInput, ClipboardEvent,
    CopyOutcome, DomTarget, GridCopyTarget, PasteAnchor, PasteEvent, PasteOutcome,
    CLIPBOARD_DOM_GAP,
};
pub use database_object_identity::database_object_identity_key;
// The T188 filter vocabulary: the aggregate (`types/index`) re-exports
// everything `types/filters.ts` declares, so the crate root carries the
// same names.
pub use data_table_bits::{
    bridge_availability, density_row_height_delta, fk_selector_label, fk_text_with_selector,
    row_height_for_density, ClipboardCellCoord, FkSelectorTranslator, GridClipboardHandlers,
    DATA_TABLE_BITS_GAP, DATA_TABLE_BITS_SURFACE, DATA_TABLE_ROW_HEIGHT, FK_SELECTOR_PICK_FROM_KEY,
};
pub use data_table_context_menu::{
    build_data_table_context_menu_items, format_row_as_sql_delete, format_row_as_sql_insert,
    format_row_as_sql_update, format_rows_as_sql_insert, format_rows_as_sql_update, invoke,
    resolve_data_table_cell_from_event, resolve_data_table_header_col_from_event,
    resolve_set_value_actions, row_to_named_record, serialize_cells,
    serialize_data_table_column_values, serialize_data_table_rows_as_csv,
    serialize_data_table_rows_as_json, serialize_data_table_rows_as_tsv, set_value_action_label,
    BuildDataTableContextMenuArgs, CellContext, CopyFormat, DataTableContextMenuHandlers,
    DataTableContextMenuLabels, DomElement, HeaderFreeze, HeaderPresentation, HeaderSort,
    HeaderStats, MenuAction, NativeMenuItemDef, RowKind, SerializeFormat, SetValueAction,
    SetValueActionId, SetValueState, LOAD_CANDIDATES_LABEL_KEY,
};
pub use database_types::{
    default_escape_sql_value, escape_ident, format_connection_addr, get_db_icon, get_db_label,
    get_driver_icon_map, get_driver_icon_parents, get_escape_sql_value,
    normalize_index_database_field_default, sort_db_types_by_popularity, ConnectionAddrInput,
    DatabaseRegistry, DatabaseTypeMeta, DbIconInfo, EscapeSqlValue, PopularityValue,
    SqlValueEscaper, SshTunnelAddr, DB_TYPE_POPULARITY_ORDER, DEFAULT_MAX_DATABASE_INDEX,
};
pub use dedicated_db_session::{
    DatabaseOverride, DedicatedConnectResult, DedicatedSessionPorts, DedicatedSessionService,
    ListDatabasesResult,
};
pub use dismiss_prefs::{create_dismiss_prefs, DismissPrefs};
pub use driver_capabilities::{
    capabilities_for_db_session, has_schema_level, relation_schema_for, supports_offset,
    CapabilityAnswer, SessionCapabilitiesEntry,
};
pub use driver_command_ids::{
    ESTIMATE_TABLE_ROWS_COMMAND, LIST_PROCESSES_COMMAND, SERVER_STATUS_SNAPSHOT_COMMAND,
};
pub use error_message::ErrorLike;
pub use export_capability::{
    driver_export_scope, min_scope, resolve_export_scope, supports_any_export,
    supports_full_table_export, DataExportCapability, ExportDriverMeta,
};
pub use export_data::{ExportDataFormat, ExportDataScope};
pub use export_format::{
    apply_csv_formula_guard, build_provided_export_filename, bytes_to_xlsx_cell,
    default_escape_sql_literal, escape_sql_ident, escape_sql_literal, format_extension,
    format_rows_as_text, format_rows_as_xlsx, format_sql_insert_script, format_sql_update_script,
    mysql_escape_sql_literal, sql_quote_char, text_mime_type, to_xlsx_cell, update_key_column,
    xlsx_sheet_rows, BinaryExportResult, ExportColumn, ExportFormatError, FormatRowsOptions,
    TextExportResult, XlsxCellValue, SQL_INSERT_BATCH_SIZE, XLSX_MIME_TYPE,
};
pub use export_table_structure::{
    export_table_structure_to_file, DdlDialect, DdlFetcher, DdlQuery, DdlRowExtractor,
    ExportTableStructureResult, SqlDialectProvider, StructureExportInput, StructureExporter,
};
pub use extract_questions::{extract_questions, parse_tool_call_questions, ExtractQuestionsResult};
pub use fetch_relation_ddl::{
    copy_rich_text_to_clipboard, copy_to_clipboard, fetch_relation_ddl, CachedDdlFetcher,
    ClipboardSteps, DdlRowValueExtractor, ObjectDdlFetcher, RelationDdlDialect,
    RelationDialectProvider, RichClipboardSteps,
};
pub use filter_db_types::{filter_db_types_by_query, DbTypeOption};
pub use filter_operators::{
    cell_types_for_operator, convert_filter_value, default_operator_for_column,
    dialect_operators_by_cell_type, filter_operator_metas, like_case_insensitive_for_meta,
    normalize_operator_for_column, operator_meta, operators_by_cell_type, operators_for_column,
    operators_for_driver, supported_operators_for_meta, value_input_for, value_input_for_literal,
    FilterDriverMeta, FilterOperatorGroup, FilterOperatorMeta, FilterValue, FilterValueInputKind,
    ALL_CELL_TYPES, ALL_OPERATORS,
};
pub use filter_types::{
    FilterCondition, FilterConnector, FilterOperator, DIALECT_FILTER_OPERATORS,
    EXISTING_FILTER_OPERATORS, SHARED_FILTER_OPERATORS,
};
pub use fk_navigation::{
    build_fk_filters, find_foreign_key, fk_nav_key, index_foreign_keys, pick_anchor_row,
    scroll_top_for_anchor_row, AnchorInput, AnchorResult, FkBreadcrumbModel, FkFilterTarget,
    FkGroupAffordance, FkIndex, FkNavInput, FkNavTargetIdentity, GridViewport, NavValue,
    FK_NAV_MAX_DEPTH,
};
pub use fk_referenced_by::{
    build_inbound_filters, catalog_key, inbound_catalog, open_referencing_rows,
    referenced_values_of_row, CatalogScope, CreatedPanel, GridErrorClassifier, InboundCatalogCache,
    InboundCatalogFetcher, InboundForeignKeyCatalog, InboundForeignKeyRef, NavEntry, NavOrigin,
    OpenReferencingRowsInput, OutcomeError, OutcomeErrorCode, ReferencedByOutcome,
    ReferencingRowsHost, SourcePanelInfo, TableChangeContext as InboundTableChangeContext,
    MAX_REFERENCING_TABS, NO_REFERENCES_EMPTY, PROBE_FAILED_MESSAGE,
};
pub use fk_selector::{
    candidates_to_picker_options, fk_group_is_pickable, format_fk_candidate,
    index_fk_candidate_sources, FkCandidateGroup, FkCandidateSource, FkCandidateSourceIndex,
    FK_SELECTOR_CANDIDATE_LIMIT,
};
pub use global_text_selection::{
    is_interactive_control, is_native_selectable, is_selectable, is_selectable_hint,
    should_prevent_select_all, ContextMenuSuppressor, DragMouseDownInput, DragSelectionGuard,
    RightDragSuppressor, SelectAllTarget, SelectionSnapshot, DOM_GAP_SURFACES,
    INTERACTIVE_CONTROL_SELECTOR, NATIVE_SELECTABLE_SELECTOR, SELECTABLE_HINT_SELECTOR,
};
pub use grid_cell::{
    build_grid_cell_view, cell_command, dispatch_cell_event, CellChild, CellCommand, CellEvent,
    CellRendererView, CellWriteTarget, ColumnCellFacts, DispatchOutcome, EditorCellFacts,
    FkButtonView, FkCellAffordance, FkTextFn, FkTextKind, GridCellInput, GridCellView, StickyInfo,
    StickyStyle, CELL_TEST_ID, CLASS_BASE, CLASS_DIRTY_BG, CLASS_DIRTY_BORDER, CLASS_DIRTY_MARK,
    CLASS_FK_JUMP, CLASS_FK_PICK, CLASS_STICKY, CLASS_STICKY_OVERLAY, EDITOR_KIND_ENUM,
    ROLE_GRIDCELL,
};
pub use grid_clipboard::{
    build_rows_matrix, build_selection_matrix, escape_csv_field, escape_html_cell_text,
    escape_markdown_cell_text_value, escape_markdown_field, parse_clipboard_matrix,
    serialize_clipboard_cells, sniff_clipboard_delimiter, to_clipboard_range, utf8_byte_length,
    ClipboardDelimiter, ClipboardFormat, ClipboardMatrix, ClipboardParseResult,
    GridClipboardAnchor, GridClipboardRange, PasteParseErrorCode, PasteParseRefusal, PasteProblem,
    PasteProblemCode, SerializeCellsOptions, GRID_PASTE_CHUNK_CELLS, GRID_PASTE_CONFIRM_CELLS,
    GRID_PASTE_MAX_BYTES, GRID_PASTE_MAX_CELLS,
};
pub use grid_errors::{
    classify_grid_error, GRID_ERROR_CODES, GRID_ERROR_NAMESPACE, REGISTERED_PREFIXES,
};
pub use grid_event_guards::{is_composing_event, ComposingEventLike, EDITABLE_TARGET_SELECTOR};
pub use grid_paste_plan::{
    clear_clipboard_source, clear_row_copy, column_writability, normalize_clipboard_text,
    peek_clipboard_source, peek_row_copy, pending_cell_key, plan_paste, record_clipboard_source,
    record_row_copy, row_copy_for_clipboard_text, source_for_clipboard_text, table_change_key,
    ClipboardSource, ColumnWritability, ColumnWritabilityReason, PasteAlignment, PasteOverflow,
    PastePlan, PastePlanOptions, RowCopyPayload,
};
pub use history_groups::{
    find_group_for_database, group_query_history, HistoryGroupEntry, HistoryScope,
    QueryHistoryGroup, HISTORY_UNKNOWN_DB_KEY,
};
pub use host_error_message::{extract_host_error_message, HostError};
pub use host_lucide_map::{
    build_host_lucide_by_id, host_lucide_seed, settings_section_icon_id, HOST_LUCIDE_MAP,
    SETTINGS_SECTION_LUCIDE_MAP,
};
pub use icon_ids::{UiIconId, UI_ICON_IDS};
pub use icon_resolver_api::{
    default_placeholder_for_db, get_active_icon_resolver, resolve_with_active,
    set_active_icon_resolver, ui_placeholder, ICON_KIND_LUCIDE, ICON_KIND_PLACEHOLDER,
    ICON_KIND_URL,
};
pub use import_connection_file::{
    import_file_display_name, import_file_password_policy, ImportPasswordPolicy,
};
pub use import_data::{
    decode_import_base64, decode_import_text, dedupe_columns, detect_csv_delimiter,
    detect_import_format, generate_insert_sql, is_supported_encoding, parse_csv_import,
    parse_import_data, parse_json_import, parse_xlsx, parse_xlsx_import, CsvParseOptions,
    ImportDiagnostic, ImportEmptyFieldPolicy, ImportFormat, ImportParseError, ImportParseErrorKind,
    ImportParseResult, ImportTextDecoder, JsonParseOptions, ParsedData, Utf8TextDecoder,
    XlsxBookReader, XlsxParseOptions, XlsxParseResult, XlsxWorkbook, IMPORT_ENCODINGS,
};
pub use import_execution::{
    build_multi_row_insert_sql, execute_import, plan_import_chunks, AbortSignal, ImportBatch,
    ImportChunk, ImportColumnPair, ImportCommitMode, ImportErrorStrategy, ImportFinalStatus,
    ImportProgress, ImportProgressStatus, ImportRowFailure, ImportRunResult, ImportSqlDialect,
    ImportSqlOptions, ImportStreamEvent, ImportStreamProgress, ImportTransport,
    PlanImportChunksOptions, IMPORT_DEFAULT_MAX_BATCHES_PER_CHUNK,
    IMPORT_DEFAULT_MAX_ROWS_PER_BATCH, IMPORT_DEFAULT_MAX_SQL_BYTES, IMPORT_STREAM_GAP,
};
pub use kv_ai_context::{
    build_kv_ai_context, compose_kv_ai_message, has_kv_ai_facts, KvAiContext, KvAiFacts,
};
pub use kv_slot_state::{
    create_kv_slot_state, get_kv_slot_state, prune_kv_slot_states, reset_kv_slot_states_for_tests,
    KvSlotSnapshot, KvSlotState,
};
pub use locale_sync::{
    apply_language, current_language, start_locale_sync, Defer, Deferred, LocaleEnginePorts,
    SettingsLanguageStore, DEFAULT_LOCALE,
};
pub use migration_job_verdict::{
    derive_cancel_disposition, derive_migration_job_verdict, derive_uncertainty,
    is_backend_scope_rejection, is_migration_job_in_flight, is_pipeline_budget_interception,
    is_plan_consumed_rejection, read_job_counter, sum_job_counters, CancelDisposition,
    CommitBoundaryFacts, JobCounterValue, MigrationJobVerdict, MigrationJobVerdictInput,
    UncertaintyReason, VerdictSeverity, PIPELINE_BUDGET_BYTES,
};
pub use native_context_menu::{DRIVER_SDK_MENU_GAP, NATIVE_CONTEXT_MENU_SURFACE};
pub use object_browser_context_menu::{
    build_object_browser_editor_menu_items, build_object_browser_list_menu_items,
    native_edit_menu_items, NativeEditItem, ObjectBrowserEditorMenuHandlers,
    ObjectBrowserEditorMenuLabels, ObjectBrowserListMenuHandlers, ObjectBrowserListMenuLabels,
    ObjectBrowserMenuAction, ObjectBrowserMenuItem,
};
pub use object_filter::{
    filter_table_items, get_object_filter, is_system_schema_name, match_name_pattern,
    matches_table_name_filter, should_show_database, should_show_schema,
    with_object_filter_options, ObjectFilterPrefs,
};
pub use query_execution_view_model::{
    get_cancel_action_state, get_cancel_capability, to_query_execution_view_model,
    CancelActionState, CancelCapability, QueryExecutionViewModel, QueryPhase,
};
pub use query_sidebar_context_menu::{
    build_favorite_sidebar_context_menu_items, build_history_sidebar_context_menu_items,
    build_history_sidebar_header_context_menu_items, BuildFavoriteSidebarContextMenuArgs,
    BuildHistorySidebarContextMenuArgs, BuildHistorySidebarHeaderContextMenuArgs,
    QuerySidebarFavoriteHandlers, QuerySidebarFavoriteLabels, QuerySidebarHistoryHandlers,
    QuerySidebarHistoryHeaderHandlers, QuerySidebarHistoryHeaderLabels, QuerySidebarHistoryLabels,
    QuerySidebarMenuItem,
};
pub use query_stream::{apply_query_stream_event, StreamableQueryTab};
pub use quick_look_dialog::{
    binary_value_bytes, build_quick_look, bytes_to_base64, is_blob_type, is_json_value,
    is_quick_lookable_value, pretty_json, raw_json_text, resolve_event, resolve_save,
    save_blob_request, to_edit_string, BlobSaveRequest, CommitValue, QuickLookBody,
    QuickLookEffect, QuickLookEvent, QuickLookInput, QuickLookModel, QuickLookView, SaveOutcome,
    BINARY_TEST_ID, BLOB_TYPE_KEYWORDS, DIALOG_CLASS, JSON_TEST_ID, PRE_CLASS, TEXTAREA_CLASS,
    TEXTAREA_ROWS, TEXTAREA_WRAP,
};
pub use relation_identity::{
    build_editor_relation_key, fold_segment, has_namespace, qualified_name_text,
    relation_base_name, relation_identity_key,
};
pub use result_editability::{
    grid_read_only_message_key, is_column_editable, origin_channel_writable,
    resolve_grid_editability, resolve_result_editability, resolve_result_write_back,
    resolve_row_addressability, GridColumn, GridEditability, GridReadOnlyReason, IdentityColumn,
    ResultTableRef, ServerEditability, RESULT_ORIGIN_WRITE_BACK_ENABLED,
};
pub use schema_cache::{
    cache_key, composite_cache_key, ddl_cache_key, DdlCacheIdentity, DdlCacheInvalidateScope,
    SchemaCacheFacade, SchemaObjectKind,
};
pub use schema_changing_sql::{
    is_data_modifying_statement, is_schema_changing_statement, is_schema_mutating_statement,
    mask_semicolons_in_literals, split_sql_statements, sql_contains_data_modifying,
    sql_contains_schema_changing_ddl, sql_may_mutate_schema,
};
pub use schema_tree_context_menu::{
    build_schema_tree_context_menu_items, BuildSchemaTreeContextMenuArgs, SchemaTreeAction,
    SchemaTreeContextMenuHandlers, SchemaTreeContextMenuLabels, SchemaTreeMenuItem,
    SchemaTreeNodeContextMenuPayload, SchemaTreeNodeKind,
};
// The T234 keyset paging vocabulary: the aggregate (`types/index`)
// re-exports everything `types/keyset.ts` declares, so the crate root
// carries the same names.
pub use keyset_paging::{
    apply_paging_info, cursor_for_page, empty_keyset_paging_state, filter_signature,
    is_usable_boundary, next_request_cursor, order_signature, paging_signature, push_cursor,
    reanchor, KeysetCursor, KeysetPagingState, KeysetRequestPlan, PushedCursor, KEYSET_STACK_LIMIT,
};
pub use keyset_types::{
    CursorValue, KeysetUnavailableReason, OrderColumn, PagePagingInfo, PagingMode,
};
pub use kv_workspace::{
    capability_key_for_slot, get_kv_slot_component, has_kv_slot_capability, KvDriverMeta,
    KvSlotComponent, KvSlotComponentProvider, KvSlotMetaProvider, KvSlotName,
    KvWorkspaceCapabilities, KV_SLOT_NAMES,
};
pub use load_batch_export_table::{
    load_batch_export_table_data, BatchDdlFetcher, BatchDdlRowExtractor, LoadBatchExportDeps,
    LoadBatchExportTableParams, RelationSchemaProvider, SchemaFetcher, TableDataFetcher,
    TableDataRequest, DEFAULT_MAX_ROWS, DEFAULT_PAGE_SIZE,
};
pub use main_window_context_menu::{
    build_main_blank_context_menu_items, build_main_connection_context_menu_items,
    build_main_group_context_menu_items, invoke as invoke_main_window, MainWindowAction,
    MainWindowConnectionHandlers, MainWindowContextMenuLabels, MainWindowMenuItem, MoveTarget,
};
pub use metadata::{
    check_temporary_result, is_provenance_current, metadata_batch_bucket, metadata_cache_key,
    metadata_config_bucket, metadata_session_key, read_temporary_result, BatchFlush,
    CurrentCallContext, CurrentProvenanceContext, ExecutionTarget, InvalidationDecision,
    InvalidationReason, MetadataBatcher, MetadataBatcherOptions, MetadataClock,
    MetadataIdentityInput, MetadataSessionIdentity, MetadataTarget, NamespaceTarget, ObjectTarget,
    ResultProvenance, SessionHandle, TemporaryResult,
};
pub use migration_job_hydration::{
    bind_backend_client, bound_backend_client, classify_job_view, hydrate_migration_jobs,
    is_stale_plan_error, job_kinds, latest_apply_job, update_migration_job_projection,
    ErrorLike as MigrationErrorLike, GetJobFn, JobClassification, JobState, JobView, ListJobsFn,
    MigrationJobClient, MigrationJobHydration, MigrationWindow, STALE_PLAN_ERROR_CODES,
};
pub use named_views::{
    build_named_view_export, is_host_filter_operator, named_view_scope_key, named_views_available,
    normalize_view_name, normalized_name_key, parse_named_view_export, read_view_filters,
    resolve_import_name, sort_named_views, view_from_table_state, view_names_equal,
    views_for_scope, BackendClientCapabilities, NamedView, NamedViewExportFile, NamedViewScope,
    PortFilterCondition, TableStateForView, ViewFilterReadout, ViewFromTableStateOptions,
    NAMED_VIEW_EXPORT_VERSION,
};
pub use panel_context_path::{format_panel_context_path, PanelContextPathArgs};
pub use process_list_result::{
    command_result_columns, command_result_rows, default_process_columns, process_column_label_key,
    process_record_to_row, ProcessColumnDef,
};
pub use product_features::{is_product_feature_enabled, ProductFeatureKey, PRODUCT_FEATURE_KEYS};
// seg3's panel modules re-export several names already present at the root
// (from the parent's established facades); only their unique names are listed
// here so the root keeps one binding per name.
pub use panel_exec_cleanup::{
    can_cancel_pane, cancel_and_cleanup_exec, cancel_and_cleanup_pane_exec, request_pane_cancel,
    session_keys_of_panel, ExecMap, PaneCancelStore, Panel, PanelExecDeps, PanelType,
    SessionController, PANEL_EXEC_CLEANUP_ASYNC_GAP, PANEL_EXEC_CLEANUP_GAP,
    PANEL_EXEC_CLEANUP_SURFACE,
};
pub use panel_query_context::{
    panel_target_database, panel_target_schema, QueryPanelContext, SchemaStateForPanel,
    SqlNamespace,
};
pub use panel_result_edit::{
    assert_result_writable, columns_from_key, columns_key, effect_dependency_key, effect_request,
    result_edit_key, start_effect, verdict_view, PanelResultEditStore, ResolveTicket,
    ResultEditabilityOptions, ResultOriginRequest, ResultWritableError, VerdictView,
    PANEL_RESULT_EDIT_GAP,
};
pub use panel_store::{
    is_pane_key_of_panel, pane_args, pane_id_of_pane_key, pane_key, pane_keys_of_panel,
    panel_id_of_pane_key, resolve_focused_pane_id, resolve_next_active, sync_pane_focus,
    validate_batch, BindParams, CommitCellRequest, ExecTable, FocusPatch, PanelExecState,
    PanelPatch, PanelState, PanelStore, PanelStoreHost, PendingHistoryQuery, ResultColumn,
    ResultEdit, ResultView, RunQueryRequest, StorePanel, StorePanelType, DEFAULT_PANE_ID,
    PANEL_STORE_GAP, PANEL_STORE_SURFACE, PANE_SEPARATOR,
};
pub use panel_types::{
    next_panel_id, panel_schema, reset_panel_id_counter, typed_resolve_next_active,
    ConnectionContext, CreateTablePanel, DatabaseObjectPanel, DbObjectKind, ErDiagramPanel,
    FkOrigin, MonitorSubPage, ObjectsPanel, PanelBase, PrivilegesPanel, ProcessListCacheData,
    ProcessesPanel, QueryPanel, RedisDbPanel, RedisPendingAction, RedisTab, ServerStatusCache,
    ServerStatusPanel, SubTabId, TablePanel, TypedPanel, ViewPanel, PANEL_TYPES_GAP,
    PANEL_TYPES_SURFACE,
};
pub use platform_shared::{
    format_call_tool_result, mcp_qualified_name, McpContentBlock, McpError, McpFetch,
    McpHttpResponse, McpJsonRpcError, McpRequestInit, McpToolCallResult, WebMcpClient,
    WebMcpClientManager, WebMcpConnectedServer, WebMcpServerConfig, WebMcpToolInfo,
};
// `ColumnSchema`/`IndexInfo`/`ForeignKeyInfo`/`TableSchema` keep a single
// canonical definition in `connection_types`; `ColumnSchema` reaches the root
// once through the `types` aggregate. `from_table_schema` now imports those
// facade shapes, so there are no prediction slices to re-export here.
pub use query_exec_actions::{
    extract_error, is_cancellation_error, notify_schema_actions, patch_exec,
    post_success_view_mode, query_error_transition, reduce_query_execution_state, run_start_state,
    should_apply_event, transition_exec, ExecutionError, QueryCancelState, QueryExecState,
    QueryExecTable, QueryExecutionTransition, QueryTerminalState, SchemaAction,
    GENERIC_FAILURE_KEY, QUERY_ERROR_QUOTA_CODE, QUERY_EXEC_ACTIONS_GAP,
    QUERY_EXEC_ACTIONS_SURFACE, QUOTA_MESSAGE_KEY, REFRESH_CONNECTION_EVENT,
};
pub use relation_prediction::{
    is_generic_key_name, normalize_data_type, normalize_identifier, predict_relations,
    related_table_boost_map, related_table_hints, singularize, split_identifier_words,
    to_prediction_table_from_schema, to_prediction_tables_from_schemas, type_family, ColumnPair,
    DeclaredForeignKey, EvidenceWeights, PredictionColumn, PredictionEvidence,
    PredictionEvidenceCode, PredictionOptions, PredictionTable, PredictionTier, RelatedTableHint,
    RelatedTableOrigin, RelationCandidate, TypeFamily, DECLARED_RELATION_BOOST,
    DEFAULT_AMBIGUITY_MARGIN, DEFAULT_HIGH_THRESHOLD, DEFAULT_MEDIUM_THRESHOLD, EVIDENCE_WEIGHTS,
    PREDICTED_RELATION_BOOST,
};
pub use relative_time::{
    format_relative_time, get_relative_time_parts, RelativeTimeParts, RelativeTimeUnit,
    RELATIVE_WINDOW_MS,
};
pub use resolve_ui_language::{SupportedUiLanguage, SUPPORTED_UI_LANGUAGES};
pub use row_to_record::row_to_record;
pub use run_predefined_edit::{predefined_edit_command, run_predefined_edit, NativeMenuPredefined};
pub use run_to_chart::{has_renderable_chart, widget_run_to_chart_data, ChartTransformResult};
pub use run_to_result::widget_run_to_statement_result;
pub use running_execution_close::{
    ask_now_resolution, deadline_from, effective_retry_ms, gate_change_effects, guard_decision,
    guard_result, host_error_answer, initial_settle_state, no_gate_answer, running_close_request,
    running_panes_of, settle_running_panes_step, settled_once, stoppable_panes, AskResolution,
    GateChangeEffects, GuardDecision, RunningCloseDecision, RunningExecutionRequest, RunningPane,
    SettleOutcome, SettleState, RUNNING_CLOSE_POLL_MS, RUNNING_CLOSE_RETRY_MS,
    RUNNING_CLOSE_TIMEOUT_MS, RUNNING_EXECUTION_CLOSE_GAP, RUNNING_EXECUTION_CLOSE_SURFACE,
};
pub use scalar_cell_editors::{
    build_number_view, build_temporal_view, build_text_view, common_flags, CommonEditorFlags,
    FocusStrategy, NumberEditorView, ScalarEditorInput, TemporalEditorView, TemporalKindValue,
    TextEditorView, DEFAULT_TEMPORAL_KIND, SCALAR_CELL_EDITORS_GAP, SCALAR_CELL_EDITORS_SURFACE,
};
pub use schema_diff_limitations_prefs::{
    clear_schema_diff_limitations_dismissed, create_schema_diff_limitations_dismiss_prefs,
    is_schema_diff_limitations_dismissed, set_schema_diff_limitations_dismissed,
    SCHEMA_DIFF_LIMITATIONS_DISMISSED_KEY,
};
pub use schema_object_search::{
    filter_object_search_results_by_type, filter_schema_objects_by_type,
    group_object_search_results, group_schema_objects, matching_columns, object_type_order,
    search_schema_objects, table_matches_object_search, ObjectSearchFilters,
    ObjectSearchMatchField, ObjectSearchObjectType, ObjectSearchResult, ObjectSearchResultGroup,
    ObjectSearchSourceKind, SchemaObjectIndex, SchemaObjectIndexEntry, SchemaObjectSearchIndex,
};
pub use server_status_trends::{
    align_timestamp, card_rate_from_history, compute_trend_time_axis, desired_tick_count,
    format_byte_rate, format_bytes, format_rate, latest_series_value, number_from, pick_tick_step,
    update_trend_history, CommandKey, StatusRecord, TrendAxisOptions, TrendKind, TrendPoint,
    TrendPrevSample, TrendSeries, TrendSeriesDef, TrendTimeAxis, TrendUpdateInput,
    TrendUpdateResult, COMMAND_KEYS, DEFAULT_MIN_WINDOW_MS, DEFAULT_REFRESH_PERIOD_MS, HISTORY_MAX,
    HISTORY_WINDOW_MS, TICK_STEP_CANDIDATES_MS, TREND_SERIES,
};
pub use splash::{
    hide_splash_plan, run_startup_wait_task, HideSplashPlan, StartupSettlement, StartupWaitResult,
    HIDE_SPLASH_DELAY_MS, SPLASH_DOM_GAP, STARTUP_TASK_GAP,
};
pub use sql_bind_params::{
    build_bind_payload_v2, coerce_param_value, default_dialect_policy, get_param_label,
    is_sensitive_param_name, param_fingerprint, params_to_payload, parse_sql_param_occurrences,
    parse_sql_params, sensitive_param_names, substitute_sql_params, ParamValue, SqlBindPayloadV2,
    SqlParam, SqlParamDialectPolicy, SqlParamFilter, SqlParamKind, SqlParamOccurrence,
    SqlParamOccurrenceV2, SqlParamStableId, SqlParamSyntax, DEFAULT_ENABLE_AT,
    DEFAULT_ENABLE_DOLLAR_NAMED, DEFAULT_ENABLE_DOLLAR_POSITIONAL, DEFAULT_ENABLE_QUESTION,
    DEFAULT_ENABLE_TEMPLATE,
};
pub use sql_completion_context::{
    contextual_keyword_completion, contextual_schema_completion, detect_alias_dot_context,
    extract_qualifier_parts, filter_completions_by_kind, filter_keywords_by_kind,
    infer_sql_completion_kind, is_table_context, last_bare_sql_keyword,
    last_bare_sql_keyword_with_offset, CompletionContextInput, CompletionOption, CompletionOutcome,
    CompletionResult, CompletionSourceFn, SqlCompletionKind, COLUMN_CONTEXT_ALLOWED_KEYWORDS,
    TABLE_CONTEXT_ALLOWED_KEYWORDS,
};
pub use sql_completions::{
    keyword_completions, sql_function_completions, SqlFunctionCompletion,
    DEFAULT_SQL_COMPLETION_DIALECT, SQL_COMPLETION_KEYWORDS,
};
pub use sql_editor_defaults::{
    infer_default_schema, infer_default_table, table_clause_fingerprint, tables_referenced_in_sql,
    ReferencedRelation, RelationSourceKind, ScopeRelations, StatementRange,
};
pub use sql_file_execution::{
    execution_error_message, sql_file_execution_plan, ConfirmationMode, ExecutionFailure,
    InvokeOutcome, SqlFileExecutionOutcome, SqlFileExecutionPlan, OVERWRITE_OPTION,
    RESTORE_PROGRESS_EVENT, RESTORE_SQL_FILE_COMMAND, SQL_FILE_EXECUTION_GAP,
};
pub use sql_format::{
    default_sql_format_options, format_sql, sql_format_indentation, sql_format_language,
    SqlDialectLookup, SqlFormatConfig, SqlFormatRequest, SqlFormatter, SqlIndent,
    DEFAULT_BREAK_BEFORE_BOOLEAN_OPERATORS, DEFAULT_INDENT_STYLE, DEFAULT_KEYWORD_CASE,
    DEFAULT_LINES_BETWEEN_QUERIES, LANGUAGE_MAP,
};
pub use sql_function_registry::{
    build_function_completions, common_functions, get_driver_functions, get_function_entries,
    resolve_dialect_family, to_sql_function_completions, DriverFunctionMeta,
    DriverFunctionRegistry, FunctionCompletion, FunctionEntry, FunctionParam,
    COMMON_FUNCTION_NAMES, DIALECT_ALIASES,
};
pub use sql_generator::{
    format_table_identifier, generate_table_sql, BaseTableSqlGenerator, GeneratedSqlType,
    TableSqlDialect,
};
pub use sql_namespace_ops::{
    namespace_branch_child_names, namespace_child_names,
    namespace_has_child as namespace_tree_has_child, overlay_column_map_owned,
};
pub use sync_taxonomy::normalize_driver_id;
pub use theme_editor_colors::{
    apply_sql_syntax_preset, build_editor_highlight_rules, default_editor_colors,
    editor_colors_from_json, editor_colors_with_pack_overlay, find_sql_property_names,
    is_sql_syntax_theme_id, parse_pack_editor_overlay, read_editor_colors,
    set_pack_editor_color_overlay, EditorColorContract, EditorColors, ExtendedHighlightColors,
    HighlightRule, Palette, SqlSyntaxPreset, StyleColors, SyntaxNodeSpan, CM_VARS,
    CODEMIRROR_COLOR_GAP, CODEMIRROR_HIGHLIGHT_GAP, SQL_SYNTAX_PRESETS,
};
pub use tunnel_draft::{
    copy_tunnel_name, draft_to_saved_tunnel, empty_tunnel_draft, is_positive_port,
    is_tunnel_draft_valid, positive_port, saved_tunnel_to_draft, timeout_seconds,
    tunnel_draft_problem, HttpProxyDraft, SshDraft, SshJumpDraft, TunnelDraft, TunnelDraftProblem,
    WebSocketDraft, DEFAULT_HTTP_PROXY_PORT, DEFAULT_SSH_PORT, DEFAULT_TUNNEL_TIMEOUT,
    SAVED_TUNNEL_KINDS,
};
pub use value_search::{
    base_cell_types, cell_types_for, filter_operator_for_mode, locate_conditions,
    locate_filter_logic, rank_tables, searchable_columns, target_key, type_scopes_for_driver,
    value_search_targets, value_search_type_groups_for, Clock, DriverExtraTypes,
    ExtraTypesProvider, SchemaReader, SearchTableValuesRequest, SearchTableValuesResponse,
    TableValueSearcher, ValueSearchDriver, ValueSearchDriverParams, ValueSearchLocateIntent,
    ValueSearchMode, ValueSearchProgress, ValueSearchStatus, ValueSearchTableResult,
    ValueSearchTarget,
};
// Duplicate grid/selection type names already re-exported by the parent's
// grid_keyboard / table_changes facades are intentionally omitted here.
pub use column_layout_prefs::{
    column_layout_storage_key, persist_column_order, persist_column_widths,
    read_persisted_column_layout, valid_order, valid_widths, ColumnLayoutStorage,
    LayoutTableContext, PersistedColumnLayout, COLUMN_LAYOUT_GAP, COLUMN_LAYOUT_STORAGE_PREFIX,
};
pub use schema_column_loading::{
    build_ensure_plan, column, invalidate_plan, load_relation_columns_step,
    project_relation_columns, read_bucket, read_identity, relation_batch_chunks,
    relation_columns_cache_key, resolve_column_relations, BucketPlan, ConnectionSchemaState,
    EnsurePlan, ReadColumnsRow, RelationColumn, RelationColumns, RelationRef, SchemaItem,
    SessionMetadataIdentity, DEFAULT_BATCH_DEBOUNCE_MS, RELATION_COLUMN_BATCH_SIZE,
    SCHEMA_COLUMN_LOADING_GAP, SCHEMA_COLUMN_LOADING_SURFACE,
};
pub use schema_store::{
    apply_patch, bind_metadata_identity, cache_path_items, collect_table_leaf_names,
    commit_connection_patch, compute_is_multi_database, connection_field,
    create_empty_connection_schema, empty_snapshot_field, is_schema_grouping_schema,
    is_system_database_name, known_table_names, latest_identity, load_databases_decision,
    merge_namespace, merge_namespace_path, namespace_tree_of, omit_table_leaf,
    parse_path_hierarchy_database_entry, patch_connection_schema, record_session_error,
    register_path_aliases, remove_connection, remove_relation, reset_store,
    resolve_preferred_database, resolve_visible_databases, schema_of_relation,
    set_active_connection, set_current_database, set_current_schema, set_loaded_tables,
    set_selected, toggle_expand, LoadDatabasesDecision, LoadForConnectionOptions,
    NamespaceMergeKind, PathDatabaseEntry, SchemaPatch, SchemaSessionState, SchemaStoreSnapshot,
    SchemaTableInfo, SetLoadedTablesOptions, VisibleDatabases, EMPTY_NAMESPACE, SCHEMA_STORE_GAP,
    SCHEMA_STORE_HELPERS_SURFACE, SCHEMA_STORE_STATE_SURFACE, SCHEMA_STORE_SURFACE,
};
pub use schema_store_selectors::{
    active_session_metadata_snapshot, binding_to_relation_requests, editor_relation_key,
    ensure_metadata_relations, get_metadata_relation, read_metadata_snapshot, relation_cache_key,
    relation_lookup_key, resolve_editor_dialect_id, snapshot_key, DialectRegistryLookup,
    EditorMetadataCacheLike, EditorRelationKey, EditorRelationRequest, IdentifierFolder,
    QualifiedRelationId, RelationBindingLike, SqlIdentifierSegment, SqlRelationSourceKind,
    SCHEMA_STORE_SELECTORS_GAP, SCHEMA_STORE_SELECTORS_SURFACE,
};
pub use selection_actions::{
    apply_begin_cell_selection, apply_clear_grid_selection, apply_grid_selection,
    apply_reconcile_selection, apply_row_mode_transition, apply_row_select,
    apply_toggle_select_all, displayed_column_names, normalize_cell_range, panel_column_order,
    reconcile_grid_selection, resolve_selection_patch, row_index_selection_reset, CellCoord,
    CellRange, GridSelection, GridSelectionMode, SelectionAction, SelectionOpts, SelectionPatch,
    TableStateForSelection,
};
pub use selection_scope::{
    default_row_selected, scope_for_operation, scope_selects_operation, selected_row_token,
    DataSyncSelectedRow, DataSyncSelectionScopeMode, DataSyncTableSelection,
    SELECTED_ROW_TOKEN_SEPARATOR, SELECTION_SCOPE_GAP, SELECTION_SCOPE_SURFACE,
};
pub use settings_store::{
    authoritative_read, is_unknown_version_value_presentation, load_fence, load_fence_current,
    next_value_presentation_revision, normalize_loaded_settings, normalize_settings_language,
    parse_stored_value_presentation, persist_merge, resolve_ui_language, settings_changed,
    theme_recovery, update_settings_merge, value_presentation_fresh, LoadRecovery,
    SETTINGS_CHANGED_EVENT, SETTINGS_STORE_GAP, SETTINGS_STORE_SURFACE, SETTINGS_THEME_STORAGE_KEY,
    THEME_CHANGED_EVENT, VALUE_PRESENTATION_VERSION,
};
pub use sql_path_prefix::{
    extract_qualified_token, is_inside_quotes, parse_qualified_path_parents,
    resolve_ensure_segments, QualifiedToken, ResolveEnsureOptions,
};
pub use sql_schema_drop_targets::{parse_schema_drop_targets, SqlSchemaDropTarget};
pub use sql_statement_range::{get_statement_at_cursor, StatementAtCursorFn, STATEMENT_RANGE_GAP};
pub use sql_transaction_guard::{
    analyze_transaction_sql, is_aborted_transaction_error, strip_sql_noise, TransactionSqlAnalysis,
};
pub use stage_cell_value::{
    compute_stage_cell_value_result, compute_stage_expression_value_result, compute_stage_result,
    has_pending_column_change, restored_cell_value, PendingRowChange, RowIdentity,
    StageCellValueInput, StageCellValueResult, StageEditInput, StageEditResult,
    StageExpressionValueInput, ValueMap, WriteExpressionId, STAGE_CELL_VALUE_GAP,
    STAGE_CELL_VALUE_SURFACE,
};
pub use stage_edit_actions::{
    compute_unstage_delete_result, STAGE_EDIT_ACTIONS_GAP, STAGE_EDIT_ACTIONS_SURFACE,
};
pub use stage_paste_writes::{
    build_row_identity, effective_pending_identity, find_pending_for_row,
    has_pending_identity_collision, row_identity_is_unique, row_identity_key,
    stage_paste_writes_into, CellPasteWrite, GridErrorCode, PasteCellProblem,
    StagePasteWritesOutcome, StagedTableState, STAGE_PASTE_WRITES_GAP,
};
pub use startup_timer::{
    format_wall_clock, StartupMark, StartupTimer, WallClockParts, STARTUP_TIMER_GAP,
};
pub use surface_bg_cache::{
    css_color_to_hex, is_safe_css_hex, persist_surface_background, DEFAULT_SURFACE_DARK,
    DEFAULT_SURFACE_LIGHT, SURFACE_BG_CACHE_GAP,
};
pub use sync_pairing::{
    compute_sync_pairing_state, normalize_sync_family, sync_category, sync_path_from_str,
    view_to_result, PairingView, SyncCategory, SyncConnection, SyncPairingCache, SyncPairingResult,
    SyncPairingState, SyncPath, SyncRegistryMeta, SYNC_PAIRING_CACHE_DELIMITER,
};
pub use table_changes::{
    clone_pending_row_change, draft_write_columns, duplicate_row_identity_keys,
    is_complete_table_change_context, normalize_edit_value, stable_serialize,
    table_change_context_key, values_equal, ChangeSeverity, ChangeWarning, ColumnValues,
    CommitOperation, CommitPendingChangesResponse, CommitPendingChangesResult,
    CommitStatementResult, CommitStatus, InsertLocation, PendingInsert, PlannedStatement,
    RefreshStatus, RowChangePlan, TableChangeContext as TableChangesContext,
};
pub use table_list_panel::{
    build_table_list_view, display_table_name, filter_label_key, filter_table_rows, filter_test_id,
    row_diff_counts, table_has_row_diffs, table_is_unchanged, table_key, FilterButtonView,
    RowDiffCounts, TableDiffFilter, TableItemBadge, TableListItemView, TableListRow, TableListView,
    TableSyncStatus, DATA_SYNC_FILTERS, IDENTICAL_KEY, MAPPING_INCOMPATIBLE_KEY,
    NO_TABLES_MATCH_KEY, ROW_DIFFS_KEY, SEARCH_PLACEHOLDER_KEY, TABLE_ITEM_TEST_ID,
    TABLE_LIST_PANEL_GAP, TABLE_LIST_PANEL_SURFACE,
};
pub use table_schema_for_sql::{
    build_pseudo_table_schema, fetch_table_schema_for_sql_generation,
    generate_table_sql_with_fallbacks, FetchTableSchemaInput, GenerateFallbackOptions,
    ReadCachedSchema, ReadColumnsValue, ReadRelationColumns,
};
pub use table_sql_actions::{
    build_no_primary_key_edit_template, build_query_open_context, build_safe_table_sql_template,
    build_table_context, build_table_sql_action, build_table_sql_actions, no_primary_key_edit_spec,
    quote_table_identifier, QueryOpenContext, QueryOpenFocus, QueryOpenSource, SqlActionExecution,
    TableActionInput, TableContext, TableSqlAction, TableSqlActionSpec, TABLE_SQL_ACTION_KINDS,
};
pub use tabs::{Tab, TabKind, Tabs};
pub use theme_pack_apply::{
    classify_theme_pack, encode_wapp_theme_pack_id, invalid_wapp_theme_id_error,
    join_relative_path, mime_for_path, parse_charts_json, parse_wapp_theme_pack_id,
    plan_theme_pack_apply, probe_icon, rewrite_css_urls, unknown_theme_pack_error,
    ThemePackApplyPlan, ThemePackApplyResult, ThemePackSelection, THEME_PACK_APPLY_GAP,
    THEME_PACK_CHANGED_EVENT, THEME_PACK_STYLE_ID, WAPP_THEME_PACK_PREFIX,
};
pub use transfer_limitations_prefs::{
    clear_transfer_limitations_dismissed, create_transfer_limitations_dismiss_prefs,
    is_transfer_limitations_dismissed, set_transfer_limitations_dismissed,
    TRANSFER_LIMITATIONS_DISMISSED_KEY,
};
pub use transfer_pairing::{
    is_transfer_target_supported, resolve_transfer_pairing, TransferPairingResult, TransferPath,
    TransferRegistry,
};
pub use tunnel_deletion::{
    delete_tunnel_and_unbind, unbind_connection_patch, DeleteTunnel, DeleteTunnelDeps,
    DeleteTunnelOutcome, GetConnections, SaveConnection, KEY_DELETE_FAILED, KEY_UNBIND_FAILED,
};
pub use update_reminder_schedule::{
    dispatch_event, on_interval_tick, start_update_reminder_schedule,
    stop_update_reminder_schedule, ScheduleDeps, ScheduleHandle, CHANNEL_AUTO, CHECK_INTERVAL_MS,
    EVENT_FOCUS, EVENT_ONLINE, EVENT_VISIBILITY_CHANGE, VISIBILITY_VISIBLE,
};
pub use updater::{
    check_for_updates, current_variant, download_and_install_update, get_update_channel,
    has_updater_channel, is_updater_supported, manifest_belongs_to_build, maybe_check_on_startup,
    normalize_sku, read_manifest_variant, relaunch_to_apply_update, StartupCheckPlan,
    UpdateChannel, UpdaterBuildConfig, UpdaterLog, UpdaterLogLevel,
};
pub use wapp_asset_origin::{
    build_wapp_asset_src, build_wapp_icon_src, configure_wapp_asset_resolver,
    is_safe_wapp_asset_path, is_valid_wapp_id, parse_wapp_asset_target, reset_wapp_asset_resolver,
    WappAssetQueryMap, WappAssetResolver, WappAssetTarget, DESKTOP_WAPP_ASSET_RESOLVER,
    WAPP_ASSET_CSP, WAPP_ASSET_PATH_PREFIX, WEB_WAPP_ASSET_RESOLVER,
};
pub use window_kind::{
    get_url_param, get_window_kind, reset_window_kind_cache, resolve_window_kind, WindowKind,
    LEGACY_MAIN_ALIASES,
};
pub use window_manager::{
    backup_window_options, connection_payload, connection_window_plan, dashboard_window_plan,
    data_sync_window_options, data_transfer_window_options, docs_window_plan,
    emit_menu_action_plan, is_main_shell_menu_id, schema_diff_window_options, settings_window_plan,
    workflow_window_plan, BackupMode, BackupPrefill, ConnectionWindowOpts, ConnectionWindowPlan,
    DashboardWindowPlan, DocsWindowPlan, MenuActionPlan, PostOnboardingSample, SingletonWindowPlan,
    MAIN_SHELL_MENU_IDS, PENDING_CONNECTION_KEY, POST_ONBOARDING_SAMPLE_KEY,
    WINDOW_CAPABILITY_LABEL_SAMPLES, WINDOW_MANAGER_GAP,
};
pub use workflow_panel::{
    connection_options as panel_connection_options, error_text, feedback_tone,
    is_connection_variable, is_loading, variable_defaults, variable_input, ConfirmKind,
    ConnectionSummary, ExecuteParams, FeedbackTone, PanelTab, VariableInput, WorkflowPanel,
    WorkflowPanelEffect, WorkflowPanelHost, WorkflowPanelLabels, WorkflowPanelOptions,
    WorkflowPanelState, WorkflowSelectOption, WorkflowStoreView, WORKFLOW_PANEL_GAP,
    WORKFLOW_PANEL_SURFACE,
};
// extract_workflow_yaml / WorkflowValidationConnection are already re-exported
// by the parent's ai-chat facade; only seg3's unique yaml helpers are listed.
pub use table_data_state::{
    AppliedFilter, CellEditLocation, DefaultOrderOverride, FilterLogic, FilterMode, TableState,
    TABLE_DATA_STATE_GAP, TABLE_DATA_STATE_SURFACE,
};
pub use table_data_store::{
    add_filter, add_sort, apply_any_column, apply_filters, apply_filters_decision, apply_raw_where,
    begin_load, cancel_edit, clear_filters, clear_highlighted_new_row, clear_pending_state,
    clear_sorts, commit_fetched_page, duplicate_identity_keys, filter_draft_equals_applied,
    invalidate_cached_data, is_complete_filter, load_failed, order_request, overlay_pending_rows,
    patch_panel, patch_panel_for_reload, remove_filter, remove_panel_state, remove_sort,
    resolve_default_table_sort, rows_to_records, sanitize_visibility, set_any_column_draft,
    set_column_order, set_default_order_override, set_detail_row, set_filter_logic,
    set_filter_mode, set_filter_panel_open, set_filters, set_frozen_column_count, set_page,
    set_page_size, set_panel_state, set_pending_status, set_raw_where_draft, set_sort,
    set_visible_columns, start_edit, target_of, update_filter, upsert_sort, FetchedPage,
    LoadTableDataParams, OrderRequest, PageCommit, PendingStatus, ReloadRequest, TableContextData,
    TableDataStoreSnapshot, TableTargetData, AMBIGUOUS_ROW_IDENTITY_ERROR, TABLE_DATA_STORE_GAP,
    TABLE_DATA_STORE_SURFACE,
};
pub use table_sort_preferences::{
    is_sort_condition, persist_table_sorts, read_persisted_table_sorts, table_sort_storage_key,
    TableSortContext, TABLE_SORT_PREFS_GAP, TABLE_SORT_PREFS_SURFACE, TABLE_SORT_STORAGE_PREFIX,
};
pub use theme_tokens::{
    build_theme_snapshot, ThemeSnapshot, ThemeSnapshotInput, THEME_SNAPSHOT_VERSION, THEME_TOKENS,
    THEME_TOKENS_DOM_GAP,
};
pub use total_rows_display::{can_page_forward, format_row_count, total_rows_view, TotalRowsView};
pub use update_reminder_store::{
    begin_check, begin_install, finish_check, install_result_patch, install_start_patch,
    is_check_due, progress_patch, restart_allowed, restart_error_patch, StatePatch,
    UpdateCheckResult, UpdateOutcome, UpdateProgress, UpdateReminderState,
    UPDATE_CHECK_INTERVAL_MS, UPDATE_REMINDER_GAP, UPDATE_REMINDER_SURFACE,
};
pub use virtual_body::{
    build_cell_input, build_column_editor_info_from_defs, build_row_cell_views, cell_id,
    column_names, container_height, dirty_cell_aria_label, dirty_cell_tooltip, dirty_cells,
    display_rows, editor_info, error_columns_by_row, error_key, fk_affordances,
    fk_navigable_by_row, format_dirty_expression, format_dirty_value, frozen_lefts,
    gutter_click_action, is_fk_cell_navigable, is_integer_cell_type, modifiers_from_flags,
    row_click_action, row_deleted, row_dirty, row_double_click_action, row_key_down_action,
    row_kind, row_state, row_tint_class, sticky_info, virtual_table, CellEditingData,
    CellRowContext, CellTypeHintsData, ColumnDefData, ColumnEditorInfoData, DirtyCellData,
    DirtyRowEntryData, DirtyRowState, DisplayRow, DraftRowViewModel, FkGroupAffordanceData,
    FkNavigationData, GutterClickAction, PresentationPlan, RowClickAction, RowKeyAction,
    RowModifiers, VirtualBodyConfig, VirtualRowData, VirtualTableResult, DEFAULT_COLUMN_WIDTH,
    GUTTER_WIDTH, OVERSCAN, VIRTUAL_BODY_GAP,
};
pub use wapp_bridge::{
    as_nonempty_string, err_envelope, host_ready_envelope, is_request_envelope,
    is_wapp_command_allowed, looks_like_not_found, ok_envelope, parse_command_invoke,
    parse_i18n_key, parse_notify, parse_request, parse_storage, response_type_of, route_for,
    theme_apply_envelope, theme_snapshot_value, timeout_disposition, to_public_connection,
    AttachBridgeOptions, BridgeApiError, BridgeConnectionSummary, BridgeEffect, BridgeErrorCode,
    BridgeErrorPayload, BridgeState, BridgeTarget, CommandInvokeInput, ConnectionConfigLike,
    NotifyInput, StorageInput, TimeoutDisposition, WappBridge, WappBridgeHost, WappErrEnvelope,
    WappOkEnvelope, WappRequestEnvelope, WappResponseEnvelope, API_ROUTES, BRIDGE_CHANNEL,
    BRIDGE_ERROR_CODES, ERROR_MESSAGE_LIMIT_CHARS, MAX_INFLIGHT_REQUESTS, MAX_REQUEST_ID_CHARS,
    NOTIFY_MIN_INTERVAL_MS, REQUEST_TIMEOUT_MS, SESSION_REQUEST_LIMIT, WAPP_BRIDGE_DOM_GAP,
    WAPP_BRIDGE_EVENT_GAP, WAPP_BRIDGE_HANDLER_GAP, WAPP_BRIDGE_IPC_GAP, WAPP_BRIDGE_SURFACE,
    WAPP_COMMAND_DENYLIST,
};
pub use wapp_i18n::{
    is_wapp_i18n_key_valid, is_wapp_i18n_locale_valid, resolve_wapp_string, wapp_i18n_candidates,
    wapp_i18n_dictionary_path, WappDictionary, WappI18n, WappI18nDeps, WAPP_I18N_GAP,
    WAPP_I18N_SURFACE,
};
pub use wapp_store::{
    by_id, fetch_patch, listener_decision, optimistic_toggle, remove_effects, set_enabled_effects,
    should_refetch_on_event, to_error_message, FetchPatch, ListenerDecision, ListenerResult,
    MutationEffects, WappError, WappStoreState, WAPPS_CHANGED_EVENT, WAPP_STORE_GAP,
    WAPP_STORE_SURFACE,
};
pub use workflow_draft_yaml::{
    draft_to_yaml_object, draft_to_yaml_value, yaml_object_to_draft, yaml_value_to_draft,
    WorkflowDraft, WorkflowDraftVariable, WorkflowStepDraft, WORKFLOW_DRAFT_YAML_GAP,
    WORKFLOW_DRAFT_YAML_SURFACE,
};
pub use workflow_list_context_menu::{
    build_workflow_history_context_menu_items, build_workflow_list_context_menu_items,
    BuildWorkflowHistoryContextMenuArgs, BuildWorkflowListContextMenuArgs,
    WorkflowHistoryContextMenuHandlers, WorkflowHistoryContextMenuLabels,
    WorkflowListContextMenuHandlers, WorkflowListContextMenuLabels, WorkflowMenuItem,
    WORKFLOW_LIST_MENU_GAP, WORKFLOW_LIST_MENU_SURFACE,
};
pub use workflow_yaml::{
    looks_like_workflow, parse_validated_workflow_definition, parse_workflow_blocks,
    parse_workflow_yaml, validate_workflow_fields,
};
// The T211 types aggregate: the members React's `types/index.ts` declares or
// re-exports that no earlier family module had already landed at the root
// (every already-rooted sibling name — the connection/context/filter/
// keyset/ai blocks above — the aggregate re-exports from `types` without
// redefining, so they appear here exactly once; `ResultProvenance` and
// `SessionHandle` root via the `metadata` block above, so the aggregate
// re-exports them from `types::metadata` without a second root import). The backend-client legs
// React imports without re-exporting (`UserPreferences`,
// `ExecutionSettings`, `StoredValuePresentationSettings`) stay reachable
// only inside `types::client_settings`/`types::value_presentation` — the
// aggregate surface mirrors React's own import-only reachability.
pub use connection_shared::SavedConnection;
pub use content_view_helpers::{
    get_panel_icon, get_panel_label, get_panel_short_label, get_panel_tab_label, get_sub_tabs,
    get_view_sub_tabs, resolve_connection_context, resolve_connection_context_by_connection,
    resolve_panel_tab_database, ContentViewConnectionContext, ContentViewPanel, PanelIcon, SubTab,
    CONTENT_VIEW_HELPERS_GAP, CONTENT_VIEW_HELPERS_SURFACE, ICON_CLASS,
};
pub use open_history_query::{
    open_history_query, ActiveConnection, HistoryQueryEntry, OpenHistoryOutcome,
    OpenHistoryQueryHost, OpenHistoryQueryOptions, PathHierarchySplit, QueryPanelState,
    OPEN_HISTORY_QUERY_GAP, OPEN_HISTORY_QUERY_SURFACE,
};
pub use query_contracts::{
    build_query_panel_diagnosis_context, has_suspicious_postgres_double_quoted_literal,
    read_current_query_panel_retry_validation_input, ActiveConnectionView, DiagnosisContext,
    DiagnosisContextInput, ExecuteKind, PanelView, PendingExecute, QueryContractsHost,
    QueryDiagnosisExecution, QueryDiagnosisSchemaState, QueryExecutionGateRefs,
    QueryPanelCallbacks, QueryPanelDiagnosisContextInput, QueryPanelProps, SchemaContext,
    QUERY_CONTRACTS_GAP, QUERY_CONTRACTS_SURFACE,
};
pub use query_drop_handler::{
    create_query_drop_handler, detect_drop_context, ChartConfigLike, DashboardWidgetRequest,
    DropEffect, DropOptions, DroppedTable, ExplainResultLike, QueryPanelWorkflows,
    RetryValidationInputLike, WorkflowEffect, WorkflowHost, WorkflowOptions, WorkflowState,
    QUERY_DROP_HANDLER_GAP, QUERY_DROP_HANDLER_SURFACE,
};
pub use query_execution_target::{
    effective_strategy, estimate_query_result_pane_height, offset_to_line,
    query_result_pane_sizing, resolve_execution_target, statement_target_info, strategy_as_str,
    view_is_chart, ExecutionTargetInput, QueryResultPaneHeightInput, ResolveExecutionTargetResult,
    ResultPaneSizing, StatementTargetInfo, QUERY_EXECUTION_TARGET_GAP,
    QUERY_EXECUTION_TARGET_SURFACE, RESULT_PANE_MIN_HEIGHT,
};
pub use result_workspace::{
    can_render_result_chart, get_top_level_sql_verb, infer_field_type, is_chartable_result,
    is_mutation_execution, resolve_result_workspace_view, strip_leading_sql_comments, ResultValue,
    ResultWorkspaceFallbackReason, ResultWorkspaceView, ResultWorkspaceViewResolution,
    StatementResultView, MUTATION_VERBS, QUERY_VERBS, RESULT_WORKSPACE_GAP,
    RESULT_WORKSPACE_SURFACE,
};
pub use tunnel_store::{
    begin_load as begin_tunnel_load, error_message, new_tunnel_id, plan_create, plan_remove,
    plan_update, plan_usage, settle_load, usage_from_lists, TunnelCreatePlan, TunnelInput,
    TunnelStoreEffect, TunnelStoreState, TUNNEL_STORE_GAP, TUNNEL_STORE_SURFACE,
};
pub use types::{
    is_valid_mcp_server_id, AppSettings, ChangedColumnDiff, CheckConstraintDiffEntry,
    ColumnDiffEntry, ColumnInfo, CommandAccessLevel, CommandCategory, Counter, DefaultTableSort,
    DriverCommandDefinition, DriverCommandMetadata, DriverSaveDialogSpec, ErrorHandlingConfig,
    ErrorHandlingStrategy, FavoriteQuery, GridDensity, HistoryEntry, HistoryListItem, Id,
    KeyDetail, KeyEntry, KeyScanResult, LogLevel, McpApprovalDecision, McpApprovalRequest,
    McpAuditDecision, McpAuditRecord, McpHttpStatus, McpHttpTransport, McpPermissionMode,
    MergeSource, MultiQueryResult, OnboardingState, QueryResult, QueryStreamEvent,
    ResultCompleteness, RuntimeResultBinding, SessionContext, SessionContextConfidence,
    SortCondition, SqlExecutionStrategy, SqlFormatOptions, StatementResult, SyncObjectKind,
    TableCompareStatus, TableComparison, TableDataResult, TableOptionsDiff, TableOptionsSnapshot,
    TableSchemaDiff, Timestamp, TransactionState, TransformColumn, TunnelSource, TunnelUsage,
    WorkflowBlock, WorkflowBlockRetry, WorkflowBlockType, WorkflowDefinition,
    WorkflowExecuteRequest, WorkflowIdRequest, WorkflowOutput, WorkflowRunGetRequest,
    WorkflowRunListItem, WorkflowRunRecord, WorkflowRunsListRequest, WorkflowSaveRequest,
    WorkflowSchedule, WorkflowStep, WorkflowStepType, WorkflowVisibility, WorkflowYamlSaveRequest,
    WritableMapping,
};
// `resolve_connection_context` takes content_view_helpers' structurally-identical
// ActiveConnectionEntry, while the root `ActiveConnectionEntry` name is held by
// ai_sessions. Expose the content-view one under an explicit alias so callers
// can build the map the resolver expects without colliding at the root.
pub use connection_page_utils::{
    consume_pending_connection, enrich_open_connection_payload, make_tab_from_payload,
    pending_open_query_sql, remove_connection_from_stores, sync_stores_active_connection,
    ConnectionLookup, ConnectionStatus, ConnectionStoreAction, ConnectionTab, ConsumedConnection,
    MainView, PendingOpenQuery, WorkspaceMode, CONNECTION_PAGE_UTILS_GAP,
    CONNECTION_PAGE_UTILS_SURFACE, DEFAULT_DATABASE_TYPE,
};
pub use content_view_helpers::ActiveConnectionEntry as ContentViewActiveConnectionEntry;
pub use expanded_db_cache_refresh::{
    schema_fingerprint, ConnectionRef, ExpandedDbCacheRefresh, RefreshDeps, RefreshEffect,
    RefreshInput, SchemaSnapshot, EXPANDED_DB_CACHE_REFRESH_GAP, EXPANDED_DB_CACHE_REFRESH_SURFACE,
};
pub use query_toolbar_width::{
    path_hierarchy_label_width, query_context_select_width, query_toolbar_expanded_min_width,
    PathSelectorSegment, SelectorSegmentKind, ToolbarWidthOptions, PATH_HIERARCHY_LABEL_MAX,
    PATH_HIERARCHY_SEGMENT_GAP, PATH_HIERARCHY_SELECT_CHROME, PATH_HIERARCHY_SELECT_MAX,
    PATH_HIERARCHY_SELECT_MIN, QUERY_TOOLBAR_ICON_BUTTON_WIDTH, QUERY_TOOLBAR_STATUS_WIDTH,
    QUERY_TOOLBAR_WIDTH_GAP, QUERY_TOOLBAR_WIDTH_SURFACE, TOOLBAR_GAP, TOOLBAR_HORIZONTAL_PADDING,
};
pub use result_workspace_index::{RESULT_WORKSPACE_INDEX_GAP, RESULT_WORKSPACE_INDEX_SURFACE};
pub use schema_tree::{
    build_legacy_table_payload, build_schema_object_payload, get_categories_for_driver,
    leaf_kind_icon, object_kind_category, set_drag_payload, DragMimeEntry, DragMimeValue,
    DragPayloadOptions, DragPayloadPlan, LegacyTableDragPayload, LegacyTableEntry,
    SchemaObjectDragKind, SchemaObjectDragPayloadV1, SchemaObjectNamespace, BASE_CATEGORIES,
    LEAF_KIND_ICONS, LEGACY_TABLE_MIME, OBJECT_KIND_CATEGORIES, SCHEMA_OBJECT_MIME,
    SCHEMA_PAYLOAD_VERSION, SCHEMA_TREE_GAP, SCHEMA_TREE_SURFACE, TEXT_PLAIN_MIME,
};
// (all names already rooted above)
pub use ai_draft_bridge::{
    AiChatDraftRequest, AiChatDraftSource, ContentViewCallbacks, OpenRelationSubTab,
    AI_CHAT_DRAFT_FOCUS, AI_DRAFT_BRIDGE_GAP, AI_DRAFT_BRIDGE_SURFACE,
};
pub use query_error_chat_prompt::{
    build_query_error_chat_prompt, send_query_error_chat_draft, QueryErrorChatContext,
    QueryErrorDraftInput, MAX_PROMPT_ERROR_LENGTH, MAX_PROMPT_SQL_LENGTH,
    QUERY_ERROR_CHAT_PROMPT_GAP, QUERY_ERROR_CHAT_PROMPT_SURFACE,
};
pub use query_execution_risk::{
    assess_execution_risk, host_guard_would_block, ExecutionRiskDecision, ExecutionRiskMode,
    HardBlockReason, HostGuardMode, QUERY_EXECUTION_RISK_GAP,
};
pub use use_query_builder_contribution::{
    client_snapshot, get_closed_snapshot, subscribe_falls_back_to_noop, QueryBuilderContribution,
    QueryBuilderRunState, USE_QUERY_BUILDER_CONTRIBUTION_GAP,
    USE_QUERY_BUILDER_CONTRIBUTION_SURFACE,
};
pub use validate_bind_params::{
    find_missing_params, MissingParam, VALIDATE_BIND_PARAMS_GAP, VALIDATE_BIND_PARAMS_SURFACE,
};
pub use workspace_mode_sidebar::{
    build_sidebar_view, build_update_view, dispatch_nav_action, progress_label, progress_ring,
    resolve_confirm_result, resolve_restart_prompt, update_label, NavAction, NavButton,
    NavButtonId, ProgressRing, RestartPrompt, SidebarView, UpdateAction, UpdateButtonView,
    UpdateIconKind, UpdateProgressPhase, UpdateState, BUTTON_ACTIVE, BUTTON_BASE, BUTTON_COLLAPSED,
    BUTTON_EXPANDED, BUTTON_INACTIVE, SIDEBAR_BASE, WORKSPACE_MODE_SIDEBAR_GAP,
    WORKSPACE_MODE_SIDEBAR_SURFACE,
};

/// One foreign key: this table's columns (a compound key stays together), the
/// target table and its columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignKeySpec {
    pub name: String,
    pub columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
}

/// One column pairing of an inbound foreign key (T5): another table's
/// column that references this table's column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboundRef {
    /// The referencing table.
    pub table: String,
    /// The referencing table's column that carries the reference.
    pub column: String,
    /// This table's column being referenced, paired with `column`.
    pub referenced_column: String,
    /// The constraint's name, as the structure view shows it.
    pub constraint: String,
}

/// One saved favorite query (T6): the desktop's `.sql`-file favorite as the
/// UI sees it. The id is the file name's stem — a ULID the host mints, a
/// rename changes the title, never the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteSpec {
    pub id: String,
    /// Owning persisted connection id ("" for an unsaved editor).
    pub connection: String,
    pub title: String,
    pub sql: String,
}

/// One parsed connection from an import file (T7), as the preview list shows
/// it. The full config stays in the host — the preview carries what the user
/// reads before confirming.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedConnectionSpec {
    pub name: String,
    pub engine: String,
    pub address: String,
}

/// A parsed import file (T7): the format it was detected as, the connections
/// it carries, what it skipped, and the handle the host stashed the parsed
/// configs under — the confirm asks by handle, so nothing is lost between
/// the preview and the save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportPreview {
    pub handle: u64,
    pub format: String,
    pub connections: Vec<ImportedConnectionSpec>,
    pub skipped: Vec<String>,
}

/// One index of a table: its name, its columns in index order, uniqueness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexSpec {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

/// One pending column change: add a column, or retype one. Drafts only —
/// nothing touches the table until the port applies the SQL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnDraft {
    AddColumn { name: String, sql_type: String },
    ChangeType { column: String, sql_type: String },
}

impl ColumnDraft {
    /// The ALTER TABLE statement this draft stands for. The preview shows it;
    /// nothing executes it but the port's apply.
    pub fn sql(&self, table: &str) -> String {
        match self {
            Self::AddColumn { name, sql_type } => {
                format!("ALTER TABLE {table} ADD COLUMN {name} {sql_type};")
            }
            Self::ChangeType { column, sql_type } => {
                format!("ALTER TABLE {table} ALTER COLUMN {column} TYPE {sql_type};")
            }
        }
    }
}

/// One column of a table: name, type, nullability. For lists, not editors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSpec {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
}

/// One table's structure, four reads in one answer (E1): the DDL, the
/// columns, the indexes and the foreign keys. The job twin of the four
/// synchronous catalog reads, fetched as one job so a remote driver costs
/// the frame loop one delivery, never four unbounded reads on it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TableStructure {
    /// The driver's own DDL render; empty is the DDL view's empty state.
    pub ddl: String,
    pub columns: Vec<ColumnSpec>,
    pub indexes: Vec<IndexSpec>,
    pub foreign_keys: Vec<ForeignKeySpec>,
}

/// The three name layers the port reports: databases, their schemas, and the
/// tables inside each schema. Orders are the port's and stay stable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    pub databases: Vec<DatabaseNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseNode {
    pub name: String,
    pub schemas: Vec<SchemaNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaNode {
    pub name: String,
    pub tables: Vec<String>,
    pub views: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub engine: String,
    /// Saved hostname or IP, if present. Not a DSN or tunnel address.
    pub host: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeQueryDatabase {
    pub connection_id: String,
    pub database: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryDatabaseContext {
    pub current: NativeQueryDatabase,
    pub choices: Vec<String>,
}

/// Safe connection test outcomes. Raw driver errors may include credentials
/// or connection strings, so they never travel in a form delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionProbeError {
    MissingAddress,
    InvalidAddress,
    Unavailable,
    TunnelUnavailable,
    AuthenticationFailed,
    CannotConnect,
    TimedOut,
}

/// The connection form's one save (E5): the fields both ports speak — the
/// memory port keeps its name/engine/address semantics and ignores the
/// credentials, the live host maps all six onto its `ConnectionConfig`
/// (the password lands sealed, never in the file's plaintext).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ConnectionDraft {
    pub name: String,
    pub engine: String,
    /// "host" or "host:port".
    pub address: String,
    pub username: String,
    pub password: String,
    pub database: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub connection_id: String,
    pub tables: Vec<String>,
}

/// One sort column: ascending first, `descending` flips it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSort {
    pub column: String,
    pub descending: bool,
}

/// One text filter: rows whose `column` contains `contains` stay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnFilter {
    pub column: String,
    pub contains: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePage {
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub page: u32,
    pub page_size: u32,
    /// Rows in the whole table, not only this page. Footers page over it.
    pub total_rows: u32,
    /// The sort this page was opened with, echoed back for display.
    pub sort: Option<ColumnSort>,
    /// The filter this page was opened with, echoed back for display.
    pub filter: Option<ColumnFilter>,
    /// The table's primary-key column names, in key order (N6). The memory
    /// port's pages carry none; the live host's reads carry the driver's
    /// answer, and the grid's commit batch addresses rows by them.
    pub primary_key: Vec<String>,
    /// Why the page cannot be edited; None means writable.
    pub read_only: Option<String>,
}

/// One job's progress, in one piece. The port hands these out when asked;
/// the UI never streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub job: u64,
    /// "running", "completed", "failed" or "stopped".
    pub status: String,
    pub percent: u32,
    pub log: Vec<String>,
}

/// One job the port runs (export or backup). The memory port advances a job
/// on every progress read; real drivers advance by themselves.
#[derive(Clone, Debug)]
struct MemoryJob {
    job: u64,
    format: String,
    range: String,
    reads: u32,
    stopped: bool,
    log: Vec<String>,
    /// The error line a failing job reports; None means the job succeeds.
    fail_line: Option<String>,
}

/// One step's run result: the step name, success, a one-line summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowStepResult {
    pub step: String,
    pub ok: bool,
    pub summary: String,
}

/// Native asynchronous delivery: retain the compact summaries for callers that
/// use them, and preserve the engine's typed result for the workflow workspace.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeWorkflowRun {
    pub summaries: Vec<WorkflowStepResult>,
    pub report: WorkflowExecutionResult,
}

/// One workflow: a name and its steps, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<String>,
}

/// The facade's display `DashboardWidget` (chart config, layout,
/// refresh, alert) lives at [`crate::dashboard_types::DashboardWidget`]:
/// this root name stays with the stub because `datazen-ui` consumes it
/// from the root in 16 files, and that crate is outside the T159 port's
/// reach — see the `dashboard_types` module docs for the ruling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DashboardWidget {
    pub id: u64,
    pub title: String,
    pub kind: String,
}

/// One object of a structure comparison: its name and whether the target
/// endpoint has an object of the same name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareObject {
    pub name: String,
    pub present_on_target: bool,
}

/// One saved SQL snippet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub name: String,
    pub sql: String,
}

/// One typed settings entry. A write validates a single entry; a failure
/// leaves every entry at its old value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Theme {
        dark: bool,
    },
    BuiltinLanguage {
        locale: BuiltinLanguage,
    },
    TablePageSize {
        rows: u32,
    },
    QueryMaxRows {
        rows: u32,
    },
    /// One layout size in integer points (L1): `key` names the resizable
    /// region (for example `navigator-width`), `value` its persisted size.
    /// Points, not floats: the entry stays `Eq` and the file stays readable,
    /// and every React-side layout number is an integer anyway.
    Layout {
        key: String,
        value: u32,
    },
}

/// Built-in locale supported by the native and shared Rune views.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuiltinLanguage {
    #[default]
    SimplifiedChinese,
    English,
}

impl BuiltinLanguage {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "zh-CN" => Some(Self::SimplifiedChinese),
            "en" => Some(Self::English),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::SimplifiedChinese => "zh-CN",
            Self::English => "en",
        }
    }
}

impl Setting {
    /// The product's validation rules, shared by both ports (L1): the same
    /// sentences the memory port has always answered, so a rejection reads
    /// the same wherever it surfaces.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Setting::Theme { .. } | Setting::BuiltinLanguage { .. } => Ok(()),
            Setting::TablePageSize { rows } => {
                if *rows == 0 || *rows > 500 {
                    return Err(format!("table page size must be 1..=500, got {rows}"));
                }
                Ok(())
            }
            Setting::QueryMaxRows { rows } => {
                if *rows == 0 || *rows > 10_000 {
                    return Err(format!("query max rows must be 1..=10000, got {rows}"));
                }
                Ok(())
            }
            Setting::Layout { key, value } => {
                if !valid_layout_key(key) {
                    return Err(format!(
                        "layout key must be lowercase letters, digits or hyphens, got {key:?}"
                    ));
                }
                if *value == 0 || *value > 10_000 {
                    return Err(format!("layout size must be 1..=10000, got {value}"));
                }
                Ok(())
            }
        }
    }
}

/// A layout key is a short lowercase identifier: it names one resizable
/// region and lands in the settings file verbatim.
fn valid_layout_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    key.len() <= 48
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Reads and writes settings, one entry at a time.
pub trait SettingsApi {
    fn read_settings(&self) -> Vec<Setting>;
    /// Validate and store one entry. An invalid value is rejected and changes
    /// nothing, including the other entries.
    fn write_setting(&mut self, setting: Setting) -> Result<(), String>;
}

/// Runs SQL against the workspace. The memory implementation answers a few
/// fixed statements; unknown SQL is an error that changes no state.
pub trait QueryApi {
    fn execute(&mut self, sql: &str) -> Result<TablePage, String>;
}

/// The fixed error an unwired desktop adapter answers with. The UI displays
/// this sentence verbatim next to the data it falls back to.
pub const DESKTOP_NOT_IMPLEMENTED: &str = "not implemented in the desktop adapter yet";

/// The desktop host's seam into the shared UI. The desktop process wires it
/// one use case at a time — H3 fills in `list_connections`, H4
/// `read_table_page`, H5 `commit_cells` — and until a method is wired it
/// answers with the fixed error, which the UI displays while the memory port
/// answers instead.
///
/// N1 adds the job twin of those use cases: work that can take real time (a
/// live driver IO) goes through [`DesktopAdapter::begin_job`] and comes back
/// through [`DesktopAdapter::poll_jobs`], so the UI's frame loop never
/// blocks. The synchronous methods stay the in-memory world's path; adapters
/// that answer every request quickly simply leave `begin_job` declined.
pub trait DesktopAdapter {
    /// The connection list from the desktop use case.
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String>;
    /// One page of a table from the desktop use case.
    fn read_table_page(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String>;
    /// Apply one batch of cell edits through the desktop use case and report
    /// how many landed. One edit is (row, column name, new value).
    fn commit_cells(&mut self, table: &str, edits: &[(u32, String, String)])
        -> Result<u32, String>;

    /// The unified unbound driver-command entry (the frontend's
    /// `execute_driver_command(driverType, …)` IPC): runs a driver command
    /// that needs no connection session (ADB helpers over `sqlite`, and any
    /// future driver command the commands layer routes the same way). The
    /// default answers the fixed error; adapters with a live driver registry
    /// dispatch by (driver_type, command) and answer the command's JSON
    /// payload. Commands that declare a saveDialog return
    /// `{ fileName, dataBase64 }` here — the host thin shell pops the native
    /// dialog, writes the bytes and replaces the payload with
    /// `{ savedPath }` (null when cancelled) before the caller sees it.
    fn execute_driver_command(
        &mut self,
        _driver_type: &str,
        _command: &str,
        _input: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.to_string())
    }

    /// Offer real work to the host. `true` = the host took the job and its
    /// answer arrives later through [`poll_jobs`](DesktopAdapter::poll_jobs);
    /// `false` = this adapter serves the synchronous methods only and the
    /// caller falls back to them. The default declines, so every existing
    /// adapter keeps its synchronous behavior unchanged.
    fn begin_job(&mut self, _job: AdapterJob) -> bool {
        false
    }

    /// Whether this adapter can request precise runtime query cancellation.
    fn supports_native_cell_preview(&self) -> bool {
        false
    }

    fn supports_native_query_stop(&self) -> bool {
        false
    }

    /// Supported native SQL drivers can explicitly run a bounded script of SELECT statements.
    fn supports_native_query_script(&self) -> bool {
        false
    }

    fn begin_native_query_script(&mut self, _generation: u32, _sql: String) -> bool {
        false
    }

    /// PostgreSQL profiles with no saved database may choose a catalogued database per editor.
    fn native_query_database_context(&self) -> Option<QueryDatabaseContext> {
        None
    }

    fn begin_native_query_at(
        &mut self, _generation: u32, _sql: String, _target: NativeQueryDatabase, _script: bool,
    ) -> bool {
        false
    }

    fn supports_native_explain(&self) -> bool {
        false
    }

    fn begin_native_explain(
        &mut self, _generation: u32, _sql: String, _target: Option<NativeQueryDatabase>,
    ) -> bool {
        false
    }

    fn request_native_query_stop(&mut self, _generation: u32) -> bool {
        false
    }

    /// Completed jobs since the last call, oldest first. The default answers
    /// nothing.
    fn poll_jobs(&mut self) -> Vec<AdapterDelivery> {
        Vec::new()
    }

    /// Dialect-exact statement ranges for the active editor connection. Native
    /// adapters may delegate to the linked engine's non-executing parser.
    fn editor_statement_ranges(
        &self,
        _sql: &str,
    ) -> Option<Result<Vec<std::ops::Range<usize>>, String>> {
        None
    }

    /// The live native host exposes its restricted data-only transfer tool.
    fn supports_native_transfer(&self) -> bool {
        false
    }

    /// The host's persisted UI settings (N8). The default answers the fixed
    /// error, and the UI falls back to the memory port's settings — the
    /// demo path.
    fn read_ui_settings(&self) -> Result<Vec<Setting>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Validate and persist one UI setting through the host's settings use
    /// case (N8). The default answers the fixed error; the UI falls back to
    /// the memory port.
    fn write_ui_setting(&mut self, _setting: Setting) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The table's DDL from the host's catalog (N10). The default answers
    /// the fixed error; the UI falls back to the memory port.
    fn table_ddl(&self, _table: &str) -> Result<String, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The table's columns from the host's catalog (N10).
    fn table_columns(&self, _table: &str) -> Result<Vec<ColumnSpec>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The table's indexes from the host's catalog (N10).
    fn table_indexes(&self, _table: &str) -> Result<Vec<IndexSpec>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The table's foreign keys from the host's catalog (N10).
    fn table_foreign_keys(&self, _table: &str) -> Result<Vec<ForeignKeySpec>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The inbound references of one table (T5): the other tables' columns
    /// that reference it. The default answers the fixed error; the demo
    /// process has no scan and the UI refuses honestly.
    fn table_inbound_references(&self, _table: &str) -> Result<Vec<InboundRef>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Save the connection form's draft (E5): the live host maps it onto
    /// its `ConnectionConfig` and persists it through the connection use
    /// case — the password lands sealed (T10). The default answers the
    /// fixed error; the UI falls back to the memory port.
    /// Test an unsaved TCP draft without replacing the active connection.
    fn begin_connection_probe(
        &mut self,
        _request_id: u64,
        _draft: &ConnectionDraft,
    ) -> Result<(), ConnectionProbeError> {
        Err(ConnectionProbeError::Unavailable)
    }

    /// Probe a Redis draft without persisting it or changing the live connection.
    fn begin_redis_probe(
        &mut self,
        _epoch: u64,
        _draft: &ConnectionDraft,
        _options: &native_kv::RedisConnectionOptions,
    ) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn save_redis_connection(
        &mut self,
        _draft: &ConnectionDraft,
        _options: &native_kv::RedisConnectionOptions,
    ) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn save_connection(&mut self, _draft: &ConnectionDraft) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The engines the host's registry admitted (E4/E5) — the connection
    /// form's whitelist; "linked means offered". Empty means no live host
    /// and the memory port's own rules apply.
    fn available_engines(&self) -> Vec<String> {
        Vec::new()
    }

    /// Engine default from the host's actual linked driver.
    fn default_connection_port(&self, _engine: &str) -> Option<u16> {
        None
    }

    /// Whether the linked driver expects a physical file path instead of a network address.
    fn connection_uses_file_path(&self, _engine: &str) -> bool {
        false
    }

    /// The saved favorite queries (T6), newest first. The default answers
    /// the fixed error; the demo process has no favorites store.
    fn favorites(&self) -> Result<Vec<FavoriteSpec>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Save a favorite (T6): returns the id it was given. The connection is
    /// the id of the persisted connection the query belongs to, or an empty
    /// string for an unsaved editor's query.
    fn save_favorite(
        &mut self,
        _connection: &str,
        _title: &str,
        _sql: &str,
    ) -> Result<String, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Rename a favorite (T6): the title changes, the file does not.
    fn rename_favorite(&mut self, _id: &str, _title: &str) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Delete a favorite (T6): the bytes move to the store's trash, the
    /// panel stops listing the entry.
    fn delete_favorite(&mut self, _id: &str) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Read and parse a connection import file (T7): the preview before
    /// anything is saved. The path names the file; the host is the side with
    /// filesystem rights. The password serves the encrypted formats
    /// (TablePlus/DataZen exports); the plain ones ignore it.
    fn parse_connections_import(
        &mut self,
        _path: &str,
        _password: &str,
    ) -> Result<ImportPreview, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Save the parsed import's connections into the host's store (T7):
    /// names already in the store are skipped, and the answer counts what
    /// actually landed. The handle is the preview's.
    fn confirm_import(&mut self, _handle: u64) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Start an export of one table through the host (N11). `path` (T2) is
    /// the user's picked save location; `None` keeps the host's default
    /// directory. The default answers the fixed error; the UI falls back to
    /// the memory port.
    fn start_export(
        &mut self,
        _table: &str,
        _format: &str,
        _range: &str,
        _path: Option<&str>,
    ) -> Result<u64, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Start a backup of the live database through the host (N11). `path`
    /// (T2) is the user's picked save location; `None` keeps the host's
    /// default directory.
    fn start_backup(&mut self, _path: Option<&str>) -> Result<u64, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// One host job's progress snapshot (N11), if the job exists.
    fn host_job_progress(&self, _job: u64) -> Option<ProgressSnapshot> {
        None
    }

    /// The saved SQL snippets (T3): the host's once its file store persists
    /// them; the fixed error means the memory port answers instead.
    fn snippets(&self) -> Result<Vec<Snippet>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Create a snippet through the host (T3); an existing name is refused.
    fn save_snippet(&mut self, _name: &str, _sql: &str) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Change a snippet's SQL through the host (T3); an unknown name is
    /// refused.
    fn update_snippet(&mut self, _name: &str, _sql: &str) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Delete a snippet through the host (T3); an unknown name is refused.
    fn delete_snippet(&mut self, _name: &str) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Ask a host job to stop (N11). A finished job answers Ok; the memory
    /// port's stop stays the fallback path's.
    fn stop_host_job(&self, _job: u64) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The dashboard's widgets from the host's dashboard use case (N13).
    /// The default answers the fixed error; the UI falls back to the memory
    /// port.
    fn dashboard_widgets(&self) -> Result<Vec<DashboardWidget>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Save a widget through the host's dashboard use case (N13): a title
    /// and its query text. An empty title is refused at the port. The
    /// default answers the fixed error; the UI falls back to the memory
    /// port.
    fn save_dashboard_widget(&mut self, _title: &str, _query: &str) -> Result<u64, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// One widget's chart data from the host (N13): labeled integers from
    /// the widget's query. The default answers the fixed error; the UI
    /// falls back to the memory port.
    fn widget_bars(&self, _id: u64) -> Result<Vec<(String, u32)>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// The host's saved workflows through the registry use case (N13),
    /// sorted by name. The default answers the fixed error; the UI falls
    /// back to the memory port.
    /// User-visible workflow definitions projected for the native run picker.
    fn native_workflow_list(&self) -> Result<Vec<WorkflowListItem>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.to_owned())
    }

    fn workflows(&self) -> Result<Vec<Workflow>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Save a workflow through the host's registry use case (N13); an edit
    /// of a stored name replaces its steps. An empty name is refused at the
    /// port. The default answers the fixed error; the UI falls back to the
    /// memory port.
    fn save_workflow(&mut self, _name: &str, _steps: &[String]) -> Result<(), String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    /// Delete the uniquely selected visible workflow via the host registry.
    fn delete_visible_workflow(&mut self, _name: &str) -> Result<(), String> {
        Err("workflow delete needs the live host".into())
    }

    /// Run a workflow through the host (N13); every step reports name,
    /// success and one summary line. The default answers the fixed error;
    /// the UI falls back to the memory port.
    fn run_workflow(&mut self, _name: &str) -> Result<Vec<WorkflowStepResult>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }
}

/// One unit of host work that can take real time — a live driver IO. The job
/// twin of the adapter's synchronous methods, plus query execution: each job
/// carries the generation of the request it answers, and the UI drops
/// deliveries whose generation the state has moved past (plan invariant 3).
#[derive(Clone, Debug, PartialEq)]
pub enum AdapterJob {
    ReadNativeTransfer {
        epoch: u32,
        side: native_transfer::TransferSide,
        choice: native_transfer::TransferChoice,
        allow_sql: bool,
    },
    ReviewNativeTransfer {
        epoch: u32,
        request: native_transfer::NativeTransferRequest,
    },
    ReviewNativeCsv {
        epoch: u32,
        choice: native_csv_import::CsvImportChoice,
    },
    AppendNativeCsv {
        epoch: u32,
        review: native_csv_import::CsvImportReview,
    },
    PreviewNativeSync {
        epoch: u32,
        request: native_transfer::SqliteReadComparison,
    },
    CompareNativeSchema {
        epoch: u32,
        request: native_transfer::SqliteReadComparison,
    },
    RunNativeTransfer {
        epoch: u32,
        request: native_transfer::NativeTransferRequest,
    },
    ReadKv(native_kv::KvRead),
    CloseKv {
        connection_id: String,
        session_generation: u32,
    },
    /// Open a connection in the host and read its catalog (N3): the live
    /// driver's databases, schemas and tables. The memory port's instant
    /// connect stays the declined path.
    OpenConnection {
        generation: u32,
        id: String,
    },
    /// Scan the live session's tables for a value (T4): one job for the
    /// whole scan, one hit per (table, column) with rows matching.
    ValueSearch {
        generation: u32,
        value: String,
        mode: String,
    },
    /// Read one page of a table, the job twin of `read_table_page`.
    ReadTable {
        generation: u32,
        table: String,
        page: u32,
        page_size: u32,
        sort: Option<ColumnSort>,
        filter: Option<ColumnFilter>,
    },
    /// Read one table's structure, four reads in one job (E1): the DDL,
    /// the columns, the indexes and the foreign keys, over the live
    /// session — the structure views render the cached answer only.
    ReadStructure {
        generation: u32,
        table: String,
    },
    /// Read one widget's chart bars (E2): the widget's query runs as a job
    /// over the live session — the bar chart renders the cached answer
    /// only, never a live query on the frame loop.
    WidgetBars {
        generation: u32,
        id: u64,
    },
    /// Read one table's inbound references (E3): the scan that walks the
    /// catalog for the tables referencing it — the job twin of
    /// `table_inbound_references`.
    InboundRefs {
        generation: u32,
        table: String,
    },
    /// Apply one batch of cell edits, the job twin of `commit_cells`. The
    /// rows are the page's, addressed by the table's primary key.
    CommitCells {
        generation: u32,
        batch: CellEditBatch,
    },
    /// Read-only preview of existing-row edits using live catalog metadata.
    PreviewCells { nonce: u32, batch: CellEditBatch },
    /// Explicit confirmation: the host rebuilds and rechecks the reviewed plan.
    CommitReviewedCells {
        generation: u32,
        batch: CellEditBatch,
        review: CellEditReview,
    },
    /// Run one workflow through the host's engine (T8): the real executor
    /// over the live session, every step kind it supports.
    RunWorkflow {
        generation: u32,
        name: String,
        variables: std::collections::BTreeMap<String, serde_json::Value>,
    },
    /// Load the newest persisted query history for one open connection.
    ReadQueryHistory {
        generation: u32,
        connection_id: String,
    },
    /// Run one SQL statement, the job twin of the workspace's `execute`.
    RunQuery {
        generation: u32,
        sql: String,
    },
}

/// End-to-end execution time and, for a non-tabular DML command, the server's
/// authoritative affected-row total. A missing total is not zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryExecutionStats {
    pub affected_rows: Option<u64>,
    pub elapsed_ms: u64,
}

/// A completed job's answer. The generation is the requesting job's; the UI
/// matches it against the live one before writing any state.
#[derive(Clone, Debug, PartialEq)]
pub enum AdapterDelivery {
    NativeTransferRead {
        epoch: u32,
        side: native_transfer::TransferSide,
        result: Result<native_transfer::TransferCatalog, String>,
    },
    NativeTransferReviewed {
        epoch: u32,
        result: Result<String, String>,
    },
    NativeTransferFinished {
        epoch: u32,
        result: Result<u64, String>,
    },
    NativeCsvReviewed {
        epoch: u32,
        result: Result<native_csv_import::CsvImportReview, String>,
    },
    NativeCsvAppended {
        epoch: u32,
        result: Result<usize, String>,
    },
    NativeSyncPreviewed {
        epoch: u32,
        result: Result<native_transfer::NativeSyncPreview, String>,
    },
    NativeSchemaCompared {
        epoch: u32,
        result: Result<native_transfer::NativeSchemaDiff, String>,
    },
    ConnectionProbed {
        request_id: u64,
        result: Result<(), ConnectionProbeError>,
    },
    RedisProbed {
        epoch: u64,
        result: Result<(), String>,
    },
    KvRead {
        request: native_kv::KvRead,
        result: Result<native_kv::KvReply, String>,
    },
    /// An open job's catalog (or its failure sentence). The connection id
    /// rides along so the UI can name the live session — the refresh path
    /// re-opens it.
    ConnectionOpened {
        generation: u32,
        id: String,
        catalog: Result<Catalog, String>,
    },
    /// A value search's hits (or its failure sentence) (T4).
    ValueSearched {
        generation: u32,
        hits: Result<Vec<ValueSearchHit>, String>,
    },
    /// A read job's page (or its failure sentence).
    TablePage {
        generation: u32,
        page: Result<TablePage, String>,
    },
    /// A structure job's four-part answer (or its failure sentence) (E1).
    /// The table rides along: the cache is per table, and a delivery for a
    /// table whose entry moved to a newer generation is dropped.
    StructureRead {
        generation: u32,
        table: String,
        structure: Result<TableStructure, String>,
    },
    /// A widget's chart bars (or its failure sentence) (E2). The widget id
    /// rides along: the cache is per widget, and a delivery for an entry
    /// that moved to a newer generation is dropped.
    WidgetBarsRead {
        generation: u32,
        id: u64,
        bars: Result<Vec<(String, u32)>, String>,
    },
    /// An inbound-refs job's list (or its failure sentence) (E3). The table
    /// rides along: the answer describes the loaded table or it drops.
    InboundRefsRead {
        generation: u32,
        table: String,
        refs: Result<Vec<InboundRef>, String>,
    },
    /// A commit job's landed edit count (or its failure sentence).
    CommitCount {
        generation: u32,
        count: Result<u32, String>,
    },
    CellPreviewRead {
        nonce: u32,
        result: Result<CellEditPreview, String>,
    },
    QueryHistoryRead {
        generation: u32,
        result: Result<QueryHistorySnapshot, String>,
    },
    /// A query job's result page (or its failure sentence).
    /// The submitted execution was observed in a terminal runtime state.
    QueryTerminal { generation: u32 },
    /// The runtime did not accept a cancellation for this execution.
    QueryStopRejected { generation: u32 },
    QueryPage {
        generation: u32,
        page: Result<TablePage, String>,
        execution_stats: Option<QueryExecutionStats>,
    },
    QueryExplained {
        generation: u32,
        result: Result<ExplainResult, String>,
    },
    /// Ordered result pages from one explicitly submitted SELECT script.
    QueryScriptPages {
        generation: u32,
        pages: Result<Vec<TablePage>, String>,
    },
    /// A workflow run's step report (or its failure sentence) (T8).
    WorkflowRun {
        generation: u32,
        results: Result<Vec<WorkflowStepResult>, String>,
    },
    /// Rich report delivered by the real native host, not by the lite fallback.
    NativeWorkflowRun {
        generation: u32,
        results: Result<NativeWorkflowRun, String>,
    },
}

/// A connection-scoped page of the native host's persisted SQL history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryHistorySnapshot {
    /// Active logical database for the requested connection, not the file path.
    pub current_database: Option<String>,
    pub entries: Vec<QueryHistoryRecord>,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryHistoryRecord {
    pub id: String,
    pub database: Option<String>,
    pub executed_at: String,
    pub sql: String,
    pub success: bool,
    pub error_message: Option<String>,
}

/// One (table, column) a value search found the value in (T4): how many
/// rows of the column match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSearchHit {
    pub table: String,
    pub column: String,
    pub matches: u64,
}

/// The grid's staged edits as one commit batch (N6): the rows the page
/// showed, addressed by the table's primary key, with the original values
/// the page loaded — the same model the React grid commits through, so a
/// row that changed underneath is a stale original, not a silent overwrite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEditBatch {
    pub table: String,
    /// The page the rows are numbered on, echoed so the host can name the
    /// read the batch belongs to.
    pub page: u32,
    pub page_size: u32,
    pub sort: Option<ColumnSort>,
    pub filter: Option<ColumnFilter>,
    pub rows: Vec<RowEdit>,
}

/// Identity of the exact plan confirmed by the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEditReview {
    pub fingerprint: String,
    pub config_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEditPreview {
    pub review: CellEditReview,
    pub updates: Vec<(String, Vec<String>)>,
    pub warnings: Vec<(String, String)>,
}

/// One edited row of the loaded page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowEdit {
    /// The row's index on the loaded page.
    pub index: u32,
    /// The row's primary-key values, PK column name → the value the page
    /// showed. Empty only when the table has no primary key — a commit the
    /// host will refuse.
    pub identity: Vec<(String, String)>,
    /// The changed columns: (column, the original value the page showed, the
    /// staged value).
    pub changes: Vec<(String, String, String)>,
}

/// The stand-in adapter while no desktop use case is wired: every method
/// answers the same fixed error.
pub struct UnimplementedAdapter;

impl DesktopAdapter for UnimplementedAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn read_table_page(
        &mut self,
        _table: &str,
        _page: u32,
        _page_size: u32,
        _sort: Option<&ColumnSort>,
        _filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(
        &mut self,
        _table: &str,
        _edits: &[(u32, String, String)],
    ) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }
}

pub trait WorkspaceApi {
    fn connections(&self) -> &[ConnectionProfile];
    fn session(&self) -> Option<&Session>;
    fn catalog(&self) -> Catalog;
    /// The table's DDL as one string the page shows verbatim. An empty string
    /// means the port cannot produce DDL; an unknown table is an error.
    fn table_ddl(&self, table: &str) -> Result<String, String>;
    /// The table's columns: one entry per column, in the table's order.
    fn table_columns(&self, table: &str) -> Result<Vec<ColumnSpec>, String>;
    /// The table's indexes; a compound index keeps its column order.
    fn table_indexes(&self, table: &str) -> Result<Vec<IndexSpec>, String>;
    /// The table's foreign keys; a compound key is not split.
    fn table_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeySpec>, String>;
    /// Save a connection from the form: name, engine, address. It goes into
    /// the port only, never the system keychain. A missing address is refused.
    fn save_connection(&mut self, draft: &ConnectionDraft) -> Result<(), String>;
    /// The saved SQL snippets.
    fn snippets(&self) -> Vec<Snippet>;
    /// Create a snippet; an existing name is refused.
    fn save_snippet(&mut self, name: &str, sql: &str) -> Result<(), String>;
    /// Change a snippet's SQL; an unknown name is refused.
    fn update_snippet(&mut self, name: &str, sql: &str) -> Result<(), String>;
    /// Delete a snippet; an unknown name is refused.
    fn delete_snippet(&mut self, name: &str) -> Result<(), String>;
    /// Start an export job for a format and a range. A missing range gets no
    /// job at all.
    fn start_export(&mut self, format: &str, range: &str) -> Result<u64, String>;
    /// One job's progress snapshot, if the job exists. The memory port
    /// advances the job one step per read.
    fn job_progress(&mut self, job: u64) -> Option<ProgressSnapshot>;
    /// Start a backup of the connected database.
    fn start_backup(&mut self) -> Result<u64, String>;
    /// D1: the two endpoints of a sync. Unknown endpoints, or the same endpoint
    /// twice, are refused.
    fn sync_endpoints(&mut self, source: &str, target: &str) -> Result<(), String>;
    /// The tables a connection can see, without opening a session.
    fn connection_tables(&self, connection: &str) -> Vec<String>;
    /// D4: a read-only preview page of one table, as the sync would copy it.
    fn sync_preview(&mut self, source: &str, table: &str) -> Result<TablePage, String>;
    /// D5: run the sync as a job. No tables selected, no job.
    fn run_sync(&mut self, source: &str, target: &str, tables: &[String]) -> Result<u64, String>;
    /// D6: the source's objects, marked when the target has the same name.
    fn compare_objects(&self, source: &str, target: &str) -> Result<Vec<CompareObject>, String>;
    /// D7: the plan as text — one statement per object the target lacks.
    /// An empty string means no differences.
    fn compare_plan(&self, source: &str, target: &str) -> Result<String, String>;
    /// D8: run the plan as a job. An empty plan has nothing to apply; the
    /// replica cannot create analytics tables, so that plan's job fails and
    /// the difference stays.
    fn run_compare_plan(&mut self, plan: &str) -> Result<u64, String>;
    /// D10: run the transfer as a job. No pairs, no job.
    fn run_transfer(&mut self, pairs: &[(String, String)]) -> Result<u64, String>;
    /// W1: the dashboard's widgets.
    fn dashboard_widgets(&self) -> Vec<DashboardWidget>;
    /// W2: save a widget (a title and its query text); an empty title is
    /// refused. Returns the widget's id.
    fn save_dashboard_widget(&mut self, title: &str, query: &str) -> Result<u64, String>;
    /// W2: the widget's chart data: labeled integers from the port.
    fn widget_bars(&self, id: u64) -> Result<Vec<(String, u32)>, String>;
    /// W4: the workflows, sorted by name.
    fn workflows(&self) -> Vec<Workflow>;
    /// W5: save a workflow by name (an edit replaces the stored steps).
    fn save_workflow(&mut self, name: &str, steps: &[String]) -> Result<(), String>;
    /// W7: run a workflow; every step reports name, success and one summary
    /// line. The memory port fails steps that apply anything.
    fn run_workflow(&mut self, name: &str) -> Result<Vec<WorkflowStepResult>, String>;
    /// Stop a running job. A stopped job reports "stopped" from then on.
    fn stop_job(&mut self, job: u64) -> Result<(), String>;
    /// Apply a DDL script to the table. The port records what it applied; a
    /// failure changes nothing.
    fn apply_ddl(&mut self, table: &str, sql: &str) -> Result<(), String>;
    /// The DDL scripts this port has applied, in order.
    fn applied_ddl(&self) -> Vec<String>;
    fn connect(&mut self, id: &str) -> Result<(), String>;
    fn disconnect(&mut self);
    fn open_table(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String>;
    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String>;
    /// Open a read-only query result page. While it is open, staging is rejected.
    fn open_result(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) -> TablePage;
    /// Apply every staged edit to the stored rows and report how many landed.
    fn commit_staged(&mut self) -> Result<u32, String>;
    /// Drop every staged edit; the stored rows keep their values.
    fn rollback_staged(&mut self);
}

#[derive(Clone, Debug)]
struct StoredTable {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    /// Staged cell edits keyed by (stored row, column index); open_table overlays
    /// them, commit_staged applies them, rollback_staged drops them.
    staged: HashMap<(u32, usize), String>,
    /// Why the table cannot be edited; None means writable.
    read_only: Option<String>,
    /// The schema the table lives in.
    schema: String,
    /// Per-column SQL types, parallel to `columns`; missing entries read "text".
    column_types: Vec<String>,
}

/// The table the workspace last opened, with the page window it asked for.
#[derive(Clone, Debug)]
struct OpenTable {
    name: String,
    page: u32,
    page_size: u32,
}

#[derive(Clone, Debug)]
pub struct MemoryWorkspace {
    connections: Vec<ConnectionProfile>,
    session: Option<Session>,
    tables: Vec<(String, StoredTable)>,
    open: Option<OpenTable>,
    /// A query result is showing: the port rejects staging until a table reopens.
    result_open: bool,
    /// DDL scripts the port has applied, in order.
    applied_ddl: Vec<String>,
    /// The settings store, one entry per kind.
    settings: Vec<Setting>,
    /// The saved SQL snippets.
    snippets: Vec<Snippet>,
    /// Export jobs the memory port runs.
    jobs: Vec<MemoryJob>,
    /// The dashboard's widgets.
    widgets: Vec<DashboardWidget>,
    /// The workflows, stored unordered; reads sort by name.
    workflows: Vec<Workflow>,
}

impl Default for MemoryWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryWorkspace {
    pub fn new() -> Self {
        let columns = vec![
            "id", "name", "city", "status", "note", "orders", "email", "country",
        ];
        let mut rows = Vec::new();
        for index in 0..40 {
            rows.push(vec![
                index.to_string(),
                format!("customer-{index}"),
                "Hangzhou".to_owned(),
                if index % 2 == 0 {
                    "active".to_owned()
                } else {
                    "paused".to_owned()
                },
                "seed".to_owned(),
                (index * 3).to_string(),
                format!("c{index}@example.com"),
                "CN".to_owned(),
            ]);
        }
        // A wide demo table for the analytics schema: 30 columns x 60 rows.
        let wide_columns: Vec<String> = (0..30).map(|i| format!("c{i}")).collect();
        let wide_rows: Vec<Vec<String>> = (0..60)
            .map(|row| {
                wide_columns
                    .iter()
                    .map(|col| format!("{col}-{row}"))
                    .collect()
            })
            .collect();
        let tables = vec![
            (
                "customers".to_owned(),
                StoredTable {
                    columns: columns.into_iter().map(str::to_owned).collect(),
                    rows,
                    staged: HashMap::new(),
                    read_only: None,
                    schema: "public".into(),
                    column_types: Vec::new(),
                },
            ),
            (
                "orders".to_owned(),
                StoredTable {
                    columns: vec!["id".into(), "name".into()],
                    rows: vec![vec!["1".into(), "order-0".into()]],
                    staged: HashMap::new(),
                    read_only: Some("no primary key".into()),
                    schema: "public".into(),
                    column_types: Vec::new(),
                },
            ),
            (
                "catalog".to_owned(),
                StoredTable {
                    columns: wide_columns,
                    rows: wide_rows,
                    staged: HashMap::new(),
                    read_only: None,
                    schema: "analytics".into(),
                    column_types: Vec::new(),
                },
            ),
        ];
        Self {
            connections: vec![
                ConnectionProfile {
                    id: "local".into(),
                    name: "Local Postgres".into(),
                    engine: "postgres".into(),
                    host: None,
                },
                ConnectionProfile {
                    id: "replica".into(),
                    name: "Read Replica".into(),
                    engine: "postgres".into(),
                    host: None,
                },
            ],
            session: None,
            tables,
            open: None,
            result_open: false,
            applied_ddl: Vec::new(),
            settings: vec![
                Setting::Theme { dark: true },
                Setting::TablePageSize { rows: 25 },
                Setting::QueryMaxRows { rows: 100 },
            ],
            snippets: Vec::new(),
            jobs: Vec::new(),
            widgets: Vec::new(),
            workflows: vec![
                Workflow {
                    name: "weekly-report".into(),
                    steps: vec!["run query".into(), "export csv".into()],
                },
                Workflow {
                    name: "audit".into(),
                    steps: vec!["compare structures".into(), "apply plan".into()],
                },
                Workflow {
                    name: "nightly-backup".into(),
                    steps: vec!["backup".into(), "verify".into()],
                },
            ],
        }
    }
}

impl SettingsApi for MemoryWorkspace {
    fn read_settings(&self) -> Vec<Setting> {
        self.settings.clone()
    }

    fn write_setting(&mut self, setting: Setting) -> Result<(), String> {
        setting.validate()?;
        self.settings
            .retain(|existing| !replaces_slot(existing, &setting));
        self.settings.push(setting);
        Ok(())
    }
}

fn setting_kind(setting: &Setting) -> u8 {
    match setting {
        Setting::Theme { .. } => 0,
        Setting::TablePageSize { .. } => 1,
        Setting::QueryMaxRows { .. } => 2,
        Setting::Layout { .. } => 3,
        Setting::BuiltinLanguage { .. } => 4,
    }
}

/// Whether `existing` holds the slot `incoming` writes: the same entry for
/// the scalar settings, the same region key for layout sizes — one size per
/// region, every region coexists.
fn replaces_slot(existing: &Setting, incoming: &Setting) -> bool {
    match (existing, incoming) {
        (Setting::Layout { key: a, .. }, Setting::Layout { key: b, .. }) => a == b,
        _ => setting_kind(existing) == setting_kind(incoming),
    }
}

impl MemoryWorkspace {
    /// A read-only result page. No flags touched: callers decide what shows.
    fn result_page(columns: Vec<String>, rows: Vec<Vec<String>>) -> TablePage {
        let total = rows.len() as u32;
        TablePage {
            table: "result".into(),
            columns,
            rows,
            page: 0,
            page_size: total.max(1),
            total_rows: total,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: Some("query result".into()),
        }
    }
}

impl QueryApi for MemoryWorkspace {
    fn execute(&mut self, sql: &str) -> Result<TablePage, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let normalized = sql
            .trim()
            .trim_end_matches(';')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_uppercase();
        match normalized.as_str() {
            "SELECT 1" => Ok(Self::result_page(
                vec!["1".to_owned()],
                vec![vec!["1".to_owned()]],
            )),
            "SELECT * FROM CUSTOMERS" => {
                let Some((_, stored)) = self.tables.iter().find(|(name, _)| name == "customers")
                else {
                    return Err("unknown table customers".into());
                };
                let max_rows = self
                    .settings
                    .iter()
                    .find_map(|setting| match setting {
                        Setting::QueryMaxRows { rows } => Some(*rows),
                        _ => None,
                    })
                    .unwrap_or(100) as usize;
                let rows = stored.rows.iter().take(max_rows).cloned().collect();
                Ok(Self::result_page(stored.columns.clone(), rows))
            }
            _ => Err(format!("cannot execute: {sql}")),
        }
    }
}

impl WorkspaceApi for MemoryWorkspace {
    fn connections(&self) -> &[ConnectionProfile] {
        &self.connections
    }

    fn catalog(&self) -> Catalog {
        let Some(session) = &self.session else {
            return Catalog {
                databases: Vec::new(),
            };
        };
        let mut schemas: Vec<(String, Vec<String>)> = Vec::new();
        for (name, stored) in &self.tables {
            if !session.tables.contains(name) {
                continue;
            }
            match schemas
                .iter_mut()
                .find(|(schema, _)| *schema == stored.schema)
            {
                Some(existing) => existing.1.push(name.clone()),
                None => schemas.push((stored.schema.clone(), vec![name.clone()])),
            }
        }
        let databases = vec![DatabaseNode {
            name: session.connection_id.clone(),
            schemas: schemas
                .into_iter()
                .map(|(name, tables)| SchemaNode { name, tables, views: Vec::new() })
                .collect(),
        }];
        Catalog { databases }
    }

    fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    fn table_ddl(&self, table: &str) -> Result<String, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        if stored.1.schema == "analytics" {
            // The memory port cannot introspect the external analytics schema.
            return Ok(String::new());
        }
        // (table_ddl continues below; the analytics branch is shared with columns)
        let lines: Vec<String> = stored
            .1
            .columns
            .iter()
            .map(|column| format!("  {column} text"))
            .collect();
        Ok(format!("CREATE TABLE {table} (\n{}\n);", lines.join(",\n")))
    }

    fn table_indexes(&self, table: &str) -> Result<Vec<IndexSpec>, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        Ok(match table {
            "customers" => vec![
                IndexSpec {
                    name: "customers_pkey".into(),
                    columns: vec!["id".into()],
                    unique: true,
                },
                IndexSpec {
                    name: "customers_name_city_idx".into(),
                    columns: vec!["name".into(), "city".into()],
                    unique: false,
                },
            ],
            "orders" => vec![IndexSpec {
                name: "orders_name_idx".into(),
                columns: vec!["name".into()],
                unique: false,
            }],
            "catalog" => vec![IndexSpec {
                name: "catalog_c0_c1_idx".into(),
                columns: vec!["c0".into(), "c1".into()],
                unique: false,
            }],
            _ => Vec::new(),
        })
    }

    fn table_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeySpec>, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let _ = stored;
        Ok(match table {
            "orders" => vec![ForeignKeySpec {
                name: "orders_customer_fk".into(),
                columns: vec!["id".into(), "name".into()],
                target_table: "customers".into(),
                target_columns: vec!["id".into(), "name".into()],
            }],
            "customers" => vec![ForeignKeySpec {
                name: "customers_orders_fk".into(),
                columns: vec!["orders".into()],
                target_table: "orders".into(),
                target_columns: vec!["id".into()],
            }],
            _ => Vec::new(),
        })
    }

    fn snippets(&self) -> Vec<Snippet> {
        self.snippets.clone()
    }

    fn save_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("snippet name is required".into());
        }
        if self.snippets.iter().any(|snippet| snippet.name == name) {
            return Err(format!("a snippet named {name} already exists"));
        }
        self.snippets.push(Snippet {
            name: name.to_owned(),
            sql: sql.to_owned(),
        });
        Ok(())
    }

    fn update_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        match self
            .snippets
            .iter_mut()
            .find(|snippet| snippet.name == name)
        {
            Some(snippet) => {
                snippet.sql = sql.to_owned();
                Ok(())
            }
            None => Err(format!("no snippet named {name}")),
        }
    }

    fn delete_snippet(&mut self, name: &str) -> Result<(), String> {
        let before = self.snippets.len();
        self.snippets.retain(|snippet| snippet.name != name);
        if self.snippets.len() == before {
            return Err(format!("no snippet named {name}"));
        }
        Ok(())
    }

    fn start_export(&mut self, format: &str, range: &str) -> Result<u64, String> {
        if range.trim().is_empty() {
            return Err("a range is required before an export can start".into());
        }
        if !matches!(format, "csv" | "json" | "sql" | "broken") {
            return Err(format!("unknown export format {format}"));
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: format.to_owned(),
            range: range.to_owned(),
            reads: 0,
            stopped: false,
            log: vec![
                "queued".to_owned(),
                format!("exporting {format} of {range}"),
            ],
            fail_line: None,
        });
        Ok(job)
    }

    fn job_progress(&mut self, job: u64) -> Option<ProgressSnapshot> {
        let entry = self.jobs.iter_mut().find(|running| running.job == job)?;
        let snapshot = |status: &str, percent: u32, entry: &MemoryJob| ProgressSnapshot {
            job,
            status: status.to_owned(),
            percent,
            log: entry.log.clone(),
        };
        if entry.stopped {
            return Some(snapshot("stopped", 100, entry));
        }
        entry.reads += 1;
        match entry.reads {
            1 => Some(snapshot("running", 0, entry)),
            2 => {
                entry.log.push("halfway".into());
                Some(snapshot("running", 50, entry))
            }
            _ => {
                let failed = entry.format == "broken"
                    || entry.range == "no-such-table"
                    || entry.fail_line.is_some();
                if entry.reads == 3 {
                    // The terminal line lands once, on the transition.
                    if entry.format == "broken" {
                        entry.log.push("the port cannot write broken files".into());
                    } else if entry.range == "no-such-table" {
                        entry.log.push("no such table no-such-table".into());
                    } else if let Some(line) = &entry.fail_line {
                        entry.log.push(line.clone());
                    } else {
                        entry.log.push("done".into());
                    }
                }
                Some(snapshot(
                    if failed { "failed" } else { "completed" },
                    100,
                    entry,
                ))
            }
        }
    }

    fn sync_endpoints(&mut self, source: &str, target: &str) -> Result<(), String> {
        for endpoint in [source, target] {
            if !self
                .connections
                .iter()
                .any(|profile| profile.id == endpoint)
            {
                return Err(format!("unknown connection {endpoint}"));
            }
        }
        if source == target {
            return Err("both ends are the same connection".into());
        }
        Ok(())
    }

    fn sync_preview(&mut self, source: &str, table: &str) -> Result<TablePage, String> {
        if !self.connections.iter().any(|profile| profile.id == source) {
            return Err(format!("unknown connection {source}"));
        }
        if !self
            .connection_tables(source)
            .iter()
            .any(|name| name == table)
        {
            return Err(format!("{source} cannot see {table}"));
        }
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let mut page = Self::result_page(stored.1.columns.clone(), stored.1.rows.clone());
        page.table = format!("sync preview: {table}");
        page.read_only = Some("sync preview".into());
        Ok(page)
    }

    fn run_sync(&mut self, source: &str, target: &str, tables: &[String]) -> Result<u64, String> {
        self.sync_endpoints(source, target)?;
        if tables.is_empty() {
            return Err("no tables selected".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: "sync".into(),
            range: format!("{} tables", tables.len()),
            reads: 0,
            stopped: false,
            log: vec![
                "queued".to_owned(),
                format!("syncing {source} into {target}"),
            ],
            fail_line: None,
        });
        Ok(job)
    }

    fn compare_objects(&self, source: &str, target: &str) -> Result<Vec<CompareObject>, String> {
        for endpoint in [source, target] {
            if !self
                .connections
                .iter()
                .any(|profile| profile.id == endpoint)
            {
                return Err(format!("unknown connection {endpoint}"));
            }
        }
        let target_tables = self.connection_tables(target);
        Ok(self
            .connection_tables(source)
            .into_iter()
            .map(|name| CompareObject {
                present_on_target: target_tables.contains(&name),
                name,
            })
            .collect())
    }

    fn run_compare_plan(&mut self, plan: &str) -> Result<u64, String> {
        if plan.trim().is_empty() {
            return Err("nothing to apply".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        let fails = plan.contains("CREATE TABLE catalog");
        self.jobs.push(MemoryJob {
            job,
            format: "compare".into(),
            range: "the compare plan".into(),
            reads: 0,
            stopped: false,
            log: vec!["queued".to_owned(), "applying the plan".into()],
            fail_line: fails.then(|| "the replica cannot create analytics tables".to_owned()),
        });
        Ok(job)
    }

    fn dashboard_widgets(&self) -> Vec<DashboardWidget> {
        self.widgets.clone()
    }

    fn save_dashboard_widget(&mut self, title: &str, query: &str) -> Result<u64, String> {
        if title.trim().is_empty() {
            return Err("a widget needs a title".into());
        }
        let id = self.widgets.last().map(|widget| widget.id + 1).unwrap_or(1);
        self.widgets.push(DashboardWidget {
            id,
            title: title.trim().to_owned(),
            kind: "bar".into(),
        });
        let _ = query;
        Ok(id)
    }

    fn workflows(&self) -> Vec<Workflow> {
        let mut workflows = self.workflows.clone();
        workflows.sort_by(|a, b| a.name.cmp(&b.name));
        workflows
    }

    fn run_workflow(&mut self, name: &str) -> Result<Vec<WorkflowStepResult>, String> {
        let flow = self
            .workflows
            .iter()
            .find(|flow| flow.name == name)
            .ok_or_else(|| format!("no workflow {name}"))?;
        Ok(flow
            .steps
            .iter()
            .map(|step| {
                if step.contains("apply") {
                    WorkflowStepResult {
                        step: step.clone(),
                        ok: false,
                        summary: "the replica refused the plan".into(),
                    }
                } else {
                    WorkflowStepResult {
                        step: step.clone(),
                        ok: true,
                        summary: "ok: 12 rows".into(),
                    }
                }
            })
            .collect())
    }

    fn save_workflow(&mut self, name: &str, steps: &[String]) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("a workflow needs a name".into());
        }
        let name = name.trim();
        match self.workflows.iter_mut().find(|flow| flow.name == name) {
            Some(existing) => existing.steps = steps.to_vec(),
            None => self.workflows.push(Workflow {
                name: name.to_owned(),
                steps: steps.to_vec(),
            }),
        }
        Ok(())
    }

    fn widget_bars(&self, id: u64) -> Result<Vec<(String, u32)>, String> {
        if !self.widgets.iter().any(|widget| widget.id == id) {
            return Err(format!("no widget {id}"));
        }
        // The memory port's demo series: a zero and a maximum, both visible.
        Ok(vec![
            ("mon".into(), 0),
            ("tue".into(), 3),
            ("wed".into(), 7),
            ("thu".into(), 12),
        ])
    }

    fn run_transfer(&mut self, pairs: &[(String, String)]) -> Result<u64, String> {
        if pairs.is_empty() {
            return Err("no column pairs to transfer".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: "transfer".into(),
            range: format!("{} pairs", pairs.len()),
            reads: 0,
            stopped: false,
            log: vec![
                "queued".to_owned(),
                format!("transferring {} column pairs", pairs.len()),
            ],
            fail_line: None,
        });
        Ok(job)
    }

    fn compare_plan(&self, source: &str, target: &str) -> Result<String, String> {
        let objects = self.compare_objects(source, target)?;
        let lines: Vec<String> = objects
            .iter()
            .filter(|object| !object.present_on_target)
            .map(|object| {
                format!(
                    "CREATE TABLE {} (...); -- missing on {}",
                    object.name, target
                )
            })
            .collect();
        Ok(lines.join("\n"))
    }

    fn connection_tables(&self, connection: &str) -> Vec<String> {
        self.tables
            .iter()
            .filter(|(_, stored)| !(connection == "replica" && stored.schema == "analytics"))
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn start_backup(&mut self) -> Result<u64, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: "backup".into(),
            range: "the database".into(),
            reads: 0,
            stopped: false,
            log: vec!["queued".to_owned(), "backing up the database".into()],
            fail_line: None,
        });
        Ok(job)
    }

    fn stop_job(&mut self, job: u64) -> Result<(), String> {
        let entry = self
            .jobs
            .iter_mut()
            .find(|running| running.job == job)
            .ok_or_else(|| format!("no job {job}"))?;
        if entry.stopped {
            return Err("the job is already stopped".into());
        }
        if entry.reads >= 3 {
            return Err("the job already finished".into());
        }
        entry.stopped = true;
        entry.log.push("stopped by the user".into());
        Ok(())
    }

    fn save_connection(&mut self, draft: &ConnectionDraft) -> Result<(), String> {
        let ConnectionDraft {
            name,
            engine,
            address,
            ..
        } = draft;
        if address.trim().is_empty() {
            return Err("address is required".into());
        }
        let name = if name.trim().is_empty() {
            "Untitled"
        } else {
            name.trim()
        };
        let engine = if engine.trim().is_empty() {
            "postgres"
        } else {
            engine.trim()
        };
        let slug = |name: &str| {
            let mut id: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect();
            while id.contains("--") {
                id = id.replace("--", "-");
            }
            id.trim_matches('-').to_owned()
        };
        let mut id = slug(name);
        if id.is_empty() {
            id = format!("connection-{}", self.connections.len() + 1);
        }
        let mut candidate = id.clone();
        let mut counter = 2;
        while self
            .connections
            .iter()
            .any(|profile| profile.id == candidate)
        {
            candidate = format!("{id}-{counter}");
            counter += 1;
        }
        self.connections.push(ConnectionProfile {
            id: candidate,
            name: name.to_owned(),
            engine: engine.to_owned(),
            host: None,
        });
        Ok(())
    }

    fn apply_ddl(&mut self, table: &str, sql: &str) -> Result<(), String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let stored = self
            .tables
            .iter_mut()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        if let Some(reason) = &stored.1.read_only {
            return Err(format!("{table} is read-only: {reason}"));
        }
        enum Change {
            Add(String, String),
            Type(usize, String),
        }
        let add_prefix = format!("ALTER TABLE {table} ADD COLUMN ");
        let type_prefix = format!("ALTER TABLE {table} ALTER COLUMN ");
        let mut changes = Vec::new();
        for line in sql.lines() {
            let line = line.trim().trim_end_matches(';');
            if let Some(rest) = line.strip_prefix(&add_prefix) {
                let Some((name, sql_type)) = rest.split_once(' ') else {
                    return Err(format!("cannot apply: {line}"));
                };
                changes.push(Change::Add(name.to_owned(), sql_type.to_owned()));
            } else if let Some(rest) = line.strip_prefix(&type_prefix) {
                let Some((column, tail)) = rest.split_once(' ') else {
                    return Err(format!("cannot apply: {line}"));
                };
                let Some(sql_type) = tail.strip_prefix("TYPE ") else {
                    return Err(format!("cannot apply: {line}"));
                };
                let index = stored
                    .1
                    .columns
                    .iter()
                    .position(|name| name == column)
                    .ok_or_else(|| format!("unknown column {column}"))?;
                changes.push(Change::Type(index, sql_type.to_owned()));
            } else {
                return Err(format!("cannot apply: {line}"));
            }
        }
        for change in changes {
            match change {
                Change::Add(name, sql_type) => {
                    while stored.1.column_types.len() < stored.1.columns.len() {
                        stored.1.column_types.push("text".into());
                    }
                    stored.1.columns.push(name);
                    stored.1.column_types.push(sql_type);
                    for row in &mut stored.1.rows {
                        row.push("NULL".into());
                    }
                }
                Change::Type(index, sql_type) => {
                    while stored.1.column_types.len() < stored.1.columns.len() {
                        stored.1.column_types.push("text".into());
                    }
                    stored.1.column_types[index] = sql_type;
                }
            }
        }
        self.applied_ddl.push(sql.to_owned());
        Ok(())
    }

    fn applied_ddl(&self) -> Vec<String> {
        self.applied_ddl.clone()
    }

    fn table_columns(&self, table: &str) -> Result<Vec<ColumnSpec>, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        Ok(stored
            .1
            .columns
            .iter()
            .enumerate()
            .map(|(index, name)| ColumnSpec {
                name: name.clone(),
                sql_type: stored
                    .1
                    .column_types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "text".into()),
                // The demo port keeps the first column a key, the rest nullable.
                nullable: index != 0,
            })
            .collect())
    }

    fn connect(&mut self, id: &str) -> Result<(), String> {
        if !self.connections.iter().any(|profile| profile.id == id) {
            return Err(format!("unknown connection {id}"));
        }
        let tables = self
            .tables
            .iter()
            .filter(|(_, stored)| !(id == "replica" && stored.schema == "analytics"))
            .map(|(name, _)| name.clone())
            .collect();
        self.session = Some(Session {
            connection_id: id.to_owned(),
            tables,
        });
        Ok(())
    }

    fn disconnect(&mut self) {
        self.session = None;
        self.open = None;
        self.result_open = false;
        for (_, stored) in self.tables.iter_mut() {
            stored.staged.clear();
        }
    }

    fn open_table(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let total = stored.1.rows.len() as u32;
        let mut order: Vec<u32> = (0..total).collect();
        if let Some(filter) = filter {
            let index = stored
                .1
                .columns
                .iter()
                .position(|name| name == &filter.column)
                .ok_or_else(|| format!("unknown column {}", filter.column))?;
            let needle = filter.contains.as_str();
            order.retain(|row| stored.1.rows[*row as usize][index].contains(needle));
        }
        if let Some(sort) = sort {
            let index = stored
                .1
                .columns
                .iter()
                .position(|name| name == &sort.column)
                .ok_or_else(|| format!("unknown column {}", sort.column))?;
            let descending = sort.descending;
            order.sort_by(|a, b| {
                stored.1.rows[*a as usize][index].cmp(&stored.1.rows[*b as usize][index])
            });
            if descending {
                order.reverse();
            }
        }
        let total = order.len() as u32;
        let page_size = page_size.max(1);
        let page = page.min(total / page_size);
        self.result_open = false;
        let window =
            (page * page_size) as usize..(((page + 1) * page_size) as usize).min(total as usize);
        let mut rows: Vec<Vec<String>> = order[window.clone()]
            .iter()
            .map(|index| stored.1.rows[*index as usize].clone())
            .collect();
        for ((row, col), value) in &stored.1.staged {
            if let Some(page_index) = order[window.clone()].iter().position(|index| index == row) {
                if let Some(record) = rows.get_mut(page_index) {
                    if let Some(cell) = record.get_mut(*col) {
                        *cell = value.clone();
                    }
                }
            }
        }
        self.open = Some(OpenTable {
            name: table.to_owned(),
            page,
            page_size,
        });
        Ok(TablePage {
            table: table.to_owned(),
            columns: stored.1.columns.clone(),
            rows,
            page,
            page_size,
            total_rows: total,
            sort: sort.cloned(),
            filter: filter.cloned(),
            primary_key: Vec::new(),
            read_only: stored.1.read_only.clone(),
        })
    }

    fn open_result(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) -> TablePage {
        self.result_open = true;
        Self::result_page(columns, rows)
    }

    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String> {
        if self.result_open {
            return Err("a query result is open: results are read-only".into());
        }
        let open = self.open.as_ref().ok_or("no open table")?;
        let stored_row = open.page * open.page_size + row;
        let stored = self
            .tables
            .iter_mut()
            .find(|(name, _)| name == &open.name)
            .ok_or_else(|| format!("unknown table {}", open.name))?;
        let index = stored
            .1
            .columns
            .iter()
            .position(|name| name == column)
            .ok_or_else(|| format!("unknown column {column}"))?;
        if let Some(reason) = &stored.1.read_only {
            return Err(format!("{} is read-only: {reason}", open.name));
        }
        if stored_row as usize >= stored.1.rows.len() {
            return Err(format!("row {row} is outside the page"));
        }
        stored.1.staged.insert((stored_row, index), value);
        Ok(())
    }

    fn commit_staged(&mut self) -> Result<u32, String> {
        if let Some(open) = &self.open {
            if let Some((_, stored)) = self.tables.iter().find(|(name, _)| name == &open.name) {
                if let Some(reason) = &stored.read_only {
                    return Err(format!("{} is read-only: {reason}", open.name));
                }
            }
        }
        let mut applied = 0u32;
        for (_, stored) in self.tables.iter_mut() {
            for ((row, col), value) in stored.staged.drain() {
                if let Some(record) = stored.rows.get_mut(row as usize) {
                    if let Some(cell) = record.get_mut(col) {
                        *cell = value;
                        applied += 1;
                    }
                }
            }
        }
        Ok(applied)
    }

    fn rollback_staged(&mut self) {
        for (_, stored) in self.tables.iter_mut() {
            stored.staged.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_open_and_stage_round_trip() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.connect("missing").is_err());
        workspace.connect("local").unwrap();
        let page = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(page.columns[1], "name");
        assert_eq!(page.rows[0][1], "customer-0");
        workspace.stage_cell(0, "name", "alice".into()).unwrap();
        let again = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(again.rows[0][1], "alice");
    }

    #[test]
    fn the_second_page_is_not_the_first_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let first = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        let second = workspace
            .open_table("customers", 1, 25, None, None)
            .unwrap();
        assert_eq!(first.rows.len(), 25);
        assert_eq!(second.rows.len(), 15);
        assert_eq!(first.rows[0][0], "0");
        assert_eq!(second.rows[0][0], "25");
        assert_eq!(first.page, 0);
        assert_eq!(second.page, 1);
        assert_eq!(second.total_rows, 40);
        assert_ne!(first.rows[0], second.rows[0]);
    }

    #[test]
    fn an_out_of_range_page_clamps_to_the_last_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let page = workspace
            .open_table("customers", 99, 25, None, None)
            .unwrap();
        assert_eq!(page.page, 1);
        assert_eq!(page.rows.len(), 15);
    }

    #[test]
    fn sorting_orders_the_page_over_the_whole_table() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let ascending = ColumnSort {
            column: "name".into(),
            descending: false,
        };
        let page = workspace
            .open_table("customers", 0, 25, Some(&ascending), None)
            .unwrap();
        assert_eq!(page.rows[0][1], "customer-0");
        assert_eq!(
            page.rows[2][1], "customer-10",
            "lexicographic ascending order"
        );
        assert_eq!(page.sort, Some(ascending.clone()));

        let descending = ColumnSort {
            column: "name".into(),
            descending: true,
        };
        let page = workspace
            .open_table("customers", 0, 25, Some(&descending), None)
            .unwrap();
        assert_eq!(
            page.rows[0][1], "customer-9",
            "descending starts at the lexicographic max"
        );
        assert_eq!(page.rows[6][1], "customer-39");
        assert_eq!(page.rows.len(), 25);
    }

    #[test]
    fn sorting_by_an_unknown_column_is_an_error() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let sort = ColumnSort {
            column: "missing".into(),
            descending: false,
        };
        assert!(workspace
            .open_table("customers", 0, 25, Some(&sort), None)
            .is_err());
    }

    #[test]
    fn a_filter_narrows_the_page_and_the_total() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let filter = ColumnFilter {
            column: "name".into(),
            contains: "customer-3".into(),
        };
        let page = workspace
            .open_table("customers", 0, 25, None, Some(&filter))
            .unwrap();
        assert_eq!(page.rows.len(), 11, "customer-3 itself and customer-30..39");
        assert_eq!(page.total_rows, 11);
        assert!(page.rows.iter().all(|row| row[1].contains("customer-3")));
        assert_eq!(page.filter, Some(filter));
    }

    #[test]
    fn a_filter_with_sort_pages_over_the_matching_rows() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let filter = ColumnFilter {
            column: "name".into(),
            contains: "customer-3".into(),
        };
        let sort = ColumnSort {
            column: "name".into(),
            descending: true,
        };
        let page = workspace
            .open_table("customers", 0, 25, Some(&sort), Some(&filter))
            .unwrap();
        assert_eq!(page.rows.len(), 11);
        assert_eq!(
            page.rows[0][1], "customer-39",
            "descending over the filtered set"
        );
        assert_eq!(page.rows[10][1], "customer-3");
    }

    #[test]
    fn filtering_an_unknown_column_is_an_error() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let filter = ColumnFilter {
            column: "missing".into(),
            contains: "x".into(),
        };
        assert!(workspace
            .open_table("customers", 0, 25, None, Some(&filter))
            .is_err());
    }

    #[test]
    fn rollback_restores_the_original_value() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        workspace.stage_cell(0, "name", "staged".into()).unwrap();
        let page = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(page.rows[0][1], "staged", "staged values show on the page");
        workspace.rollback_staged();
        let page = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(page.rows[0][1], "customer-0");
    }

    #[test]
    fn commit_keeps_the_new_value_after_reopening() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        workspace.stage_cell(0, "name", "kept".into()).unwrap();
        assert_eq!(workspace.commit_staged().unwrap(), 1);
        let page = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(page.rows[0][1], "kept");
        workspace.rollback_staged();
        let page = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(page.rows[0][1], "kept", "rollback cannot undo a commit");
    }

    #[test]
    fn staged_values_follow_their_row_through_sorting() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        workspace.stage_cell(0, "name", "aaa-first".into()).unwrap();
        let sort = ColumnSort {
            column: "name".into(),
            descending: false,
        };
        let page = workspace
            .open_table("customers", 0, 25, Some(&sort), None)
            .unwrap();
        assert_eq!(
            page.rows[0][1], "aaa-first",
            "the staged name sorts with its row"
        );
    }

    #[test]
    fn open_result_is_read_only_and_blocks_staging() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        let page = workspace.open_result(
            vec!["a".into(), "b".into()],
            vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]],
        );
        assert_eq!(page.table, "result");
        assert_eq!(page.total_rows, 2);
        assert_eq!(page.read_only.as_deref(), Some("query result"));
        assert!(
            workspace.stage_cell(0, "name", "x".into()).is_err(),
            "staging while a result is open is rejected"
        );

        workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        workspace.stage_cell(0, "name", "x".into()).unwrap();
        assert_eq!(
            workspace
                .open_table("customers", 0, 25, None, None)
                .unwrap()
                .rows[0][1],
            "x",
            "reopening a table releases the result lock"
        );
    }

    #[test]
    fn execute_answers_fixed_sql_with_a_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let page = workspace.execute("select 1").unwrap();
        assert_eq!(page.table, "result");
        assert_eq!(page.columns, vec!["1".to_owned()]);
        assert_eq!(page.rows, vec![vec!["1".to_owned()]]);
        assert_eq!(page.read_only.as_deref(), Some("query result"));

        let page = workspace.execute("SELECT  *  FROM customers;").unwrap();
        assert_eq!(page.columns[1], "name");
        assert_eq!(
            page.rows.len(),
            40,
            "under the 100-row cap the whole table returns"
        );
        assert_eq!(page.rows[0][1], "customer-0");

        workspace
            .write_setting(Setting::QueryMaxRows { rows: 10 })
            .unwrap();
        let page = workspace.execute("select * from customers").unwrap();
        assert_eq!(page.rows.len(), 10, "the query cap comes from the setting");
    }

    #[test]
    fn unknown_sql_is_an_error_and_changes_nothing() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        workspace.stage_cell(0, "name", "staged".into()).unwrap();
        let before = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();

        assert!(workspace.execute("drop table customers").is_err());
        assert!(workspace.execute("").is_err());

        let after = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(
            after.rows, before.rows,
            "the table and its staged values are untouched"
        );
        workspace.stage_cell(1, "name", "more".into()).unwrap();
        let page = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(
            page.rows[1][1], "more",
            "staging still works after failed executes"
        );
    }

    #[test]
    fn execute_without_a_session_is_an_error() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.execute("select 1").is_err());
    }

    #[test]
    fn the_catalog_reports_three_levels_in_a_stable_order() {
        let mut workspace = MemoryWorkspace::new();
        assert!(
            workspace.catalog().databases.is_empty(),
            "no session, no catalog"
        );
        workspace.connect("local").unwrap();
        let catalog = workspace.catalog();
        assert_eq!(catalog.databases.len(), 1);
        assert_eq!(catalog.databases[0].name, "local");
        let schemas = &catalog.databases[0].schemas;
        assert_eq!(schemas.len(), 2, "two schemas");
        assert_eq!(schemas[0].name, "public");
        assert_eq!(schemas[0].tables, vec!["customers", "orders"]);
        assert_eq!(schemas[1].name, "analytics");
        assert_eq!(schemas[1].tables, vec!["catalog"]);
        assert_eq!(
            workspace.catalog(),
            catalog,
            "the order is stable across calls"
        );
    }

    #[test]
    fn column_drafts_are_data_not_state_changes() {
        let add = ColumnDraft::AddColumn {
            name: "score".into(),
            sql_type: "int".into(),
        };
        let retype = ColumnDraft::ChangeType {
            column: "name".into(),
            sql_type: "varchar(80)".into(),
        };
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let before = workspace.table_columns("customers").unwrap();
        let drafts = vec![add, retype];
        let after = workspace.table_columns("customers").unwrap();
        assert_eq!(before, after, "drafts alone change no columns");
        assert_eq!(drafts.len(), 2);
    }

    #[test]
    fn drafts_preview_as_alter_statements() {
        let add = ColumnDraft::AddColumn {
            name: "score".into(),
            sql_type: "int".into(),
        };
        assert_eq!(
            add.sql("customers"),
            "ALTER TABLE customers ADD COLUMN score int;"
        );
        let retype = ColumnDraft::ChangeType {
            column: "name".into(),
            sql_type: "varchar(80)".into(),
        };
        assert_eq!(
            retype.sql("customers"),
            "ALTER TABLE customers ALTER COLUMN name TYPE varchar(80);",
            "a retype draft shows its ALTER COLUMN ... TYPE SQL"
        );
    }

    #[test]
    fn applying_a_draft_adds_the_column_and_the_port_records_it() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let sql = "ALTER TABLE customers ADD COLUMN score int;";
        workspace.apply_ddl("customers", sql).unwrap();
        let columns = workspace.table_columns("customers").unwrap();
        assert_eq!(columns.len(), 9);
        assert_eq!(columns[8].name, "score");
        assert_eq!(columns[8].sql_type, "int");
        let page = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert!(
            page.rows.iter().all(|row| row.len() == 9),
            "rows grow with the column"
        );
        assert_eq!(page.rows[0][8], "NULL");
        assert_eq!(
            workspace.applied_ddl(),
            vec![sql.to_owned()],
            "the port records what it applied"
        );

        workspace
            .apply_ddl(
                "customers",
                "ALTER TABLE customers ALTER COLUMN score TYPE bigint;",
            )
            .unwrap();
        assert_eq!(
            workspace.table_columns("customers").unwrap()[8].sql_type,
            "bigint"
        );
    }

    #[test]
    fn a_failed_apply_changes_nothing() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        assert!(workspace
            .apply_ddl("orders", "ALTER TABLE orders ADD COLUMN x int;")
            .is_err());
        assert_eq!(
            workspace.table_columns("orders").unwrap().len(),
            2,
            "read-only tables refuse"
        );
        assert!(workspace
            .apply_ddl("customers", "DROP TABLE nope;")
            .is_err());
        assert_eq!(
            workspace.table_columns("customers").unwrap().len(),
            8,
            "unknown statements refuse"
        );
        assert!(
            workspace
                .apply_ddl(
                    "customers",
                    "ALTER TABLE customers ADD COLUMN ok int;\nDROP TABLE nope;"
                )
                .is_err(),
            "one bad line fails the whole script"
        );
        assert_eq!(
            workspace.table_columns("customers").unwrap().len(),
            8,
            "nothing half-applied"
        );
        assert!(
            workspace.applied_ddl().is_empty(),
            "failures are not recorded"
        );
    }

    #[test]
    fn an_invalid_setting_is_rejected_and_the_old_values_stay() {
        let mut workspace = MemoryWorkspace::new();
        let before = workspace.read_settings();
        assert_eq!(before.len(), 3);
        assert!(before.contains(&Setting::TablePageSize { rows: 25 }));

        assert!(workspace
            .write_setting(Setting::TablePageSize { rows: 0 })
            .is_err());
        assert!(workspace
            .write_setting(Setting::TablePageSize { rows: 501 })
            .is_err());
        assert!(workspace
            .write_setting(Setting::QueryMaxRows { rows: 0 })
            .is_err());
        assert_eq!(
            workspace.read_settings(),
            before,
            "failed writes leave every entry at its old value"
        );

        assert!(workspace
            .write_setting(Setting::TablePageSize { rows: 50 })
            .is_ok());
        let after = workspace.read_settings();
        assert!(after.contains(&Setting::TablePageSize { rows: 50 }));
        assert!(
            after.contains(&Setting::Theme { dark: true }),
            "the other entries survive"
        );
        assert!(after.contains(&Setting::QueryMaxRows { rows: 100 }));
        assert_eq!(after.len(), 3, "one entry per kind, replaced in place");

        assert!(workspace
            .write_setting(Setting::Theme { dark: false })
            .is_ok());
        assert!(workspace
            .read_settings()
            .contains(&Setting::Theme { dark: false }));
    }

    #[test]
    fn layout_settings_validate_and_dedup_per_region_key() {
        let mut workspace = MemoryWorkspace::new();

        // A valid size lands; the same region key replaces in place, a
        // different region key coexists (L1).
        workspace
            .write_setting(Setting::Layout {
                key: "navigator-width".into(),
                value: 280,
            })
            .unwrap();
        workspace
            .write_setting(Setting::Layout {
                key: "editor-height".into(),
                value: 320,
            })
            .unwrap();
        workspace
            .write_setting(Setting::Layout {
                key: "navigator-width".into(),
                value: 300,
            })
            .unwrap();
        let settings = workspace.read_settings();
        assert!(settings.contains(&Setting::Layout {
            key: "navigator-width".into(),
            value: 300
        }));
        assert!(settings.contains(&Setting::Layout {
            key: "editor-height".into(),
            value: 320
        }));
        let navigator = settings
            .iter()
            .filter(|s| matches!(s, Setting::Layout { key, .. } if key == "navigator-width"))
            .count();
        assert_eq!(navigator, 1, "one entry per region key");

        // The shared rules: bad keys and out-of-range sizes are refused, and
        // nothing changes.
        let before = workspace.read_settings();
        for bad in [
            Setting::Layout {
                key: "Navigator".into(),
                value: 280,
            },
            Setting::Layout {
                key: "has space".into(),
                value: 280,
            },
            Setting::Layout {
                key: "".into(),
                value: 280,
            },
            Setting::Layout {
                key: "navigator-width".into(),
                value: 0,
            },
            Setting::Layout {
                key: "navigator-width".into(),
                value: 10_001,
            },
        ] {
            assert!(
                workspace.write_setting(bad).is_err(),
                "the bad entry is refused"
            );
        }
        assert_eq!(
            workspace.read_settings(),
            before,
            "failed writes change nothing"
        );

        // The shared validation speaks for the scalar settings too.
        assert!(Setting::TablePageSize { rows: 999 }.validate().is_err());
        assert!(Setting::QueryMaxRows { rows: 0 }.validate().is_err());
        assert!(Setting::Layout {
            key: "editor-height".into(),
            value: 456
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn a_connection_without_an_address_cannot_be_saved() {
        let mut workspace = MemoryWorkspace::new();
        let before = workspace.connections().len();
        let draft = |name: &str, engine: &str, address: &str| ConnectionDraft {
            name: name.into(),
            engine: engine.into(),
            address: address.into(),
            ..ConnectionDraft::default()
        };
        assert!(workspace
            .save_connection(&draft("Remote", "postgres", ""))
            .is_err());
        assert!(workspace
            .save_connection(&draft("Remote", "postgres", "   "))
            .is_err());
        assert_eq!(workspace.connections().len(), before, "nothing was saved");

        // The credential fields ride along without changing the memory
        // port's semantics (E5: the draft is the shared shape).
        let mut postgres = draft("Remote Postgres", "postgres", "db.example.com:5432");
        postgres.username = "app".into();
        postgres.database = "analytics".into();
        workspace.save_connection(&postgres).unwrap();
        let saved = workspace.connections().last().unwrap();
        assert_eq!(saved.id, "remote-postgres");
        assert_eq!(saved.name, "Remote Postgres");
        assert_eq!(saved.engine, "postgres");
        assert_eq!(workspace.connections().len(), before + 1);

        // A duplicate name gets its own id; both are in the port, not the keychain.
        workspace
            .save_connection(&draft("Remote Postgres", "mysql", "db2.example.com:3306"))
            .unwrap();
        assert_eq!(
            workspace.connections().last().unwrap().id,
            "remote-postgres-2"
        );
    }

    #[test]
    fn snippets_add_update_delete_and_refuse_duplicates() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.snippets().is_empty());
        workspace
            .save_snippet("top", "SELECT * FROM customers LIMIT 10")
            .unwrap();
        assert_eq!(workspace.snippets().len(), 1);

        assert!(
            workspace.save_snippet("top", "SELECT 1").is_err(),
            "a duplicate name is refused"
        );
        assert_eq!(
            workspace.snippets()[0].sql,
            "SELECT * FROM customers LIMIT 10",
            "the refused save changed nothing"
        );

        workspace.update_snippet("top", "SELECT 1").unwrap();
        assert_eq!(
            workspace.snippets()[0].sql,
            "SELECT 1",
            "update changes the sql"
        );
        assert!(workspace.update_snippet("missing", "SELECT 1").is_err());
        assert!(
            workspace.save_snippet("  ", "SELECT 1").is_err(),
            "a name is required"
        );

        workspace.delete_snippet("top").unwrap();
        assert!(workspace.snippets().is_empty());
        assert!(workspace.delete_snippet("top").is_err());
    }

    #[test]
    fn an_export_needs_a_range_to_get_a_job() {
        let mut workspace = MemoryWorkspace::new();
        assert!(
            workspace.start_export("csv", "").is_err(),
            "no range, no job"
        );
        assert!(workspace.start_export("csv", "   ").is_err());
        assert!(
            workspace.start_export("exe", "customers").is_err(),
            "unknown format"
        );
        let job = workspace.start_export("csv", "customers").unwrap();
        assert_eq!(job, 1, "job ids count from one");
        let second = workspace.start_export("json", "orders").unwrap();
        assert_eq!(second, 2);
    }

    #[test]
    fn a_progress_snapshot_advances_and_failures_show_their_error_line() {
        let mut workspace = MemoryWorkspace::new();
        let job = workspace.start_export("csv", "customers").unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.percent, 0);
        assert_eq!(first.log.len(), 2);
        let second = workspace.job_progress(job).unwrap();
        assert_eq!(second.percent, 50);
        let third = workspace.job_progress(job).unwrap();
        assert_eq!(third.status, "completed");
        assert_eq!(third.percent, 100);
        assert!(third.log.last() == Some(&"done".to_owned()));
        let settled = workspace.job_progress(job).unwrap();
        assert_eq!(settled, third, "a finished job stays settled");

        let broken = workspace.start_export("broken", "customers").unwrap();
        workspace.job_progress(broken).unwrap();
        workspace.job_progress(broken).unwrap();
        let failed = workspace.job_progress(broken).unwrap();
        assert_eq!(failed.status, "failed");
        assert!(
            failed.log.last() == Some(&"the port cannot write broken files".to_owned()),
            "the failure's error line is in the snapshot"
        );

        let missing = workspace.start_export("csv", "no-such-table").unwrap();
        workspace.job_progress(missing).unwrap();
        workspace.job_progress(missing).unwrap();
        let failed = workspace.job_progress(missing).unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.log.last() == Some(&"no such table no-such-table".to_owned()));
        assert!(
            workspace.job_progress(999).is_none(),
            "an unknown job has no snapshot"
        );
    }

    #[test]
    fn a_backup_stops_and_reports_stopped() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.start_backup().is_err(), "no session, no backup");
        workspace.connect("local").unwrap();
        let job = workspace.start_backup().unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.log[1], "backing up the database");

        workspace.job_progress(job).unwrap();
        workspace.stop_job(job).unwrap();
        let stopped = workspace.job_progress(job).unwrap();
        assert_eq!(stopped.status, "stopped", "a stopped job reports stopped");
        assert!(stopped.log.last() == Some(&"stopped by the user".to_owned()));
        let settled = workspace.job_progress(job).unwrap();
        assert_eq!(settled, stopped, "stopped stays settled");

        assert!(
            workspace.stop_job(job).is_err(),
            "stopping twice is refused"
        );
        assert!(workspace.stop_job(999).is_err());
    }

    #[test]
    fn sync_endpoints_refuse_the_same_connection_twice() {
        let mut workspace = MemoryWorkspace::new();
        assert!(
            workspace.sync_endpoints("local", "local").is_err(),
            "the same end twice"
        );
        assert!(workspace.sync_endpoints("local", "missing").is_err());
        assert!(workspace.sync_endpoints("missing", "local").is_err());
        workspace.sync_endpoints("local", "replica").unwrap();

        // The replica connection sees no analytics tables.
        workspace.connect("replica").unwrap();
        let session = workspace.session().unwrap();
        assert!(session.tables.contains(&"customers".to_owned()));
        assert!(
            !session.tables.contains(&"catalog".to_owned()),
            "the replica lacks analytics"
        );
    }

    #[test]
    fn a_sync_preview_is_a_read_only_page() {
        let mut workspace = MemoryWorkspace::new();
        let page = workspace.sync_preview("local", "customers").unwrap();
        assert_eq!(page.table, "sync preview: customers");
        assert_eq!(page.rows.len(), 40);
        assert_eq!(page.read_only.as_deref(), Some("sync preview"));
        assert!(
            workspace.sync_preview("replica", "catalog").is_err(),
            "the replica cannot preview analytics"
        );
        assert!(workspace.sync_preview("local", "missing").is_err());
    }

    #[test]
    fn a_sync_without_selected_tables_gets_no_job() {
        let mut workspace = MemoryWorkspace::new();
        assert!(
            workspace.run_sync("local", "replica", &[]).is_err(),
            "no selection, no job"
        );
        assert!(workspace
            .run_sync("local", "local", &["customers".to_owned()])
            .is_err());
        let tables = vec!["customers".to_owned(), "orders".to_owned()];
        let job = workspace.run_sync("local", "replica", &tables).unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.log[1], "syncing local into replica");
        workspace.job_progress(job).unwrap();
        let done = workspace.job_progress(job).unwrap();
        assert_eq!(
            done.status, "completed",
            "a sync job finishes like the others"
        );
    }

    #[test]
    fn the_same_object_name_is_marked_on_both_ends() {
        let workspace = MemoryWorkspace::new();
        let objects = workspace.compare_objects("local", "replica").unwrap();
        let find = |name: &str| objects.iter().find(|object| object.name == name).unwrap();
        assert!(
            find("customers").present_on_target,
            "the same name is marked present"
        );
        assert!(find("orders").present_on_target);
        assert!(
            !find("catalog").present_on_target,
            "the replica lacks analytics"
        );
        assert_eq!(objects.len(), 3);
        assert!(workspace.compare_objects("local", "missing").is_err());
        // A connection against itself: every name matches.
        let same = workspace.compare_objects("local", "local").unwrap();
        assert!(same.iter().all(|object| object.present_on_target));
    }

    #[test]
    fn a_compare_plan_lists_missing_objects_and_can_be_empty() {
        let workspace = MemoryWorkspace::new();
        let plan = workspace.compare_plan("local", "replica").unwrap();
        assert!(
            plan.contains("CREATE TABLE catalog"),
            "the plan names the object the target lacks: {plan}"
        );
        assert!(plan.contains("-- missing on replica"));
        assert_eq!(
            workspace.compare_plan("local", "local").unwrap(),
            "",
            "the same endpoint twice: no differences, an empty plan"
        );
        assert!(workspace.compare_plan("local", "missing").is_err());
    }

    #[test]
    fn a_failed_plan_apply_keeps_the_plan() {
        let mut workspace = MemoryWorkspace::new();
        assert!(
            workspace.run_compare_plan("").is_err(),
            "an empty plan has nothing to apply"
        );
        assert!(workspace.run_compare_plan("   ").is_err());

        let plan = workspace.compare_plan("local", "replica").unwrap();
        let job = workspace.run_compare_plan(&plan).unwrap();
        workspace.job_progress(job).unwrap();
        workspace.job_progress(job).unwrap();
        let failed = workspace.job_progress(job).unwrap();
        assert_eq!(failed.status, "failed");
        assert!(
            failed.log.last() == Some(&"the replica cannot create analytics tables".to_owned()),
            "the failure says why"
        );
        // The difference is still there: the plan survives the failure.
        assert_eq!(workspace.compare_plan("local", "replica").unwrap(), plan);

        // A plan the port can apply finishes.
        let job = workspace
            .run_compare_plan("CREATE TABLE other (...);")
            .unwrap();
        workspace.job_progress(job).unwrap();
        workspace.job_progress(job).unwrap();
        let done = workspace.job_progress(job).unwrap();
        assert_eq!(done.status, "completed");
    }

    #[test]
    fn a_transfer_without_pairs_gets_no_job() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.run_transfer(&[]).is_err(), "no pairs, no job");
        let pairs = vec![("id".to_owned(), "id".to_owned())];
        let job = workspace.run_transfer(&pairs).unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.log[1], "transferring 1 column pairs");
        workspace.job_progress(job).unwrap();
        assert_eq!(workspace.job_progress(job).unwrap().status, "completed");
    }

    #[test]
    fn a_dashboard_starts_with_no_widgets() {
        let workspace = MemoryWorkspace::new();
        assert!(
            workspace.dashboard_widgets().is_empty(),
            "the widget list starts empty"
        );
    }

    #[test]
    fn a_bar_widget_gets_its_integers_from_the_port() {
        let mut workspace = MemoryWorkspace::new();
        assert!(
            workspace.save_dashboard_widget("", "select 1").is_err(),
            "a title is required"
        );
        let id = workspace
            .save_dashboard_widget("Weekly", "select 1")
            .unwrap();
        assert_eq!(id, 1);
        let bars = workspace.widget_bars(id).unwrap();
        assert_eq!(bars.first().map(|(label, _)| label.as_str()), Some("mon"));
        assert_eq!(bars[0].1, 0, "the zero bar is in the data");
        assert_eq!(bars.last().unwrap().1, 12, "the maximum is in the data");
        assert!(workspace.widget_bars(999).is_err());
        let second = workspace
            .save_dashboard_widget("Monthly", "select 2")
            .unwrap();
        assert_eq!(second, 2);
        assert_eq!(workspace.dashboard_widgets().len(), 2);
    }

    #[test]
    fn workflows_read_sorted_by_name() {
        let workspace = MemoryWorkspace::new();
        let workflows = workspace.workflows();
        let names: Vec<&str> = workflows.iter().map(|flow| flow.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["audit", "nightly-backup", "weekly-report"],
            "reads sort by name"
        );
        assert_eq!(workflows[0].steps, vec!["compare structures", "apply plan"]);
    }

    #[test]
    fn saving_a_workflow_replaces_its_steps() {
        let mut workspace = MemoryWorkspace::new();
        assert!(
            workspace.save_workflow("", &[]).is_err(),
            "a name is required"
        );
        workspace
            .save_workflow(
                "audit",
                &["reordered".to_owned(), "compare structures".to_owned()],
            )
            .unwrap();
        let audit = workspace
            .workflows()
            .into_iter()
            .find(|flow| flow.name == "audit")
            .unwrap();
        assert_eq!(audit.steps, vec!["reordered", "compare structures"]);
        let count = workspace.workflows().len();
        workspace
            .save_workflow("fresh", &["one".to_owned()])
            .unwrap();
        assert_eq!(workspace.workflows().len(), count + 1, "a new name appends");
    }

    #[test]
    fn workflow_runs_separate_failing_and_successful_steps() {
        let mut workspace = MemoryWorkspace::new();
        let results = workspace.run_workflow("audit").unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].step, "compare structures");
        assert!(results[0].ok);
        assert_eq!(results[0].summary, "ok: 12 rows");
        assert_eq!(results[1].step, "apply plan");
        assert!(!results[1].ok, "the applying step fails");
        assert_eq!(results[1].summary, "the replica refused the plan");
        assert!(workspace.run_workflow("missing").is_err());
        let backup = workspace.run_workflow("nightly-backup").unwrap();
        assert!(
            backup.iter().all(|step| step.ok),
            "nothing to apply, all steps pass"
        );
    }

    #[test]
    fn table_ddl_returns_the_create_statement_or_empty() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let ddl = workspace.table_ddl("customers").unwrap();
        assert!(ddl.starts_with("CREATE TABLE customers ("));
        assert!(ddl.contains("  id text"));
        assert!(ddl.contains("  name text"));
        assert!(ddl.ends_with(");"));
        assert_eq!(
            workspace.table_ddl("catalog").unwrap(),
            "",
            "external schema tables have no DDL to show"
        );
        assert!(workspace.table_ddl("missing").is_err());
    }

    #[test]
    fn table_columns_lists_one_entry_per_column() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let columns = workspace.table_columns("customers").unwrap();
        assert_eq!(columns.len(), 8, "one row per column");
        assert_eq!(columns[0].name, "id");
        assert_eq!(columns[0].sql_type, "text");
        assert!(!columns[0].nullable, "the first column is the key");
        assert_eq!(columns[1].name, "name");
        assert!(columns[1].nullable);
        assert!(workspace.table_columns("missing").is_err());
    }

    #[test]
    fn a_compound_index_keeps_its_column_order() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let indexes = workspace.table_indexes("customers").unwrap();
        assert_eq!(indexes.len(), 2);
        assert_eq!(indexes[0].name, "customers_pkey");
        assert_eq!(indexes[0].columns, vec!["id"]);
        assert!(indexes[0].unique);
        assert_eq!(
            indexes[1].columns,
            vec!["name", "city"],
            "the compound index keeps its column order"
        );
        assert!(!indexes[1].unique);
        assert!(workspace.table_indexes("missing").is_err());
    }

    #[test]
    fn a_compound_foreign_key_is_not_split() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let keys = workspace.table_foreign_keys("orders").unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "orders_customer_fk");
        assert_eq!(
            keys[0].columns,
            vec!["id", "name"],
            "this side stays together"
        );
        assert_eq!(keys[0].target_table, "customers");
        assert_eq!(
            keys[0].target_columns,
            vec!["id", "name"],
            "the target side too"
        );
        assert!(workspace.table_foreign_keys("catalog").unwrap().is_empty());
        assert!(workspace.table_foreign_keys("missing").is_err());
    }

    #[test]
    fn a_read_only_table_rejects_staging_and_commit() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let page = workspace.open_table("orders", 0, 25, None, None).unwrap();
        assert_eq!(page.read_only.as_deref(), Some("no primary key"));
        assert!(
            workspace.stage_cell(0, "name", "x".into()).is_err(),
            "staging on a read-only page is rejected"
        );
        assert!(
            workspace.commit_staged().is_err(),
            "committing while a read-only page is open is rejected"
        );

        workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        workspace.stage_cell(0, "name", "ok".into()).unwrap();
        assert_eq!(
            workspace.commit_staged().unwrap(),
            1,
            "a writable table is unaffected"
        );
    }

    #[test]
    fn staging_addresses_the_row_behind_the_open_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace
            .open_table("customers", 1, 25, None, None)
            .unwrap();
        workspace.stage_cell(0, "name", "staged".into()).unwrap();
        let again = workspace
            .open_table("customers", 1, 25, None, None)
            .unwrap();
        assert_eq!(again.rows[0][1], "staged", "page 2 row 0 is stored row 25");
        let first = workspace
            .open_table("customers", 0, 25, None, None)
            .unwrap();
        assert_eq!(first.rows[0][1], "customer-0");
        assert_eq!(first.rows[24][1], "customer-24");
    }

    #[test]
    fn the_unwired_adapter_answers_every_method_with_the_fixed_error() {
        let mut adapter = UnimplementedAdapter;
        assert_eq!(
            adapter.list_connections().unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "list_connections is not wired yet"
        );
        assert_eq!(
            adapter
                .read_table_page("customers", 0, 25, None, None)
                .unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "read_table_page is not wired yet"
        );
        let edits = vec![(0u32, String::from("name"), String::from("alice"))];
        assert_eq!(
            adapter.commit_cells("customers", &edits).unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "commit_cells is not wired yet"
        );
        // N13: the workflow and dashboard surface starts declined like every
        // other host method; the memory port stays the fallback.
        assert_eq!(
            adapter.dashboard_widgets().unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "dashboard_widgets is not wired yet"
        );
        assert_eq!(
            adapter
                .save_dashboard_widget("Users", "SELECT 1")
                .unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "save_dashboard_widget is not wired yet"
        );
        assert_eq!(
            adapter.widget_bars(1).unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "widget_bars is not wired yet"
        );
        assert_eq!(
            adapter.workflows().unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "workflows is not wired yet"
        );
        assert_eq!(
            adapter
                .save_workflow("nightly", &["SELECT 1".to_owned()])
                .unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "save_workflow is not wired yet"
        );
        assert_eq!(
            adapter.run_workflow("nightly").unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "run_workflow is not wired yet"
        );
    }
}
