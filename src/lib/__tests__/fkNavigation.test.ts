import { describe, expect, it } from 'vitest';
import type { ForeignKeyInfo } from '../../types';
import {
  FK_NAV_MAX_DEPTH,
  buildFkFilters,
  fkNavKey,
  findForeignKey,
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

const composite: ForeignKeyInfo = foreignKey({
  name: 'orders_parent_fk',
  columns: ['parent_a', 'parent_b'],
  referencedTable: 'sales.parents',
  referencedColumns: ['x', 'y'],
});

describe('indexForeignKeys', () => {
  it('maps every composite member to the same group', () => {
    const index = indexForeignKeys([composite]);

    const first = index.get('parent_a');
    const second = index.get('parent_b');

    expect(first).toBeDefined();
    expect(second).toBe(first);
    expect(first?.localColumns).toEqual(['parent_a', 'parent_b']);
    expect(first?.targetColumns).toEqual(['x', 'y']);
    expect(first?.constraintName).toBe('orders_parent_fk');
    expect(first?.referenceTable).toBe('sales.parents');
  });

  it('labels a single-column target and a composite target differently', () => {
    const index = indexForeignKeys([foreignKey(), composite]);

    expect(index.get('customer_id')?.targetLabel).toBe('customers.id');
    expect(index.get('parent_a')?.targetLabel).toBe('sales.parents.(x, y)');
  });

  it('keeps the first constraint when two constraints share a column', () => {
    const index = indexForeignKeys([
      foreignKey({ name: 'first_fk' }),
      foreignKey({ name: 'second_fk' }),
    ]);

    expect(index.get('customer_id')?.constraintName).toBe('first_fk');
  });
});

describe('isFkCellNavigable', () => {
  const index = indexForeignKeys([composite]);
  const row = { parent_a: 1, parent_b: 'k' };

  it('draws the arrow only on the first visible member', () => {
    const visible = ['parent_a', 'parent_b'];

    expect(
      isFkCellNavigable({
        columnName: 'parent_a',
        row,
        index,
        visibleColumns: visible,
        editing: false,
      }),
    ).toBe(true);
    expect(
      isFkCellNavigable({
        columnName: 'parent_b',
        row,
        index,
        visibleColumns: visible,
        editing: false,
      }),
    ).toBe(false);
  });

  it('moves the arrow to the next visible member when the first one is hidden', () => {
    const visible = ['parent_b'];

    expect(
      isFkCellNavigable({
        columnName: 'parent_b',
        row,
        index,
        visibleColumns: visible,
        editing: false,
      }),
    ).toBe(true);
    expect(
      isFkCellNavigable({
        columnName: 'parent_a',
        row,
        index,
        visibleColumns: visible,
        editing: false,
      }),
    ).toBe(false);
  });

  it('draws no arrow when every member is hidden', () => {
    expect(
      isFkCellNavigable({
        columnName: 'parent_a',
        row,
        index,
        visibleColumns: [],
        editing: false,
      }),
    ).toBe(false);
  });

  it('returns false when any composite component is null', () => {
    expect(
      isFkCellNavigable({
        columnName: 'parent_a',
        row: { parent_a: 1, parent_b: null },
        index,
        visibleColumns: ['parent_a', 'parent_b'],
        editing: false,
      }),
    ).toBe(false);
    expect(
      isFkCellNavigable({
        columnName: 'parent_a',
        row: { parent_a: undefined, parent_b: 'k' },
        index,
        visibleColumns: ['parent_a', 'parent_b'],
        editing: false,
      }),
    ).toBe(false);
  });

  it('returns false for a column that is not part of a foreign key', () => {
    expect(
      isFkCellNavigable({
        columnName: 'note',
        row,
        index,
        visibleColumns: ['note'],
        editing: false,
      }),
    ).toBe(false);
  });

  it('returns false while the cell is being edited', () => {
    expect(
      isFkCellNavigable({
        columnName: 'parent_a',
        row,
        index,
        visibleColumns: ['parent_a', 'parent_b'],
        editing: true,
      }),
    ).toBe(false);
  });
});

describe('buildFkFilters', () => {
  it('pairs composite columns positionally', () => {
    const filters = buildFkFilters({ columns: ['x', 'y'] }, [1, 'k']);

    expect(filters).toEqual([
      { column: 'x', operator: 'eq', value: 1 },
      { column: 'y', operator: 'eq', value: 'k' },
    ]);
  });

  it('returns null for any null component or length mismatch', () => {
    expect(buildFkFilters({ columns: ['x', 'y'] }, [1, null])).toBeNull();
    expect(buildFkFilters({ columns: ['x', 'y'] }, [1])).toBeNull();
    expect(buildFkFilters({ columns: ['x'] }, [1, 2])).toBeNull();
    expect(buildFkFilters({ columns: [] }, [])).toBeNull();
  });
});

describe('fkNavKey', () => {
  const target = { database: 'app', schema: 'public', table: 'orders', columns: ['id'] };

  it('includes schema and values', () => {
    const key = fkNavKey(target, [1]);

    expect(fkNavKey({ ...target, schema: 'audit' }, [1])).not.toBe(key);
    expect(fkNavKey(target, [2])).not.toBe(key);
    expect(fkNavKey(target, [1])).toBe(key);
  });

  it('does not confuse a missing schema with an empty one', () => {
    expect(fkNavKey({ ...target, schema: null }, [1])).not.toBe(
      fkNavKey({ ...target, schema: '' }, [1]),
    );
  });
});

describe('findForeignKey', () => {
  const foreignKeys = [composite, foreignKey({ name: 'orders_customer_fk' })];

  it('prefers the constraint name', () => {
    const found = findForeignKey(foreignKeys, ['customer_id'], 'orders_parent_fk');

    expect(found?.name).toBe('orders_parent_fk');
  });

  it('matches the ordered columns when no name is given', () => {
    expect(findForeignKey(foreignKeys, ['customer_id'], null)?.name).toBe('orders_customer_fk');
    expect(findForeignKey(foreignKeys, ['parent_a', 'parent_b'], null)?.name).toBe(
      'orders_parent_fk',
    );
  });

  it('does not match a different column order', () => {
    expect(findForeignKey(foreignKeys, ['parent_b', 'parent_a'], null)).toBeNull();
  });
});

describe('scrollTopForAnchorRow', () => {
  it('clamps to zero', () => {
    expect(scrollTopForAnchorRow(0, 28)).toBe(0);
    expect(scrollTopForAnchorRow(-3, 28)).toBe(0);
    expect(scrollTopForAnchorRow(5, 28)).toBe(140);
  });
});

describe('FK_NAV_MAX_DEPTH', () => {
  it('bounds the navigation stack', () => {
    expect(FK_NAV_MAX_DEPTH).toBe(32);
  });
});
