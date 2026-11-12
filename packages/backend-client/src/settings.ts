/** Transport-neutral settings; native paths, launch configs and credentials stay with the host. */
export interface SettingsThemePreference {
  mode: 'light' | 'dark' | 'system';
  packId: string | null;
}
export type SettingsMcpPermissionMode = 'read_only' | 'safe_write' | 'high_risk_write';
export interface SettingsSqlFormatOptions {
  keywordCase: 'upper' | 'lower' | 'preserve';
  indentStyle: '2spaces' | '4spaces' | 'tab';
  breakBeforeBooleanOperators: boolean;
  linesBetweenQueries: number;
}
export type SettingsSqlExecutionStrategy =
  | 'current_statement'
  | 'entire_script'
  | 'largest_statement'
  | 'ask';

export interface UserPreferences {
  theme: SettingsThemePreference;
  language: string;
  editorFontSize: number;
  editorFontFamily: string;
  confirmOnDelete: boolean;
  autoCommit: boolean;
  confirmDangerousExecution: boolean;
  defaultPageSize: number;
  autoChartOnQuery: boolean;
  wappSettings: Record<string, unknown>;
  editorCompletionQuotePolicy?: 'unquoted' | 'always' | 'both';
  editorCompletionIncludeTablePrefix?: boolean;
  enableFkPrediction?: boolean;
  keymapPreset?: 'default' | 'dbeaver' | 'navicat';
  customKeymap?: Partial<Record<string, string>>;
  sqlExecutionStrategy?: SettingsSqlExecutionStrategy;
  sqlSyntaxTheme?: string | null;
  sqlFormatOptions?: SettingsSqlFormatOptions;
  sqlSnippets?: Array<{ id: string; prefix: string; descriptionKey: string; template: string }>;
  workflowStepResultOrder?: 'asc' | 'desc';
}

export interface ExecutionSettings {
  limitSelectResults: boolean;
  queryResultLimit: number;
  safeMode: boolean;
  connectionPoolSize: number;
  mcpDisabledTools: string[];
  mcpPermissionMode: SettingsMcpPermissionMode;
  mcpAllowedConnectionIds: string[];
  mcpAllowedDatabases: string[];
  mcpAllowedNamespaces: string[];
  driverSettings: Record<string, unknown>;
  aiStrictEgress: boolean;
}

/**
 * Onboarding wizard state (form-split F4: behavior field, shared).
 *
 * `undefined` or `version < 1` → the wizard shows; a fresh install materializes
 * `for_fresh_install`, an upgrade `for_existing_install` (upgrade ≠ first run).
 */
export interface SettingsOnboardingState {
  completed: boolean;
  version: number;
}

/** How a table read obtains its row count (form-split F4: behavior field, shared). */
export type SettingsRowCountMode = 'auto' | 'exact' | 'estimated' | 'none';

/**
 * Value-presentation renderer options (issue-32 T06, design §5).
 *
 * Mirror of the canonical `TimestampOptions` in
 * `packages/frontend/src/lib/valuePresentation/timestampConfig.ts` — written
 * field by field, never imported from frontend (backend-client sits below
 * frontend in the layering). Only the timestamp renderer exists in v1; other
 * renderers carry their own option JSON under the same
 * `{ enabled, options }` entry shape.
 */
export type SettingsTimestampInterpretation =
  | { kind: 'native' }
  | { kind: 'unix'; unit: 'seconds' | 'milliseconds' }
  | { kind: 'text' };

export type SettingsTimeZoneSpec =
  | { kind: 'utc' }
  | { kind: 'local' }
  | { kind: 'iana'; name: string };

export interface SettingsTimestampOptions {
  interpretation: SettingsTimestampInterpretation;
  format: 'iso' | 'iso-tz' | 'locale';
  displayTimeZone: SettingsTimeZoneSpec;
  sourceTimeZone?: SettingsTimeZoneSpec;
}

/** One renderer's stored settings: global switch + its (validated) options. */
export interface SettingsValuePresentationRendererSettings {
  enabled: boolean;
  options: SettingsTimestampOptions;
}

/**
 * A name pattern inside a rule match (issue-32 T08, design §5).
 * Mirror of `ValuePresentationNamePattern` in
 * `packages/frontend/src/lib/valuePresentation/valuePresentationRules.ts`
 * — written field by field, never imported from frontend. Only the
 * pattern source and restricted flags are stored, never a RegExp.
 */
export type SettingsValuePresentationNamePattern =
  | { kind: 'exact'; value: string }
  | { kind: 'regex'; source: string; flags?: string };

/** One rule's match conditions: all present conditions are ANDed. */
export interface SettingsValuePresentationRuleMatch {
  connectionId?: string;
  database?: string;
  namespace?: string;
  table?: SettingsValuePresentationNamePattern;
  column?: SettingsValuePresentationNamePattern;
  databaseTypeName?: SettingsValuePresentationNamePattern;
}

/**
 * One global value-presentation rule (issue-32 T08, design §5).
 * Mirror of `ValuePresentationRule` in
 * `packages/frontend/src/lib/valuePresentation/valuePresentationRules.ts`.
 */
export interface SettingsValuePresentationRule {
  name: string;
  enabled: boolean;
  match: SettingsValuePresentationRuleMatch;
  options: SettingsTimestampOptions;
}

