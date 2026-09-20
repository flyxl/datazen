import { useMemo } from 'react';
import {
  getQbDialectAdapter,
  generateJoinClause,
  generateLimitOffset,
  tableSourceExpr,
  columnRef,
} from '../../../lib/sqlDialects/queryBuilder';
import { isExactNumericLiteral } from '../validation';
import { classifyColumnType, TypeCategory } from '../typeCategory';
import type {
  QbConditionGroup,
  QbCondition,
  QbSortItem,
  QbColumnSelection,
  QbGroupByItem,
  QbJoin,
} from '../types';

// ── Public types ──────────────────────────────────────────────

export interface GenerateSqlInput {
  selectedTables: string[];
  selectedColumns: QbColumnSelection[];
  joins: QbJoin[];
  tableAliases: Record<string, string>;
  where: QbConditionGroup;
  /** HAVING root group. Optional so existing callers keep compiling. */
  having?: QbConditionGroup;
  orderBy: QbSortItem[];
  groupBy: QbGroupByItem[];
  distinct: boolean;
  limit: number | null;
  offset: number | null;
  databaseType?: string;
  /**
   * Per-table column type map (table → column → raw dataType string).
   * Used by {@link formatValue} to decide quoting strategy:
   * temporal columns always quoted, boolean emits TRUE/FALSE, etc.
   */
  columnTypeMap?: Record<string, Record<string, string>>;
}

// ── Value formatting helpers ──────────────────────────────────

/**
 * Format a raw value for inclusion in a SQL expression.
 * - `null` / empty → `NULL`
 * - Boolean columns → `TRUE` / `FALSE` keywords
 * - Temporal columns → always single-quoted (even numeric-looking values like `10`)
 * - Numeric columns → unquoted for pure numeric literals
 * - Everything else → single-quoted with escaped single quotes
 */
function formatValue(value: string | null, columnType?: string): string {
  if (value === null || value === '') return 'NULL';

  if (columnType) {
    const category = classifyColumnType(columnType);

    // Boolean: emit native TRUE/FALSE keywords
    if (category === TypeCategory.Boolean) {
      const lower = value.toLowerCase().trim();
      if (lower === 'true' || lower === '1' || lower === 'yes') return 'TRUE';
      if (lower === 'false' || lower === '0' || lower === 'no') return 'FALSE';
      // Fall through to quoted string for other values
    }

    // Temporal: always quote — never trust isExactNumericLiteral here
    if (category === TypeCategory.Temporal) {
      return `'${value.replace(/'/g, "''")}'`;
    }

    // Numeric: use the existing heuristic
    if (category === TypeCategory.Numeric) {
      if (isExactNumericLiteral(value)) return value;
    }
  }

  // Fallback: original behavior (no column type info)
  if (isExactNumericLiteral(value)) return value;
  return `'${value.replace(/'/g, "''")}'`;
}

/** Parse a comma-separated IN-list value into individual SQL-escaped strings. */
function parseInValues(value: string | null, columnType?: string): string[] {
  if (!value) return [];
  return value
    .split(',')
    .map((v) => v.trim())
    .filter(Boolean)
    .map((v) => formatValue(v, columnType));
}

// ── Condition formatting ──────────────────────────────────────

