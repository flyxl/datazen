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
} from '../../types';

export interface Nl2SqlState {
  input: string;
  generatedSql: string;
  isGenerating: boolean;
  requestId: string | null;
}

export const initialNl2Sql: Nl2SqlState = {
  input: '',
  generatedSql: '',
  isGenerating: false,
  requestId: null,
};

export interface AiStore {
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
    dbSessionId: string;
    database: string;
    currentTable?: string;
    recentQueries?: string[];
    contextFiles?: string[];
    contextTables?: string[];
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

  initChatSession: () => void;
  sendChatMessage: (params: {
    dbSessionId?: string;
    database?: string;
    content: string;
    includeSchema?: boolean;
    contextFiles?: string[];
    contextTables?: string[];
  }) => Promise<void>;
  clearChat: () => void;

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
