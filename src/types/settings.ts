/**
 * App settings and filter/sort types.
 *
 * NOTE: this module has no importers. `AppSettings` — the type the app
 * actually uses — lives in `./index`, and every declaration still here is a
 * duplicate of one there too. The `AppSettings` copy this branch extended has
 * been deleted; removing the remaining `connection.ts` / `settings.ts`
 * duplication is a separate cleanup.
 */
import type { Value } from './connection';

export type {
  SettingsMcpPermissionMode as McpPermissionMode,
  SettingsSqlFormatOptions as SqlFormatOptions,
  SettingsSqlExecutionStrategy as SqlExecutionStrategy,
} from '@datazen/backend-client';

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
