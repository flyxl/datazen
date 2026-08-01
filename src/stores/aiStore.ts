import { create } from 'zustand';
import { aiCommands } from '../commands/ai';
import type {
  AiProviderConfig,
  AiProviderType,
  ModelInfo,
  ProviderListItem,
} from '../types';

interface AiStore {
  config: AiProviderConfig | null;
  isConfigured: boolean;
  providers: ProviderListItem[];
  models: ModelInfo[];
  configLoading: boolean;
  configError: string | null;
  validating: boolean;
  saving: boolean;

  loadConfig: () => Promise<void>;
  loadProviders: () => Promise<void>;
  loadModels: (providerType: AiProviderType) => Promise<void>;
  validateConfig: (config: AiProviderConfig) => Promise<boolean>;
  saveConfig: (config: AiProviderConfig) => Promise<boolean>;
  deleteConfig: () => Promise<void>;
  clearError: () => void;
}

export const useAiStore = create<AiStore>((set) => ({
  config: null,
  isConfigured: false,
  providers: [],
  models: [],
  configLoading: false,
  configError: null,
  validating: false,
  saving: false,

  loadConfig: async () => {
    set({ configLoading: true, configError: null });
    try {
      const config = await aiCommands.getConfig();
      set({
        config,
        isConfigured: config !== null,
        configLoading: false,
      });
    } catch (e) {
      set({
        configLoading: false,
        configError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  loadProviders: async () => {
    try {
      const providers = await aiCommands.getProviders();
      set({ providers });
    } catch (e) {
      console.error('Failed to load AI providers:', e);
    }
  },

  loadModels: async (providerType) => {
    try {
      const models = await aiCommands.getModels(providerType);
      set({ models });
    } catch (e) {
      console.error('Failed to load models:', e);
    }
  },

  validateConfig: async (config) => {
    set({ validating: true, configError: null });
    try {
      await aiCommands.validateConfig(config);
      set({ validating: false });
      return true;
    } catch (e) {
      set({
        validating: false,
        configError: e instanceof Error ? e.message : String(e),
      });
      return false;
    }
  },

  saveConfig: async (config) => {
    set({ saving: true, configError: null });
    try {
      await aiCommands.saveConfig(config);
      set({
        config,
        isConfigured: true,
        saving: false,
      });
      return true;
    } catch (e) {
      set({
        saving: false,
        configError: e instanceof Error ? e.message : String(e),
      });
      return false;
    }
  },

  deleteConfig: async () => {
    try {
      await aiCommands.deleteConfig();
      set({
        config: null,
        isConfigured: false,
        models: [],
      });
    } catch (e) {
      set({
        configError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearError: () => set({ configError: null }),
}));
