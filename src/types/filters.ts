/**
 * Filter types for the data grid.
 *
 * These live in their own file because they grow with the filter contract,
 * while `./index` is the aggregate the rest of the app imports from. The
 * aggregate re-exports everything here, so `import type { FilterCondition }
 * from '../types'` keeps working for every caller.
 */
import type { Value } from './connection';

/**
 * The filter operators the product understands.
 *
 * The first ten are the operators that already shipped; their spelling and
 * their rendered SQL are frozen. The next nine are shared extensions every
 * ANSI-ish dialect can render. The last three are dialect-specific and only
 * a driver that declares them can run them.
 *
 * The order matches the Rust `FilterOperator` enum exactly.
 */
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
  | 'isNotNull'
  | 'notIn'
  | 'between'
  | 'notBetween'
  | 'contains'
  | 'notContains'
  | 'startsWith'
  | 'endsWith'
  | 'isEmpty'
  | 'isNotEmpty'
  | 'regex'
  | 'notRegex'
  | 'jsonContains';

/**
 * The operators that shipped before the extension.
 *
 * This list is the regression baseline for "the existing operators keep their
 * relative order and their SQL"; nothing in the runtime returns it.
 */
export const EXISTING_FILTER_OPERATORS: readonly FilterOperator[] = [
  'eq',
  'ne',
  'gt',
  'lt',
  'gte',
  'lte',
  'like',
  'in',
  'isNull',
  'isNotNull',
];

/**
 * The operators the host renders itself: the existing ten plus the nine
 * shared extensions. This is the default whitelist of a driver.
 */
export const SHARED_FILTER_OPERATORS: readonly FilterOperator[] = [
  ...EXISTING_FILTER_OPERATORS,
  'notIn',
  'between',
  'notBetween',
  'contains',
  'notContains',
  'startsWith',
  'endsWith',
  'isEmpty',
  'isNotEmpty',
];

/** The operators only a driver can render; a driver has to declare them. */
export const DIALECT_FILTER_OPERATORS: readonly FilterOperator[] = [
  'regex',
  'notRegex',
  'jsonContains',
];

export interface FilterCondition {
  column: string;
  operator: FilterOperator;
  value?: Value;
  /**
   * How this condition joins the condition before it.
   *
   * The first condition has no predecessor, so its own connector is
   * meaningless and the backend ignores it. `undefined` falls back to the
   * request-wide logical operator, which is how every filter list behaved
   * before per-condition connectors existed.
   */
  connector?: 'and' | 'or';
}
