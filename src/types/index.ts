/** Public type barrel — prefer importing from here. */
export type { DatabaseType } from '../extensions/generated';

export type { TunnelKind, HttpProxyTunnelConfig, WebSocketTunnelConfig, SavedTunnel } from './tunnel';

export type {
  SslMode,
  SshAuthMethod,
  SshTunnelConfig,
  ConnectionConfig,
  ServerInfo,
  DriverCapabilities,
  TableType,
  DatabaseObjectKind,
  DatabaseObject,
  PrivilegeGrant,
  TableInfo,
  ColumnSchema,
  IndexInfo,
  ForeignKeyInfo,
  TableSchema,
  Value,
  ColumnInfo,
  QueryResult,
  StatementResult,
  MultiQueryResult,
  QueryStreamEvent,
  ExplainPlanDetail,
  ExplainPlanNode,
  ExplainResult,
  QueryHistoryEntry,
  FavoriteQuery,
  ContextEntry,
  ContextKind,
  ContextItem,
} from './connection';

export type {
  McpPermissionMode,
  SqlFormatOptions,
  SqlExecutionStrategy,
  AppSettings,
  FilterOperator,
  FilterCondition,
  SortCondition,
} from './settings';

export type {
  KeyEntry,
  KeyScanResult,
  KeyDetail,
  TableDataResult,
} from './redis';

export type {
  AiProviderType,
  AiDataEgressLevel,
  AiToolPermissionPolicy,
  AiSafetyGateConfig,
  AiModelProfile,
  AiSettingsConfig,
  AiProviderConfig,
  ModelInfo,
  ProviderListItem,
  DiagnosisResult,
  ExplainAnalysis,
  Bottleneck,
  ExplainSuggestion,
  AiQuestionOption,
  AiQuestion,
  AiToolCall,
  AiToolResult,
  AiChatMessage,
  AiChatSession,
  StreamChunkPayload,
  StreamErrorPayload,
} from './ai';

export type {
  WorkflowVariable,
  CommandCategory,
  CommandAccessLevel,
  DriverSaveDialogSpec,
  DriverCommandMetadata,
  DriverCommandDefinition,
  WorkflowStepType,
  ErrorHandlingConfig,
  WorkflowStep,
  MergeSource,
  TransformColumn,
  WorkflowOutput,
  WorkflowDefinition,
  WorkflowSchedule,
  WorkflowListItem,
  StepStatus,
  StepExecutionResult,
  WorkflowExecutionResult,
  HistoryListItem,
  HistoryEntry,
} from './workflow';

export type {
  ConnectionDiagnosis,
  ConnectionSolution,
  QueryCategory,
  QueryAnalysis,
} from './diagnosis';

export type {
  TableCompareStatus,
  SyncObjectKind,
  TableComparison,
  ColumnDiffEntry,
  ChangedColumnDiff,
  TableSchemaDiff,
} from './schema_diff';

export type { McpServerConfig, McpToolInfo } from './mcp';

export type { ThemePreference } from './theme';
export type * from './dashboard';
export type * from './chart';
export type * from './wapp';
