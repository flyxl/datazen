/**
 * App settings types.
 *
 * NOTE: this module has no importers. `AppSettings` — the type the app
 * actually uses — lives in `./index`, and the filter and sort types that used
 * to be duplicated here now live there too, in one place. The remaining
 * `connection.ts` / `settings.ts` duplication is a separate cleanup.
 */
export type {
  SettingsMcpPermissionMode as McpPermissionMode,
  SettingsSqlFormatOptions as SqlFormatOptions,
  SettingsSqlExecutionStrategy as SqlExecutionStrategy,
} from '@datazen/backend-client';
