/**
 * SQL dialect adapter for the Visual Query Builder.
 *
 * Encapsulates per-database differences in identifier quoting, ILIKE support,
 * LIMIT/OFFSET formatting, NULL comparison syntax, and IN-list generation.
 */

import { DB_REGISTRY } from '../databaseTypes';
import type { DatabaseType } from '../../types';
import type { QbJoin } from '../../components/query-builder/types';

// ── Adapter interface ─────────────────────────────────────────

export interface QbDialectAdapter {
  /**
   * Wrap an identifier (table/column/alias) in the dialect-specific quote
   * characters, escaping any of those characters inside the name.
   *
   * Escaping matters for user-supplied aliases: emitting
   * `AS "my "alias""` is broken SQL at best, and an identifier-injection
   * vector at worst.
   */
  quoteIdentifier(name: string): string;
  /** Whether this dialect supports ILIKE for case-insensitive LIKE. */
  supportsILike: boolean;
  /**
   * Format a LIMIT / OFFSET clause (no leading space).
   * Either bound may be `null`: an offset without a limit is legal on some
   * dialects and needs a placeholder limit on others (`LIMIT -1` in SQLite).
   * Returns `null` when the dialect cannot express the requested pagination at
   * all (SQL Server) — callers must surface that rather than silently returning
   * an unpaginated query.
   */
  formatLimitOffset(limit: number | null, offset: number | null): string | null;
  /** Format an `IS [NOT] NULL` comparison. */
  formatNullComparison(quotedColumn: string, isNull: boolean): string;
  /** Format an `[NOT] IN (val1, val2, …)` clause. Values are already SQL-escaped. */
  formatInList(quotedColumn: string, values: string[], negated: boolean): string;
}

// ── Per-dialect implementations ───────────────────────────────

const postgresqlAdapter: QbDialectAdapter = {
  quoteIdentifier: (n) => `"${n.replaceAll('"', '""')}"`,
  supportsILike: true,
  formatLimitOffset: (l, o) => {
    const hasOffset = o !== null && o > 0;
    if (l === null) return hasOffset ? `OFFSET ${o}` : null;
    return hasOffset ? `LIMIT ${l} OFFSET ${o}` : `LIMIT ${l}`;
  },
  formatNullComparison: (c, isNull) => `${c} IS${isNull ? '' : ' NOT'} NULL`,
  formatInList: (c, v, neg) => `${c} ${neg ? 'NOT ' : ''}IN (${v.join(', ')})`,
};

/** MySQL cannot express `OFFSET` without a `LIMIT`; this is the documented sentinel. */
const MYSQL_UNBOUNDED_LIMIT = '18446744073709551615';

const mysqlAdapter: QbDialectAdapter = {
  quoteIdentifier: (n) => `\`${n.replaceAll('`', '``')}\``,
  supportsILike: false,
  formatLimitOffset: (l, o) => {
    const hasOffset = o !== null && o > 0;
    if (l === null) {
      return hasOffset ? `LIMIT ${MYSQL_UNBOUNDED_LIMIT} OFFSET ${o}` : null;
    }
    return hasOffset ? `LIMIT ${o}, ${l}` : `LIMIT ${l}`;
  },
  formatNullComparison: (c, isNull) => `${c} IS${isNull ? '' : ' NOT'} NULL`,
  formatInList: (c, v, neg) => `${c} ${neg ? 'NOT ' : ''}IN (${v.join(', ')})`,
};

const sqliteAdapter: QbDialectAdapter = {
  quoteIdentifier: (n) => `"${n.replaceAll('"', '""')}"`,
  supportsILike: false,
  formatLimitOffset: (l, o) => {
    const hasOffset = o !== null && o > 0;
    // SQLite requires LIMIT before OFFSET; `-1` means "no limit".
    if (l === null) return hasOffset ? `LIMIT -1 OFFSET ${o}` : null;
    return hasOffset ? `LIMIT ${l} OFFSET ${o}` : `LIMIT ${l}`;
  },
  formatNullComparison: (c, isNull) => `${c} IS${isNull ? '' : ' NOT'} NULL`,
  formatInList: (c, v, neg) => `${c} ${neg ? 'NOT ' : ''}IN (${v.join(', ')})`,
};

const sqlserverAdapter: QbDialectAdapter = {
  quoteIdentifier: (n) => `[${n.replaceAll(']', ']]')}]`,
  supportsILike: false,
  // SQL Server needs ORDER BY + OFFSET/FETCH; v1 does not emit it, and the
  // validation layer turns that into a visible error instead of a silent
  // unpaginated query.
  formatLimitOffset: () => null,
  formatNullComparison: (c, isNull) => `${c} IS${isNull ? '' : ' NOT'} NULL`,
  formatInList: (c, v, neg) => `${c} ${neg ? 'NOT ' : ''}IN (${v.join(', ')})`,
};

