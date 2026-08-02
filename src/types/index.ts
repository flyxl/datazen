/** Built-in database engine identifiers. */
export type BuiltinDatabaseType = 'postgresql' | 'mysql' | 'mariadb' | 'sqlite' | 'redis';

/** All database types: built-in + plugin-provided. */
import type { PluginDatabaseType } from '../plugins/generated';
export type DatabaseType = BuiltinDatabaseType | PluginDatabaseType;

export type SslMode = 'disable' | 'prefer' | 'require' | 'verifyCa' | 'verifyFull';

export interface SshTunnelConfig {
  enabled: boolean;
  host: string;
  port: number;
  username: string;
  authMethod: 'password' | 'private_key';
  password?: string;
  privateKeyPath?: string;
  passphrase?: string;
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
  sshTunnel?: SshTunnelConfig;
  colorTag?: string;
  group?: string;
  lastConnectedAt?: string;
  serverVersion?: string;
}

export interface ServerInfo {
  serverVersion: string;
  serverType: string;
}

export type TableType = 'table' | 'view' | 'materializedView' | 'systemTable';

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

export type Value =
  | string
  | number
  | boolean
  | null
  | Record<string, unknown>
  | unknown[];

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
}

export interface MultiQueryResult {
  results: StatementResult[];
  totalTimeMs: number;
}

export interface ExplainResult {
  planText: string;
  planJson?: unknown;
  totalCost?: number;
  estimatedRows?: number;
}

export interface QueryHistoryEntry {
  id: string;
  connectionId: string;
  database: string;
  sql: string;
  executedAt: string;
  executionTimeMs: number;
  rowsAffected?: number;
  success: boolean;
  errorMessage?: string;
}

export interface FavoriteQuery {
  id: string;
  title: string;
  sql: string;
  createdAt: string;
}

export interface AppSettings {
  theme: 'light' | 'dark' | 'system';
  language: string;
  limitSelectResults: boolean;
  queryResultLimit: number;
  editorFontSize: number;
  editorFontFamily: string;
  confirmOnDelete: boolean;
  autoCommit: boolean;
  defaultPageSize: number;
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

// ── Key-Value (Redis) types ──

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

/** Raw backend response — rows are 2D arrays. */
export interface TableDataResult {
  columns: ColumnSchema[];
  rows: (Value | null)[][];
  totalRows?: number;
  page: number;
  pageSize: number;
}

// ── AI Types ──

export type AiProviderType = 'open_ai' | 'anthropic' | 'custom';

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
  models: ModelInfo[];
  defaultModel: string;
  supportsStreaming: boolean;
  supportsTools: boolean;
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

export interface AiChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  reasoning?: string;
}

export interface AiChatSession {
  id: string;
  messages: AiChatMessage[];
  isStreaming: boolean;
  streamContent: string;
  streamReasoning: string;
  requestId: string | null;
}

export interface StreamChunkPayload {
  requestId: string;
  content: string;
  reasoning?: string;
  done: boolean;
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
}

export interface StreamErrorPayload {
  requestId: string;
  error: string;
}

// ── Skill types ──

export interface SkillVariable {
  name: string;
  type: string; // 'string' | 'number' | 'connection'
  description: string;
  required?: boolean;
  default?: unknown;
}

export type SkillStepType = 'query' | 'ai' | 'condition' | 'foreach';

export interface ErrorHandlingConfig {
  strategy: 'abort' | 'skip' | 'fallback';
  fallbackSteps?: SkillStep[];
}

export interface SkillStep {
  type: SkillStepType;
  id: string;
  // query fields
  sql?: string;
  connection?: string;
  // ai fields
  prompt?: string;
  // common
  timeoutSecs?: number;
  onError?: ErrorHandlingConfig;
  // condition fields
  if?: string;
  thenSteps?: SkillStep[];
  elseSteps?: SkillStep[];
  // foreach fields
  items?: string;
  asVar?: string;
  steps?: SkillStep[];
  maxIterations?: number;
}

export interface SkillOutput {
  format: string;
  template?: string;
}

export interface SkillDefinition {
  id: string;
  name: string;
  description: string;
  version?: string;
  author?: string;
  variables: SkillVariable[];
  steps: SkillStep[];
  output?: SkillOutput;
  timeoutSecs?: number;
  errorHandling?: ErrorHandlingConfig;
}

export interface SkillListItem {
  id: string;
  name: string;
  description: string;
  variables: SkillVariable[];
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

export interface SkillExecutionResult {
  success: boolean;
  finalOutput: string;
  steps: StepExecutionResult[];
  totalTimeMs: number;
  error?: string;
}

export interface HistoryListItem {
  id: string;
  skillId: string;
  skillName: string;
  success: boolean;
  totalTimeMs: number;
  createdAt: string;
}

export interface HistoryEntry {
  id: string;
  skillId: string;
  skillName: string;
  variables: Record<string, unknown>;
  result: SkillExecutionResult;
  createdAt: string;
}

// ── Phase 8: Schema docs + Connection diagnosis + Query analysis ──

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

// ── MCP Client types ──

export interface McpServerConfig {
  id: string;
  name: string;
  transport: 'stdio';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  enabled: boolean;
}

export interface McpClientStatus {
  serverId: string;
  serverName: string;
  toolsCount: number;
}

export interface McpToolInfo {
  serverId: string;
  serverName: string;
  toolName: string;
  description?: string;
}
