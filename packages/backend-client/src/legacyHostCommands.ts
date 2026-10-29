/**
 * The legacy host-command rail (form-split F6 prep — R6 ledger burn).
 *
 * **Transitional escape hatch, exactly the `DriverGatewayMethodMap`
 * lifecycle** (plan §5.3): the `src/commands/*` wrappers and a handful of
 * feature hooks speak these host commands directly over the transport, and
 * they are declared in `MethodMap` only so those call sites could migrate
 * off `@tauri-apps/api/core` without changing a single one of them — the
 * migration swaps the import line, the calls stay byte-identical.
 *
 * Names stay snake_case on purpose: they are the *existing* IPC command
 * names, not future kernel names. The whole rail — this union, the mapped
 * slice below, and the `legacyInvoke` helper — shrinks as the kernel
 * implements real domains (the D-phase) and is deleted when the last
 * command leaves it.
 *
 * The payload type is deliberately loose (`Record<string, unknown>`): the
 * typed contracts live at the call-site wrappers (those wrappers ARE the
 * current documentation of the wire), and the desktop transport passes the
 * payload through flat — byte-identical to the `invoke` it replaces,
 * whatever shape the call site uses (flat bags, nested `request`, plugin
 * argument bags).
 */

import type { MethodMap } from './transport';
import { useBackendClient } from './client';

