/**
 * Filter operator metadata (data-browsing design 08 §3.5).
 *
 * Everything the filter editor needs to know about an operator lives in this
 * table: which value control it needs, which column types it applies to,
 * whether it is a dialect operator, and whether its value is a LIKE pattern or
 * a literal. Keeping that as **data** is the point: adding an operator must
 * mean adding one row here, not touching three branches in the JSX.
 *
 * The dispatch key is the column's **normalised cell type** — the host's single
 * type judgement (`resolveCellType`, `src/lib/cellTypes.ts`). The seven render
 * families are a projection of it, and dispatching on them would fold `enum` and
 * `uuid` back into `text`, which offers a `LIKE` on an enumeration column: an
 * entry the database is guaranteed to reject.
 */
import type { FilterOperator, Value } from '../types';
import { SHARED_FILTER_OPERATORS } from '../types';
import type { DatabaseTypeMeta } from './databaseMeta';
import { resolveCellType, type CellType, type CellTypeHints } from './cellTypes';

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
   * The cell types the operator applies to.
   *
   * A driver whitelist narrows this further, never widens it: an operator the
   * column type does not support is not offered, so the entry point cannot
   * produce a query the database is guaranteed to reject.
   *
   * The list is **derived** from {@link OPERATORS_BY_CELL_TYPE}, so the two
   * can never drift apart.
   */
  cellTypes: readonly CellType[];
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

function meta(
  id: FilterOperator,
  valueInput: FilterValueInputKind,
  group: FilterOperatorGroup,
  escapesLikePattern = false,
): FilterOperatorMeta {
  return {
    id,
    valueInput,
    cellTypes: cellTypesForOperator(id),
    dialect: group === 'dialect',
    group,
    escapesLikePattern,
  };
}

/**
 * The operators each **cell type** offers, in display order.
 *
 * The order of the operators that already shipped is frozen
 * (`eq ne gt lt gte lte like in isNull isNotNull`): every list below keeps
 * that order inside itself, so adding an operator never reshuffles the ones
 * a user already knows.
 *
 * `enum` and `uuid` have their own lists instead of sharing the text one:
 * a `LIKE` against an enumeration column needs an explicit cast in
 * PostgreSQL, so offering it is offering an entry the database rejects.
 */
const OPERATORS_BY_CELL_TYPE: Record<CellType, readonly FilterOperator[]> = {
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
  enum: ['eq', 'ne', 'in', 'notIn', 'isNull', 'isNotNull'],
  number: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'in', 'notIn', 'isNull', 'isNotNull'],
  bool: ['eq', 'ne', 'isNull', 'isNotNull'],
  date: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  time: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  datetime: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  json: ['eq', 'ne', 'isNull', 'isNotNull'],
  bytes: ['eq', 'ne', 'isNull', 'isNotNull'],
  uuid: ['eq', 'ne', 'in', 'notIn', 'isNull', 'isNotNull'],
  // An unknown type is handled as text — never as an error.
  unknown: [
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
};
/**
 * The dialect operators a column type may use.
 *
 * They are never part of a family's list: they only show up when the driver
 * declares them, and they are appended after the shared ones.
 */
const DIALECT_OPERATORS_BY_CELL_TYPE: Record<CellType, readonly FilterOperator[]> = {
  text: ['regex', 'notRegex'],
  unknown: ['regex', 'notRegex'],
  number: [],
  enum: [],
  bool: [],
  date: [],
  time: [],
  datetime: [],
  json: ['jsonContains'],
  bytes: [],
  uuid: [],
};

/** Every cell type, in the order the matrix lists them. */
const ALL_CELL_TYPES: readonly CellType[] = [
  'text',
  'enum',
  'number',
  'bool',
  'date',
  'time',
  'datetime',
  'json',
  'bytes',
  'uuid',
  'unknown',
];

/**
 * The cell types that offer an operator — the inverse of the matrix.
 *
 * It is **derived**, never written a second time: the matrix is the only
 * place that says which operator belongs to which type.
 */
