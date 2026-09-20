/**
 * HAVING coverage — the clause the builder did not have at all.
 *
 * Split in two halves: what the generator emits, and what the validator says
 * about it. Both are pure functions, so they are tested without React.
 */
import { describe, it, expect } from 'vitest';
import { generateSql } from '../hooks/useSqlGenerator';
import type { GenerateSqlInput } from '../hooks/useSqlGenerator';
import { validateQuery } from '../validation';
import type { QbCondition, QbConditionGroup } from '../types';

function group(conditions: QbCondition[] = [], id = 'g'): QbConditionGroup {
  return { id, logic: 'AND', conditions, groups: [] };
}

let conditionSeq = 0;

function condition(patch: Partial<QbCondition> = {}): QbCondition {
  conditionSeq += 1;
  return {
    id: `c${conditionSeq}`,
    table: 'sales',
    column: 'qty',
    operator: '>=',
    value: '2000',
    conjunction: 'AND',
    ...patch,
  };
}

function baseInput(overrides: Partial<GenerateSqlInput> = {}): GenerateSqlInput {
  return {
    selectedTables: ['sales'],
    selectedColumns: [{ table: 'sales', column: 'qty', aggregate: 'SUM' }],
    joins: [],
    tableAliases: {},
    where: group(),
    having: group(),
    orderBy: [],
    groupBy: [],
    distinct: false,
    limit: null,
    offset: null,
    databaseType: 'postgresql',
    ...overrides,
  };
}

describe('generateSql — HAVING', () => {
  it('emits no HAVING when the clause is empty', () => {
    const sql = generateSql(baseInput());
    expect(sql).not.toContain('HAVING');
  });

  it('wraps the operand in its aggregate', () => {
    const sql = generateSql(
      baseInput({ having: group([condition({ aggregate: 'SUM', value: '2000' })]) }),
    );
    expect(sql).toContain('HAVING SUM("sales"."qty") >= 2000');
  });

  it('supports a bare (grouped) column operand', () => {
    const sql = generateSql(
      baseInput({
        having: group([condition({ column: 'region', operator: '=', value: 'EU' })]),
        groupBy: [{ table: 'sales', column: 'region' }],
      }),
    );
    expect(sql).toContain('HAVING "sales"."region" = \'EU\'');
  });

  it('places HAVING after GROUP BY and before ORDER BY', () => {
    const sql = generateSql(
      baseInput({
        groupBy: [{ table: 'sales', column: 'region' }],
        having: group([condition({ aggregate: 'SUM' })]),
        orderBy: [{ table: 'sales', column: 'region', direction: 'ASC' }],
        limit: 10,
      }),
    );
    const groupPos = sql.indexOf('GROUP BY');
    const havingPos = sql.indexOf('HAVING');
    const orderPos = sql.indexOf('ORDER BY');
    const limitPos = sql.indexOf('LIMIT');
    expect(groupPos).toBeGreaterThan(-1);
    expect(havingPos).toBeGreaterThan(groupPos);
    expect(orderPos).toBeGreaterThan(havingPos);
    expect(limitPos).toBeGreaterThan(orderPos);
  });

  it('joins several HAVING conditions with their conjunctions', () => {
    const sql = generateSql(
      baseInput({
        having: group([
          condition({ aggregate: 'SUM' }),
          condition({
            aggregate: 'COUNT',
            column: 'id',
            operator: '>',
            value: '10',
            conjunction: 'OR',
          }),
        ]),
      }),
    );
    expect(sql).toContain('HAVING SUM("sales"."qty") >= 2000 OR COUNT("sales"."id") > 10');
  });

  it('keeps the alias qualifier in HAVING', () => {
    const sql = generateSql(
      baseInput({
        tableAliases: { sales: 's' },
        having: group([condition({ aggregate: 'SUM' })]),
      }),
    );
    expect(sql).toContain('HAVING SUM("s"."qty") >= 2000');
  });

  it('parenthesises a nested HAVING group', () => {
    const sql = generateSql(
      baseInput({
        having: {
          id: 'root',
          logic: 'AND',
          conditions: [condition({ aggregate: 'SUM' })],
          groups: [
            {
              id: 'sub',
              logic: 'OR',
              conditions: [
                condition({ aggregate: 'AVG', value: '10' }),
                condition({ aggregate: 'MAX', value: '99' }),
              ],
              groups: [],
            },
          ],
        },
      }),
    );
    expect(sql).toMatch(/HAVING SUM\("sales"\."qty"\) >= 2000 AND \(AVG/);
  });

  it('keeps the aggregate out of WHERE', () => {
    const sql = generateSql(
      baseInput({
        where: group([condition({ column: 'status', operator: '=', value: 'paid' })]),
        having: group([condition({ aggregate: 'SUM' })]),
      }),
    );
    const whereIdx = sql.indexOf('WHERE');
    const havingIdx = sql.indexOf('HAVING');
    expect(whereIdx).toBeGreaterThan(-1);
    expect(havingIdx).toBeGreaterThan(whereIdx);
    // The aggregate belongs to HAVING only: the WHERE slice must not contain it.
    expect(sql.slice(whereIdx, havingIdx)).not.toContain('SUM');
    expect(sql.slice(havingIdx)).toContain('SUM("sales"."qty")');
  });
});

describe('validateQuery — HAVING', () => {
  it('warns when an operand is neither aggregated nor grouped', () => {
    const diagnostics = validateQuery(
      baseInput({
        having: group([condition({ column: 'qty', aggregate: undefined })]),
      }),
    );
    const found = diagnostics.find((d) => d.code === 'having-non-grouped');
    expect(found).toBeDefined();
    expect(found!.severity).toBe('warning');
    expect(found!.detail).toBe('sales.qty');
  });

  it('stays quiet for an aggregated operand', () => {
    const diagnostics = validateQuery(
      baseInput({ having: group([condition({ aggregate: 'SUM' })]) }),
    );
    expect(diagnostics.some((d) => d.code === 'having-non-grouped')).toBe(false);
  });

  it('stays quiet for a grouped column operand', () => {
    const diagnostics = validateQuery(
      baseInput({
        having: group([condition({ column: 'region' })]),
        groupBy: [{ table: 'sales', column: 'region' }],
      }),
    );
    expect(diagnostics.some((d) => d.code === 'having-non-grouped')).toBe(false);
  });

  it('accepts a per-column group-by flag as "grouped"', () => {
    const diagnostics = validateQuery(
      baseInput({
        selectedColumns: [{ table: 'sales', column: 'qty', groupBy: true }],
        having: group([condition({ column: 'qty', aggregate: undefined })]),
      }),
    );
    expect(diagnostics.some((d) => d.code === 'having-non-grouped')).toBe(false);
  });

  it('looks inside nested HAVING groups', () => {
    const diagnostics = validateQuery(
      baseInput({
        having: {
          id: 'root',
          logic: 'AND',
          conditions: [],
          groups: [
            { id: 'sub', logic: 'OR', conditions: [condition({ column: 'qty' })], groups: [] },
          ],
        },
      }),
    );
    expect(diagnostics.some((d) => d.code === 'having-non-grouped')).toBe(true);
  });

  it('does not block OK (warning, not error)', () => {
    const diagnostics = validateQuery(baseInput({ having: group([condition({ column: 'qty' })]) }));
    expect(diagnostics.filter((d) => d.severity === 'error')).toHaveLength(0);
  });
});
