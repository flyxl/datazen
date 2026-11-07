/**
 * Web 后端的 `unsupportedMethods` 声明（WR-E2）。
 *
 * 两个来源，全部 `capability`、零 `notImplemented`（契约 §7：能力差异走
 * `BackendTransport.unsupportedMethods` + guard 的 `CapabilityUnsupported`，
 * **使用时**才失败；「此形态永不可服务」与「尚未实现」不得混报）：
 *
 * 1. **driverGateway 的 `open_base64_with_dialog`**（契约 §6.9/§8）：对话框
 *    语义属于平台宿主而非后端，web 形态用浏览器 API（或功能缺位）承接，
 *    **永不上 HTTP**；
 * 2. **`LegacyHostMethodMap` 全部键**（约 200 个 snake_case 桌面宿主命令，
 *    `packages/backend-client/src/legacyHostCommands.ts`）：契约 §8 明文
 *    「legacy IPC 命令轨不镜像」——它们随 WR-A2 command 薄壳化逐域收敛进
 *    `MethodMap` 的内核域，收敛一个域、本文件减一域；域客户端不在此列的
 *    （driverGateway 四方法已上 HTTP）不受影响。
 *
 * **已收敛域账**（每域收敛时从本表删除，两侧「域客户端方法 + HTTP 路由」
 * 同时点亮）：
 * - 查询资产域（W8 刀3a web 收口）：`add_favorite_query` /
 *   `delete_favorite_query` / `get_favorite_queries` /
 *   `get_query_history_page` / `delete_query_history` /
 *   `clear_query_history` / `purge_history` → `queryAssets` 域客户端
 *   （契约 §6.12）。桌面文件系统语义两命令**留表**：`get_favorites_root` /
 *   `refresh_favorites`（服务端收藏是库行，无根目录可扫）。
 * - Schema Diff 域（W8 刀3b web 收口）：profiles 读写删、compare、plans
 *   二、deploy、jobs 读回/取消/恢复核验十一命令 → `schemaDiff` 域客户端
 *   （契约 §6.13）。迁移历史读面 `get_migration_run` / `list_migration_runs`
 *   **留表**（契约裁决：随 sync 域收敛）。
 *
 * 运行时清单由 `LegacyCommandName` 类型钉住：写错名字过不了
 * `readonly LegacyCommandName[]` 注解，漏名字过不了文末的编译期穷尽断言
 * （`Exclude` 结果必须为 `never`）。守不住时两种失败都会指向这里。
 *
 * WR-F1（三缺口合拢）登记：mcp / security / aiHost 三域**不在本表**——
 * 它们的契约键（`mcp.*` / `security.*` / `aiHost.*`，点分前缀）不在
 * `MethodMap`（wire-direct 域，见 `packages/backend-client/src/
 * {mcpClient,securityClient}.ts` 头注），不经 guard 的
 * `unsupportedMethods` 面，也不经 `transport.call`；与上列 legacy
 * snake_case 键（含 11 个 `mcp_*` 宿主命令）零重名，无冲突可记。路由形状
 * 见 `httpRoutes.ts` 的 `WR_F1_HTTP_ROUTES`。
 */

import type { LegacyCommandName, MethodMap, UnsupportedMethodKind } from '@datazen/backend-client';

/** driverGateway 五键中唯一不上 HTTP 的（§6.9 表的「排除」行）。 */
const CAPABILITY_EXCLUDED: readonly (keyof MethodMap)[] = ['open_base64_with_dialog'];

/**
 * 不上 HTTP 的 legacy 宿主命令全集（契约 §8 的排除面）。
 * 与 `LegacyCommandName` 逐字对齐——穷尽性由下方编译期断言看守。
 */
