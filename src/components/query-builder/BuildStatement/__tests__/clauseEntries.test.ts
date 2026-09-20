/**
 * GROUP BY / ORDER BY entries are unioned from two store fields, so the clause
 * list has to remember which one owns each entry — otherwise removing a chip
 * clears nothing and the entry reappears.
 */
import { describe, expect, it } from 'vitest';
import { buildGroupByEntries, buildOrderByEntries } from '../clauseEntries';
import type { QbColumnSelection, QbGroupByItem, QbSortItem } from '../../types';

const column = (patch: Partial<QbColumnSelection>): QbColumnSelection => ({
  table: 'sales',
  column: 'qty',
  ...patch,
});

describe('buildGroupByEntries', () => {
  it('marks store-level items with their index', () => {
    const groupBy: QbGroupByItem[] = [
      { table: 'sales', column: 'region' },
      { table: 'sales', column: 'channel' },
    ];
    const entries = buildGroupByEntries(groupBy, []);
    expect(entries).toEqual([
      { table: 'sales', column: 'region', source: 'store', index: 0 },
      { table: 'sales', column: 'channel', source: 'store', index: 1 },
    ]);
  });

  it('appends per-column flags as column-owned entries', () => {
    const entries = buildGroupByEntries([], [column({ groupBy: true })]);
    expect(entries).toEqual([{ table: 'sales', column: 'qty', source: 'column' }]);
  });

  it('shows a column flagged twice only once, and lets the store list win', () => {
    const entries = buildGroupByEntries(
      [{ table: 'sales', column: 'qty' }],
      [column({ groupBy: true })],
    );
    expect(entries).toHaveLength(1);
    expect(entries[0]!.source).toBe('store');
  });

  it('ignores columns that are not flagged', () => {
    expect(buildGroupByEntries([], [column({})])).toEqual([]);
  });
});

describe('buildOrderByEntries', () => {
  it('carries direction and aggregate for store-level items', () => {
    const orderBy: QbSortItem[] = [
      { table: 'sales', column: 'total', direction: 'DESC', aggregate: 'SUM' },
    ];
    expect(buildOrderByEntries(orderBy, [])).toEqual([
      {
        table: 'sales',
        column: 'total',
        source: 'store',
        index: 0,
        direction: 'DESC',
        aggregate: 'SUM',
      },
    ]);
  });

  it('carries the per-column sort direction and aggregate', () => {
    const entries = buildOrderByEntries(
      [],
      [column({ column: 'amount', sort: 'ASC', aggregate: 'AVG' })],
    );
    expect(entries).toEqual([
      {
        table: 'sales',
        column: 'amount',
        source: 'column',
        direction: 'ASC',
        aggregate: 'AVG',
      },
    ]);
  });

  it('keeps the relative order: store list first, then per-column sorts', () => {
    const entries = buildOrderByEntries(
      [{ table: 'sales', column: 'region', direction: 'ASC' }],
      [column({ column: 'qty', sort: 'DESC' })],
    );
    expect(entries.map((e) => e.column)).toEqual(['region', 'qty']);
    expect(entries.map((e) => e.source)).toEqual(['store', 'column']);
  });

  it('deduplicates a column that is sorted both ways', () => {
    const entries = buildOrderByEntries(
      [{ table: 'sales', column: 'qty', direction: 'ASC' }],
      [column({ sort: 'DESC' })],
    );
    expect(entries).toHaveLength(1);
    expect(entries[0]!.source).toBe('store');
    expect(entries[0]!.direction).toBe('ASC');
  });
});
