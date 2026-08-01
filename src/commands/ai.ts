import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AiChatMessage,
  AiProviderConfig,
  AiProviderType,
  ConnectionDiagnosis,
  DiagnosisResult,
  ExplainAnalysis,
  FilterCondition,
  McpClientStatus,
  McpServerConfig,
  McpToolInfo,
  ModelInfo,
  QueryAnalysis,
  SkillDefinition,
  SkillListItem,
  ProviderListItem,
  StreamChunkPayload,
  StreamErrorPayload,
} from '../types';

export const aiCommands = {
  getProviders: () => invoke<ProviderListItem[]>('ai_get_providers'),

  getModels: (providerType: AiProviderType) =>
    invoke<ModelInfo[]>('ai_get_models', { providerType }),

  validateConfig: (config: AiProviderConfig) =>
    invoke<void>('ai_validate_config', { config }),

  saveConfig: (config: AiProviderConfig) =>
    invoke<void>('ai_save_config', { config }),

  getConfig: () => invoke<AiProviderConfig | null>('ai_get_config'),

  deleteConfig: () => invoke<void>('ai_delete_config'),

  generateSql: (params: {
    connectionId: string;
    database: string;
    naturalLanguage: string;
    requestId: string;
    currentTable?: string;
    recentQueries?: string[];
  }) => invoke<string>('ai_generate_sql', params),

  diagnoseError: (params: {
    connectionId: string;
    database: string;
    sql: string;
    errorMessage: string;
  }) => invoke<DiagnosisResult>('ai_diagnose_error', params),

  analyzeExplain: (params: {
    connectionId: string;
    explainOutput: string;
    originalSql: string;
  }) => invoke<ExplainAnalysis>('ai_analyze_explain', params),

  chat: (params: {
    connectionId?: string;
    database?: string;
    messages: AiChatMessage[];
    requestId: string;
    includeSchema?: boolean;
  }) => invoke<string>('ai_chat', params),

  parseFilter: (params: {
    connectionId: string;
    database: string;
    table: string;
    naturalLanguage: string;
  }) => invoke<FilterCondition[]>('ai_parse_filter', params),

  mcpGetStatus: () => invoke<{ running: boolean; transport: string }>('mcp_get_status'),
  mcpStartStdio: () => invoke<void>('mcp_start_stdio'),
  mcpStop: () => invoke<void>('mcp_stop'),

  skillList: () => invoke<SkillListItem[]>('skill_list'),
  skillExecute: (params: {
    skillId: string;
    variables: Record<string, unknown>;
    connectionId?: string;
  }) => invoke<string>('skill_execute', params),
  skillSave: (skill: SkillDefinition) => invoke<void>('skill_save', { skill }),
  skillDelete: (skillId: string) => invoke<void>('skill_delete', { skillId }),
  skillReload: () => invoke<void>('skill_reload'),

  generateSchemaDoc: (params: {
    connectionId: string;
    database: string;
  }) => invoke<string>('ai_generate_schema_doc', params),

  diagnoseConnection: (params: {
    connectionId: string;
    errorMessage: string;
  }) => invoke<ConnectionDiagnosis>('ai_diagnose_connection', params),

  analyzeQueries: (params: {
    connectionId?: string;
  }) => invoke<QueryAnalysis>('ai_analyze_queries', params),

  mcpClientConnect: (config: McpServerConfig) =>
    invoke<void>('mcp_client_connect', { config }),
  mcpClientDisconnect: (serverId: string) =>
    invoke<void>('mcp_client_disconnect', { serverId }),
  mcpClientList: () => invoke<McpClientStatus[]>('mcp_client_list'),
  mcpClientTools: () => invoke<McpToolInfo[]>('mcp_client_tools'),
  mcpClientCallTool: (params: {
    serverId: string;
    toolName: string;
    arguments: Record<string, unknown>;
  }) => invoke<string>('mcp_client_call_tool', params),
};

export function onAiStreamChunk(
  callback: (payload: StreamChunkPayload) => void,
): Promise<UnlistenFn> {
  return listen<StreamChunkPayload>('ai:stream-chunk', (event) => {
    callback(event.payload);
  });
}

export function onAiStreamError(
  callback: (payload: StreamErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<StreamErrorPayload>('ai:stream-error', (event) => {
    callback(event.payload);
  });
}