const LEGACY_HTTP_OFF: readonly LegacyCommandName[] = [
  'abort_save',
  'ai_analyze_explain',
  'ai_analyze_queries',
  'ai_apply_write',
  'ai_cancel',
  'ai_connection_context_counts',
  'ai_connection_notes_get',
  'ai_connection_notes_set',
  'ai_chat',
  'ai_delete_config',
  'ai_deny_write',
  'ai_diagnose_connection',
  'ai_diagnose_error',
  'ai_fetch_remote_models',
  'ai_generate_schema_doc',
  'ai_generate_sql',
  'ai_get_config',
  'ai_get_providers',
  'ai_get_settings_config',
  'ai_list_write_audit',
  'ai_parse_filter',
  'ai_propose_write',
  'ai_grant_editor_session',
  'ai_revoke_editor_grant',
  'ai_touch_editor_grant',
  'ai_save_config',
  'ai_save_settings_config',
  'ai_set_active_profile',
  'ai_validate_config',
  'append_save_text',
  'apply_write',
  'apply_data_sync',
  'apply_data_transfer_job',
  'authorize_monitor_widget_schedule',
  'backup_database',
  'begin_save_with_dialog',
  'begin_session_transaction',
  'cancel_data_sync',
  'cancel_data_transfer',
  'cancel_query',
  'check_sync_conflicts',
  'classify_data_sync_pair',
  'classify_transfer_pair',
  'close_database',
  'commit_session_transaction',
  'connect',
  'connect_dedicated',
  'context_get_dir',
  'context_list_files',
  'context_read_files',
  'create_widget_from_sql',
  'create_widget_from_workflow',
  'delete_connection',
  'delete_dashboard',
  'delete_sync_profile',
  'delete_sync_task',
  'delete_transfer_profile',
  'delete_tunnel',
  'detect_connection_import_path',
  'disconnect',
  'duplicate_connection',
  'execute_data_sync',
  'execute_data_transfer',
  'execute_wapp_driver_command',
  'export_app_data',
  'export_artifact',
  'export_connections',
  'export_dashboard_with_dialog',
  'export_tables_stream',
  'find_dashboard_workflow_refs',
  'finish_save',
  'generate_data_sync_sql',
  'get_available_drivers',
  'get_connection_info',
  'get_connections',
  'get_dashboard',
  'get_data_sync_comparison_page',
  'get_data_sync_job',
  'get_data_sync_job_details',
  'get_data_sync_job_preview',
  'get_favorites_root',
  'get_groups',
  'get_job',
  'get_migration_run',
  'get_monitor_widget_schedule_grant',
  'get_open_databases',
  'get_privileges',
  'get_sync_profiles',
  'get_sync_tasks',
  'get_transfer_job_details',
  'get_transfer_profiles',
  'get_tunnel',
  'get_tunnel_summaries',
  'get_tunnel_usage',
  'get_tunnels',
  'get_wapp_manifest',
  'get_widget_run',
  'import_app_data',
  'import_connections_at_path',
  'import_connections_from_app',
  'import_connections_preview',
  'import_connections_with_dialog',
  'import_dashboard_with_dialog',
  'inspect_data_sync',
  'inspect_data_transfer',
  'inspect_sql_file_transfer',
  'inspect_wapp_package_with_dialog',
  'install_wapp',
  'list_dashboards',
  'list_data_sync_jobs',
  'list_jobs',
  'list_migration_runs',
  'list_wapps',
  'list_widget_runs',
  'mcp_client_call_tool',
  'mcp_client_connect',
  'mcp_client_disconnect',
  'mcp_client_list',
  'mcp_client_tools',
  'mcp_get_status',
  'mcp_list_all_tools',
  'mcp_reload',
  'mcp_start_stdio',
  'mcp_stop',
  'onboarding_complete',
  'open_text_with_dialog',
  'pick_data_transfer_sql_file',
  'pick_import_file',
  'ping_connection',
  'prepare_data_transfer_job',
  'preview_data_transfer',
  'prompt_list',
  'prompt_remove_override',
  'prompt_set_override',
  'propose_write',
  'read_wapp_file',
  'refresh_favorites',
  'release_connection',
  'remove_wapp',
  'reorder_connections',
  'restart_app',
  'restore_sql_file',
  'revoke_monitor_widget_schedule',
  'rollback_session_transaction',
  'run_dashboard_widget',
  'save_connection',
  'save_dashboard',
  'save_encryption_key_with_dialog',
  'save_groups',
  'save_sql_file',
  'save_sync_profile',
  'save_transfer_profile',
  'save_tunnel',
  'seed_sample_db',
  'session_transaction_status',
  'set_dashboard_refresh_paused',
  'set_wapp_enabled',
  'start_data_sync_apply_job',
  'start_data_sync_prepare_job',
  'test_connection',
  'test_tunnel',
  'update_hidden_widget_sql',
  'verify_data_sync_recovery',
  'wapp_audit_log',
  'wapp_session',
  'wapp_storage_get',
  'wapp_storage_remove',
  'wapp_storage_set',
  'workflow_delete',
  'workflow_execute',
  'workflow_get',
  'workflow_get_dir',
  'workflow_get_yaml',
  'workflow_history_clear',
  'workflow_history_get',
  'workflow_history_list',
  'workflow_list',
  'workflow_reload',
  'workflow_save',
  'workflow_save_yaml',
];

/** `AssertNever<X>` 仅在 `X` 为 `never` 时可实例化——漏名即在此处编译失败。 */
type AssertNever<T extends never> = T;

/**
 * 编译期穷尽证明：`LegacyCommandName` 减去清单必须为空。导出以免被当作
 * 未使用的本地类型；它永远不实例化任何值。
 */
export type LegacyCoverageProof = AssertNever<
  Exclude<LegacyCommandName, (typeof LEGACY_HTTP_OFF)[number]>
>;

/**
 * 组装 web 后端的不可服务声明。guard 据此在使用点抛
 * `CapabilityUnsupported`（405 的 HTTP 映射，契约 §7）；分发表之外万一漏网
 * 的键，由 `backendTransport.ts` 的 call() 兜底抛同码错误。
 */
export function buildWebUnsupportedMethods(): Partial<
  Record<keyof MethodMap, UnsupportedMethodKind>
> {
  const gaps: Partial<Record<keyof MethodMap, UnsupportedMethodKind>> = {};
  for (const name of CAPABILITY_EXCLUDED) gaps[name] = 'capability';
  for (const name of LEGACY_HTTP_OFF) gaps[name] = 'capability';
  return gaps;
}
