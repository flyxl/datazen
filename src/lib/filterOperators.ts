/**
 * Filter operator metadata (data-browsing design 08 §3.5).
 *
 * Everything the filter editor needs to know about an operator lives in this
 * table: which value control it needs, which column types it applies to,
 * whether it is a dialect operator, and whether its value is a LIKE pattern or
 * a literal. Keeping that as **data** is the point: adding an operator must
 * mean adding one row here, not touching three branches in the JSX.
 *
 * The dispatch key is the column's type family, taken from the host's single
 * type-normalisation source (`classifyDataType`). The type-aware cell types of
 * the editor track (05) refine that source later; when they land, this file
 * changes in exactly one place — {@link OPERATORS_BY_FAMILY} — instead of
 * growing a second, parallel type ladder.
 */
import type { FilterOperator, Value } from '../types';
import { SHARED_FILTER_OPERATORS } from '../types';
import type { DatabaseTypeMeta } from './databaseMeta';
import { classifyDataType, type DataTypeFamily } from './dataTypeColors';

/**
 * Which control the value of a condition is edited with.
 *
 * - `none`  — the operator carries no value (`isNull` / `isNotNull` /
 *   `isEmpty` / `isNotEmpty`); the editor renders no value field at all.
 * - `single` — one literal.
 * - `pair`  — two literals (`between` / `notBetween`).
 * - `tokens` — a list of literals (`in` / `notIn`).
 */
export type FilterValueInputKind = 'none' | 'single' | 'pair' | 'tokens';

/**
 * How the operator is grouped in the UI and in the documentation.
 *
 * `compare` — value comparison, `pattern` — text patterns, `set` — membership,
 * `null` — null checks, `empty` — empty-string checks, `dialect` — only a
 * driver can render these.
 */
export type FilterOperatorGroup = 'compare' | 'pattern' | 'set' | 'null' | 'empty' | 'dialect';

export interface FilterOperatorMeta {
  /** The operator this entry describes. */
  id: FilterOperator;
  /** Which value control the editor renders for it. */
  valueInput: FilterValueInputKind;
  /**
   * The column type families the operator applies to.
   *
   * A driver whitelist narrows this further, never widens it: an operator the
   * column type does not support is not offered, so the entry point cannot
   * produce a query the database is guaranteed to reject.
   */
  columnFamilies: readonly DataTypeFamily[];
  /** Only a driver that declares the operator can render it. */
  dialect: boolean;
  /** UI grouping. */
  group: FilterOperatorGroup;
  /**
   * Whether the value is a LIKE **pattern** (the user writes the wildcards) or
   * a **literal** (the wildcards are escaped by the back end).
   *
   * `like` is a pattern: `%` in the value is a wildcard. `contains` and
   * friends are literals: `%` in the value is a percent sign and the back end
   * escapes it. The two share the `LIKE` keyword and nothing else — this flag
   * is the one place that keeps them apart.
   */
  escapesLikePattern: boolean;
}

const TEXT: readonly DataTypeFamily[] = ['text'];
const NUMERIC: readonly DataTypeFamily[] = ['number'];
const TEMPORAL: readonly DataTypeFamily[] = ['datetime'];
const SIMPLE: readonly DataTypeFamily[] = ['bool', 'json', 'binary', 'null'];

function meta(
  id: FilterOperator,
  valueInput: FilterValueInputKind,
  columnFamilies: readonly DataTypeFamily[],
  group: FilterOperatorGroup,
  escapesLikePattern = false,
): FilterOperatorMeta {
  return { id, valueInput, columnFamilies, dialect: group === 'dialect', group, escapesLikePattern };
}

/**
 * The operator table.
 *
 * Every one of the 22 operators has an entry; a missing one would make the
 * editor fall back to "a plain text box" for an operator that needs two boxes
 * or a chip list.
 */
