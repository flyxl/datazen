import { describe, expect, it } from 'vitest';
import { parseSchemaDropTargets } from '../sqlSchemaDropTargets';

describe('parseSchemaDropTargets', () => {
  it('finds dropped tables, including qualified and multiple targets', () => {
    expect(
      parseSchemaDropTargets('DROP TABLE IF EXISTS "sales"."orders", customers CASCADE;'),
    ).toEqual([{ schema: 'sales', table: 'orders' }, { table: 'customers' }]);
  });

  it('finds a dropped column on a qualified table', () => {
    expect(
      parseSchemaDropTargets('ALTER TABLE sales.orders DROP COLUMN IF EXISTS legacy_id;'),
    ).toEqual([{ schema: 'sales', table: 'orders', column: 'legacy_id' }]);
  });

  it('ignores statements that do not delete schema objects', () => {
    expect(parseSchemaDropTargets('SELECT * FROM orders')).toEqual([]);
    expect(parseSchemaDropTargets('ALTER TABLE orders ADD COLUMN note TEXT')).toEqual([]);
    expect(parseSchemaDropTargets("SELECT 'DROP TABLE orders' AS note")).toEqual([]);
    expect(parseSchemaDropTargets('-- DROP TABLE orders\nSELECT 1')).toEqual([]);
  });
});
