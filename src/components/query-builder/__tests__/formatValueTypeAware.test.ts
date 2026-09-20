/**
 * Type-aware formatValue: temporal values must always be quoted regardless of
 * looking numeric, and boolean values should emit TRUE/FALSE.
 */
import { describe, expect, it } from 'vitest';
import { generateSql } from '../hooks/useSqlGenerator';
import type { GenerateSqlInput } from '../hooks/useSqlGenerator';
import type { QbConditionGroup } from '../types';

function emptyGroup(id = 'root'): QbConditionGroup {
  return { id, logic: 'AND', conditions: [], groups: [] };
}

function baseInput(overrides: Partial<GenerateSqlInput> = {}): GenerateSqlInput {
  return {
    selectedTables: [],
    selectedColumns: [],
    joins: [],
    tableAliases: {},
    where: emptyGroup(),
    orderBy: [],
    groupBy: [],
    distinct: false,
    limit: null,
    offset: null,
    ...overrides,
  };
}

describe('type-aware formatValue', () => {
  it('quotes a numeric-looking value for a date column', () => {
    // BUG: WHERE created_at > 10 → date > integer error in PostgreSQL
    const sql = generateSql(
      baseInput({
        selectedTables: ['events'],
        selectedColumns: [{ table: 'events', column: 'id' }],
        columnTypeMap: { events: { created_at: 'date' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'events',
              column: 'created_at',
              operator: '>',
              value: '10',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain("'10'");
    expect(sql).not.toContain('> 10');
  });

  it('quotes a numeric-looking value for a timestamp column', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['logs'],
        selectedColumns: [{ table: 'logs', column: 'id' }],
        columnTypeMap: { logs: { created_at: 'timestamp without time zone' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'logs',
              column: 'created_at',
              operator: '>=',
              value: '0',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain("'0'");
  });

  it('still emits pure numeric unquoted for integer columns', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['orders'],
        selectedColumns: [{ table: 'orders', column: 'id' }],
        columnTypeMap: { orders: { total: 'integer' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'orders',
              column: 'total',
              operator: '>',
              value: '100',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain('> 100');
    expect(sql).not.toContain("'100'");
  });

  it('quotes date strings for date columns (not numeric-looking)', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['events'],
        selectedColumns: [{ table: 'events', column: 'id' }],
        columnTypeMap: { events: { created_at: 'date' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'events',
              column: 'created_at',
              operator: '=',
              value: '2025-01-01',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain("'2025-01-01'");
  });

  it('quotes temporal IN-list values', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['events'],
        selectedColumns: [{ table: 'events', column: 'id' }],
        columnTypeMap: { events: { created_at: 'date' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'events',
              column: 'created_at',
              operator: 'IN',
              value: '1,2,3',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    // IN-list values for a temporal column should be quoted
    expect(sql).toContain("'1'");
    expect(sql).toContain("'2'");
    expect(sql).toContain("'3'");
  });

  it('still works without columnTypeMap (backward compatible)', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['orders'],
        selectedColumns: [{ table: 'orders', column: 'id' }],
        // No columnTypeMap — legacy behavior
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'orders',
              column: 'total',
              operator: '>',
              value: '100',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain('> 100');
  });

  it('handles boolean column values without quotes', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['tasks'],
        selectedColumns: [{ table: 'tasks', column: 'id' }],
        columnTypeMap: { tasks: { done: 'boolean' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'tasks',
              column: 'done',
              operator: '=',
              value: 'true',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain('= TRUE');
  });

  it('handles boolean false as FALSE', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['tasks'],
        selectedColumns: [{ table: 'tasks', column: 'id' }],
        columnTypeMap: { tasks: { done: 'boolean' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'tasks',
              column: 'done',
              operator: '=',
              value: 'false',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain('= FALSE');
  });

  it('quotes "true"/"false" for non-boolean columns', () => {
    const sql = generateSql(
      baseInput({
        selectedTables: ['logs'],
        selectedColumns: [{ table: 'logs', column: 'id' }],
        columnTypeMap: { logs: { status: 'varchar' } },
        where: {
          ...emptyGroup(),
          conditions: [
            {
              id: 'c1',
              table: 'logs',
              column: 'status',
              operator: '=',
              value: 'true',
              conjunction: 'AND',
            },
          ],
        },
      }),
    );
    expect(sql).toContain("'true'");
  });
});