export const FILTER_OPERATOR_META: Record<FilterOperator, FilterOperatorMeta> = {
  // ── comparison ──────────────────────────────────────────────────────────
  eq: meta('eq', 'single', [...TEXT, ...NUMERIC, ...TEMPORAL, ...SIMPLE], 'compare'),
  ne: meta('ne', 'single', [...TEXT, ...NUMERIC, ...TEMPORAL, ...SIMPLE], 'compare'),
  gt: meta('gt', 'single', [...TEXT, ...NUMERIC, ...TEMPORAL], 'compare'),
  lt: meta('lt', 'single', [...TEXT, ...NUMERIC, ...TEMPORAL], 'compare'),
  gte: meta('gte', 'single', [...TEXT, ...NUMERIC, ...TEMPORAL], 'compare'),
  lte: meta('lte', 'single', [...TEXT, ...NUMERIC, ...TEMPORAL], 'compare'),
  between: meta('between', 'pair', [...NUMERIC, ...TEMPORAL], 'compare'),
  notBetween: meta('notBetween', 'pair', [...NUMERIC, ...TEMPORAL], 'compare'),

  // ── patterns ───────────────────────────────────────────────────────────
  // `like` takes the wildcards from the user; the four below escape them.
  like: meta('like', 'single', TEXT, 'pattern'),
  contains: meta('contains', 'single', TEXT, 'pattern', true),
  notContains: meta('notContains', 'single', TEXT, 'pattern', true),
  startsWith: meta('startsWith', 'single', TEXT, 'pattern', true),
  endsWith: meta('endsWith', 'single', TEXT, 'pattern', true),

  // ── membership ─────────────────────────────────────────────────────────
  in: meta('in', 'tokens', [...TEXT, ...NUMERIC], 'set'),
  notIn: meta('notIn', 'tokens', [...TEXT, ...NUMERIC], 'set'),

  // ── null / empty ───────────────────────────────────────────────────────
  isNull: meta('isNull', 'none', [...TEXT, ...NUMERIC, ...TEMPORAL, ...SIMPLE], 'null'),
  isNotNull: meta('isNotNull', 'none', [...TEXT, ...NUMERIC, ...TEMPORAL, ...SIMPLE], 'null'),
  isEmpty: meta('isEmpty', 'none', TEXT, 'empty'),
  isNotEmpty: meta('isNotEmpty', 'none', TEXT, 'empty'),

  // ── dialect specific ───────────────────────────────────────────────────
  regex: meta('regex', 'single', TEXT, 'dialect'),
  notRegex: meta('notRegex', 'single', TEXT, 'dialect'),
  jsonContains: meta('jsonContains', 'single', ['json'], 'dialect'),
};

/**
 * The operators offered for each column type family, in display order.
 *
 * The relative order of the operators that already shipped is frozen
 * (`eq ne gt lt gte lte like in isNull isNotNull`): the list below keeps that
 * order inside every family, so adding an operator never reshuffles the ones
 * a user already knows.
 */
const OPERATORS_BY_FAMILY: Record<DataTypeFamily, readonly FilterOperator[]> = {
  text: [
    'eq',
    'ne',
    'gt',
    'lt',
    'gte',
    'lte',
    'like',
    'contains',
    'notContains',
    'startsWith',
    'endsWith',
    'in',
    'notIn',
    'isNull',
    'isNotNull',
    'isEmpty',
    'isNotEmpty',
  ],
  number: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'in', 'notIn', 'isNull', 'isNotNull'],
  datetime: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  bool: ['eq', 'ne', 'isNull', 'isNotNull'],
  json: ['eq', 'ne', 'isNull', 'isNotNull'],
  binary: ['eq', 'ne', 'isNull', 'isNotNull'],
  null: ['eq', 'ne', 'isNull', 'isNotNull'],
};

/**
 * The dialect operators a column type may use.
 *
 * They are never part of a family's list: they only show up when the driver
 * declares them, and they are appended after the shared ones.
 */
const DIALECT_OPERATORS_BY_FAMILY: Record<DataTypeFamily, readonly FilterOperator[]> = {
  text: ['regex', 'notRegex'],
  number: [],
  datetime: [],
  bool: [],
  json: ['jsonContains'],
  binary: [],
  null: [],
};

/** The operators a driver serves when it says nothing: the shared nineteen. */
export function supportedOperatorsForMeta(
  meta: DatabaseTypeMeta | undefined,
): readonly FilterOperator[] {
  const declared = meta?.dataGrid?.filterOperators;
  if (!declared || declared.length === 0) return SHARED_FILTER_OPERATORS;
  return declared;
}

