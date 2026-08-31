import { describe, expect, it, vi, beforeEach } from 'vitest';
import { buildQueryDiagnosisContext } from '../../lib/aiQueryActions';

const mockAiCommands = {
  getConfig: vi.fn(),
  getProviders: vi.fn(),
  fetchRemoteModels: vi.fn(),
  validateConfig: vi.fn(),
  saveConfig: vi.fn(),
  deleteConfig: vi.fn(),
  generateSql: vi.fn(),
  diagnoseError: vi.fn(),
  analyzeExplain: vi.fn(),
  chat: vi.fn(),
  parseFilter: vi.fn(),
  workflowList: vi.fn(),
  workflowExecute: vi.fn(),
  generateSchemaDoc: vi.fn(),
  diagnoseConnection: vi.fn(),
  analyzeQueries: vi.fn(),
  mcpClientConnect: vi.fn(),
  mcpClientDisconnect: vi.fn(),
  mcpClientList: vi.fn(),
  mcpClientTools: vi.fn(),
  mcpClientCallTool: vi.fn(),
};

const mockUpdateSettings = vi.fn();
let mockMcpClientServers: Array<{
  id: string;
  name: string;
  transport: 'stdio';
  command?: string;
  enabled: boolean;
}> = [];

function syncMcpSettingsMock() {
  mockUpdateSettings.mockImplementation(
    async (partial?: { mcpClientServers?: typeof mockMcpClientServers }) => {
      if (partial?.mcpClientServers) {
        mockMcpClientServers = partial.mcpClientServers;
      }
    },
  );
}
syncMcpSettingsMock();

vi.mock('../settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({
      settings: { mcpClientServers: mockMcpClientServers },
      updateSettings: mockUpdateSettings,
    }),
  },
}));

const mockUnlisten = vi.fn();
const streamChunkHandler = vi.fn();
const streamErrorHandler = vi.fn();

vi.mock('../../commands/ai', () => ({
  aiCommands: mockAiCommands,
  onAiStreamChunk: vi.fn().mockImplementation((cb) => {
    streamChunkHandler.mockImplementation(cb);
    return Promise.resolve(mockUnlisten);
  }),
  onAiStreamError: vi.fn().mockImplementation((cb) => {
    streamErrorHandler.mockImplementation(cb);
    return Promise.resolve(mockUnlisten);
  }),
  onAiConfigChanged: vi.fn().mockResolvedValue(mockUnlisten),
}));