function formatCondition(
  cond: QbCondition,
  aliases: Record<string, string>,
  adapter: ReturnType<typeof getQbDialectAdapter>,
  columnTypeMap?: Record<string, Record<string, string>>,
): string {
  // Alias-aware: a hand-written alias must change the emitted qualifier, or the
  // alias would be declared in FROM/JOIN and then never used.
  const ref = columnRef(cond.table, cond.column, aliases, adapter);
  // HAVING filters aggregates (`HAVING SUM(qty) >= 2000`).
  const col = cond.aggregate ? `${cond.aggregate}(${ref})` : ref;

  // Look up the column's raw dataType for type-aware value formatting.
  const colType = columnTypeMap?.[cond.table]?.[cond.column];

  switch (cond.operator) {
    case '=':
    case '!=':
    case '>':
    case '<':
    case '>=':
    case '<=':
      return `${col} ${cond.operator} ${formatValue(cond.value, colType)}`;

    case 'LIKE':
      return `${col} LIKE ${formatValue(cond.value, colType)}`;

    case 'NOT LIKE':
      return `${col} NOT LIKE ${formatValue(cond.value, colType)}`;

    case 'IN':
      return adapter.formatInList(col, parseInValues(cond.value, colType), false);

    case 'NOT IN':
      return adapter.formatInList(col, parseInValues(cond.value, colType), true);

    case 'IS NULL':
      return adapter.formatNullComparison(col, true);

    case 'IS NOT NULL':
      return adapter.formatNullComparison(col, false);

    default:
      return `${col} = ${formatValue(cond.value, colType)}`;
  }
}

// ── WHERE clause builder (recursive) ─────────────────────────

/** Build the inner expression of a condition group (without the leading ` WHERE `). */
function buildGroupExpr(
  group: QbConditionGroup,
  aliases: Record<string, string>,
  adapter: ReturnType<typeof getQbDialectAdapter>,
  columnTypeMap?: Record<string, Record<string, string>>,
): string {
  const parts: string[] = [];

  // Each row's own AND/OR links it to the row above it. Joining with
  // `group.logic` instead (the previous behaviour) silently discarded the
  // per-row conjunction the user picked in the editor — `a AND b OR c` came out
  // as `a AND b AND c`. The first row has no row above it, so its conjunction is
  // unused (the editor hides the control there for exactly that reason).
  group.conditions.forEach((cond, index) => {
    const expr = formatCondition(cond, aliases, adapter, columnTypeMap);
    if (index === 0) parts.push(expr);
    else parts.push(`${cond.conjunction} ${expr}`);
  });

  for (const subGroup of group.groups) {
    const sub = buildGroupExpr(subGroup, aliases, adapter, columnTypeMap);
    // A sub-group has no per-row conjunction, so it is linked with the group's
    // own logic.
    if (sub) parts.push(`${group.logic} (${sub})`);
  }

  if (parts.length === 0) return '';
  return parts.join(' ');
}

function buildWhereClause(
  group: QbConditionGroup,
  aliases: Record<string, string>,
  adapter: ReturnType<typeof getQbDialectAdapter>,
  columnTypeMap?: Record<string, Record<string, string>>,
): string {
  const expr = buildGroupExpr(group, aliases, adapter, columnTypeMap);
  if (!expr) return '';
  return ` WHERE ${expr}`;
}

/** HAVING — same expression builder, different keyword and position. */
function buildHavingClause(
  group: QbConditionGroup | undefined,
  aliases: Record<string, string>,
  adapter: ReturnType<typeof getQbDialectAdapter>,
  columnTypeMap?: Record<string, Record<string, string>>,
): string {
  if (!group) return '';
  const expr = buildGroupExpr(group, aliases, adapter, columnTypeMap);
  if (!expr) return '';
  return ` HAVING ${expr}`;
}

// ── Core SQL generator ────────────────────────────────────────

