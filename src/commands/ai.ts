import { invoke } from '@tauri-apps/api/core';
import { requirePlatform } from '../platform/contracts';

type UnlistenFn = () => void;
import type {
  AiChatMessage,
  AiProviderConfig,
  AiSettingsConfig,
  ConnectionDiagnosis,
  DiagnosisResult,
  ExplainAnalysis,
  FilterCondition,
  HistoryEntry,
  HistoryListItem,
  McpClientStatus,
  McpServerConfig,
  McpToolInfo,
  ModelInfo,
  QueryAnalysis,
  WorkflowDefinition,
  WorkflowExecutionResult,
  WorkflowListItem,
  ProviderListItem,
  StreamChunkPayload,
  StreamErrorPayload,
} from '../types';
import { mergeDraftCommandFields } from '../windows/workflow/draftBridge';
import type {
  AiGrantReceipt,
  AiGrantRequest,
  AiTargetClaim,
  CancelReceipt,
} from '../ai/targetClaim';

export type {
  AiGrantReceipt,
  AiGrantRequest,
  AiSessionHandle,
  AiTargetClaim,
  CancelReceipt,
} from '../ai/targetClaim';

export type PromptScenario =
  | 'nl2sql'
  | 'diagnose'
  | 'nl_filter'
  | 'schema_doc_select_tables'
  | 'schema_doc'
  | 'connection_diagnose'
  | 'query_summary'
  | 'explain_analysis'
  | 'chat'
  | 'workflow_generate';

export type PromptSource = 'default' | 'driver' | 'user';

type E2eInvokeCall = { cmd: string; args: unknown };

function e2eStubInvoke<T>(cmd: string, args: unknown, real: () => Promise<T>): Promise<T> {
  const calls = (globalThis as typeof globalThis & { __invokeCalls?: E2eInvokeCall[] })
    .__invokeCalls;
  if (Array.isArray(calls)) {
    calls.push({ cmd, args });
    return Promise.resolve('' as T);
  }
  return real();
}

/**
 * Invoke that always reaches the native host.
 *
 * New AI runtime-lifecycle commands (grant/revoke/touch) deliberately bypass
 * the E2E stub capture used by `ai_chat`/`ai_generate_sql`: the stub returns
 * an empty string for every captured call, which would be an invalid receipt
 * and must never be mistaken for a granted session.
 */
function nativeInvoke<T>(cmd: string, args?: object): Promise<T> {
  return invoke<T>(cmd, args as Record<string, unknown> | undefined);
}

export interface PromptInfo {
  scenario: PromptScenario;
  label: string;
  source: PromptSource;
  system: string;
  defaultSystem: string;
}

export interface PromptOverrideEntry {
  driverType: string;
  scenario: PromptScenario;
  system: string;
}

