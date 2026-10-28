/** Database engine identifiers for the current build (injected via resolve-drivers). */
import type { SslMode, DatabaseObjectKind, CountStrategy } from '@datazen/extension-points';
import type { Value } from '@datazen/backend-client';

export type { DatabaseType } from '../extensions/generated';

// ── 上收至包的契约类型（单一来源，宿主仅 re-export 保持既有导入路径）──
// 连接/元数据契约 → @datazen/extension-points
export type { SslMode, DatabaseObjectKind, CountStrategy } from '@datazen/extension-points';
// 流事件/DTO → @datazen/backend-client
export type {
  Value,
  ColumnInfo,
  QueryResult,
  StatementResult,
  MultiQueryResult,
  QueryStreamEvent,
  CommandCategory,
  CommandAccessLevel,
  DriverSaveDialogSpec,
  DriverCommandMetadata,
  DriverCommandDefinition,
} from '@datazen/backend-client';

import type { DatabaseType } from '../extensions/generated';

import type { SshAuthMethod } from '@datazen/driver-sdk';

export type {
  TunnelKind,
  TunnelSource,
  SshAuthMethod,
  HttpProxyTunnelConfig,
  WebSocketTunnelConfig,
  SavedTunnel,
  SavedTunnelSshConfig,
  SavedTunnelSummary,
  TunnelUsage,
} from '@datazen/driver-sdk';

// The filter contract lives in its own file; it is re-exported here so every
// caller keeps importing the filter types from the aggregate.
export * from './filters';
export type {
  CursorValue,
  KeysetUnavailableReason,
  OrderColumn,
  PagePagingInfo,
  PagingMode,
} from './keyset';

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
  tunnelKind?: import('@datazen/driver-sdk').TunnelKind;
  tunnelId?: string;
  httpProxyTunnel?: import('@datazen/driver-sdk').HttpProxyTunnelConfig;
  websocketTunnel?: import('@datazen/driver-sdk').WebSocketTunnelConfig;
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
  /** Whether the dialect accepts OFFSET in pagination. */
  supportsOffset: boolean;
  /** Whether the engine has a real second namespace level (schema). */
  hasSchemaLevel: boolean;
}

export interface DatabaseObject {
  kind: DatabaseObjectKind;
  schema?: string | null;
  name: string;
  /** PostgreSQL routine identity arguments used to disambiguate overloads. */
  signature?: string | null;
  /** Relation attached to a trigger, when exposed by the driver. */
  targetSchema?: string | null;
  targetName?: string | null;
}

export interface PrivilegeGrant {
  grantee: string;
  objectSchema?: string | null;
  objectName: string;
  privilege: string;
}

export type {
  TableType,
  TableInfo,
  ColumnSchema,
  IndexInfo,
  ForeignKeyInfo,
  TableSchema,
  TableOptionsSnapshot,
} from '@datazen/driver-sdk';
import type { ColumnSchema, TableOptionsSnapshot } from '@datazen/driver-sdk';


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

/** Backend sort orders for the paged history read. */
export type HistorySort = 'recent' | 'oldest' | 'slowest';

/**
 * One page of query history plus the number of rows the filter matched *before*
 * the page was cut.
 *
 * `total` is what lets the UI admit it is showing a slice: without it, a
 * `limit`-ed page is indistinguishable from the whole table.
 */
export interface QueryHistoryPage {
  entries: QueryHistoryEntry[];
  total: number;
}

/**
 * A saved statement. Since the file-first switch this is one `.sql` file
 * under the favorites root, named by `id`, with these fields in `--`
 * front-matter above the statement.
 */