function generateSql(input: GenerateSqlInput): string {
  if (input.selectedTables.length === 0 || input.selectedColumns.length === 0) {
    return '';
  }

  const adapter = getQbDialectAdapter(input.databaseType);
  const q = (name: string) => adapter.quoteIdentifier(name);
  const aliases = input.tableAliases ?? {};
  const columnTypeMap = input.columnTypeMap;

  // 1. SELECT
  const selectItems = input.selectedColumns.map((col) => {
    const colPath = columnRef(col.table, col.column, aliases, adapter);
    const expr = col.aggregate ? `${col.aggregate}(${colPath})` : colPath;
    return col.alias ? `${expr} AS ${q(col.alias)}` : expr;
  });
  const distinct = input.distinct ? 'DISTINCT ' : '';
  const selectClause = `SELECT ${distinct}${selectItems.join(', ')}`;

  // 2. FROM — declare the alias here; every other clause references it.
  const firstTable = input.selectedTables[0];
  const fromClause = ` FROM ${tableSourceExpr(firstTable, aliases, adapter)}`;

  // 3. JOIN
  const joinClause = generateJoinClause(input.joins, input.tableAliases, adapter, firstTable);

  // 4. WHERE — merge per-column where conditions into the root where group
  const perColumnConditions = input.selectedColumns
    .filter((c) => c.where)
    .map((c, i) => ({
      ...c.where!,
      conjunction: (i === 0 ? 'AND' : c.where!.conjunction || 'AND') as 'AND' | 'OR',
    }));
  const effectiveWhere: QbConditionGroup = {
    ...input.where,
    conditions: [...input.where.conditions, ...perColumnConditions],
  };
  const whereClause = buildWhereClause(effectiveWhere, aliases, adapter, columnTypeMap);

  // 5. GROUP BY — merge store-level groupBy with per-column groupBy flags
  const perColumnGroupBy: QbGroupByItem[] = input.selectedColumns
    .filter((c) => c.groupBy)
    .map((c) => ({ table: c.table, column: c.column }));
  const effectiveGroupBy = [...input.groupBy, ...perColumnGroupBy];
  const groupByClause =
    effectiveGroupBy.length > 0
      ? ` GROUP BY ${effectiveGroupBy
          .map((g) => columnRef(g.table, g.column, aliases, adapter))
          .join(', ')}`
      : '';

  // 6. HAVING — after GROUP BY, before ORDER BY.
  const havingClause = buildHavingClause(input.having, aliases, adapter, columnTypeMap);

  // 7. ORDER BY — merge store-level orderBy with per-column sort.  //
  // A per-column sort on an *aggregated* column must order by the aggregate
  // expression, not the bare column: `ORDER BY x` is rejected by every engine
  // once `x` is aggregated but not grouped (`SUM(x) … GROUP BY y ORDER BY x`).
  const perColumnSorts: QbSortItem[] = input.selectedColumns
    .filter((c) => c.sort)
    .map((c) => ({
      table: c.table,
      column: c.column,
      direction: c.sort!,
      aggregate: c.aggregate,
    }));
  const orderByItems = [
    ...input.orderBy.map((o) => `${columnRef(o.table, o.column, aliases, adapter)} ${o.direction}`),
    ...perColumnSorts.map((o) => {
      const ref = columnRef(o.table, o.column, aliases, adapter);
      const expr = o.aggregate ? `${o.aggregate}(${ref})` : ref;
      return `${expr} ${o.direction}`;
    }),
  ];
  const orderByClause = orderByItems.length > 0 ? ` ORDER BY ${orderByItems.join(', ')}` : '';

  // 8. LIMIT / OFFSET
  const limitOffsetClause = generateLimitOffset(input.limit, input.offset, adapter);

  return `${selectClause}${fromClause}${joinClause}${whereClause}${groupByClause}${havingClause}${orderByClause}${limitOffsetClause};`;
}

// ── Public hook ───────────────────────────────────────────────

/**
 * React hook that memoises the generated SQL string from the current
 * query builder state. Returns an empty string when there are no
 * selected tables or columns.
 */
export function useSqlGenerator(input: GenerateSqlInput): string {
  return useMemo(
    () => generateSql(input),
    // Zustand state references are stable primitives / structured clones,
    // so we serialise the complex objects for the dep array.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      input.selectedTables,
      input.selectedColumns,
      input.joins,
      input.tableAliases,
      input.where,
      input.having,
      input.orderBy,
      input.groupBy,
      input.distinct,
      input.limit,
      input.offset,
      input.databaseType,
      input.columnTypeMap,
    ],
  );
}

// ── Pure-function export for testing without React ────────────

export { generateSql };
