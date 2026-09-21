/** Database engine identifiers for the current build (injected via resolve-drivers). */
export type { DatabaseType } from '../extensions/generated';
import type { DatabaseType } from '../extensions/generated';

export type {
  TunnelKind,
  HttpProxyTunnelConfig,
  WebSocketTunnelConfig,
  SavedTunnel,
  SavedTunnelSshConfig,
} from './tunnel';

export type SslMode = 'disable' | 'prefer' | 'require' | 'verifyCa' | 'verifyFull';

export type SshAuthMethod = 'password' | 'private_key' | 'agent';

export interface SshTunnelConfig {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  authMethod: SshAuthMethod;
  password?: string;
  privateKeyPath?: string;
  passphrase?: string;
  /** Optional ProxyJump hop. */
  jump?: SshTunnelConfig;
}

export interface ConnectionConfig {
  id: string;
  name: string;
  databaseType: DatabaseType;
  host?: string;
  port?: number;
  database?: string;
  /** Presto/Trino schema within catalog */
  schema?: string;
  username?: string;
  password?: string;
  sslMode: SslMode;
  connectionTimeout?: number;
  /** Host-injected pool size; not typically set in the connection form. */
  maxPoolSize?: number;
  sshTunnel?: SshTunnelConfig;
  tunnelKind?: import('./tunnel').TunnelKind;
  tunnelId?: string;
  httpProxyTunnel?: import('./tunnel').HttpProxyTunnelConfig;
  websocketTunnel?: import('./tunnel').WebSocketTunnelConfig;
  colorTag?: string;
  group?: string;
  lastConnectedAt?: string;
  serverVersion?: string;
  /** Opaque per-driver connection options (e.g. Redis topology/TLS). */
  options?: Record<string, unknown>;
  /** When true, the host rejects mutating SQL and row edits. */
  readOnly?: boolean;
  /** When true, sorted first within the connection group in the navigator. */
  pinned?: boolean;
}

export interface ServerInfo {
  serverVersion: string;
  serverType: string;
}

/** Capabilities reported by the concrete driver for a live database session. */
export interface DriverCapabilities {
  supportsCancelQuery: boolean;
  /** True only when the driver implements opaque execution-handle cancel. */
  supportsQueryExecutionCancel: boolean;
  supportsExplain: boolean;
  supportsStreamingResults: boolean;
}

export type TableType = 'table' | 'view' | 'materializedView' | 'systemTable';

export type DatabaseObjectKind = 'function' | 'procedure' | 'trigger' | 'sequence' | 'type';

export interface DatabaseObject {
  kind: DatabaseObjectKind;
  schema?: string | null;
  name: string;
}

export interface PrivilegeGrant {
  grantee: string;
  objectSchema?: string | null;
  objectName: string;
  privilege: string;
}

export interface TableInfo {
  name: string;
  schema?: string;
  tableType: TableType;
  rowCount?: number;
}

export interface ColumnSchema {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string | null;
  isPrimaryKey?: boolean;
  isAutoIncrement?: boolean;
  comment?: string;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary: boolean;
  indexType?: string;
}

export interface ForeignKeyInfo {
  name: string;
  columns: string[];
  referencedTable: string;
  referencedColumns: string[];
  onUpdate?: string;
  onDelete?: string;
}

export interface TableSchema {
  tableName: string;
  columns: ColumnSchema[];
  primaryKeys: string[];
  indexes: IndexInfo[];
  foreignKeys: ForeignKeyInfo[];
}

export type Value = string | number | boolean | null | Record<string, unknown> | unknown[];

export interface ColumnInfo {
  name: string;
  dataType: string;
  nullable: boolean;
}

export interface QueryResult {
  columns: ColumnInfo[];
  rows: (Value | null)[][];
  rowsAffected?: number;
  executionTimeMs: number;
}

export interface StatementResult {
  sql: string;
  columns: ColumnInfo[];
  rows: (Value | null)[][];
  rowsAffected?: number;
  executionTimeMs: number;
  truncated?: boolean;
  pinned?: boolean;
}

export interface MultiQueryResult {
  results: StatementResult[];
  totalTimeMs: number;
}

/** IPC events for `execute_query_stream`. Independent of SQL LIMIT. */
export type QueryStreamEvent =
  | { type: 'executionStarted'; executionId: string }
  | { type: 'statementStart'; index: number; sql: string; columns: ColumnInfo[] }
  | { type: 'rows'; index: number; rows: (Value | null)[][] }
  | {
      type: 'statementEnd';
      index: number;
      rowsAffected?: number;
      executionTimeMs: number;
      truncated: boolean;
    }
  | { type: 'done'; totalTimeMs: number };