const genericAdapter: QbDialectAdapter = {
  quoteIdentifier: (n) => `"${n.replaceAll('"', '""')}"`,
  supportsILike: false,
  formatLimitOffset: (l, o) => {
    const hasOffset = o !== null && o > 0;
    if (l === null) return hasOffset ? `OFFSET ${o}` : null;
    return hasOffset ? `LIMIT ${l} OFFSET ${o}` : `LIMIT ${l}`;
  },
  formatNullComparison: (c, isNull) => `${c} IS${isNull ? '' : ' NOT'} NULL`,
  formatInList: (c, v, neg) => `${c} ${neg ? 'NOT ' : ''}IN (${v.join(', ')})`,
};

// ── Table alias resolution ────────────────────────────────────

/** Table name → alias. An empty/missing entry means "use the table name". */
export type QbTableAliases = Record<string, string>;

/**
 * The qualifier a column reference must use: the alias when the table has one,
 * otherwise the table name. Every column position (SELECT, WHERE, GROUP BY,
 * ORDER BY, JOIN … ON) must agree on this or the statement will not resolve.
 */
export function tableQualifier(table: string, aliases: QbTableAliases): string {
  const alias = aliases[table];
  return alias && alias !== table ? alias : table;
}

/**
 * The alias-declaring form for a FROM/JOIN target: `"orders"` or
 * `"orders" AS "o"`. Joining a table by its alias without declaring it is a
 * syntax error, which is why the JOIN clause must not use {@link tableQualifier}
 * here.
 */
export function tableSourceExpr(
  table: string,
  aliases: QbTableAliases,
  adapter: QbDialectAdapter,
): string {
  const qualifier = tableQualifier(table, aliases);
  const quoted = adapter.quoteIdentifier(table);
  return qualifier === table ? quoted : `${quoted} AS ${adapter.quoteIdentifier(qualifier)}`;
}

/** Fully qualified, quoted column reference: `"o"."total"`. */
export function columnRef(
  table: string,
  column: string,
  aliases: QbTableAliases,
  adapter: QbDialectAdapter,
): string {
  return `${adapter.quoteIdentifier(tableQualifier(table, aliases))}.${adapter.quoteIdentifier(column)}`;
}

// ── JOIN clause generation ────────────────────────────────────

/** One emitted `JOIN <table> ON <predicates>` step. */
export interface QbJoinStep {
  type: QbJoin['type'];
  targetTable: string;
  /** Each predicate is `sourceTable.sourceColumn = targetTable.targetColumn`. */
  predicates: Array<{ sourceTable: string; sourceColumn: string; targetColumn: string }>;
  /** True for a join the walk could not attach to the FROM table's graph. */
  detached?: boolean;
}

/**
 * Walk the join graph outward from the FROM table and return one step per
 * joined table.
 *
 * A flat `JOIN rightTable ON left = right` per entry is **not** enough: the
 * entries arrive in graph order, not query order, so a naive rendering can
 * re-join the FROM table and reference tables that have not been joined yet
 * (`FROM a JOIN a ON b.x = a.id JOIN b ON c.y = b.id` — both invalid).
 *
 * This is the single source of truth for both the generated SQL and the
 * statement view in the Build tab, so what the user reads in the clause list
 * cannot drift from what is emitted:
 *  - each step joins a table that is reachable from an already-included table;
 *  - the predicate is oriented so the included side comes first;
 *  - repeated pairs (composite keys, duplicated entries) merge into one step
 *    with an extra `AND` predicate rather than a second `JOIN` of the same
 *    table;
 *  - joins not connected to the FROM table are appended with `detached: true`,
 *    so the preview still reflects what the user drew (the engine will reject
 *    it, but silently dropping their intent would be worse).
 *
 * @param fromTable - Table already present in the FROM clause (the join graph root).
 */