/** Every host command still spoken directly by the legacy invoke wrappers. */
export type LegacyCommandName =
  'abort_save' | 'add_favorite_query' | 'ai_analyze_explain' | 'ai_analyze_queries' |
  'ai_cancel' | 'ai_chat' | 'ai_delete_config' | 'ai_diagnose_connection' |
  'ai_diagnose_error' | 'ai_fetch_remote_models' | 'ai_generate_schema_doc' | 'ai_generate_sql' |
  'ai_get_config' | 'ai_get_providers' | 'ai_get_settings_config' | 'ai_parse_filter' |
  'ai_grant_editor_session' | 'ai_revoke_editor_grant' | 'ai_touch_editor_grant' |
  'ai_save_config' | 'ai_save_settings_config' | 'ai_set_active_profile' | 'ai_validate_config' |
  'append_save_text' | 'apply_data_sync' | 'apply_data_transfer_job' | 'authorize_monitor_widget_schedule' |
  'backup_database' | 'begin_save_with_dialog' | 'begin_session_transaction' | 'cancel_data_sync' |
  'cancel_data_transfer' | 'cancel_query' | 'cancel_schema_diff_deploy' | 'check_sync_conflicts' |
  'classify_data_sync_pair' | 'classify_transfer_pair' | 'clear_query_history' | 'close_database' |
  'commit_session_transaction' | 'compare_table_schemas' | 'connect' | 'connect_dedicated' |
  'context_get_dir' | 'context_list_files' | 'context_read_files' | 'create_widget_from_sql' |
  'create_widget_from_workflow' | 'delete_connection' | 'delete_dashboard' | 'delete_favorite_query' |
  'delete_query_history' | 'delete_schema_diff_profile' | 'delete_sync_profile' | 'delete_sync_task' |
  'delete_transfer_profile' | 'delete_tunnel' | 'detect_connection_import_path' | 'disconnect' |
  'duplicate_connection' | 'execute_data_sync' | 'execute_data_transfer' | 'execute_schema_diff_deploy' |
  'execute_wapp_driver_command' | 'export_app_data' | 'export_artifact' | 'export_connections' |
  'export_dashboard_with_dialog' | 'export_tables_stream' | 'find_dashboard_workflow_refs' | 'finish_save' |
  'generate_data_sync_sql' | 'get_available_drivers' | 'get_connection_info' | 'get_connections' |
  'get_dashboard' | 'get_data_sync_comparison_page' | 'get_data_sync_job' | 'get_data_sync_job_details' |
  'get_data_sync_job_preview' | 'get_favorite_queries' | 'get_favorites_root' | 'get_groups' |
  'get_job' | 'get_migration_run' | 'get_monitor_widget_schedule_grant' | 'get_open_databases' |
  'get_privileges' | 'get_query_history_page' | 'get_schema_diff_job_details' | 'get_schema_diff_profiles' |
  'get_sync_profiles' | 'get_sync_tasks' | 'get_transfer_job_details' | 'get_transfer_profiles' |
  'get_tunnel' | 'get_tunnel_summaries' | 'get_tunnel_usage' | 'get_tunnels' |
  'get_wapp_manifest' | 'get_widget_run' | 'import_app_data' | 'import_connections_at_path' |
  'import_connections_from_app' | 'import_connections_preview' | 'import_connections_with_dialog' | 'import_dashboard_with_dialog' |
  'inspect_data_sync' | 'inspect_data_transfer' | 'inspect_sql_file_transfer' | 'inspect_wapp_package_with_dialog' |
  'install_wapp' | 'list_dashboards' | 'list_data_sync_jobs' | 'list_jobs' |
  'list_migration_runs' | 'list_schema_diff_jobs' | 'list_wapps' | 'list_widget_runs' |
  'mcp_client_call_tool' | 'mcp_client_connect' | 'mcp_client_disconnect' | 'mcp_client_list' |
  'mcp_client_tools' | 'mcp_get_status' | 'mcp_list_all_tools' | 'mcp_reload' |
  'mcp_start_stdio' | 'mcp_stop' | 'onboarding_complete' | 'open_text_with_dialog' |
  'pick_data_transfer_sql_file' | 'pick_import_file' | 'ping_connection' | 'prepare_data_transfer_job' |
  'prepare_schema_diff_plan' | 'prepare_schema_unified_plan' |
  'preview_data_transfer' | 'prompt_list' | 'prompt_remove_override' | 'prompt_set_override' |
  'purge_history' | 'read_wapp_file' | 'refresh_favorites' | 'release_connection' |
  'remove_wapp' | 'reorder_connections' | 'restart_app' | 'restore_sql_file' |
  'revoke_monitor_widget_schedule' | 'rollback_session_transaction' | 'run_dashboard_widget' | 'save_connection' |
  'save_dashboard' | 'save_encryption_key_with_dialog' | 'save_groups' | 'save_schema_diff_profile' |
  'save_sql_file' | 'save_sync_profile' | 'save_transfer_profile' | 'save_tunnel' |
  'seed_sample_db' | 'session_transaction_status' | 'set_dashboard_refresh_paused' | 'set_wapp_enabled' |
  'start_data_sync_apply_job' | 'start_data_sync_prepare_job' | 'test_connection' | 'test_tunnel' |
  'update_hidden_widget_sql' | 'verify_data_sync_recovery' | 'verify_schema_diff_job_recovery' | 'wapp_audit_log' |
  'wapp_session' | 'wapp_storage_get' | 'wapp_storage_remove' | 'wapp_storage_set' |
  'workflow_delete' | 'workflow_execute' | 'workflow_get' | 'workflow_get_dir' |
  'workflow_get_yaml' | 'workflow_history_clear' | 'workflow_history_get' | 'workflow_history_list' |
  'workflow_list' | 'workflow_reload' | 'workflow_save' | 'workflow_save_yaml';

/**
 * The legacy host-command slice of `MethodMap`.
 *
 * A mapped type over the union: every entry is `{ request: Record<string,
 * unknown>; response: unknown }` — the name is compile-checked (the wiring
 * test cross-checks it against the host registration), the payload shape is
 * the call site's own business, exactly as with the direct `invoke` this
 * replaces.
 */
export type LegacyHostMethodMap = {
  [command in LegacyCommandName]: { request: Record<string, unknown>; response: unknown };
};

/**
 * The legacy-rail `invoke` replacement: same name, same generic, same
 * argument convention as `@tauri-apps/api/core`'s `invoke`, so a migrating
 * file swaps one import line and nothing else.
 *
 * The payload is forwarded verbatim — `undefined` when omitted, exactly as
 * `invoke` does — and the desktop transport routes snake_case names with a
 * flat passthrough, which keeps every call byte-identical to the `invoke`
 * it replaces, whatever shape the call site uses.
 */
export function legacyInvoke<T = unknown>(
  command: LegacyCommandName,
  payload?: Record<string, unknown>,
): Promise<T> {
  return useBackendClient().transport.call(
    command,
    payload as MethodMap[LegacyCommandName]['request'],
  ) as Promise<T>;
}