/**
 * Whether this database ignores case in `LIKE` under its default collation.
 *
 * Copy only: the flag never changes the SQL, and `false` means "say nothing"
 * rather than "case sensitive" — a column level `COLLATE` can still differ.
 */
export function likeCaseInsensitiveForMeta(meta: DatabaseTypeMeta | undefined): boolean {
  return meta?.dataGrid?.likeCaseInsensitive === true;
}

/**
 * The operators a column may be filtered with.
 *
 * `dataType` is the driver's type string for the column; `supported` is the
 * driver's whitelist. The result is the intersection of "the column type can
 * take it" and "this database serves it", in a stable order.
 */
export function operatorsForColumn(
  dataType: string | undefined,
  supported: readonly FilterOperator[],
): FilterOperator[] {
  const family = classifyDataType(dataType);
  const candidates = [
    ...OPERATORS_BY_FAMILY[family],
    ...DIALECT_OPERATORS_BY_FAMILY[family],
  ];
  return candidates.filter((operator) => supported.includes(operator));
}

/** Which value control an operator is edited with. */
export function valueInputFor(operator: FilterOperator): FilterValueInputKind {
  return FILTER_OPERATOR_META[operator].valueInput;
}

/**
 * The operator a condition falls back to when its current one does not fit
 * the column any more (the user picked another column, or the driver dropped
 * the operator).
 *
 * The value is kept: changing the column is usually "same value, other
 * column", and throwing the value away would lose the user's work.
 */
export function normalizeOperatorForColumn(
  current: FilterOperator,
  dataType: string | undefined,
  supported: readonly FilterOperator[],
): FilterOperator {
  const allowed = operatorsForColumn(dataType, supported);
  if (allowed.includes(current)) return current;
  if (allowed.includes('eq')) return 'eq';
  return allowed[0] ?? current;
}

/** The operator a freshly added condition starts with. */
export function defaultOperatorForColumn(dataType: string | undefined): FilterOperator {
  const family = classifyDataType(dataType);
  return OPERATORS_BY_FAMILY[family].includes('eq') ? 'eq' : OPERATORS_BY_FAMILY[family][0] ?? 'eq';
}

/** The value shape a filter condition carries. */
export type FilterValue = Value | undefined;

function asText(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return '';
}

function asTokens(value: FilterValue): string[] {
  if (Array.isArray(value)) return value.map((entry) => asText(entry));
  const text = asText(value);
  if (text === '') return [];
  return text
    .split(',')
    .map((part) => part.trim())
    .filter((part) => part !== '');
}

function asPair(value: FilterValue): [string, string] {
  if (Array.isArray(value)) {
    const [lower = '', upper = ''] = value;
    return [asText(lower), asText(upper)];
  }
  const text = asText(value);
  const separator = text.indexOf(',');
  if (separator < 0) return [text, ''];
  return [text.slice(0, separator).trim(), text.slice(separator + 1).trim()];
}

/**
 * Convert a value when the operator changes and the value control changes
 * with it.
 *
 * The conversions keep what the user typed as far as the new control can hold
 * it: a single value becomes the lower bound of a pair, a pair becomes the
 * lower bound of a single value, a token list becomes a comma separated text.
 * Switching to an operator that carries no value keeps the old value, so
 * switching back restores it.
 */
export function convertFilterValue(
  from: FilterValueInputKind,
  to: FilterValueInputKind,
  value: FilterValue,
): FilterValue {
  if (from === to) return value;
  if (to === 'none' || from === 'none') return value;

  if (to === 'tokens') {
    const tokens = from === 'pair' ? asPair(value).filter((part) => part !== '') : asTokens(value);
    return tokens;
  }
  if (to === 'pair') {
    if (from === 'tokens') {
      const [lower = '', upper = ''] = asTokens(value);
      return [lower, upper];
    }
    return [asText(value), ''];
  }
  // to === 'single'
  if (from === 'tokens') return asTokens(value).join(', ');
  if (from === 'pair') return asPair(value)[0];
  return value;
}
