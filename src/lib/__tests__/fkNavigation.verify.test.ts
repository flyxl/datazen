/**
 * Independent re-verification of the DB-07 pure foreign-key navigation logic.
 *
 * This harness re-derives the three semantic rules of the 07 分册 from the
 * contract text rather than from the delivered test names, so a mistake in
 * `fkNavigation.test.ts` cannot hide here:
 *
 * 1. a composite key is located **as one group** (same group object, positional
 *    pairing, one empty member makes the whole group unlocatable, the arrow
 *    lands on the group's first **visible** member);
 * 2. a NULL never degrades into `IS NULL`;
 * 3. the constraint lookup口径 is the same as the Rust one: the name wins, the
 *    ordered columns are the fallback.
 */

import { describe, expect, it } from 'vitest';
import type { ForeignKeyInfo } from '../../types';
import {
  buildFkFilters,
  findForeignKey,
  fkNavKey,
  indexForeignKeys,
  isFkCellNavigable,
  scrollTopForAnchorRow,
} from '../fkNavigation';

function foreignKey(overrides: Partial<ForeignKeyInfo> = {}): ForeignKeyInfo {
  return {
    name: 'orders_customer_fk',
    columns: ['customer_id'],
    referencedTable: 'customers',
    referencedColumns: ['id'],
    ...overrides,
  };
}

const composite = foreignKey({
  name: 'orders_parent_fk',
  columns: ['a', 'b'],
  referencedTable: 'parents',
  referencedColumns: ['x', 'y'],
});

describe('a composite key is one group', () => {
  it('maps every member column to the very same group object', () => {
    const index = indexForeignKeys([composite]);

    expect(index.get('a')).toBe(index.get('b'));
    expect(index.get('a')).toBeDefined();
    expect(index.get('a')?.localColumns).toEqual(['a', 'b']);
    expect(index.get('a')?.targetColumns).toEqual(['x', 'y']);
  });

  it('pairs the referenced columns positionally with the local columns', () => {
    const filters = buildFkFilters({ columns: ['x', 'y'] }, [1, 'k']);

    expect(filters).toEqual([
      { column: 'x', operator: 'eq', value: 1 },
      { column: 'y', operator: 'eq', value: 'k' },
    ]);
    // The pairing is by position, never by name.
    expect(buildFkFilters({ columns: ['y', 'x'] }, [1, 'k'])).toEqual([
      { column: 'y', operator: 'eq', value: 1 },
      { column: 'x', operator: 'eq', value: 'k' },
    ]);
  });

  it('makes the whole group unlocatable when any member is empty', () => {
    const index = indexForeignKeys([composite]);
    const visible = ['a', 'b'];

    for (const row of [
      { a: 1, b: null },
      { a: null, b: 'k' },
      { a: 1, b: undefined },
      { a: undefined, b: undefined },
      // A member that is not in the row at all is empty too.
      { a: 1 },
    ]) {
      expect(
        isFkCellNavigable({
          columnName: 'a',
          row,
          index,
          visibleColumns: visible,
          editing: false,
        }),
      ).toBe(false);
      expect(
        isFkCellNavigable({
          columnName: 'b',
          row,
          index,
          visibleColumns: visible,
          editing: false,
        }),
      ).toBe(false);
    }
  });

  it('puts the arrow on the first visible member only', () => {
    const index = indexForeignKeys([composite]);
    const row = { a: 1, b: 'k' };

    const navigable = (columnName: string, visibleColumns: string[]) =>
      isFkCellNavigable({ columnName, row, index, visibleColumns, editing: false });

    // Both visible: the arrow is on the first member.
    expect(navigable('a', ['a', 'b'])).toBe(true);
    expect(navigable('b', ['a', 'b'])).toBe(false);

    // The first member hidden: the arrow moves to the next visible member.
    expect(navigable('a', ['b'])).toBe(false);
    expect(navigable('b', ['b'])).toBe(true);

    // Every member hidden: no arrow anywhere.
    expect(navigable('a', [])).toBe(false);
    expect(navigable('b', [])).toBe(false);

    // A non-member column never gets an arrow.
    expect(navigable('note', ['note', 'a', 'b'])).toBe(false);
  });
});

describe('a NULL never degrades into IS NULL', () => {
  it('refuses to build filters for a null or missing value', () => {
    expect(buildFkFilters({ columns: ['x'] }, [null])).toBeNull();
    expect(buildFkFilters({ columns: ['x'] }, [undefined])).toBeNull();
    expect(buildFkFilters({ columns: ['x', 'y'] }, [1, null])).toBeNull();
    expect(buildFkFilters({ columns: ['x', 'y'] }, [1])).toBeNull();
    expect(buildFkFilters({ columns: ['x'] }, [1, 2])).toBeNull();
    expect(buildFkFilters({ columns: [] }, [])).toBeNull();
  });

  it('never emits an IS NULL operator', () => {
    const samples: Array<readonly unknown[]> = [
      [null],
      [undefined],
      [1, null],
      [1],
    ];
    for (const values of samples) {
      const filters = buildFkFilters({ columns: ['x', 'y'] }, values as never);
      expect(filters).toBeNull();
    }
    // And the happy path only ever uses `eq`.
    const filters = buildFkFilters({ columns: ['x', 'y'] }, [1, 'k']);
    expect(filters?.every((filter) => filter.operator === 'eq')).toBe(true);
    expect(
      filters?.some((filter) => /null/i.test(String(filter.operator))),
    ).toBe(false);
  });
});

describe('the constraint lookup口径 matches the Rust one', () => {
  const foreignKeys = [composite, foreignKey({ name: 'orders_customer_fk' })];

  it('prefers the name and falls back to the ordered columns', () => {
    // The name wins even when the columns point at another constraint.
    expect(findForeignKey(foreignKeys, ['customer_id'], 'orders_parent_fk')?.name).toBe(
      'orders_parent_fk',
    );
    // An unknown name falls back to the ordered columns.
    expect(findForeignKey(foreignKeys, ['customer_id'], 'nope')?.name).toBe(
      'orders_customer_fk',
    );
    // No name: the ordered columns decide.
    expect(findForeignKey(foreignKeys, ['customer_id'], null)?.name).toBe(
      'orders_customer_fk',
    );
    expect(findForeignKey(foreignKeys, ['a', 'b'], null)?.name).toBe('orders_parent_fk');
    // A different order is a different key.
    expect(findForeignKey(foreignKeys, ['b', 'a'], null)).toBeNull();
    // A blank name is no name.
    expect(findForeignKey(foreignKeys, ['customer_id'], '')?.name).toBe(
      'orders_customer_fk',
    );
  });
});

describe('the navigation key distinguishes schema and value', () => {
  const target = { database: 'app', schema: 'public', table: 'orders', columns: ['id'] };

  it('separates schema, table and value', () => {
    const key = fkNavKey(target, [1]);
    expect(fkNavKey({ ...target, schema: 'audit' }, [1])).not.toBe(key);
    expect(fkNavKey({ ...target, table: 'lines' }, [1])).not.toBe(key);
    expect(fkNavKey({ ...target, database: 'other' }, [1])).not.toBe(key);
    expect(fkNavKey(target, [2])).not.toBe(key);
    expect(fkNavKey(target, [1])).toBe(key);
  });
});

describe('the viewport offset is clamped', () => {
  it('never goes below zero', () => {
    expect(scrollTopForAnchorRow(0, 28)).toBe(0);
    expect(scrollTopForAnchorRow(-3, 28)).toBe(0);
    expect(scrollTopForAnchorRow(5, 28)).toBe(140);
  });
});
