import type { GenerateSqlInput } from './hooks/useSqlGenerator';
import { getQbDialectAdapter } from '../../lib/sqlDialects/queryBuilder';
import { validateTemporalValue } from './temporalValue';

/**
 * Query builder diagnostics.
 *
 * The generator is deliberately tolerant: it always returns *something* so the
 * preview keeps updating while the user is mid-edit. That tolerance is exactly
 * how invalid states used to reach the editor as silently-wrong SQL
 * (`col = NULL`, `IN ()`, `= 007`, a dropped pagination clause). This module
 * names those states so the panel can block OK and explain why.
 *
 * `error` blocks OK; `warning` is shown but still buildable.
 */
export type QbDiagnosticCode =
  | 'no-tables'
  | 'no-columns'
  | 'empty-condition-value'
  | 'empty-in-list'
  | 'invalid-temporal-literal'
  | 'invalid-limit'
  | 'unsupported-pagination'
  | 'alias-duplicate'
  | 'alias-shadows-table'
  | 'unknown-join-table'
  | 'self-join'
  | 'composite-join-incomplete'
  | 'having-non-grouped';

export interface QbDiagnostic {
  code: QbDiagnosticCode;
  severity: 'error' | 'warning';
  /** i18n key under `query.visualBuilder.diag.*`. */
  messageKey: string;
  /** Concrete offending values, for the message body. */
  detail?: string;
}

const IN_OPERATORS = new Set(['IN', 'NOT IN']);

/** True for a numeric literal that survives a round-trip unchanged. */
export function isExactNumericLiteral(value: string): boolean {
  if (!/^-?\d+(\.\d+)?$/.test(value)) return false;
  // `007`, `1.50`, `-0` merely look numeric. Emitting them unquoted either
  // breaks (`007` is not a valid numeric literal in PostgreSQL) or changes the
  // value (`1.50` → `1.5`); quoting them preserves what the user typed.
  return String(Number(value)) === value;
}

/** The subset of state the validator reads (joins/autoJoins carry the grouping). */
export type QueryValidationInput = GenerateSqlInput & {
  autoJoins?: Array<{
    constraint?: string;
    leftTable: string;
    leftColumn: string;
    rightTable: string;
    rightColumn: string;
  }>;
};

/** Collect every diagnostic for the current builder state. */
export function validateQuery(input: QueryValidationInput): QbDiagnostic[] {
  const out: QbDiagnostic[] = [];

  if (input.selectedTables.length === 0) {
    out.push({ code: 'no-tables', severity: 'error', messageKey: 'noTables' });
    // Nothing else can be judged meaningfully without a FROM table.
    return out;
  }

  if (input.selectedColumns.length === 0) {
    out.push({ code: 'no-columns', severity: 'error', messageKey: 'noColumns' });
  }

  collectConditionDiagnostics(input, out);
  collectHavingDiagnostics(input, out);
  collectAliasDiagnostics(input, out);
  collectJoinDiagnostics(input, out);
  collectCompositeJoinDiagnostics(input, out);
  collectPaginationDiagnostics(input, out);

  return out;
}

function collectConditionDiagnostics(input: QueryValidationInput, out: QbDiagnostic[]): void {
  const visit = (conditions: typeof input.where.conditions) => {
    for (const cond of conditions) {
      const isNullOp = cond.operator === 'IS NULL' || cond.operator === 'IS NOT NULL';
      if (isNullOp) continue;

      const colType = input.columnTypeMap?.[cond.table]?.[cond.column];

      if (IN_OPERATORS.has(cond.operator)) {
        const hasAny = (cond.value ?? '').split(',').some((v) => v.trim().length > 0);
        if (!hasAny) {
          // `IN ()` is a syntax error.
          out.push({
            code: 'empty-in-list',
            severity: 'error',
            messageKey: 'emptyInList',
            detail: `${cond.table}.${cond.column}`,
          });
        }
        if (hasAny && validateTemporalValue(cond.value ?? '', colType) === 'invalid') {
          out.push({
            code: 'invalid-temporal-literal',
            severity: 'error',
            messageKey: 'invalidTemporalLiteral',
            detail: `${cond.table}.${cond.column} → ${cond.value}`,
          });
        }
        continue;
      }

      if ((cond.value ?? '') === '') {
        // `col = NULL` is never true, and `col LIKE NULL` never matches: the
        // user gets an empty result with no hint that anything is wrong.
        out.push({
          code: 'empty-condition-value',
          severity: 'error',
          messageKey: 'emptyConditionValue',
          detail: `${cond.table}.${cond.column} ${cond.operator}`,
        });
        continue;
      }

      // `WHERE created_at > 10` fails at execution with
      // `operator does not exist: timestamp > integer` — catch it in preview.
      if (validateTemporalValue(cond.value ?? '', colType) === 'invalid') {
        out.push({
          code: 'invalid-temporal-literal',
          severity: 'error',
          messageKey: 'invalidTemporalLiteral',
          detail: `${cond.table}.${cond.column} → ${cond.value}`,
        });
      }
    }
  };

  const walkGroups = (group: typeof input.where): void => {
    visit(group.conditions);
    for (const sub of group.groups) walkGroups(sub);
  };
  walkGroups(input.where);

  // Per-column WHERE clauses are merged into the same output.
  for (const col of input.selectedColumns) {
    if (col.where) visit([col.where]);
  }
}

/**
 * HAVING filters groups, so every operand must be an aggregate or a grouped
 * column. `HAVING qty > 5` on a bare column is rejected by PostgreSQL, MySQL
 * (only_full_group_by) and SQL Server alike — worth saying before OK rather
 * than after execution. A warning, not an error: some engines do allow it for
 * grouped-only queries, and the user may still want to run it.
 */