export interface ExplainPlanDetail {
  key: string;
  value: string;
}

export interface ExplainPlanNode {
  id: string;
  label: string;
  cost?: number;
  rows?: number;
  details: ExplainPlanDetail[];
  children: ExplainPlanNode[];
}

export interface ExplainResult {
  planText: string;
  planJson?: unknown;
  planTree?: ExplainPlanNode;
  totalCost?: number;
  estimatedRows?: number;
}

export interface QueryHistoryEntry {
  id: string;
  connectionId: string;
  database: string;
  schema?: string | null;
  sql: string;
  executedAt: string;
  executionTimeMs: number;
  rowsAffected?: number;
  success: boolean;
  errorMessage?: string;
}

export interface FavoriteQuery {
  id: string;
  connectionId: string;
  title: string;
  sql: string;
  createdAt: string;
}

export interface ContextEntry {
  name: string;
  path: string;
  isDir: boolean;
  size?: number;
  children?: ContextEntry[];
}

export type ContextKind = 'file' | 'dir' | 'table';

export interface ContextItem {
  kind: ContextKind;
  id: string;
  name: string;
  path?: string;
  database?: string;
}

import type { ThemePreference } from './theme';
import type { MonitorSettings } from './dashboard';

export type McpPermissionMode = 'read_only' | 'safe_write' | 'high_risk_write';

export interface SqlFormatOptions {
  keywordCase: 'upper' | 'lower' | 'preserve';
  indentStyle: '2spaces' | '4spaces' | 'tab';
  breakBeforeBooleanOperators: boolean;
  linesBetweenQueries: number;
}

export type SqlExecutionStrategy =
  | 'current_statement'
  | 'entire_script'
  | 'largest_statement'
  | 'ask';

export interface AppSettings {
  theme: ThemePreference;
  language: string;
  limitSelectResults: boolean;
  queryResultLimit: number;
  editorFontSize: number;
  editorFontFamily: string;
  confirmOnDelete: boolean;
  autoCommit: boolean;
  safeMode: boolean;
  confirmDangerousExecution: boolean;
  defaultPageSize: number;
  connectionPoolSize: number;
  logLevel: 'trace' | 'debug' | 'info' | 'warn' | 'error';
  logPath: string;
  mcpServerEnabled: boolean;
  mcpDisabledTools: string[];
  mcpPermissionMode: McpPermissionMode;
  mcpAllowedConnectionIds: string[];
  contextDir: string;
  checkForUpdatesOnStartup: boolean;
  autoChartOnQuery: boolean;
  monitor: MonitorSettings;
  driverSettings: Record<string, unknown>;
  wappSettings: Record<string, unknown>;
  mcpClientServers?: McpServerConfig[];
  aiStrictEgress: boolean;
  editorCompletionIncludeTablePrefix?: boolean;
  editorCompletionQuotePolicy?: 'unquoted' | 'always' | 'both';
  keymapPreset?: 'default' | 'dbeaver' | 'navicat';
  customKeymap?: Partial<Record<string, string>>;
  sqlFormatOptions?: SqlFormatOptions;
  sqlExecutionStrategy?: SqlExecutionStrategy;
  sqlSnippets?: Array<{ id: string; prefix: string; descriptionKey: string; template: string }>;
  sqlSyntaxTheme?: string;
  workflowStepResultOrder?: 'asc' | 'desc';
  onboarding?: { completed: boolean; version: number };
}

export type FilterOperator =
  | 'eq'
  | 'ne'
  | 'gt'
  | 'lt'
  | 'gte'
  | 'lte'
  | 'like'
  | 'in'
  | 'isNull'
  | 'isNotNull';

export interface FilterCondition {
  column: string;
  operator: FilterOperator;
  value?: Value;
}

export interface SortCondition {
  column: string;
  descending: boolean;
}

export interface KeyEntry {
  key: string;
  keyType: string;
  ttl: number;
  size: number;
  preview: string;
}

export interface KeyScanResult {
  cursor: number;
  keys: KeyEntry[];
  dbSize: number;
}

export interface KeyDetail {
  key: string;
  keyType: string;
  ttl: number;
  value: unknown;
}

export interface TableDataResult {
  columns: ColumnSchema[];
  rows: (Value | null)[][];
  totalRows?: number;
  page: number;
  pageSize: number;
}

export type AiProviderType = 'open_ai' | 'deep_seek' | 'ollama' | 'custom';
export type AiDataEgressLevel = 'strict' | 'sample_masked' | 'unrestricted';
export type AiToolPermissionPolicy = 'disabled' | 'read_only' | 'require_confirm' | 'unrestricted';

