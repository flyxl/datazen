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
 * W8 web 半边收口（刀3c/3d/3e）：workflow 十一、dashboard 十五、dataSync
 * 二十命令随域客户端收敛消亡（与 `webUnsupportedMethods.ts` 的
 * LEGACY_HTTP_OFF 同步裁剪，保住穷尽断言）；`workflow_get_dir` 与两个
 * `*_with_dialog` 桌面专属面、`apply_data_sync` / `get_data_sync_comparison_page`
 * （不在契约 §6.16 二十二方法内的桌面专属同步面）留轨。
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
  | 'abort_save'
  | 'add_favorite_query'
  | 'ai_analyze_explain'
  | 'ai_analyze_queries'
  | 'ai_apply_write'
  | 'ai_cancel'
  | 'ai_chat'
  | 'ai_delete_config'
  | 'ai_deny_write'
  | 'ai_diagnose_connection'
  | 'ai_diagnose_error'
  | 'ai_fetch_remote_models'
  | 'ai_generate_schema_doc'
  | 'ai_generate_sql'
  | 'ai_get_config'
  | 'ai_get_providers'
  | 'ai_get_settings_config'
  | 'ai_list_skills'
  | 'ai_list_write_audit'
  | 'ai_parse_filter'
  | 'ai_propose_write'
  | 'ai_grant_editor_session'
  | 'ai_revoke_editor_grant'
  | 'ai_touch_editor_grant'
  | 'ai_save_config'
  | 'ai_save_settings_config'
  | 'ai_set_active_profile'
  | 'ai_validate_config'
  | 'ai_connection_notes_get'
  | 'ai_connection_notes_set'
  | 'ai_connection_context_counts'
  | 'ai_memory_list'
  | 'ai_org_memory_list'
  | 'ai_memory_search'
  | 'ai_memory_stats'
  | 'ai_memory_create'
  | 'ai_memory_update'
  | 'ai_memory_confirm'
  | 'ai_memory_reject'
  | 'ai_memory_delete'
  | 'ai_memory_clear'
  | 'ai_memory_export'
  | 'ai_memory_correction_candidate'
  | 'ai_memory_extract_candidates'
  | 'ai_memory_mark_stale'
  | 'ai_sessions_list'
  | 'ai_sessions_get'
  | 'ai_sessions_upsert'
  | 'ai_sessions_delete'
  | 'ai_sessions_export'
  | 'append_save_text'
  | 'apply_write'
  | 'apply_data_sync'
  | 'apply_data_transfer_job'
  | 'backup_database'
  | 'begin_save_with_dialog'
  | 'begin_session_transaction'
  | 'cancel_data_transfer'
  | 'cancel_query'
  | 'cancel_schema_diff_deploy'
  | 'classify_transfer_pair'
  | 'clear_query_history'
  | 'close_database'
  | 'commit_session_transaction'
  | 'compare_table_schemas'
  | 'connect'
  | 'connect_dedicated'
  | 'context_get_dir'
  | 'context_list_files'
  | 'context_read_files'
  | 'delete_connection'
  | 'delete_favorite_query'
  | 'delete_query_history'
  | 'delete_schema_diff_profile'
  | 'delete_transfer_profile'
  | 'delete_tunnel'
  | 'detect_connection_import_path'
  | 'disconnect'
  | 'duplicate_connection'
  | 'execute_data_transfer'
  | 'execute_schema_diff_deploy'
  | 'execute_wapp_driver_command'
  | 'export_app_data'
  | 'export_artifact'
  | 'export_connections'
  | 'export_dashboard_with_dialog'
  | 'export_tables_stream'
  | 'finish_save'
  | 'get_available_drivers'
  | 'get_connection_info'
  | 'get_connections'
  | 'get_data_sync_comparison_page'
  | 'get_favorite_queries'
  | 'get_favorites_root'
  | 'get_groups'
  | 'get_job'
  | 'get_open_databases'
  | 'get_privileges'
  | 'get_query_history_page'
  | 'get_schema_diff_job_details'
  | 'get_schema_diff_profiles'
  | 'get_transfer_job_details'
  | 'get_transfer_profiles'
  | 'get_tunnel'
  | 'get_tunnel_summaries'
  | 'get_tunnel_usage'
  | 'get_tunnels'
  | 'get_wapp_manifest'
  | 'import_app_data'
  | 'import_connections_at_path'
  | 'import_connections_from_app'
  | 'import_connections_preview'
  | 'import_connections_with_dialog'
  | 'import_dashboard_with_dialog'
  | 'inspect_data_transfer'
  | 'inspect_sql_file_transfer'
  | 'inspect_language_pack_file_with_dialog'
  | 'inspect_wapp_package_with_dialog'
  | 'install_language_pack'
  | 'install_wapp'
  | 'list_language_packs'
  | 'list_jobs'
  | 'list_schema_diff_jobs'
  | 'list_wapps'
  | 'mcp_client_call_tool'
  | 'mcp_client_connect'
  | 'mcp_client_disconnect'
  | 'mcp_client_list'
  | 'mcp_cancel_all_approvals'
  | 'mcp_cancel_approval'
  | 'mcp_client_tools'
  | 'mcp_get_status'
  | 'mcp_http_disable'
  | 'mcp_http_enable'
  | 'mcp_http_regenerate_token'
  | 'mcp_http_reload'
  | 'mcp_http_status'
  | 'mcp_list_all_tools'
  | 'mcp_list_audit_log'
  | 'mcp_reload'
  | 'mcp_respond_approval'
  | 'mcp_start_stdio'
  | 'mcp_stop'
  | 'onboarding_complete'
  | 'open_text_with_dialog'
  | 'pick_data_transfer_sql_file'
  | 'pick_import_file'
  | 'ping_connection'
  | 'prepare_data_transfer_job'
  | 'prepare_schema_diff_plan'
  | 'prepare_schema_unified_plan'
  | 'preview_data_transfer'
  | 'prompt_list'
  | 'prompt_remove_override'
  | 'prompt_set_override'
  | 'propose_write'
  | 'purge_history'
  | 'read_language_pack_messages'
  | 'read_wapp_file'
  | 'refresh_favorites'
  | 'release_connection'
  | 'remove_language_pack'
  | 'remove_wapp'
  | 'reorder_connections'
  | 'restart_app'
  | 'restore_sql_file'
  | 'rollback_session_transaction'
  | 'save_connection'
  | 'save_encryption_key_with_dialog'
  | 'save_groups'
  | 'save_schema_diff_profile'
  | 'save_sql_file'
  | 'save_transfer_profile'
  | 'save_tunnel'
  | 'seed_sample_db'
  | 'session_transaction_status'
  | 'set_wapp_enabled'
  | 'test_connection'
  | 'test_tunnel'
  | 'verify_schema_diff_job_recovery'
  | 'wapp_audit_log'
  | 'wapp_session'
  | 'wapp_storage_get'
  | 'wapp_storage_remove'
  | 'wapp_storage_set'
  | 'workflow_get_dir';
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