describe('aiStore', () => {
  let useAiStore: typeof import('../aiStore').useAiStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockMcpClientServers = [];
    syncMcpSettingsMock();
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const mod = await import('../aiStore');
    useAiStore = mod.useAiStore;
    useAiStore.setState({
      config: null,
      isConfigured: false,
      providers: [],
      configLoading: false,
      configError: null,
      nl2sql: { input: '', generatedSql: '', isGenerating: false, requestId: null },
      chatSession: null,
      workflowChat: null,
      workflows: [],
      mcpServers: [],
      mcpTools: [],
    });
  });

  describe('config management', () => {
    it('loadConfig success and error', async () => {
      mockAiCommands.getConfig.mockResolvedValueOnce({ provider: 'openai', apiKey: 'k' });
      await useAiStore.getState().loadConfig();
      expect(useAiStore.getState().isConfigured).toBe(true);

      mockAiCommands.getConfig.mockRejectedValueOnce(new Error('load fail'));
      await useAiStore.getState().loadConfig();
      expect(useAiStore.getState().configError).toBe('load fail');
    });

    it('loadConfig skips without Tauri', async () => {
      delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
      await useAiStore.getState().loadConfig();
      expect(mockAiCommands.getConfig).not.toHaveBeenCalled();
    });

    it('loadProviders normalizes providers', async () => {
      mockAiCommands.getProviders.mockResolvedValueOnce([{ id: 'openai', name: 'OpenAI' }]);
      await useAiStore.getState().loadProviders();
      const providers = useAiStore.getState().providers;
      expect(providers.map((p) => p.providerType)).toContain('deep_seek');
      expect(providers.map((p) => p.providerType)).toContain('ollama');
    });

    it('fetchRemoteModels success and error', async () => {
      mockAiCommands.fetchRemoteModels.mockResolvedValueOnce([{ id: 'gpt-4', name: 'GPT-4' }]);
      const models = await useAiStore.getState().fetchRemoteModels('openai', 'https://api', 'key');
      expect(models).toHaveLength(1);

      mockAiCommands.fetchRemoteModels.mockRejectedValueOnce(new Error('fetch fail'));
      const empty = await useAiStore.getState().fetchRemoteModels('openai', 'x', 'y');
      expect(empty).toEqual([]);
      expect(useAiStore.getState().configError).toBe('fetch fail');
    });

    it('validateConfig and saveConfig', async () => {
      const cfg = { provider: 'openai' as const, apiKey: 'k', model: 'gpt-4' };
      mockAiCommands.validateConfig.mockResolvedValueOnce(undefined);
      expect(await useAiStore.getState().validateConfig(cfg)).toBe(true);

      mockAiCommands.validateConfig.mockRejectedValueOnce(new Error('invalid'));
      expect(await useAiStore.getState().validateConfig(cfg)).toBe(false);

      mockAiCommands.saveConfig.mockResolvedValueOnce(undefined);
      expect(await useAiStore.getState().saveConfig(cfg)).toBe(true);
      expect(useAiStore.getState().isConfigured).toBe(true);

      mockAiCommands.saveConfig.mockRejectedValueOnce(new Error('save fail'));
      expect(await useAiStore.getState().saveConfig(cfg)).toBe(false);
    });

    it('deleteConfig and clearError', async () => {
      mockAiCommands.deleteConfig.mockResolvedValueOnce(undefined);
      await useAiStore.getState().deleteConfig();
      expect(useAiStore.getState().isConfigured).toBe(false);

      useAiStore.setState({ configError: 'err' });
      useAiStore.getState().clearError();
      expect(useAiStore.getState().configError).toBeNull();
    });
  });

  describe('NL2SQL', () => {
    it('generateSql skips empty input', async () => {
      await useAiStore.getState().generateSql({ connectionId: 'c', database: 'db' });
      expect(mockAiCommands.generateSql).not.toHaveBeenCalled();
    });

    it('generateSql sets generating and handles error', async () => {
      useAiStore.getState().setNl2SqlInput('show users');
      mockAiCommands.generateSql.mockRejectedValueOnce(new Error('gen fail'));
      await useAiStore.getState().generateSql({ connectionId: 'c', database: 'db' });
      expect(useAiStore.getState().nl2sql.isGenerating).toBe(false);
      expect(useAiStore.getState().nl2sqlError).toBe('gen fail');
    });

    it('clearNl2Sql resets state', () => {
      useAiStore.getState().setNl2SqlInput('x');
      useAiStore.getState().clearNl2Sql();
      expect(useAiStore.getState().nl2sql.input).toBe('');
    });
  });

  describe('diagnosis and explain', () => {
    it('passes the helper redacted diagnosis payload to the AI command', async () => {
      const secrets = ['store-json-secret', 'store-password-secret', 'store-error-secret'];
      const context = buildQueryDiagnosisContext({
        sql: `SELECT '{\\"token\\":\\"store-json-secret\\"}', password = 'store-password-secret'`,
        errorMessage: `query failed: {\\"token\\":\\"store-error-secret\\"}`,
        connectionId: 'c',
        dbSessionId: 'session',
        databaseType: 'postgresql',
        database: 'db',
      });
      expect(context.ok).toBe(true);
      if (!context.ok) return;

      mockAiCommands.diagnoseError.mockResolvedValueOnce({ changes: [] });
      await useAiStore.getState().diagnoseError(context.context.diagnosisParams);

      const payload = mockAiCommands.diagnoseError.mock.calls[0]?.[0];
      for (const secret of secrets) expect(JSON.stringify(payload)).not.toContain(secret);
    });

    it('diagnoseError success and error', async () => {
      mockAiCommands.diagnoseError.mockResolvedValueOnce({ changes: [{ sql: 'fix' }] });
      await useAiStore.getState().diagnoseError({
        connectionId: 'c',
        database: 'db',
        sql: 'SELECT',
        errorMessage: 'syntax',
      });
      expect(useAiStore.getState().diagnosis?.changes).toHaveLength(1);

      mockAiCommands.diagnoseError.mockRejectedValueOnce(new Error('diag fail'));
      await useAiStore.getState().diagnoseError({
        connectionId: 'c',
        database: 'db',
        sql: 'SELECT',
        errorMessage: 'syntax',
      });
      expect(useAiStore.getState().diagnosisError).toBe('diag fail');
    });

    it('clearDiagnosis resets', () => {
      useAiStore.setState({ diagnosis: { changes: [] }, isDiagnosing: true });
      useAiStore.getState().clearDiagnosis();
      expect(useAiStore.getState().diagnosis).toBeNull();
    });

    it('analyzeExplain success and error', async () => {
      mockAiCommands.analyzeExplain.mockResolvedValueOnce({ bottlenecks: [], suggestions: [] });
      await useAiStore.getState().analyzeExplain({
        connectionId: 'c',
        explainOutput: 'Seq Scan',
        originalSql: 'SELECT 1',
      });
      expect(useAiStore.getState().explainAnalysis).not.toBeNull();

      mockAiCommands.analyzeExplain.mockRejectedValueOnce(new Error('explain fail'));
      await useAiStore.getState().analyzeExplain({
        connectionId: 'c',
        explainOutput: 'x',
        originalSql: 'y',
      });
      expect(useAiStore.getState().explainError).toBe('explain fail');
    });

    it('clearExplainAnalysis resets', () => {
      useAiStore.getState().clearExplainAnalysis();
      expect(useAiStore.getState().explainAnalysis).toBeNull();
    });
  });

  describe('smart filter', () => {
    it('parseFilter skips empty input', async () => {
      const result = await useAiStore.getState().parseFilter({
        connectionId: 'c',
        database: 'db',
        table: 'users',
      });
      expect(result).toBeNull();
    });

    it('parseFilter success and error', async () => {
      useAiStore.getState().setNlFilterInput('age > 18');
      mockAiCommands.parseFilter.mockResolvedValueOnce([
        { column: 'age', operator: '>', value: 18 },
      ]);
      const filters = await useAiStore.getState().parseFilter({
        connectionId: 'c',
        database: 'db',
        table: 'users',
      });
      expect(filters).toHaveLength(1);

      useAiStore.getState().setNlFilterInput('bad');
      mockAiCommands.parseFilter.mockRejectedValueOnce(new Error('parse fail'));
      expect(
        await useAiStore.getState().parseFilter({
          connectionId: 'c',
          database: 'db',
          table: 'users',
        }),
      ).toBeNull();
      expect(useAiStore.getState().nlFilterError).toBe('parse fail');
    });

    it('clearNlFilter resets', () => {
      useAiStore.getState().clearNlFilter();
      expect(useAiStore.getState().nlFilterInput).toBe('');
    });
  });

  describe('chat', () => {
    it('sendChatMessage no-ops without session', async () => {
      await useAiStore.getState().sendChatMessage({ content: 'hi' });
      expect(mockAiCommands.chat).not.toHaveBeenCalled();
    });

    it('sendChatMessage adds user message and handles error', async () => {
      useAiStore.getState().initChatSession();
      mockAiCommands.chat.mockRejectedValueOnce(new Error('chat fail'));
      await useAiStore.getState().sendChatMessage({ content: 'hello' });
      const session = useAiStore.getState().chatSession!;
      expect(session.messages.some((m) => m.role === 'user' && m.content === 'hello')).toBe(true);
      expect(session.messages.some((m) => m.content.includes('chat fail'))).toBe(true);
    });

    it('sendChatMessage responds to ask_questions tool call', async () => {
      useAiStore.getState().initChatSession();
      const session = useAiStore.getState().chatSession!;
      useAiStore.setState({
        chatSession: {
          ...session,
          messages: [
            {
              role: 'assistant',
              content: '',
              toolCalls: [{ id: 'tc-1', name: 'ask_questions', arguments: {} }],
            },
          ],
        },
      });
      mockAiCommands.chat.mockResolvedValueOnce(undefined);
      await useAiStore.getState().sendChatMessage({ content: 'my answer' });
      expect(mockAiCommands.chat).toHaveBeenCalledWith(
        expect.objectContaining({
          messages: expect.arrayContaining([
            expect.objectContaining({ role: 'tool', content: 'my answer', toolCallId: 'tc-1' }),
          ]),
        }),
      );
    });

    it('clearChat resets messages', () => {
      useAiStore.getState().initChatSession();
      useAiStore.getState().clearChat();
      expect(useAiStore.getState().chatSession!.messages).toEqual([]);
    });
  });

  describe('workflow chat', () => {
    it('sendWorkflowChatMessage and clearWorkflowChat', async () => {
      useAiStore.getState().initWorkflowChat();
      mockAiCommands.chat.mockResolvedValueOnce(undefined);
      await useAiStore.getState().sendWorkflowChatMessage({ content: 'gen workflow' });
      expect(mockAiCommands.chat).toHaveBeenCalledWith(
        expect.objectContaining({ scenario: 'workflow_generate' }),
      );
      useAiStore.getState().clearWorkflowChat();
      expect(useAiStore.getState().workflowChat!.messages).toEqual([]);
    });
  });

  describe('stream handling', () => {
    it('handleStreamChunk updates nl2sql', () => {
      const requestId = 'req-nl2sql';
      useAiStore.setState({
        nl2sql: { input: 'q', generatedSql: '', isGenerating: true, requestId },
      });
      useAiStore.getState().handleStreamChunk({ requestId, content: 'SELECT 1', done: false });
      expect(useAiStore.getState().nl2sql.generatedSql).toContain('SELECT');

      useAiStore.getState().handleStreamChunk({ requestId, content: '', done: true });
      expect(useAiStore.getState().nl2sql.isGenerating).toBe(false);
    });

    it('handleStreamChunk completes chat session', () => {
      useAiStore.getState().initChatSession();
      const requestId = useAiStore.getState().chatSession!.requestId ?? 'req-chat';
      useAiStore.setState({
        chatSession: { ...useAiStore.getState().chatSession!, requestId, isStreaming: true },
      });
      useAiStore.getState().handleStreamChunk({
        requestId,
        content: 'Hello',
        done: false,
      });
      expect(useAiStore.getState().chatSession!.streamContent).toBe('Hello');

      useAiStore.getState().handleStreamChunk({
        requestId,
        content: ' world',
        done: true,
      });
      const msgs = useAiStore.getState().chatSession!.messages;
      expect(msgs[msgs.length - 1].role).toBe('assistant');
    });

    it('handleStreamChunk tracks MCP tool name during stream', () => {
      useAiStore.getState().initChatSession();
      const requestId = useAiStore.getState().chatSession!.requestId ?? 'req-mcp';
      useAiStore.setState({
        chatSession: { ...useAiStore.getState().chatSession!, requestId, isStreaming: true },
      });
      useAiStore.getState().handleStreamChunk({
        requestId,
        content: '',
        done: false,
        toolCalls: [{ id: 'tc-1', name: 'mcp/files/read_file', arguments: {} }],
      });
      expect(useAiStore.getState().chatSession!.streamMcpToolName).toBe('mcp/files/read_file');
    });

    it('setupEventListeners wires chunk and error handlers', async () => {
      const cleanup = await useAiStore.getState().setupEventListeners();
      expect(typeof cleanup).toBe('function');
      cleanup();

      useAiStore.setState({
        nl2sql: { input: '', generatedSql: '', isGenerating: true, requestId: 'err-req' },
      });
      streamErrorHandler({ requestId: 'err-req', error: 'stream error' });
      expect(useAiStore.getState().nl2sqlError).toBe('stream error');
    });
  });

  describe('workflows', () => {
    it('loadWorkflows and executeWorkflow', async () => {
      vi.useFakeTimers();
      mockAiCommands.workflowList.mockResolvedValueOnce([{ id: 'wf-1', name: 'Test' }]);
      const loadPromise = useAiStore.getState().loadWorkflows();
      await vi.runAllTimersAsync();
      await loadPromise;
      expect(useAiStore.getState().workflows).toHaveLength(1);
      vi.useRealTimers();

      mockAiCommands.workflowExecute.mockResolvedValueOnce({ success: true, output: 'ok' });
      await useAiStore.getState().executeWorkflow({ workflowId: 'wf-1', variables: {} });
      expect(useAiStore.getState().workflowExecutionResult).toEqual({
        success: true,
        output: 'ok',
      });

      mockAiCommands.workflowExecute.mockRejectedValueOnce(new Error('exec fail'));
      await useAiStore.getState().executeWorkflow({ workflowId: 'wf-1', variables: {} });
      expect(useAiStore.getState().workflowError).toContain('exec fail');
    });

    it('clearWorkflowResult', () => {
      useAiStore.setState({ workflowExecutionResult: { success: true }, workflowError: 'x' });
      useAiStore.getState().clearWorkflowResult();
      expect(useAiStore.getState().workflowExecutionResult).toBeNull();
    });
  });

  describe('schema doc and analysis', () => {
    it('generateSchemaDoc and clearSchemaDoc', async () => {
      mockAiCommands.generateSchemaDoc.mockResolvedValueOnce('# Schema');
      await useAiStore.getState().generateSchemaDoc({ connectionId: 'c', database: 'db' });
      expect(useAiStore.getState().schemaDoc).toBe('# Schema');
      useAiStore.getState().clearSchemaDoc();
      expect(useAiStore.getState().schemaDoc).toBeNull();
    });

    it('diagnoseConnection and clearConnectionDiagnosis', async () => {
      mockAiCommands.diagnoseConnection.mockResolvedValueOnce({ suggestions: [] });
      await useAiStore
        .getState()
        .diagnoseConnection({ connectionId: 'c', errorMessage: 'timeout' });
      expect(useAiStore.getState().connectionDiagnosis).not.toBeNull();
      useAiStore.getState().clearConnectionDiagnosis();
      expect(useAiStore.getState().connectionDiagnosis).toBeNull();
    });

    it('analyzeQueries and clearQueryAnalysis', async () => {
      mockAiCommands.analyzeQueries.mockResolvedValueOnce({ summary: 'ok' });
      await useAiStore.getState().analyzeQueries({ connectionId: 'c' });
      expect(useAiStore.getState().queryAnalysis).not.toBeNull();
      useAiStore.getState().clearQueryAnalysis();
      expect(useAiStore.getState().queryAnalysis).toBeNull();
    });
  });

  describe('MCP', () => {
    it('connectMcpServer loads servers and tools from saved config', async () => {
      mockMcpClientServers = [
        { id: 's1', name: 'Server', transport: 'stdio', command: 'node', enabled: true },
      ];
      mockAiCommands.mcpClientConnect.mockResolvedValueOnce(undefined);
      mockAiCommands.mcpClientList.mockResolvedValueOnce([
        { serverId: 's1', serverName: 'Server', toolsCount: 1 },
      ]);
      mockAiCommands.mcpClientTools.mockResolvedValueOnce([{ toolName: 'tool1', serverId: 's1' }]);
      await useAiStore.getState().connectMcpServer('s1');
      expect(mockAiCommands.mcpClientConnect).toHaveBeenCalledWith(
        expect.objectContaining({ id: 's1', command: 'node' }),
      );
      expect(useAiStore.getState().mcpServers).toHaveLength(1);
      expect(useAiStore.getState().mcpTools).toHaveLength(1);
    });

    it('connectMcpServer throws when saved config missing', async () => {
      await expect(useAiStore.getState().connectMcpServer('missing')).rejects.toThrow(
        'MCP server config not found: missing',
      );
    });

    it('connectMcpServer throws on error', async () => {
      mockMcpClientServers = [
        { id: 's1', name: 'S', transport: 'stdio', command: 'x', enabled: true },
      ];
      mockAiCommands.mcpClientConnect.mockRejectedValueOnce(new Error('connect fail'));
      await expect(useAiStore.getState().connectMcpServer('s1')).rejects.toThrow('connect fail');
      expect(useAiStore.getState().mcpServerErrors.s1).toBe('connect fail');
      expect(useAiStore.getState().mcpError).toBeNull();
    });

    it('saveMcpClientServers persists via settingsStore', async () => {
      const configs = [
        { id: 's1', name: 'Server', transport: 'stdio' as const, command: 'node', enabled: true },
      ];
      await useAiStore.getState().saveMcpClientServers(configs);
      expect(mockUpdateSettings).toHaveBeenCalledWith({ mcpClientServers: configs });
    });

    it('saveMcpClientServers disconnects removed connected servers', async () => {
      useAiStore.setState({
        mcpServers: [{ serverId: 's1', serverName: 'Server', toolsCount: 1 }],
      });
      mockAiCommands.mcpClientDisconnect.mockResolvedValueOnce(undefined);
      mockAiCommands.mcpClientList.mockResolvedValueOnce([]);
      mockAiCommands.mcpClientTools.mockResolvedValueOnce([]);
      await useAiStore.getState().saveMcpClientServers([]);
      expect(mockAiCommands.mcpClientDisconnect).toHaveBeenCalledWith('s1');
    });

    it('saveMcpClientServers reconnects when connected server config changes', async () => {
      mockMcpClientServers = [
        { id: 's1', name: 'Server', transport: 'stdio', command: 'node', enabled: true },
      ];
      useAiStore.setState({
        mcpServers: [{ serverId: 's1', serverName: 'Server', toolsCount: 1 }],
      });
      mockAiCommands.mcpClientDisconnect.mockResolvedValue(undefined);
      mockAiCommands.mcpClientConnect.mockResolvedValue(undefined);
      mockAiCommands.mcpClientList.mockResolvedValue([
        { serverId: 's1', serverName: 'Server', toolsCount: 1 },
      ]);
      mockAiCommands.mcpClientTools.mockResolvedValue([]);

      const updated = [
        {
          id: 's1',
          name: 'Server',
          transport: 'stdio' as const,
          command: 'node-v2',
          enabled: true,
        },
      ];
      await useAiStore.getState().saveMcpClientServers(updated);

      expect(mockAiCommands.mcpClientDisconnect).toHaveBeenCalledWith('s1');
      expect(mockAiCommands.mcpClientConnect).toHaveBeenCalledWith(
        expect.objectContaining({ id: 's1', command: 'node-v2' }),
      );
    });

    it('disconnectMcpServer and callMcpTool', async () => {
      mockAiCommands.mcpClientDisconnect.mockResolvedValueOnce(undefined);
      mockAiCommands.mcpClientList.mockResolvedValueOnce([]);
      mockAiCommands.mcpClientTools.mockResolvedValueOnce([]);
      await useAiStore.getState().disconnectMcpServer('s1');

      mockAiCommands.mcpClientCallTool.mockResolvedValueOnce('result');
      const out = await useAiStore.getState().callMcpTool({
        serverId: 's1',
        toolName: 't',
        arguments: {},
      });
      expect(out).toBe('result');

      mockAiCommands.mcpClientCallTool.mockRejectedValueOnce(new Error('call fail'));
      await expect(
        useAiStore.getState().callMcpTool({ serverId: 's1', toolName: 't', arguments: {} }),
      ).rejects.toThrow('call fail');
    });

    it('clearMcpError', () => {
      useAiStore.setState({ mcpError: 'err' });
      useAiStore.getState().clearMcpError();
      expect(useAiStore.getState().mcpError).toBeNull();
    });

    it('clearMcpServerError', () => {
      useAiStore.setState({ mcpServerErrors: { s1: 'fail' } });
      useAiStore.getState().clearMcpServerError('s1');
      expect(useAiStore.getState().mcpServerErrors).toEqual({});
    });
  });
});
