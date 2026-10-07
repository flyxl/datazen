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

export type McpPermissionMode = 'read_only' | 'safe_write' | 'high_risk_write';

/** Configurable SQL beautifier options. */
export interface SqlFormatOptions {
  keywordCase: 'upper' | 'lower' | 'preserve';
  indentStyle: '2spaces' | '4spaces' | 'tab';
  /** Put AND / OR at the start of the next line instead of the end of the current one. */
  breakBeforeBooleanOperators: boolean;
  /** Blank lines inserted between consecutive statements. */
  linesBetweenQueries: number;
}

/**
 * Which SQL the Execute action submits when there is no explicit selection.
 * `ask` prompts whenever the script holds more than one statement.
 */
export type SqlExecutionStrategy =
  | 'current_statement'
  | 'entire_script'
  | 'largest_statement'
  | 'ask';

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