export const aiCommands = {
  getProviders: () => invoke<ProviderListItem[]>('ai_get_providers'),
  fetchRemoteModels: (protocol: string, endpoint: string, apiKey: string) =>
    invoke<ModelInfo[]>('ai_fetch_remote_models', { protocol, endpoint, apiKey }),
  validateConfig: (config: AiProviderConfig) => invoke<void>('ai_validate_config', { config }),
  saveConfig: (config: AiProviderConfig) => invoke<void>('ai_save_config', { config }),
  getConfig: () => invoke<AiProviderConfig | null>('ai_get_config'),
  deleteConfig: () => invoke<void>('ai_delete_config'),
  getSettingsConfig: () => invoke<AiSettingsConfig>('ai_get_settings_config'),
  saveSettingsConfig: (config: AiSettingsConfig) =>
    invoke<void>('ai_save_settings_config', { config }),
  setActiveProfile: (profileId: string) => invoke<void>('ai_set_active_profile', { profileId }),

  generateSql: (params: {
    dbSessionId: string;
    database: string;
    naturalLanguage: string;
    requestId: string;
    currentTable?: string;
    recentQueries?: string[];
    contextFiles?: string[];
    contextTables?: string[];
  }) => e2eStubInvoke('ai_generate_sql', params, () => invoke<string>('ai_generate_sql', params)),
  diagnoseError: (params: {
    dbSessionId: string;
    database: string;
    sql: string;
    errorMessage: string;
  }) => invoke<DiagnosisResult>('ai_diagnose_error', params),
  analyzeExplain: (params: { dbSessionId: string; explainOutput: string; originalSql: string }) =>
    invoke<ExplainAnalysis>('ai_analyze_explain', params),
  chat: (params: {
    dbSessionId?: string;
    database?: string;
    messages: AiChatMessage[];
    requestId: string;
    includeSchema?: boolean;
    scenario?: PromptScenario;
    contextFiles?: string[];
    contextTables?: string[];
    targetClaim?: AiTargetClaim;
  }) => e2eStubInvoke('ai_chat', params, () => invoke<string>('ai_chat', params)),
  parseFilter: (params: {
    dbSessionId: string;
    database: string;
    table: string;
    naturalLanguage: string;
  }) => invoke<FilterCondition[]>('ai_parse_filter', params),

  mcpGetStatus: () => invoke<{ running: boolean; transport: string }>('mcp_get_status'),
  mcpStartStdio: () => invoke<void>('mcp_start_stdio'),
  mcpStop: () => invoke<void>('mcp_stop'),
  mcpReload: () => invoke<void>('mcp_reload'),
  mcpListAllTools: () => invoke<string[]>('mcp_list_all_tools'),

  workflowList: () => invoke<WorkflowListItem[]>('workflow_list'),
  workflowExecute: (params: {
    workflowId: string;
    variables: Record<string, unknown>;
    connectionId?: string;
  }) => invoke<WorkflowExecutionResult>('workflow_execute', params),
  workflowSave: (workflow: WorkflowDefinition) =>
    invoke<void>('workflow_save', { workflow: mergeDraftCommandFields(workflow) }),
  workflowSaveYaml: (yaml: string) => invoke<WorkflowDefinition>('workflow_save_yaml', { yaml }),
  workflowGetYaml: (workflowId: string) => invoke<string>('workflow_get_yaml', { workflowId }),
  workflowDelete: (workflowId: string) => invoke<void>('workflow_delete', { workflowId }),
  workflowReload: () => invoke<void>('workflow_reload'),
  workflowGetDir: () => invoke<string>('workflow_get_dir'),
  workflowGet: (workflowId: string) => invoke<WorkflowDefinition>('workflow_get', { workflowId }),

  workflowHistoryList: (workflowId?: string) =>
    invoke<HistoryListItem[]>('workflow_history_list', { workflowId: workflowId ?? null }),
  workflowHistoryGet: (historyId: string) =>
    invoke<HistoryEntry>('workflow_history_get', { historyId }),
  workflowHistoryClear: (workflowId?: string) =>
    invoke<number>('workflow_history_clear', { workflowId: workflowId ?? null }),

  generateSchemaDoc: (params: { dbSessionId: string; database: string }) =>
    invoke<string>('ai_generate_schema_doc', params),
  diagnoseConnection: (params: { connectionId: string; errorMessage: string }) =>
    invoke<ConnectionDiagnosis>('ai_diagnose_connection', params),
  analyzeQueries: (params: { dbSessionId?: string }) =>
    invoke<QueryAnalysis>('ai_analyze_queries', params),

  mcpClientConnect: (config: McpServerConfig) => invoke<void>('mcp_client_connect', { config }),
  mcpClientDisconnect: (serverId: string) => invoke<void>('mcp_client_disconnect', { serverId }),
  mcpClientList: () => invoke<McpClientStatus[]>('mcp_client_list'),
  mcpClientTools: () => invoke<McpToolInfo[]>('mcp_client_tools'),
  mcpClientCallTool: (params: {
    serverId: string;
    toolName: string;
    arguments: Record<string, unknown>;
  }) => invoke<string>('mcp_client_call_tool', params),

  /**
   * Cancel a running AI streaming request (FR-01).
   *
   * Resolves with the host's three-state receipt. Rejections are intentional:
   * the caller (sessionCancel) decides whether a failed IPC should leave the
   * run untouched instead of pretending it stopped.
   */
  cancel: (requestId: string) => invoke<CancelReceipt>('ai_cancel', { requestId }),

  /** Share an editor pane's runtime session with the AI runtime (P6 §14.4). */
  grantEditorSession: (params: AiGrantRequest) =>
    nativeInvoke<AiGrantReceipt>('ai_grant_editor_session', params),
  revokeEditorGrant: (grantId: string) =>
    nativeInvoke<boolean>('ai_revoke_editor_grant', { grantId }),
  touchEditorGrant: (grantId: string) =>
    nativeInvoke<boolean>('ai_touch_editor_grant', { grantId }),

  promptList: (driverType?: string) =>
    invoke<PromptInfo[]>('prompt_list', { driverType: driverType ?? null }),
  promptSetOverride: (entry: PromptOverrideEntry) => invoke<void>('prompt_set_override', { entry }),
  promptRemoveOverride: (driverType: string, scenario: PromptScenario) =>
    invoke<void>('prompt_remove_override', { driverType, scenario }),
};

export function onAiStreamChunk(
  callback: (payload: StreamChunkPayload) => void,
): Promise<UnlistenFn> {
  // Form-split F6 prep (R6 burn): host broadcasts ride the EventChannel port.
  return requirePlatform().events.listenCrossWindow('ai:stream-chunk', (raw) =>
    callback(raw as StreamChunkPayload),
  );
}

export function onAiStreamError(
  callback: (payload: StreamErrorPayload) => void,
): Promise<UnlistenFn> {
  return requirePlatform().events.listenCrossWindow('ai:stream-error', (raw) =>
    callback(raw as StreamErrorPayload),
  );
}

export function onAiConfigChanged(callback: (isConfigured: boolean) => void): Promise<UnlistenFn> {
  return requirePlatform().events.listenCrossWindow('ai:config-changed', (raw) =>
    callback(raw as boolean),
  );
}