export function buildJoinSteps(joins: QbJoin[], fromTable?: string): QbJoinStep[] {
  if (joins.length === 0) return [];

  const included = new Set<string>();
  if (fromTable) included.add(fromTable);
  else if (joins[0]) included.add(joins[0].leftTable);

  const remaining = [...joins];
  const steps: QbJoinStep[] = [];
  const stepByTarget = new Map<string, QbJoinStep>();

  let progressed = true;
  while (remaining.length > 0 && progressed) {
    progressed = false;
    for (let i = 0; i < remaining.length; i += 1) {
      const join = remaining[i]!;
      const leftIncluded = included.has(join.leftTable);
      const rightIncluded = included.has(join.rightTable);
      if (!leftIncluded && !rightIncluded) continue;

      // Orient so the already-included side is the source of the predicate.
      const sourceTable = leftIncluded ? join.leftTable : join.rightTable;
      const sourceColumn = leftIncluded ? join.leftColumn : join.rightColumn;
      const targetTable = leftIncluded ? join.rightTable : join.leftTable;
      const targetColumn = leftIncluded ? join.rightColumn : join.leftColumn;

      remaining.splice(i, 1);
      i -= 1;
      progressed = true;

      if (included.has(targetTable)) {
        // Both sides already in the query: this is an extra predicate between
        // two joined tables (composite key, or a duplicated entry), not a new
        // table. Merge it onto whichever existing step owns either side, in the
        // orientation that step already uses.
        //
        // A self-join (same table on both sides) cannot be expressed without
        // aliases, so it is intentionally dropped rather than emitted invalid.
        const existing = stepByTarget.get(targetTable) ?? stepByTarget.get(sourceTable);
        if (existing) {
          if (existing.targetTable === targetTable) {
            existing.predicates.push({ sourceTable, sourceColumn, targetColumn });
          } else {
            existing.predicates.push({
              sourceTable: targetTable,
              sourceColumn: targetColumn,
              targetColumn: sourceColumn,
            });
          }
        }
        continue;
      }

      const step: QbJoinStep = {
        type: join.type,
        targetTable,
        predicates: [{ sourceTable, sourceColumn, targetColumn }],
      };
      steps.push(step);
      stepByTarget.set(targetTable, step);
      included.add(targetTable);
    }
  }

  for (const join of remaining) {
    steps.push({
      type: join.type,
      targetTable: join.rightTable,
      predicates: [
        {
          sourceTable: join.leftTable,
          sourceColumn: join.leftColumn,
          targetColumn: join.rightColumn,
        },
      ],
      detached: true,
    });
  }

  return steps;
}

/**
 * Generate SQL JOIN clauses from a list of QbJoin entries.
 *
 * @param fromTable - Table already present in the FROM clause (the join graph root).
 */
export function generateJoinClause(
  joins: QbJoin[],
  aliases: Record<string, string>,
  adapter: QbDialectAdapter,
  fromTable?: string,
): string {
  const steps = buildJoinSteps(joins, fromTable);
  if (steps.length === 0) return '';

  const colRef = (table: string, column: string) => columnRef(table, column, aliases, adapter);

  const clauses = steps.map((step) => {
    const on = step.predicates
      .map(
        (p) =>
          `${colRef(p.sourceTable, p.sourceColumn)} = ${colRef(step.targetTable, p.targetColumn)}`,
      )
      .join(' AND ');
    return `${step.type} JOIN ${tableSourceExpr(step.targetTable, aliases, adapter)} ON ${on}`;
  });

  return `\n${clauses.join('\n')}`;
}

// ── LIMIT / OFFSET clause generation ──────────────────────────

/**
 * Generate SQL LIMIT / OFFSET clause.
 *
 * @param limit - LIMIT value (null = no limit).
 * @param offset - OFFSET value (null = no offset).
 * @param adapter - Dialect adapter for dialect-specific formatting.
 * @returns The formatted clause string (including leading space), or empty string when neither is set.
 */
export function generateLimitOffset(
  limit: number | null,
  offset: number | null,
  adapter: QbDialectAdapter,
): string {
  if (limit === null && (offset === null || offset === 0)) return '';
  const result = adapter.formatLimitOffset(limit, offset);
  return result ? ` ${result}` : '';
}

// ── Factory ───────────────────────────────────────────────────

const DIALECT_FAMILY_MAP: Record<string, QbDialectAdapter> = {
  postgresql: postgresqlAdapter,
  mysql: mysqlAdapter,
  sqlite: sqliteAdapter,
  sqlserver: sqlserverAdapter,
};

/**
 * Resolve the appropriate {@link QbDialectAdapter} for a given database type string.
 *
 * Falls back to the `generic` adapter when the type is unknown or the registry
 * entry does not specify a dialect family.
 */
export function getQbDialectAdapter(dbType?: string): QbDialectAdapter {
  if (!dbType) return genericAdapter;

  const meta = DB_REGISTRY[dbType as DatabaseType];
  const family = meta?.sqlDialect ?? (dbType as string);
  return DIALECT_FAMILY_MAP[family] ?? genericAdapter;
}