/**
 * Versioned value-presentation settings, namespaced per renderer id.
 * `rules` (T08) is an optional array of global rules; table-column
 * overrides are not stored here either (T10).
 */
export interface SettingsValuePresentationSettings {
  version: number;
  renderers: Record<string, SettingsValuePresentationRendererSettings>;
  rules?: ReadonlyArray<SettingsValuePresentationRule>;
}

/** Dashboard monitor settings (form-split F4: behavior field, shared). Mirrors `datazen::dashboard::types::MonitorSettings`. */
export interface SettingsMonitorEmailSettings {
  enabled?: boolean;
  host?: string;
  port?: number;
  username?: string;
  password?: string;
  from?: string;
  to?: string[];
}

export interface SettingsMonitorSettings {
  trayEnabled: boolean;
  closeToTray: boolean;
  defaultWebhookUrl?: string;
  email?: SettingsMonitorEmailSettings;
  maxConcurrentQueries: number;
  exportIncludeDashboardRuns: boolean;
  runRetentionCount: number;
  runRetentionDays: number;
}

export interface SharedSettings {
  preferences: UserPreferences;
  execution: ExecutionSettings;
  /** Form-split F4: the three behavior fields migrated from the desktop blob. */
  monitor: SettingsMonitorSettings;
  onboarding: SettingsOnboardingState | null;
  rowCountMode: SettingsRowCountMode;
  /** Value-presentation settings (issue-32 T06, design §5/§6). */
  valuePresentation: SettingsValuePresentationSettings;
}

/** Whitelist projection: no native config or credentials enter the shared transport. */
export function toSharedSettings(
  settings: UserPreferences &
    ExecutionSettings & {
      monitor?: SettingsMonitorSettings;
      onboarding?: SettingsOnboardingState | null;
      rowCountMode?: SettingsRowCountMode;
      valuePresentation?: SettingsValuePresentationSettings;
    },
): SharedSettings {
  const {
    theme,
    language,
    editorFontSize,
    editorFontFamily,
    confirmOnDelete,
    autoCommit,
    confirmDangerousExecution,
    defaultPageSize,
    autoChartOnQuery,
    wappSettings,
    editorCompletionQuotePolicy,
    editorCompletionIncludeTablePrefix,
    enableFkPrediction,
    keymapPreset,
    customKeymap,
    sqlExecutionStrategy,
    sqlSyntaxTheme,
    sqlFormatOptions,
    sqlSnippets,
    workflowStepResultOrder,
  } = settings;
  const preferences: UserPreferences = {
    theme,
    language,
    editorFontSize,
    editorFontFamily,
    confirmOnDelete,
    autoCommit,
    confirmDangerousExecution,
    defaultPageSize,
    autoChartOnQuery,
    wappSettings,
    editorCompletionQuotePolicy: editorCompletionQuotePolicy ?? 'unquoted',
    editorCompletionIncludeTablePrefix: editorCompletionIncludeTablePrefix ?? true,
    enableFkPrediction: enableFkPrediction ?? false,
    keymapPreset: keymapPreset ?? 'default',
    customKeymap: customKeymap ?? {},
    sqlExecutionStrategy: sqlExecutionStrategy ?? 'current_statement',
    sqlSyntaxTheme: sqlSyntaxTheme ?? null,
    sqlFormatOptions: sqlFormatOptions ?? {
      keywordCase: 'upper',
      indentStyle: '2spaces',
      breakBeforeBooleanOperators: true,
      linesBetweenQueries: 1,
    },
    sqlSnippets: sqlSnippets ?? [],
    workflowStepResultOrder: workflowStepResultOrder ?? 'desc',
  };
  const {
    limitSelectResults,
    queryResultLimit,
    safeMode,
    connectionPoolSize,
    mcpDisabledTools,
    mcpPermissionMode,
    mcpAllowedConnectionIds,
    mcpAllowedDatabases,
    mcpAllowedNamespaces,
    driverSettings,
    aiStrictEgress,
  } = settings;
  const execution: ExecutionSettings = {
    limitSelectResults,
    queryResultLimit,
    safeMode,
    connectionPoolSize,
    mcpDisabledTools,
    mcpPermissionMode,
    mcpAllowedConnectionIds,
    mcpAllowedDatabases,
    mcpAllowedNamespaces,
    driverSettings,
    aiStrictEgress,
  };
  return {
    preferences,
    execution,
    monitor: settings.monitor ?? defaultMonitorSettings(),
    onboarding: settings.onboarding ?? null,
    rowCountMode: settings.rowCountMode ?? 'auto',
    // Issue-32 T06: passed through verbatim, never validated here — option
    // validation happens at the frontend host boundary when the snapshot is
    // built; unknown renderers and unknown versions are preserved as-is.
    valuePresentation: settings.valuePresentation ?? { version: 1, renderers: {} },
  };
}

function defaultMonitorSettings(): SettingsMonitorSettings {
  return {
    trayEnabled: true,
    closeToTray: true,
    maxConcurrentQueries: 2,
    exportIncludeDashboardRuns: true,
    runRetentionCount: 200,
    runRetentionDays: 30,
  };
}
