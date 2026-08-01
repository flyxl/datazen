import { create } from 'zustand';
import { aiCommands, onAiStreamChunk, onAiStreamError } from '../commands/ai';
import type {
  AiProviderConfig,
  AiProviderType,
  DiagnosisResult,
  ExplainAnalysis,
  ModelInfo,
  ProviderListItem,
  StreamChunkPayload,
} from '../types';

interface Nl2SqlState {
  input: string;
  generatedSql: string;
  isGenerating: boolean;
  requestId: string | null;
}

const initialNl2Sql: Nl2SqlState = {
  input: '',
  generatedSql: '',
  isGenerating: false,
  requestId: null,
};

interface AiStore {
  config: AiProviderConfig | null;
  isConfigured: boolean;
  providers: ProviderListItem[];
  models: ModelInfo[];
  configLoading: boolean;
  configError: string | null;
  validating: boolean;
  saving: boolean;

  nl2sql: Nl2SqlState;
  nl2sqlError: string | null;
  diagnosis: DiagnosisResult | null;
  isDiagnosing: boolean;
  diagnosisError: string | null;
  explainAnalysis: ExplainAnalysis | null;
  isAnalyzingExplain: boolean;
  explainError: string | null;

  loadConfig: () => Promise<void>;
  loadProviders: () => Promise<void>;
  loadModels: (providerType: AiProviderType) => Promise<void>;
  validateConfig: (config: AiProviderConfig) => Promise<boolean>;
  saveConfig: (config: AiProviderConfig) => Promise<boolean>;
  deleteConfig: () => Promise<void>;
  clearError: () => void;

  setNl2SqlInput: (input: string) => void;
  generateSql: (params: {
    connectionId: string;
    database: string;
    currentTable?: string;
    recentQueries?: string[];
  }) => Promise<void>;
  clearNl2Sql: () => void;

  diagnoseError: (params: {
    connectionId: string;
    database: string;
    sql: string;
    errorMessage: string;
  }) => Promise<void>;
  clearDiagnosis: () => void;

  analyzeExplain: (params: {
    connectionId: string;
    explainOutput: string;
    originalSql: string;
  }) => Promise<void>;
  clearExplainAnalysis: () => void;

  handleStreamChunk: (payload: StreamChunkPayload) => void;
  setupEventListeners: () => Promise<() => void>;
}

export const useAiStore = create<AiStore>((set, get) => ({
  config: null,
  isConfigured: false,
  providers: [],
  models: [],
  configLoading: false,
  configError: null,
  validating: false,
  saving: false,

  nl2sql: { ...initialNl2Sql },
  nl2sqlError: null,
  diagnosis: null,
  isDiagnosing: false,
  diagnosisError: null,
  explainAnalysis: null,
  isAnalyzingExplain: false,
  explainError: null,

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

  // ── NL2SQL ──

  setNl2SqlInput: (input) =>
    set((s) => ({ nl2sql: { ...s.nl2sql, input } })),

  generateSql: async (params) => {
    const { nl2sql } = get();
    if (!nl2sql.input.trim()) return;

    const requestId = crypto.randomUUID();
    set({
      nl2sql: {
        ...nl2sql,
        generatedSql: '',
        isGenerating: true,
        requestId,
      },
      nl2sqlError: null,
    });

    try {
      await aiCommands.generateSql({
        ...params,
        naturalLanguage: nl2sql.input,
        requestId,
      });
    } catch (e) {
      set((s) => ({
        nl2sql: { ...s.nl2sql, isGenerating: false },
        nl2sqlError: e instanceof Error ? e.message : String(e),
      }));
    }
  },

  clearNl2Sql: () => set({ nl2sql: { ...initialNl2Sql }, nl2sqlError: null }),

  // ── Diagnosis ──

  diagnoseError: async (params) => {
    set({ isDiagnosing: true, diagnosisError: null, diagnosis: null });
    try {
      const result = await aiCommands.diagnoseError(params);
      set({ diagnosis: result, isDiagnosing: false });
    } catch (e) {
      set({
        isDiagnosing: false,
        diagnosisError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearDiagnosis: () =>
    set({ diagnosis: null, isDiagnosing: false, diagnosisError: null }),

  // ── EXPLAIN Analysis ──

  analyzeExplain: async (params) => {
    set({ isAnalyzingExplain: true, explainAnalysis: null, explainError: null });
    try {
      const result = await aiCommands.analyzeExplain(params);
      set({ explainAnalysis: result, isAnalyzingExplain: false });
    } catch (e) {
      set({
        isAnalyzingExplain: false,
        explainError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearExplainAnalysis: () =>
    set({ explainAnalysis: null, isAnalyzingExplain: false, explainError: null }),

  // ── Stream handling ──

  handleStreamChunk: (payload) => {
    const { nl2sql } = get();
    if (payload.requestId !== nl2sql.requestId) return;

    set((s) => ({
      nl2sql: {
        ...s.nl2sql,
        generatedSql: payload.content
          ? s.nl2sql.generatedSql + payload.content
          : s.nl2sql.generatedSql,
        isGenerating: payload.done ? false : s.nl2sql.isGenerating,
      },
    }));
  },

  setupEventListeners: async () => {
    const unChunk = await onAiStreamChunk((payload) => {
      get().handleStreamChunk(payload);
    });
    const unError = await onAiStreamError((payload) => {
      const { nl2sql } = get();
      if (payload.requestId === nl2sql.requestId) {
        set((s) => ({
          nl2sql: { ...s.nl2sql, isGenerating: false },
          nl2sqlError: payload.error,
        }));
      }
    });
    return () => {
      unChunk();
      unError();
    };
  },
}));