function cellTypesForOperator(operator: FilterOperator): readonly CellType[] {
  return ALL_CELL_TYPES.filter(
    (cellType) =>
      OPERATORS_BY_CELL_TYPE[cellType].includes(operator) ||
      DIALECT_OPERATORS_BY_CELL_TYPE[cellType].includes(operator),
  );
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
  eq: meta('eq', 'single', 'compare'),
  ne: meta('ne', 'single', 'compare'),
  gt: meta('gt', 'single', 'compare'),
  lt: meta('lt', 'single', 'compare'),
  gte: meta('gte', 'single', 'compare'),
  lte: meta('lte', 'single', 'compare'),
  between: meta('between', 'pair', 'compare'),
  notBetween: meta('notBetween', 'pair', 'compare'),

  // ── patterns ───────────────────────────────────────────────────────────
  // `like` takes the wildcards from the user; the four below escape them.
  like: meta('like', 'single', 'pattern'),
  contains: meta('contains', 'single', 'pattern', true),
  notContains: meta('notContains', 'single', 'pattern', true),
  startsWith: meta('startsWith', 'single', 'pattern', true),
  endsWith: meta('endsWith', 'single', 'pattern', true),

  // ── membership ─────────────────────────────────────────────────────────
  in: meta('in', 'tokens', 'set'),
  notIn: meta('notIn', 'tokens', 'set'),

  // ── null / empty ───────────────────────────────────────────────────────
  isNull: meta('isNull', 'none', 'null'),
  isNotNull: meta('isNotNull', 'none', 'null'),
  isEmpty: meta('isEmpty', 'none', 'empty'),
  isNotEmpty: meta('isNotEmpty', 'none', 'empty'),

  // ── dialect specific ───────────────────────────────────────────────────
  regex: meta('regex', 'single', 'dialect'),
  notRegex: meta('notRegex', 'single', 'dialect'),
  jsonContains: meta('jsonContains', 'single', 'dialect'),
};
/**
 * The operators offered for each column type family, in display order.
 *
 * The relative order of the operators that already shipped is frozen
 * (`eq ne gt lt gte lte like in isNull isNotNull`): the list below keeps that
 * order inside every family, so adding an operator never reshuffles the ones
 * a user already knows.
 */
/**
 * The operators a driver serves, given what it declared.
 *
 * **An empty or missing list means the shared nineteen.** A driver that says
 * nothing has not opted out of anything — it has simply said nothing, and the
 * grid must keep working for it. This is the one place that rule lives, so
 * the shipped path and the tested path cannot drift apart: the panel reads
 * the driver's metadata through {@link supportedOperatorsForMeta}, and every
 * consumer of a plain operator list runs it through this function.
 */
export function operatorsForDriver(
  declared: readonly FilterOperator[] | undefined,
): readonly FilterOperator[] {
  if (!declared || declared.length === 0) return SHARED_FILTER_OPERATORS;
  return declared;
}

/**
 * The operators this driver serves, read from its metadata.
 *
 * A driver that declares none gets the shared set; the dialect specific
 * operators only show up when the driver names them (the back end stays the
 * authority).
 */
export function supportedOperatorsForMeta(
  meta: DatabaseTypeMeta | undefined,
): readonly FilterOperator[] {
  return operatorsForDriver(meta?.dataGrid?.filterOperators);
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
  hints?: CellTypeHints,
): FilterOperator[] {
  const cellType = resolveCellType(dataType, hints);
  const candidates = [
    ...OPERATORS_BY_CELL_TYPE[cellType],
    ...DIALECT_OPERATORS_BY_CELL_TYPE[cellType],
  ];
  return candidates.filter((operator) => supported.includes(operator));
}

/**
 * Which value control an operator is edited with.
 *
 * A condition that was persisted by another build may name an operator this
 * build does not know. The panel then degrades to a single value field
 * instead of throwing while it renders — an operator the user cannot change
 * anyway is better than a panel that fails to open.
 */
export function valueInputFor(operator: FilterOperator): FilterValueInputKind {
  const meta = FILTER_OPERATOR_META[operator] as FilterOperatorMeta | undefined;
  return meta?.valueInput ?? 'single';
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
  hints?: CellTypeHints,
): FilterOperator {
  const allowed = operatorsForColumn(dataType, supported, hints);
  if (allowed.includes(current)) return current;
  if (allowed.includes('eq')) return 'eq';
  return allowed[0] ?? current;
}

/** The operator a freshly added condition starts with. */
export function defaultOperatorForColumn(
  dataType: string | undefined,
  hints?: CellTypeHints,
): FilterOperator {
  const cellType = resolveCellType(dataType, hints);
  const operators = OPERATORS_BY_CELL_TYPE[cellType];
  return operators.includes('eq') ? 'eq' : operators[0] ?? 'eq';
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
