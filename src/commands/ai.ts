import { invoke } from '@tauri-apps/api/core';
import type {
  AiProviderConfig,
  AiProviderType,
  ModelInfo,
  ProviderListItem,
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
};