export interface AiSafetyGateConfig {
  redactCredentials: boolean;
  dataEgressLevel: AiDataEgressLevel;
  maxSampleRows: number;
  dbToolPolicy: AiToolPermissionPolicy;
  mcpToolPolicy: AiToolPermissionPolicy;
  requireSqlConfirm: boolean;
  maxContextBytes: number;
}

export interface AiModelProfile {
  id: string;
  name: string;
  providerType: AiProviderType;
  apiKey?: string;
  endpoint?: string;
  model: string;
  maxTokens?: number;
  extra?: Record<string, unknown>;
  safetyGate: AiSafetyGateConfig;
  isDefault?: boolean;
}

export interface AiSettingsConfig {
  activeProfileId: string;
  profiles: AiModelProfile[];
}

export interface AiProviderConfig {
  providerType: AiProviderType;
  apiKey?: string;
  endpoint?: string;
  model: string;
  maxTokens?: number;
  extra?: Record<string, unknown>;
}

export interface ModelInfo {
  id: string;
  displayName: string;
  contextWindow: number;
  supportsStreaming: boolean;
  supportsTools: boolean;
}

export interface ProviderListItem {
  providerType: AiProviderType;
  displayName: string;
  supportsStreaming: boolean;
  supportsTools: boolean;
  defaultEndpoint: string;
  defaultProtocol: string;
}

export interface DiagnosisResult {
  explanation: string;
  suggestedSql: string | null;
  changes: string[];
}

export interface ExplainAnalysis {
  summary: string;
  bottlenecks: Bottleneck[];
  suggestions: ExplainSuggestion[];
}

export interface Bottleneck {
  node: string;
  description: string;
  severity: 'high' | 'medium' | 'low';
}

export interface ExplainSuggestion {
  description: string;
  sql: string | null;
  impact: string;
}

export interface AiQuestionOption {
  id: string;
  label: string;
}

export interface AiQuestion {
  id: string;
  prompt: string;
  options: AiQuestionOption[];
  allowMultiple?: boolean;
}

export interface AiToolCall {
  id: string;
  name: string;
  arguments: string;
}

export interface AiToolResult {
  toolCallId: string;
  content: string;
}

export interface AiChatMessage {
  id?: string;
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  reasoning?: string;
  questions?: AiQuestion[];
  toolCalls?: AiToolCall[];
  toolCallId?: string;
}

export interface AiChatSession {
  id: string;
  sessionKey: string;
  messages: AiChatMessage[];
  isStreaming: boolean;
  streamContent: string;
  streamReasoning: string;
  streamMcpToolName: string | null;
  requestId: string | null;
}

export interface StreamChunkPayload {
  requestId: string;
  content: string;
  reasoning?: string;
  done: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
  toolCalls?: AiToolCall[];
}

export interface StreamErrorPayload {
  requestId: string;
  error: string;
}

export interface WorkflowVariable {
  name: string;
  type: string;
  description: string;
  required?: boolean;
  default?: unknown;
}

export type CommandCategory = 'query' | 'mutate' | 'admin' | 'observe' | 'pubSub' | 'stream' | 'io';
export type CommandAccessLevel = 'read' | 'write' | 'highRisk';

export interface DriverSaveDialogSpec {
  fileNameField: string;
  dataBase64Field: string;
  filterName: string;
  extensions: string[];
  resultPathField: string;
}

export interface DriverCommandMetadata {
  category: CommandCategory;
  risk?: CommandAccessLevel | null;
  workflow: boolean;
  ui: boolean;
  deprecated: boolean;
  replacedBy?: string | null;
  requiresConnection: boolean;
  saveDialog?: DriverSaveDialogSpec | null;
}

export interface DriverCommandDefinition {
  id: string;
  name: string;
  description?: string | null;
  inputSchema: Record<string, unknown>;
  outputSchema?: Record<string, unknown> | null;
  permissions: string[];
  metadata: DriverCommandMetadata;
}

export type WorkflowStepType =
  | 'query'
  | 'command'
  | 'ai'
  | 'condition'
  | 'foreach'
  | 'merge'
  | 'transform';

export interface ErrorHandlingConfig {
  strategy: 'abort' | 'skip' | 'fallback';
  fallbackSteps?: WorkflowStep[];
}

