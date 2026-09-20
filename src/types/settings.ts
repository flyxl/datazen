/** App settings and filter/sort types. */
import type { Value } from './connection';

import type { ThemePreference } from './theme';
import type { MonitorSettings } from './dashboard';
import type { McpServerConfig } from './diagnosis';

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