function collectHavingDiagnostics(input: QueryValidationInput, out: QbDiagnostic[]): void {
  if (!input.having) return;

  const grouped = new Set<string>();
  for (const item of input.groupBy ?? []) grouped.add(`${item.table}.${item.column}`);
  for (const col of input.selectedColumns) {
    if (col.groupBy) grouped.add(`${col.table}.${col.column}`);
  }

  const visit = (group: typeof input.having): void => {
    for (const cond of group.conditions) {
      if (cond.aggregate) continue;
      if (grouped.has(`${cond.table}.${cond.column}`)) continue;
      out.push({
        code: 'having-non-grouped',
        severity: 'warning',
        messageKey: 'havingNonGrouped',
        detail: `${cond.table}.${cond.column}`,
      });
    }
    for (const sub of group.groups) visit(sub);
  };
  visit(input.having);
}

function collectAliasDiagnostics(input: QueryValidationInput, out: QbDiagnostic[]): void {
  const aliases = input.tableAliases ?? {};
  const seen = new Map<string, string>();

  for (const table of input.selectedTables) {
    const alias = aliases[table];
    if (!alias) continue;

    if (alias === table) continue;

    // An alias identical to another selected table's name makes every
    // reference to that name ambiguous.
    if (input.selectedTables.includes(alias)) {
      out.push({
        code: 'alias-shadows-table',
        severity: 'error',
        messageKey: 'aliasShadowsTable',
        detail: `${table} → ${alias}`,
      });
      continue;
    }

    const owner = seen.get(alias);
    if (owner) {
      out.push({
        code: 'alias-duplicate',
        severity: 'error',
        messageKey: 'aliasDuplicate',
        detail: `${owner}, ${table} → ${alias}`,
      });
    } else {
      seen.set(alias, table);
    }
  }
}

function collectJoinDiagnostics(input: QueryValidationInput, out: QbDiagnostic[]): void {
  const tables = new Set(input.selectedTables);
  for (const join of input.joins) {
    if (join.leftTable === join.rightTable) {
      // Cannot be expressed without alias instances; the generator drops it, so
      // say so instead of quietly producing a query with one join fewer.
      out.push({
        code: 'self-join',
        severity: 'error',
        messageKey: 'selfJoin',
        detail: join.leftTable,
      });
      continue;
    }
    for (const side of [join.leftTable, join.rightTable]) {
      if (!tables.has(side)) {
        out.push({
          code: 'unknown-join-table',
          severity: 'error',
          messageKey: 'unknownJoinTable',
          detail: side,
        });
      }
    }
  }
}

/**
 * A composite FK contributes one pair per column. Confirming only some of them
 * emits a JOIN whose ON clause is missing predicates — it parses, and returns
 * wrong rows. Compare what is in `joins` against the detected candidate groups.
 */
function collectCompositeJoinDiagnostics(input: QueryValidationInput, out: QbDiagnostic[]): void {
  const detectedByConstraint = new Map<string, Set<string>>();
  const pairId = (left: string, leftColumn: string, right: string, rightColumn: string) =>
    `${left}.${leftColumn}->${right}.${rightColumn}`;

  for (const candidate of input.autoJoins ?? []) {
    if (!candidate.constraint) continue;
    const set = detectedByConstraint.get(candidate.constraint) ?? new Set<string>();
    set.add(
      pairId(
        candidate.leftTable,
        candidate.leftColumn,
        candidate.rightTable,
        candidate.rightColumn,
      ),
    );
    detectedByConstraint.set(candidate.constraint, set);
  }

  const confirmedByConstraint = new Map<string, Set<string>>();
  for (const join of input.joins) {
    if (!join.constraint) continue;
    const set = confirmedByConstraint.get(join.constraint) ?? new Set<string>();
    set.add(pairId(join.leftTable, join.leftColumn, join.rightTable, join.rightColumn));
    confirmedByConstraint.set(join.constraint, set);
  }

  for (const [constraint, detected] of detectedByConstraint) {
    const confirmed = confirmedByConstraint.get(constraint);
    if (!confirmed) continue;
    if (confirmed.size === 0 || confirmed.size >= detected.size) continue;
    out.push({
      code: 'composite-join-incomplete',
      severity: 'error',
      messageKey: 'compositeJoinIncomplete',
      detail: `${constraint.split('::')[1] ?? constraint} (${confirmed.size}/${detected.size})`,
    });
  }
}

function collectPaginationDiagnostics(input: QueryValidationInput, out: QbDiagnostic[]): void {
  const { limit, offset } = input;

  if (limit !== null && (!Number.isInteger(limit) || limit < 0)) {
    out.push({
      code: 'invalid-limit',
      severity: 'error',
      messageKey: 'invalidLimit',
      detail: String(limit),
    });
  }
  if (offset !== null && (!Number.isInteger(offset) || offset < 0)) {
    out.push({
      code: 'invalid-limit',
      severity: 'error',
      messageKey: 'invalidLimit',
      detail: String(offset),
    });
  }

  const wantsPagination = (limit !== null && limit > 0) || (offset !== null && offset > 0);
  if (wantsPagination) {
    const adapter = getQbDialectAdapter(input.databaseType);
    if (adapter.formatLimitOffset(limit, offset ?? 0) === null) {
      // Silently dropping pagination returns a completely different result set.
      out.push({
        code: 'unsupported-pagination',
        severity: 'error',
        messageKey: 'unsupportedPagination',
        detail: input.databaseType ?? 'unknown',
      });
    }
  }
}
