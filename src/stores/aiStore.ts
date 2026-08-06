import { create } from 'zustand';
import { aiCommands, onAiStreamChunk, onAiStreamError, onAiConfigChanged } from '../commands/ai';
import { extractSqlFromResponse } from '../lib/extractSql';
import { normalizeAiProviders } from '../lib/aiProviders';
import { extractQuestions, parseToolCallQuestions } from '../lib/extractQuestions';
import type {
  AiChatMessage,
  AiChatSession,
  AiProviderConfig,
  ConnectionDiagnosis,
  DiagnosisResult,
  ExplainAnalysis,
  FilterCondition,
  McpClientStatus,
  McpServerConfig,
  McpToolInfo,
  ModelInfo,
  ProviderListItem,
  QueryAnalysis,
  WorkflowExecutionResult,
  WorkflowListItem,
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

  remoteModels: ModelInfo[];
  fetchingRemoteModels: boolean;

  loadConfig: () => Promise<void>;
  loadProviders: () => Promise<void>;
  fetchRemoteModels: (protocol: string, endpoint: string, apiKey: string) => Promise<ModelInfo[]>;
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
    contextFiles?: string[];
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
    contextFiles?: string[];
  }) => Promise<void>;
  clearChat: () => void;

  workflowChat: AiChatSession | null;
  initWorkflowChat: () => void;
  sendWorkflowChatMessage: (params: {
    connectionId?: string;
    database?: string;
    content: string;
    includeSchema?: boolean;
    contextFiles?: string[];
  }) => Promise<void>;
  clearWorkflowChat: () => void;

  handleStreamChunk: (payload: StreamChunkPayload) => void;
  setupEventListeners: () => Promise<() => void>;

  workflows: WorkflowListItem[];
  workflowsLoading: boolean;
  workflowExecutionResult: WorkflowExecutionResult | null;
  isExecutingWorkflow: boolean;
  workflowError: string | null;

  loadWorkflows: () => Promise<void>;
  executeWorkflow: (params: {
    workflowId: string;
    variables: Record<string, unknown>;
    connectionId?: string;
  }) => Promise<void>;
  clearWorkflowResult: () => void;

  schemaDoc: string | null;
  isGeneratingSchemaDoc: boolean;
  schemaDocError: string | null;

  connectionDiagnosis: ConnectionDiagnosis | null;
  isDiagnosingConnection: boolean;
  connectionDiagnosisError: string | null;

  queryAnalysis: QueryAnalysis | null;
  isAnalyzingQueries: boolean;
  queryAnalysisError: string | null;

  generateSchemaDoc: (params: { connectionId: string; database: string }) => Promise<void>;
  clearSchemaDoc: () => void;
  diagnoseConnection: (params: { connectionId: string; errorMessage: string }) => Promise<void>;
  clearConnectionDiagnosis: () => void;
  analyzeQueries: (params: { connectionId?: string }) => Promise<void>;
  clearQueryAnalysis: () => void;

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

  remoteModels: [],
  fetchingRemoteModels: false,

  loadConfig: async () => {
    if (!('__TAURI_INTERNALS__' in globalThis)) return;
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
    if (!('__TAURI_INTERNALS__' in globalThis)) return;
    try {
      const providers = normalizeAiProviders(await aiCommands.getProviders());
      set({ providers });
    } catch (e) {
      console.error('Failed to load AI providers:', e);
    }
  },

  fetchRemoteModels: async (protocol, endpoint, apiKey) => {
    set({ fetchingRemoteModels: true, configError: null });
    try {
      const models = await aiCommands.fetchRemoteModels(protocol, endpoint, apiKey);
      set({ remoteModels: models, fetchingRemoteModels: false });
      return models;
    } catch (e) {
      set({
        fetchingRemoteModels: false,
        configError: e instanceof Error ? e.message : String(e),
      });
      return [];
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
    console.debug('[AI] generateSql:', { ...params, input: nl2sql.input, requestId });
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
      console.error('[AI] generateSql error:', e);
      set((s) => ({
        nl2sql: { ...s.nl2sql, isGenerating: false },
        nl2sqlError: e instanceof Error ? e.message : String(e),
      }));
    }
  },

  clearNl2Sql: () => set({ nl2sql: { ...initialNl2Sql }, nl2sqlError: null }),

  // ── Diagnosis ──

  diagnoseError: async (params) => {
    console.debug('[AI] diagnoseError:', { sql_len: params.sql?.length, error: params.errorMessage });
    set({ isDiagnosing: true, diagnosisError: null, diagnosis: null });
    try {
      const result = await aiCommands.diagnoseError(params);
      console.debug('[AI] diagnoseError result:', { changes: result?.changes?.length });
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
    console.debug('[AI] analyzeExplain:', { connectionId: params.connectionId, sql_len: params.originalSql?.length });
    set({ isAnalyzingExplain: true, explainAnalysis: null, explainError: null });
    try {
      const result = await aiCommands.analyzeExplain(params);
      console.debug('[AI] analyzeExplain result:', { bottlenecks: result?.bottlenecks?.length, suggestions: result?.suggestions?.length });
      set({ explainAnalysis: result, isAnalyzingExplain: false });
    } catch (e) {
      console.error('[AI] analyzeExplain error:', e);
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

    console.debug('[AI] parseFilter:', { connectionId, database, table, input: nlFilterInput });
    set({ isParsingFilter: true, nlFilterError: null, parsedFilters: null });

    try {
      const filters = await aiCommands.parseFilter({
        connectionId,
        database,
        table,
        naturalLanguage: nlFilterInput,
      });
      console.debug('[AI] parseFilter result:', { filterCount: filters?.length });
      set({ parsedFilters: filters, isParsingFilter: false });
      return filters;
    } catch (e) {
      console.error('[AI] parseFilter error:', e);
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
        streamReasoning: '',
        requestId: null,
      },
    });
  },

  sendChatMessage: async ({ connectionId, database, content, includeSchema = true, contextFiles }) => {
    const { chatSession } = get();
    if (!chatSession) return;

    console.debug('[AI] sendChatMessage:', { connectionId, database, contentLen: content.length, includeSchema, historyLen: chatSession.messages.length });

    const newMessages: AiChatMessage[] = [];
    const lastMsg = chatSession.messages[chatSession.messages.length - 1];
    if (lastMsg?.toolCalls && lastMsg.toolCalls.length > 0) {
      const askCall = lastMsg.toolCalls.find((tc) => tc.name === 'ask_questions');
      if (askCall) {
        newMessages.push({ role: 'tool', content, toolCallId: askCall.id });
      } else {
        newMessages.push({ role: 'user', content });
      }
    } else {
      newMessages.push({ role: 'user', content });
    }

    const requestId = crypto.randomUUID();

    set({
      chatSession: {
        ...chatSession,
        messages: [...chatSession.messages, ...newMessages],
        isStreaming: true,
        streamContent: '',
        streamReasoning: '',
        requestId,
      },
    });

    try {
      await aiCommands.chat({
        connectionId,
        database,
        messages: [...chatSession.messages, ...newMessages],
        requestId,
        includeSchema,
        contextFiles,
      });
    } catch (e) {
      const session = get().chatSession;
      if (session) {
        set({
          chatSession: {
            ...session,
            isStreaming: false,
            streamContent: '',
            streamReasoning: '',
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

  // ── Workflow Chat ──

  workflowChat: null,

  initWorkflowChat: () => {
    set({
      workflowChat: {
        id: crypto.randomUUID(),
        messages: [],
        isStreaming: false,
        streamContent: '',
        streamReasoning: '',
        requestId: null,
      },
    });
  },

  sendWorkflowChatMessage: async ({ connectionId, database, content, includeSchema = true, contextFiles }) => {
    const { workflowChat } = get();
    if (!workflowChat) return;

    const newMessages: AiChatMessage[] = [];
    const lastMsg = workflowChat.messages[workflowChat.messages.length - 1];
    if (lastMsg?.toolCalls && lastMsg.toolCalls.length > 0) {
      const askCall = lastMsg.toolCalls.find((tc) => tc.name === 'ask_questions');
      if (askCall) {
        newMessages.push({ role: 'tool', content, toolCallId: askCall.id });
      } else {
        newMessages.push({ role: 'user', content });
      }
    } else {
      newMessages.push({ role: 'user', content });
    }

    const requestId = crypto.randomUUID();

    set({
      workflowChat: {
        ...workflowChat,
        messages: [...workflowChat.messages, ...newMessages],
        isStreaming: true,
        streamContent: '',
        streamReasoning: '',
        requestId,
      },
    });

    try {
      await aiCommands.chat({
        connectionId,
        database,
        messages: [...workflowChat.messages, ...newMessages],
        requestId,
        includeSchema,
        scenario: 'workflow_generate',
        contextFiles,
      });
    } catch (e) {
      const session = get().workflowChat;
      if (session) {
        set({
          workflowChat: {
            ...session,
            isStreaming: false,
            streamContent: '',
            streamReasoning: '',
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

  clearWorkflowChat: () => {
    const { workflowChat } = get();
    if (workflowChat) {
      set({
        workflowChat: {
          ...workflowChat,
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
    const { nl2sql, chatSession, workflowChat } = get();

    if (payload.requestId === nl2sql.requestId) {
      const accumulated = payload.content
        ? nl2sql.generatedSql + payload.content
        : nl2sql.generatedSql;
      set({
        nl2sql: {
          ...nl2sql,
          generatedSql: payload.done ? extractSqlFromResponse(accumulated) : accumulated,
          isGenerating: payload.done ? false : nl2sql.isGenerating,
        },
      });
      return;
    }

    const targetSession =
      chatSession && payload.requestId === chatSession.requestId
        ? 'chatSession'
        : workflowChat && payload.requestId === workflowChat.requestId
          ? 'workflowChat'
          : null;

    if (targetSession) {
      const session = (targetSession === 'chatSession' ? chatSession : workflowChat)!;
      const newContent = (session.streamContent || '') + (payload.content || '');
      const newReasoning = (session.streamReasoning || '') + (payload.reasoning || '');
      if (payload.done) {
        const { cleanContent, questions: xmlQuestions } = extractQuestions(newContent);
        const toolCalls = payload.toolCalls && payload.toolCalls.length > 0 ? payload.toolCalls : undefined;

        let questions = xmlQuestions.length > 0 ? xmlQuestions : undefined;
        if (!questions && toolCalls) {
          const tcQuestions = parseToolCallQuestions(toolCalls);
          if (tcQuestions.length > 0) questions = tcQuestions;
        }

        const assistantMessage: AiChatMessage = {
          role: 'assistant',
          content: cleanContent,
          reasoning: newReasoning || undefined,
          questions,
          toolCalls,
        };
        set({
          [targetSession]: {
            ...session,
            messages: [...session.messages, assistantMessage],
            isStreaming: false,
            streamContent: '',
            streamReasoning: '',
            requestId: null,
          },
        });
      } else {
        set({
          [targetSession]: {
            ...session,
            streamContent: newContent,
            streamReasoning: newReasoning,
          },
        });
      }
    }
  },

  setupEventListeners: async () => {
    if (!('__TAURI_INTERNALS__' in globalThis)) return () => {};
    const unChunk = await onAiStreamChunk((payload) => {
      get().handleStreamChunk(payload);
    });
    const unError = await onAiStreamError((payload) => {
      const { nl2sql, chatSession, workflowChat } = get();
      if (payload.requestId === nl2sql.requestId) {
        set((s) => ({
          nl2sql: { ...s.nl2sql, isGenerating: false },
          nl2sqlError: payload.error,
        }));
      }
      const errTarget =
        chatSession && payload.requestId === chatSession.requestId
          ? 'chatSession'
          : workflowChat && payload.requestId === workflowChat.requestId
            ? 'workflowChat'
            : null;
      if (errTarget) {
        const session = (errTarget === 'chatSession' ? chatSession : workflowChat)!;
        set({
          [errTarget]: {
            ...session,
            isStreaming: false,
            streamContent: '',
            requestId: null,
            messages: [
              ...session.messages,
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

  workflows: [],
  workflowsLoading: false,
  workflowExecutionResult: null,
  isExecutingWorkflow: false,
  workflowError: null,

  loadWorkflows: async () => {
    // Delay showing the spinner by 100ms; if the load finishes sooner, never
    // flash a loading state. Once shown, keep it visible for at least 200ms
    // so a 101ms completion doesn't cause a blink.
    const SHOW_AFTER_MS = 100;
    const MIN_VISIBLE_MS = 200;

    let showTimer: ReturnType<typeof setTimeout> | null = null;
    let loadingShownAt: number | null = null;

    showTimer = setTimeout(() => {
      loadingShownAt = Date.now();
      set({ workflowsLoading: true });
    }, SHOW_AFTER_MS);

    const finishLoading = async () => {
      if (showTimer !== null) {
        clearTimeout(showTimer);
        showTimer = null;
      }
      if (loadingShownAt !== null) {
        const remain = MIN_VISIBLE_MS - (Date.now() - loadingShownAt);
        if (remain > 0) {
          await new Promise((r) => setTimeout(r, remain));
        }
      }
      set({ workflowsLoading: false });
    };

    try {
      const workflows = await aiCommands.workflowList();
      await finishLoading();
      set({ workflows });
    } catch (e) {
      await finishLoading();
      set({ workflowError: String(e) });
    }
  },

  executeWorkflow: async ({ workflowId, variables, connectionId }) => {
    set({ isExecutingWorkflow: true, workflowExecutionResult: null, workflowError: null });
    try {
      const result = await aiCommands.workflowExecute({ workflowId, variables, connectionId });
      set({ isExecutingWorkflow: false, workflowExecutionResult: result });
    } catch (e) {
      set({ isExecutingWorkflow: false, workflowError: String(e) });
    }
  },

  clearWorkflowResult: () =>
    set({ workflowExecutionResult: null, workflowError: null }),

  schemaDoc: null,
  isGeneratingSchemaDoc: false,
  schemaDocError: null,

  connectionDiagnosis: null,
  isDiagnosingConnection: false,
  connectionDiagnosisError: null,

  queryAnalysis: null,
  isAnalyzingQueries: false,
  queryAnalysisError: null,

  generateSchemaDoc: async ({ connectionId, database }) => {
    set({ isGeneratingSchemaDoc: true, schemaDoc: null, schemaDocError: null });
    try {
      const doc = await aiCommands.generateSchemaDoc({ connectionId, database });
      set({ schemaDoc: doc, isGeneratingSchemaDoc: false });
    } catch (e) {
      set({
        isGeneratingSchemaDoc: false,
        schemaDocError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearSchemaDoc: () => set({ schemaDoc: null, schemaDocError: null, isGeneratingSchemaDoc: false }),

  diagnoseConnection: async ({ connectionId, errorMessage }) => {
    set({ isDiagnosingConnection: true, connectionDiagnosis: null, connectionDiagnosisError: null });
    try {
      const result = await aiCommands.diagnoseConnection({ connectionId, errorMessage });
      set({ connectionDiagnosis: result, isDiagnosingConnection: false });
    } catch (e) {
      set({
        isDiagnosingConnection: false,
        connectionDiagnosisError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearConnectionDiagnosis: () => set({ connectionDiagnosis: null, connectionDiagnosisError: null, isDiagnosingConnection: false }),

  analyzeQueries: async ({ connectionId }) => {
    set({ isAnalyzingQueries: true, queryAnalysis: null, queryAnalysisError: null });
    try {
      const result = await aiCommands.analyzeQueries({ connectionId });
      set({ queryAnalysis: result, isAnalyzingQueries: false });
    } catch (e) {
      set({
        isAnalyzingQueries: false,
        queryAnalysisError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearQueryAnalysis: () => set({ queryAnalysis: null, queryAnalysisError: null, isAnalyzingQueries: false }),

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

// Listen for cross-window config changes so all windows stay in sync.
if ('__TAURI_INTERNALS__' in globalThis) {
  void onAiConfigChanged(() => {
    void useAiStore.getState().loadConfig();
  });
}