export interface WorkflowStep {
  type: WorkflowStepType;
  id: string;
  sql?: string;
  connection?: string;
  database?: string;
  command?: string;
  input?: Record<string, unknown>;
  prompt?: string;
  timeoutSecs?: number;
  onError?: ErrorHandlingConfig;
  if?: string;
  thenSteps?: WorkflowStep[];
  elseSteps?: WorkflowStep[];
  items?: string;
  asVar?: string;
  steps?: WorkflowStep[];
  maxIterations?: number;
  sources?: MergeSource[];
  columns?: string[];
  from?: string;
  addColumns?: TransformColumn[];
  filter?: string;
  sortBy?: string;
  offset?: number;
  limit?: number;
}

export interface MergeSource {
  source: string;
  columns?: Record<string, string>;
  add?: Record<string, unknown>;
}

export interface TransformColumn {
  name: string;
  expr: string;
}

export interface WorkflowOutput {
  format: string;
  template?: string;
}

export interface WorkflowDefinition {
  id: string;
  name: string;
  description: string;
  version?: string;
  author?: string;
  variables: WorkflowVariable[];
  connection?: string;
  database?: string;
  steps: WorkflowStep[];
  output?: WorkflowOutput;
  timeoutSecs?: number;
  errorHandling?: ErrorHandlingConfig;
  schedule?: WorkflowSchedule;
  visibility?: 'user' | 'dashboardHidden';
}

export interface WorkflowSchedule {
  enabled: boolean;
  interval_secs?: number;
  intervalSecs?: number;
}

export interface WorkflowListItem {
  id: string;
  name: string;
  description: string;
  variables: WorkflowVariable[];
  scheduled?: boolean;
}

export type StepStatus = 'success' | 'failed' | 'skipped' | 'timed_out';

export interface StepExecutionResult {
  stepId: string;
  stepType: string;
  status: StepStatus;
  result?: Record<string, unknown>;
  executionTimeMs: number;
  error?: string;
  connectionName?: string;
  sqlExecuted?: string;
}

export interface WorkflowExecutionResult {
  success: boolean;
  finalOutput: string;
  steps: StepExecutionResult[];
  totalTimeMs: number;
  error?: string;
}

export interface HistoryListItem {
  id: string;
  workflowId: string;
  workflowName: string;
  success: boolean;
  totalTimeMs: number;
  createdAt: string;
}

export interface HistoryEntry {
  id: string;
  workflowId: string;
  workflowName: string;
  variables: Record<string, unknown>;
  result: WorkflowExecutionResult;
  createdAt: string;
}

export interface ConnectionDiagnosis {
  diagnosis: string;
  possibleCauses: string[];
  solutions: ConnectionSolution[];
  category: string;
}

export interface ConnectionSolution {
  description: string;
  command?: string;
}

export interface QueryCategory {
  name: string;
  count: number;
  examples: string[];
}

export interface QueryAnalysis {
  summary: string;
  categories: QueryCategory[];
  insights: string[];
  frequentTables: string[];
  recommendations: string[];
}

export type TableCompareStatus = 'identical' | 'different' | 'source_only' | 'target_only';
export type SyncObjectKind = 'table' | 'view' | 'function' | 'procedure';

export interface TableComparison {
  table: string;
  kind?: SyncObjectKind;
  status: TableCompareStatus;
  sourceRows: number | null;
  targetRows: number | null;
}

export interface ColumnDiffEntry {
  name: string;
  dataType: string;
  nullable: boolean;
  isPrimaryKey: boolean;
}

export interface ChangedColumnDiff {
  name: string;
  source: ColumnDiffEntry;
  target: ColumnDiffEntry;
  changes: string[];
}

export interface TableSchemaDiff {
  table: string;
  sourceOnlyColumns: ColumnDiffEntry[];
  targetOnlyColumns: ColumnDiffEntry[];
  changedColumns: ChangedColumnDiff[];
  sourceOnlyIndexes: string[];
  targetOnlyIndexes: string[];
  sourceOnlyForeignKeys: string[];
  targetOnlyForeignKeys: string[];
  missingOnTarget?: ColumnDiffEntry[];
  extraOnTarget?: ColumnDiffEntry[];
  added: ColumnDiffEntry[];
  removed: ColumnDiffEntry[];
  changed: ChangedColumnDiff[];
  sourceDdl?: string;
  targetDdl?: string;
}

export interface McpServerConfig {
  id: string;
  name: string;
  transport: 'stdio';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  enabled?: boolean;
  enabledForAi?: boolean;
}

export interface McpToolInfo {
  serverId: string;
  serverName: string;
  toolName: string;
  qualifiedName: string;
  description?: string;
  inputSchema: Record<string, unknown>;
}

export function isValidMcpServerId(id: string): boolean {
  return /^[a-zA-Z0-9_-]+$/.test(id);
}

export interface McpClientStatus {
  serverId: string;
  serverName: string;
  toolsCount: number;
}
