import { create } from 'zustand';
import { aiCommands, onAiStreamChunk, onAiStreamError } from '../commands/ai';
import type {
  AiChatMessage,
  AiChatSession,
  AiProviderConfig,
  AiProviderType,
  DiagnosisResult,
  ExplainAnalysis,
  FilterCondition,
  McpClientStatus,
  McpServerConfig,
  McpToolInfo,
  ModelInfo,
  ProviderListItem,
  SkillListItem,
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

  chatSession: AiChatSession | null;

  nlFilterInput: string;
  parsedFilters: FilterCondition[] | null;
  isParsingFilter: boolean;
  nlFilterError: string | null;

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

  setNlFilterInput: (input: string) => void;
  parseFilter: (params: {
    connectionId: string;
    database: string;
    table: string;
  }) => Promise<FilterCondition[] | null>;
  clearNlFilter: () => void;

  initChatSession: () => void;
  sendChatMessage: (params: {
    connectionId?: string;
    database?: string;
    content: string;
    includeSchema?: boolean;
  }) => Promise<void>;
  clearChat: () => void;

  handleStreamChunk: (payload: StreamChunkPayload) => void;
  setupEventListeners: () => Promise<() => void>;

  skills: SkillListItem[];
  skillsLoading: boolean;
  skillExecutionResult: string | null;
  isExecutingSkill: boolean;
  skillError: string | null;

  loadSkills: () => Promise<void>;
  executeSkill: (params: {
    skillId: string;
    variables: Record<string, unknown>;
    connectionId?: string;
  }) => Promise<void>;
  clearSkillResult: () => void;

  mcpServers: McpClientStatus[];
  mcpTools: McpToolInfo[];
  mcpConnecting: boolean;
  mcpError: string | null;

  connectMcpServer: (config: McpServerConfig) => Promise<void>;
  disconnectMcpServer: (serverId: string) => Promise<void>;
  loadMcpServers: () => Promise<void>;
  loadMcpTools: () => Promise<void>;
  callMcpTool: (params: {
    serverId: string;
    toolName: string;
    arguments: Record<string, unknown>;
  }) => Promise<string>;
  clearMcpError: () => void;
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

  chatSession: null,

  nlFilterInput: '',
  parsedFilters: null,
  isParsingFilter: false,
  nlFilterError: null,

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

  // ── Smart Filter ──

  setNlFilterInput: (input) => set({ nlFilterInput: input }),

  parseFilter: async ({ connectionId, database, table }) => {
    const { nlFilterInput } = get();
    if (!nlFilterInput.trim()) return null;

    set({ isParsingFilter: true, nlFilterError: null, parsedFilters: null });

    try {
      const filters = await aiCommands.parseFilter({
        connectionId,
        database,
        table,
        naturalLanguage: nlFilterInput,
      });
      set({ parsedFilters: filters, isParsingFilter: false });
      return filters;
    } catch (e) {
      set({
        isParsingFilter: false,
        nlFilterError: e instanceof Error ? e.message : String(e),
      });
      return null;
    }
  },

  clearNlFilter: () =>
    set({ nlFilterInput: '', parsedFilters: null, isParsingFilter: false, nlFilterError: null }),

  // ── Chat ──

  initChatSession: () => {
    set({
      chatSession: {
        id: crypto.randomUUID(),
        messages: [],
        isStreaming: false,
        streamContent: '',
        requestId: null,
      },
    });
  },

  sendChatMessage: async ({ connectionId, database, content, includeSchema = true }) => {
    const { chatSession } = get();
    if (!chatSession) return;

    const userMessage: AiChatMessage = { role: 'user', content };
    const requestId = crypto.randomUUID();

    set({
      chatSession: {
        ...chatSession,
        messages: [...chatSession.messages, userMessage],
        isStreaming: true,
        streamContent: '',
        requestId,
      },
    });

    try {
      await aiCommands.chat({
        connectionId,
        database,
        messages: [...chatSession.messages, userMessage],
        requestId,
        includeSchema,
      });
    } catch (e) {
      const session = get().chatSession;
      if (session) {
        set({
          chatSession: {
            ...session,
            isStreaming: false,
            streamContent: '',
            requestId: null,
            messages: [
              ...session.messages,
              { role: 'assistant', content: `Error: ${e instanceof Error ? e.message : String(e)}` },
            ],
          },
        });
      }
    }
  },

  clearChat: () => {
    const { chatSession } = get();
    if (chatSession) {
      set({
        chatSession: {
          ...chatSession,
          messages: [],
          isStreaming: false,
          streamContent: '',
          requestId: null,
        },
      });
    }
  },

  // ── Stream handling ──

  handleStreamChunk: (payload) => {
    const { nl2sql, chatSession } = get();

    if (payload.requestId === nl2sql.requestId) {
      set((s) => ({
        nl2sql: {
          ...s.nl2sql,
          generatedSql: payload.content
            ? s.nl2sql.generatedSql + payload.content
            : s.nl2sql.generatedSql,
          isGenerating: payload.done ? false : s.nl2sql.isGenerating,
        },
      }));
      return;
    }

    if (chatSession && payload.requestId === chatSession.requestId) {
      const newContent = (chatSession.streamContent || '') + (payload.content || '');
      if (payload.done) {
        const assistantMessage: AiChatMessage = { role: 'assistant', content: newContent };
        set({
          chatSession: {
            ...chatSession,
            messages: [...chatSession.messages, assistantMessage],
            isStreaming: false,
            streamContent: '',
            requestId: null,
          },
        });
      } else {
        set({
          chatSession: {
            ...chatSession,
            streamContent: newContent,
          },
        });
      }
    }
  },

  setupEventListeners: async () => {
    const unChunk = await onAiStreamChunk((payload) => {
      get().handleStreamChunk(payload);
    });
    const unError = await onAiStreamError((payload) => {
      const { nl2sql, chatSession } = get();
      if (payload.requestId === nl2sql.requestId) {
        set((s) => ({
          nl2sql: { ...s.nl2sql, isGenerating: false },
          nl2sqlError: payload.error,
        }));
      }
      if (chatSession && payload.requestId === chatSession.requestId) {
        set({
          chatSession: {
            ...chatSession,
            isStreaming: false,
            streamContent: '',
            requestId: null,
            messages: [
              ...chatSession.messages,
              { role: 'assistant', content: `Error: ${payload.error}` },
            ],
          },
        });
      }
    });
    return () => {
      unChunk();
      unError();
    };
  },

  skills: [],
  skillsLoading: false,
  skillExecutionResult: null,
  isExecutingSkill: false,
  skillError: null,

  loadSkills: async () => {
    set({ skillsLoading: true });
    try {
      const skills = await aiCommands.skillList();
      set({ skills, skillsLoading: false });
    } catch (e) {
      set({ skillsLoading: false, skillError: String(e) });
    }
  },

  executeSkill: async ({ skillId, variables, connectionId }) => {
    set({ isExecutingSkill: true, skillExecutionResult: null, skillError: null });
    try {
      const result = await aiCommands.skillExecute({ skillId, variables, connectionId });
      set({ isExecutingSkill: false, skillExecutionResult: result });
    } catch (e) {
      set({ isExecutingSkill: false, skillError: String(e) });
    }
  },

  clearSkillResult: () =>
    set({ skillExecutionResult: null, skillError: null }),

  mcpServers: [],
  mcpTools: [],
  mcpConnecting: false,
  mcpError: null,

  connectMcpServer: async (config) => {
    set({ mcpConnecting: true, mcpError: null });
    try {
      await aiCommands.mcpClientConnect(config);
      await get().loadMcpServers();
      await get().loadMcpTools();
      set({ mcpConnecting: false });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ mcpConnecting: false, mcpError: msg });
      throw new Error(msg);
    }
  },

  disconnectMcpServer: async (serverId) => {
    try {
      await aiCommands.mcpClientDisconnect(serverId);
      await get().loadMcpServers();
      await get().loadMcpTools();
    } catch (e) {
      set({ mcpError: e instanceof Error ? e.message : String(e) });
    }
  },

  loadMcpServers: async () => {
    try {
      const servers = await aiCommands.mcpClientList();
      set({ mcpServers: servers });
    } catch (e) {
      console.error('Failed to load MCP servers:', e);
    }
  },

  loadMcpTools: async () => {
    try {
      const tools = await aiCommands.mcpClientTools();
      set({ mcpTools: tools });
    } catch (e) {
      console.error('Failed to load MCP tools:', e);
    }
  },

  callMcpTool: async ({ serverId, toolName, arguments: args }) => {
    try {
      return await aiCommands.mcpClientCallTool({ serverId, toolName, arguments: args });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ mcpError: msg });
      throw new Error(msg);
    }
  },

  clearMcpError: () => set({ mcpError: null }),
}));
