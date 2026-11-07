import type { AiTargetClaim, CancelReceipt } from '../../ai/targetClaim';
import type {
  AiAgentMode,
  AiChatContextSnapshot,
  AiChatMessage,
  AiChatSession,
  AiMemoryReference,
  AiModelProfile,
  AiProviderConfig,
  AiSessionSummary,
  AiSettingsConfig,
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
} from '../../types';

export interface Nl2SqlState {
  input: string;
  /** Raw accumulated content during streaming (before extractSql cleanup). */
  streamingSql: string;
  /** Extracted SQL preview for display during streaming. */
  streamingPreview: string;
  /** Final cleaned SQL after generation completes. */
  generatedSql: string;
  /** Memories the host injected for the latest generation. */
  memoryUsed: AiMemoryReference[];
  isGenerating: boolean;
  requestId: string | null;
}

export const initialNl2Sql: Nl2SqlState = {
  input: '',
  streamingSql: '',
  streamingPreview: '',
  generatedSql: '',
  memoryUsed: [],
  isGenerating: false,
  requestId: null,
};

export interface AiStore {
  config: AiProviderConfig | null;
  settingsConfig: AiSettingsConfig | null;
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
  /** Sessions for the current connection/database, newest first. */
  chatSessions: AiSessionSummary[];
  sessionHistoryLoading: boolean;
  sessionHistoryError: string | null;
  /** Visible failure of legacy localStorage -> AppDb migration/fallback. */
  sessionMigrationError: string | null;
  /** Count of legacy conversations migrated successfully on this load. */
  sessionMigrationSuccess: number;
  /** Non-blocking Ask-mode candidate notice; cleared by the user or a new run. */
  memoryCandidateNotice: { requestId: string; count: number } | null;

  nlFilterInput: string;
  parsedFilters: FilterCondition[] | null;
  isParsingFilter: boolean;
  nlFilterError: string | null;

  remoteModels: ModelInfo[];
  fetchingRemoteModels: boolean;

  loadConfig: () => Promise<void>;
  loadSettingsConfig: () => Promise<void>;
  saveSettingsConfig: (config: AiSettingsConfig) => Promise<boolean>;
  setActiveProfile: (profileId: string) => Promise<boolean>;
  saveProfile: (profile: AiModelProfile) => Promise<boolean>;
  deleteProfile: (profileId: string) => Promise<boolean>;
  loadProviders: () => Promise<void>;
  fetchRemoteModels: (protocol: string, endpoint: string, apiKey: string) => Promise<ModelInfo[]>;
  validateConfig: (config: AiProviderConfig) => Promise<boolean>;
  saveConfig: (config: AiProviderConfig) => Promise<boolean>;
  deleteConfig: () => Promise<void>;
  clearError: () => void;

  setNl2SqlInput: (input: string) => void;
  generateSql: (params: {
    dbSessionId: string;
    database: string;
    currentTable?: string;
    recentQueries?: string[];
    contextFiles?: string[];
    contextTables?: string[];
    connectionId?: string;
    memoryMode?: 'off' | 'ask' | 'auto';
    memoryScope?:
      | 'organization'
      | 'user'
      | 'connection'
      | 'database'
      | 'schema'
      | 'table'
      | 'column';
  }) => Promise<void>;
  clearNl2Sql: () => void;

  diagnoseError: (params: {
    dbSessionId: string;
    database: string;
    sql: string;
    errorMessage: string;
  }) => Promise<void>;
  clearDiagnosis: () => void;

  analyzeExplain: (params: {
    dbSessionId: string;
    explainOutput: string;
    originalSql: string;
  }) => Promise<void>;
  clearExplainAnalysis: () => void;

  setNlFilterInput: (input: string) => void;
  parseFilter: (params: {
    dbSessionId: string;
    database: string;
    table: string;
  }) => Promise<FilterCondition[] | null>;
  clearNlFilter: () => void;

  initChatSession: (
    dbSessionId?: string,
    database?: string,
    connectionId?: string,
  ) => Promise<void>;
  refreshChatSessions: () => Promise<void>;
  selectChatSession: (id: string) => Promise<void>;
  newChatSession: () => void;
  deleteChatSession: (id: string) => Promise<void>;
  exportChatSession: (format: 'json' | 'markdown') => Promise<void>;
  sendChatMessage: (params: {
    dbSessionId?: string;
    connectionId?: string;
    database?: string;
    content: string;
    includeSchema?: boolean;
    agentMode?: AiAgentMode;
    contextFiles?: string[];
    contextTables?: string[];
    /** Frozen editor/error/result snapshot for this turn. */
    autoContext?: AiChatContextSnapshot;
    /** Explicit Prompt template selection; omitted = database default. */
    templateId?: string;
    /** Database type used to pick a default Prompt template. */
    databaseType?: string;
    /** Target this run claims; omitted = independent. */
    targetClaim?: AiTargetClaim;
    memoryMode?: 'off' | 'ask' | 'auto';
    memoryScope?:
      | 'organization'
      | 'user'
      | 'connection'
      | 'database'
      | 'schema'
      | 'table'
      | 'column';
  }) => Promise<void>;
  clearChat: () => void;
  dismissMemoryCandidateNotice: () => void;
  /** Cancel the in-flight chat run; resolves with the host receipt. */
  cancelChat: (requestId?: string) => Promise<CancelReceipt | null>;
  /** Called by the session canceller's 8s fallback. */
  handleCancelTimeout: (requestId: string) => void;

  workflowChat: AiChatSession | null;
  initWorkflowChat: () => void;
  sendWorkflowChatMessage: (params: {
    dbSessionId?: string;
    database?: string;
    content: string;
    includeSchema?: boolean;
    contextFiles?: string[];
    contextTables?: string[];
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

  generateSchemaDoc: (params: { dbSessionId: string; database: string }) => Promise<void>;
  clearSchemaDoc: () => void;
  diagnoseConnection: (params: { connectionId: string; errorMessage: string }) => Promise<void>;
  clearConnectionDiagnosis: () => void;
  analyzeQueries: (params: { dbSessionId?: string }) => Promise<void>;
  clearQueryAnalysis: () => void;

  mcpServers: McpClientStatus[];
  mcpTools: McpToolInfo[];
  mcpConnecting: boolean;
  mcpConnectingServerId: string | null;
  mcpError: string | null;
  mcpServerErrors: Record<string, string>;

  connectMcpServer: (serverId: string) => Promise<void>;
  disconnectMcpServer: (serverId: string) => Promise<void>;
  saveMcpClientServers: (configs: McpServerConfig[]) => Promise<void>;
  loadMcpServers: () => Promise<void>;
  loadMcpTools: () => Promise<void>;
  callMcpTool: (params: {
    serverId: string;
    toolName: string;
    arguments: Record<string, unknown>;
  }) => Promise<string>;
  clearMcpError: () => void;
  clearMcpServerError: (serverId: string) => void;
}

// Re-export message types used by store consumers that import from aiStore path.
export type { AiChatMessage, AiChatSession };