export interface FavoriteQuery {
  id: string;
  connectionId: string;
  title: string;
  sql: string;
  createdAt: string;
  /** Absent when the file carries no `updatedAt`, or the change predates it. */
  updatedAt?: string | null;
  /** Target database the statement was saved against, for later re-binding. */
  database?: string | null;
  /** Reserved for the keyword track; parsed and round-tripped, not yet bound. */
  keyword?: string | null;
  /** Directory relative to the favorites root, `/`-separated. Absent = root. */
  folder?: string | null;
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

import type { MonitorSettings } from './dashboard';

export type {
  SettingsMcpPermissionMode as McpPermissionMode,
  SettingsSqlFormatOptions as SqlFormatOptions,
  SettingsSqlExecutionStrategy as SqlExecutionStrategy,
} from '@datazen/backend-client';

import type { UserPreferences, ExecutionSettings } from '@datazen/backend-client';
import type { DefaultTableSort, GridDensity, RowCountMode } from './settings';
import type { PagePagingInfo } from './keyset';

export type { DefaultTableSort, GridDensity, RowCountMode } from './settings';

export interface AppSettings extends UserPreferences, ExecutionSettings {
  sqlSyntaxTheme?: string;
  logLevel: 'trace' | 'debug' | 'info' | 'warn' | 'error';
  logPath: string;
  /** When true, GUI may start embedded MCP on launch. Default false. */
  mcpServerEnabled: boolean;
  contextDir: string;
  /**
   * Directory holding the favorites, one ULID-named `.sql` file each. Absent or
   * empty = `{appData}/favorites`, which is what `getFavoritesRoot` reports.
   * Point it at a synced folder to move favorites through iCloud/Dropbox/git.
   */
  favoritesRoot?: string;
  /** Check GitHub for app updates on startup (Basic builds only). Default false. */
  checkForUpdatesOnStartup: boolean;
  /** Dashboard monitor / tray / retention settings. */
  monitor: MonitorSettings;
  /** Saved external MCP Client server configs. Runtime connections are separate. */
  mcpClientServers?: McpServerConfig[];
  /** Onboarding wizard state. `undefined` or `version < 1` → show wizard. */
  onboarding?: { completed: boolean; version: number };
  /**
   * How a table read obtains its row count. `auto` leaves the decision to the
   * driver, which is what every read did before this setting existed.
   */
  rowCountMode: RowCountMode;
  /**
   * The row count above which the `auto` mode asks for an estimate instead of
   * counting. Only the `auto` mode reads it; `<= 0` means "always count".
   */
  rowCountEstimateThreshold: number;
  /**
   * The order a table read uses when the user did not ask for one.
   * `primaryKeyAsc` is the driver's own default, which is what every read did
   * before this setting existed.
   */
  defaultTableSort: DefaultTableSort;
  /**
   * The row density of the data grids (DB-17): a **delta** each grid applies
   * to its own default row height. `standard` is a delta of zero — the
   * behaviour of every grid before this setting existed, and the default.
   */
  gridDensity: GridDensity;
  /**
   * Alternating row backgrounds in the data grids (DB-17). Default `true`:
   * odd rows have been tinted since the grids exist — off is the opt-in.
   */
  gridAlternatingRows: boolean;
  /**
   * The row-number gutter of the data grids (DB-17). Default `true`: the `#`
   * column has always been there — off is the opt-in, and it trades the
   * gutter's click-to-select affordance for one more column of data.
   */
  gridRowNumbers: boolean;
  /**
   * Auto-hide grid scrollbars (DB-17): visible only while the grid is
   * hovered. Default `false` — scrollbars stay visible, as they always were.
   */
  gridAutoHideScrollbar: boolean;
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
  /** The row count, or `null` when the backend could not count. */
  totalRows?: number | null;
  /**
   * How much `totalRows` may be trusted. `null` (or a missing key) means
   * "this response carries no fresh row count" — the caller asked to reuse
   * the one it had, or the backend is older than this field — and the caller
   * keeps its own value. It is not the same as `'unsupported'`, which says
   * the count was attempted and this database cannot provide one.
   */
  totalRowsKind?: CountStrategy | null;
  page: number;
  pageSize: number;
  /**
   * How the page was paged.
   *
   * A missing key means the back end is older than this field — the page was
   * read by offset, which is what every read did before it existed. A
   * `null` value says the same thing; both are "this response carries no
   * paging decision".
   */
  paging?: PagePagingInfo | null;
}

// The AI contract lives in its own file; it is re-exported here so every
// caller keeps importing the AI types from the aggregate.
export * from './ai';

// ── Workflow types ──

export interface WorkflowVariable {
  name: string;
  type: string; // 'string' | 'number' | 'connection'
  description: string;
  required?: boolean;
  default?: unknown;
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
  // merge step
  sources?: MergeSource[];
  columns?: string[];
  // transform step
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

export type WorkflowBlockType = 'sessionBlock' | 'transactionBlock';

/**
 * Block-level retry. The unit is always the whole block — a block never
 * resumes from the middle — so there is deliberately no per-step count.
 */
export interface WorkflowBlockRetry {
  block: number;
}

export interface WorkflowBlock {
  id: string;
  type: WorkflowBlockType;
  /** Block target, level 2 of the step > block > workflow > profile chain. */
  connection?: string;
  /** Block target (multi-db connections). */
  database?: string;
  /** Run this block concurrently with *other* blocks, never with itself. */
  parallel?: boolean;
  steps: WorkflowStep[];
  /** transactionBlock only. */
  retry?: WorkflowBlockRetry;
}

export interface WorkflowDefinition {
  id: string;
  name: string;
  description: string;
  version?: string;
  author?: string;
  variables: WorkflowVariable[];
  /** Default connection inherited by data-operation steps. */
  connection?: string;
  /** Default database inherited by data-operation steps (multi-db connections). */
  database?: string;
  steps: WorkflowStep[];
  /** v2 resource blocks; each block holds its own steps and target. */
  blocks?: WorkflowBlock[];
  output?: WorkflowOutput;
  timeoutSecs?: number;
  errorHandling?: ErrorHandlingConfig;
  schedule?: WorkflowSchedule;
  /** `user` | `dashboardHidden` — hidden workflows are dashboard-owned SQL bindings. */
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

// ── Data Sync types ──

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

export interface CheckConstraintDiffEntry {
  name: string;
  expression: string;
}

export interface TableOptionsDiff {
  source: TableOptionsSnapshot;
  target: TableOptionsSnapshot;
  changes: string[];
}

export interface TableSchemaDiff {
  table: string;
  /** The selected table exists only on the target and is planned as DROP TABLE. */
  targetOnly?: boolean;
  /** Present on source, missing on target → ADD on deploy. */
  missingOnTarget?: ColumnDiffEntry[];
  /** Present on target only → DROP on deploy. */
  extraOnTarget?: ColumnDiffEntry[];
  /** Alias of missingOnTarget (legacy). */
  added: ColumnDiffEntry[];
  /** Alias of extraOnTarget (legacy). */
  removed: ColumnDiffEntry[];
  changed: ChangedColumnDiff[];
  missingCheckConstraints?: CheckConstraintDiffEntry[];
  extraCheckConstraints?: CheckConstraintDiffEntry[];
  tableOptions?: TableOptionsDiff;
  sourceDdl?: string;
  targetDdl?: string;
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
  /** When false, tools from this server are excluded from AI Chat. Default true. */
  enabledForAi?: boolean;
}

/** Mirrors Rust `validate_mcp_server_id`: ^[a-zA-Z0-9_-]+$ */
export function isValidMcpServerId(id: string): boolean {
  return /^[a-zA-Z0-9_-]+$/.test(id);
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
  qualifiedName: string;
  description?: string;
  inputSchema: Record<string, unknown>;
}
