/** Database engine identifiers for the current build (injected via resolve-drivers). */
export type { DatabaseType } from '../extensions/generated';
import type { DatabaseType } from '../extensions/generated';

export type { TunnelKind, HttpProxyTunnelConfig, WebSocketTunnelConfig } from './tunnel';

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
  /** Preferred tunnel strategy. When absent, inferred from legacy ssh/http/ws fields. */
  tunnelKind?: TunnelKind;
  httpProxyTunnel?: HttpProxyTunnelConfig;
  websocketTunnel?: WebSocketTunnelConfig;
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
  /** Schema namespace when known (PG search_path is not session-tracked yet → usually absent). */
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

/** §4.2 Configurable SQL beautifier options. */
export interface SqlFormatOptions {
  keywordCase: 'upper' | 'lower' | 'preserve';
  indentStyle: '2spaces' | '4spaces' | 'tab';
  /** Put AND / OR at the start of the next line instead of the end of the current one. */
  breakBeforeBooleanOperators: boolean;
  /** Blank lines inserted between consecutive statements. */
  linesBetweenQueries: number;
}

/**
 * §5.1 Which SQL the Execute action submits when there is no explicit selection.
 * `ask` prompts whenever the script holds more than one statement.
 */
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
  /** Require WHERE on UPDATE/DELETE; also block TRUNCATE/DROP. Default true. */
  safeMode: boolean;
  /** When Safe Mode is OFF, show a confirm dialog before high-risk/production execution. Default true. */
  confirmDangerousExecution: boolean;
  defaultPageSize: number;
  /** Max DB session pool size (Postgres/MySQL). Default 10; applies on next connect. */
  connectionPoolSize: number;
  logLevel: 'trace' | 'debug' | 'info' | 'warn' | 'error';
  logPath: string;
  /** When true, GUI may start embedded MCP on launch. Default false. */
  mcpServerEnabled: boolean;
  mcpDisabledTools: string[];
  mcpPermissionMode: McpPermissionMode;
  /** Persistent connection IDs exposed to MCP. Empty = all connections. */
  mcpAllowedConnectionIds: string[];
  contextDir: string;
  /** Check GitHub for app updates on startup (Basic builds only). Default false. */
  checkForUpdatesOnStartup: boolean;
  /** Switch to chart view after query when the result is chartable. Default false. */
  autoChartOnQuery: boolean;
  /** Dashboard monitor / tray / retention settings. */
  monitor: MonitorSettings;
  /** Opaque per-driver settings keyed by driver id (e.g. `"redis"`). */
  driverSettings: Record<string, unknown>;
  /** Opaque per-wapp settings keyed by wapp id. Reserved for future workspace app configs. */
  wappSettings: Record<string, unknown>;
  /** Saved external MCP Client server configs. Runtime connections are separate. */
  mcpClientServers?: McpServerConfig[];
  /** Strip query result rows before AI requests leave the device. Default true. */
  aiStrictEgress: boolean;
  /** Automatically qualify column completions with a table name or alias. Default true. */
  editorCompletionIncludeTablePrefix?: boolean;
  /** Identifier quotation policy in SQL autocomplete ('unquoted' | 'always' | 'both'). Default 'unquoted'. */
  editorCompletionQuotePolicy?: 'unquoted' | 'always' | 'both';
  /** Keyboard shortcut preset ('default' | 'dbeaver' | 'navicat'). Default 'default'. */
  keymapPreset?: 'default' | 'dbeaver' | 'navicat';
  /** User-customized keyboard shortcut overrides keyed by action ID. */
  customKeymap?: Partial<Record<string, string>>;
  /** §4.2 SQL beautifier configuration. */
  sqlFormatOptions?: SqlFormatOptions;
  /** §5.1 Execute-action statement targeting strategy. Default 'current_statement'. */
  sqlExecutionStrategy?: SqlExecutionStrategy;
  /** §6.4 User-defined SQL snippets, merged after the built-in library. */
  sqlSnippets?: Array<{ id: string; prefix: string; descriptionKey: string; template: string }>;
  /** SQL syntax highlighting color preset ('default' follows the active theme pack). */
  sqlSyntaxTheme?: string;
  /** Workflow result step tabs order: 'desc' = last step first (default), 'asc' = first step first. */
  workflowStepResultOrder?: 'asc' | 'desc';
  /** Onboarding wizard state. `undefined` or `version < 1` → show wizard. */
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
