import { create } from 'zustand';
import { aiCommands, onAiStreamChunk, onAiStreamError, onAiConfigChanged } from '../commands/ai';
import { redactSensitiveText } from '../lib/aiQueryActions';
import { extractSqlFromResponse } from '../lib/extractSql';
import { normalizeAiProviders } from '../lib/aiProviders';
import { extractQuestions, parseToolCallQuestions } from '../lib/extractQuestions';
import type { AiChatMessage } from '../types';
import { initialNl2Sql, type AiStore } from './ai/types';
import { useSettingsStore } from './settingsStore';

function findMcpToolName(toolCalls?: { name: string }[]): string | null {
  const mcpCall = toolCalls?.find((tc) => tc.name.startsWith('mcp/'));
  return mcpCall?.name ?? null;
}

export type { AiStore } from './ai/types';
export { initialNl2Sql } from './ai/types';

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
    } catch {
      console.error('Failed to load AI providers');
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

  setNl2SqlInput: (input) => set((s) => ({ nl2sql: { ...s.nl2sql, input } })),

  generateSql: async (params) => {
    const { nl2sql } = get();
    if (!nl2sql.input.trim()) return;

    const requestId = crypto.randomUUID();
    console.debug('[AI] generateSql:', {
      dbSessionId: params.dbSessionId,
      database: params.database,
      currentTable: params.currentTable,
      recentQueriesCount: params.recentQueries?.length ?? 0,
      contextFilesCount: params.contextFiles?.length ?? 0,
      contextTablesCount: params.contextTables?.length ?? 0,
      inputLen: nl2sql.input.length,
      requestId,
    });
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
      console.error('[AI] generateSql failed');
      set((s) => ({
        nl2sql: { ...s.nl2sql, isGenerating: false },
        nl2sqlError: e instanceof Error ? e.message : String(e),
      }));
    }
  },

  clearNl2Sql: () => set({ nl2sql: { ...initialNl2Sql }, nl2sqlError: null }),

  // ── Diagnosis ──

  diagnoseError: async (params) => {
    const safeParams = {
      ...params,
      sql: redactSensitiveText(params.sql),
      errorMessage: redactSensitiveText(params.errorMessage),
    };
    console.debug('[AI] diagnoseError:', {
      dbSessionId: params.dbSessionId,
      database: params.database,
      sqlLen: params.sql.length,
      safeSqlLen: safeParams.sql.length,
      sqlRedacted: safeParams.sql !== params.sql,
      errorLen: params.errorMessage.length,
      safeErrorLen: safeParams.errorMessage.length,
      errorRedacted: safeParams.errorMessage !== params.errorMessage,
    });
    set({ isDiagnosing: true, diagnosisError: null, diagnosis: null });
    try {
      const result = await aiCommands.diagnoseError(safeParams);
      console.debug('[AI] diagnoseError result:', { changes: result?.changes?.length });
      set({ diagnosis: result, isDiagnosing: false });
    } catch (e) {
      set({
        isDiagnosing: false,
        diagnosisError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearDiagnosis: () => set({ diagnosis: null, isDiagnosing: false, diagnosisError: null }),

  // ── EXPLAIN Analysis ──

  analyzeExplain: async (params) => {
    console.debug('[AI] analyzeExplain:', {
      dbSessionId: params.dbSessionId,
      sql_len: params.originalSql?.length,
    });
    set({ isAnalyzingExplain: true, explainAnalysis: null, explainError: null });
    try {
      const result = await aiCommands.analyzeExplain(params);
      console.debug('[AI] analyzeExplain result:', {
        bottlenecks: result?.bottlenecks?.length,
        suggestions: result?.suggestions?.length,
      });
      set({ explainAnalysis: result, isAnalyzingExplain: false });
    } catch (e) {
      console.error('[AI] analyzeExplain failed');
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

  parseFilter: async ({ dbSessionId, database, table }) => {
    const { nlFilterInput } = get();
    if (!nlFilterInput.trim()) return null;

    console.debug('[AI] parseFilter:', {
      dbSessionId,
      database,
      table,
      inputLen: nlFilterInput.length,
    });
    set({ isParsingFilter: true, nlFilterError: null, parsedFilters: null });

    try {
      const filters = await aiCommands.parseFilter({
        dbSessionId,
        database,
        table,
        naturalLanguage: nlFilterInput,
      });
      console.debug('[AI] parseFilter result:', { filterCount: filters?.length });
      set({ parsedFilters: filters, isParsingFilter: false });
      return filters;
    } catch (e) {
      console.error('[AI] parseFilter failed');
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
        streamMcpToolName: null,
        requestId: null,
      },
    });
  },

  sendChatMessage: async ({
    dbSessionId,
    database,
    content,
    includeSchema = true,
    contextFiles,
    contextTables,
  }) => {
    const { chatSession } = get();
    if (!chatSession) return;

    console.debug('[AI] sendChatMessage:', {
      dbSessionId,
      database,
      contentLen: content.length,
      includeSchema,
      historyLen: chatSession.messages.length,
    });

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
        streamMcpToolName: null,
        requestId,
      },
    });

    try {
      await aiCommands.chat({
        dbSessionId,
        database,
        messages: [...chatSession.messages, ...newMessages],
        requestId,
        includeSchema,
        contextFiles,
        contextTables,
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
            streamMcpToolName: null,
            requestId: null,
            messages: [
              ...session.messages,
              {
                role: 'assistant',
                content: `Error: ${e instanceof Error ? e.message : String(e)}`,
              },
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
          streamMcpToolName: null,
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
        streamMcpToolName: null,
        requestId: null,
      },
    });
  },

  sendWorkflowChatMessage: async ({
    dbSessionId,
    database,
    content,
    includeSchema = true,
    contextFiles,
    contextTables,
  }) => {
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
        streamMcpToolName: null,
        requestId,
      },
    });

    try {
      await aiCommands.chat({
        dbSessionId,
        database,
        messages: [...workflowChat.messages, ...newMessages],
        requestId,
        includeSchema,
        scenario: 'workflow_generate',
        contextFiles,
        contextTables,
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
            streamMcpToolName: null,
            requestId: null,
            messages: [
              ...session.messages,
              {
                role: 'assistant',
                content: `Error: ${e instanceof Error ? e.message : String(e)}`,
              },
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
          streamMcpToolName: null,
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
      const mcpToolName = findMcpToolName(payload.toolCalls) ?? session.streamMcpToolName ?? null;
      if (payload.done) {
        const { cleanContent, questions: xmlQuestions } = extractQuestions(newContent);
        const toolCalls =
          payload.toolCalls && payload.toolCalls.length > 0 ? payload.toolCalls : undefined;

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
            streamMcpToolName: null,
            requestId: null,
          },
        });
      } else {
        set({
          [targetSession]: {
            ...session,
            streamContent: newContent,
            streamReasoning: newReasoning,
            streamMcpToolName: mcpToolName,
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
            streamMcpToolName: null,
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

  clearWorkflowResult: () => set({ workflowExecutionResult: null, workflowError: null }),

  schemaDoc: null,
  isGeneratingSchemaDoc: false,
  schemaDocError: null,

  connectionDiagnosis: null,
  isDiagnosingConnection: false,
  connectionDiagnosisError: null,

  queryAnalysis: null,
  isAnalyzingQueries: false,
  queryAnalysisError: null,

  generateSchemaDoc: async ({ dbSessionId, database }) => {
    set({ isGeneratingSchemaDoc: true, schemaDoc: null, schemaDocError: null });
    try {
      const doc = await aiCommands.generateSchemaDoc({ dbSessionId, database });
      set({ schemaDoc: doc, isGeneratingSchemaDoc: false });
    } catch (e) {
      set({
        isGeneratingSchemaDoc: false,
        schemaDocError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearSchemaDoc: () =>
    set({ schemaDoc: null, schemaDocError: null, isGeneratingSchemaDoc: false }),

  diagnoseConnection: async ({ connectionId, errorMessage }) => {
    set({
      isDiagnosingConnection: true,
      connectionDiagnosis: null,
      connectionDiagnosisError: null,
    });
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

  clearConnectionDiagnosis: () =>
    set({
      connectionDiagnosis: null,
      connectionDiagnosisError: null,
      isDiagnosingConnection: false,
    }),

  analyzeQueries: async ({ dbSessionId }) => {
    set({ isAnalyzingQueries: true, queryAnalysis: null, queryAnalysisError: null });
    try {
      const result = await aiCommands.analyzeQueries({ dbSessionId });
      set({ queryAnalysis: result, isAnalyzingQueries: false });
    } catch (e) {
      set({
        isAnalyzingQueries: false,
        queryAnalysisError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  clearQueryAnalysis: () =>
    set({ queryAnalysis: null, queryAnalysisError: null, isAnalyzingQueries: false }),

  mcpServers: [],
  mcpTools: [],
  mcpConnecting: false,
  mcpConnectingServerId: null,
  mcpError: null,
  mcpServerErrors: {},

  connectMcpServer: async (serverId) => {
    const config = useSettingsStore
      .getState()
      .settings.mcpClientServers?.find((c) => c.id === serverId);
    if (!config) {
      const msg = `MCP server config not found: ${serverId}`;
      set({ mcpError: msg });
      throw new Error(msg);
    }
    set((s) => {
      const nextErrors = { ...s.mcpServerErrors };
      delete nextErrors[serverId];
      return {
        mcpConnecting: true,
        mcpConnectingServerId: serverId,
        mcpError: null,
        mcpServerErrors: nextErrors,
      };
    });
    try {
      await aiCommands.mcpClientConnect(config);
      await get().loadMcpServers();
      await get().loadMcpTools();
      set({ mcpConnecting: false, mcpConnectingServerId: null });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set((s) => ({
        mcpConnecting: false,
        mcpConnectingServerId: null,
        mcpServerErrors: { ...s.mcpServerErrors, [serverId]: msg },
      }));
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

  saveMcpClientServers: async (configs) => {
    const prevConfigs = useSettingsStore.getState().settings.mcpClientServers ?? [];
    const connectedIds = new Set(get().mcpServers.map((s) => s.serverId));
    const nextIds = new Set(configs.map((c) => c.id));

    await useSettingsStore.getState().updateSettings({ mcpClientServers: configs });

    for (const id of connectedIds) {
      if (!nextIds.has(id)) {
        await get().disconnectMcpServer(id);
      }
    }

    for (const config of configs) {
      if (!connectedIds.has(config.id)) continue;
      const prev = prevConfigs.find((c) => c.id === config.id);
      if (!prev || JSON.stringify(prev) === JSON.stringify(config)) continue;
      await get().disconnectMcpServer(config.id);
      if (config.enabled) {
        try {
          await get().connectMcpServer(config.id);
        } catch {
          // connectMcpServer already records mcpError
        }
      }
    }
  },

  loadMcpServers: async () => {
    try {
      const servers = await aiCommands.mcpClientList();
      set({ mcpServers: servers });
    } catch {
      console.error('Failed to load MCP servers');
    }
  },

  loadMcpTools: async () => {
    try {
      const tools = await aiCommands.mcpClientTools();
      set({ mcpTools: tools });
    } catch {
      console.error('Failed to load MCP tools');
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

  clearMcpServerError: (serverId) =>
    set((s) => {
      const next = { ...s.mcpServerErrors };
      delete next[serverId];
      return { mcpServerErrors: next };
    }),
}));

// Listen for cross-window config changes so all windows stay in sync.
if ('__TAURI_INTERNALS__' in globalThis) {
  void onAiConfigChanged(() => {
    void useAiStore.getState().loadConfig();
  });
}

if (typeof window !== 'undefined') {
  (window as Window & { __datazenAiStore?: typeof useAiStore }).__datazenAiStore = useAiStore;
}
