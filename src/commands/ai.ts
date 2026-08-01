import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AiProviderConfig,
  AiProviderType,
  DiagnosisResult,
  ModelInfo,
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
