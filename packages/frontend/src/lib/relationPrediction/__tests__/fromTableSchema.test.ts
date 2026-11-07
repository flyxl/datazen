import { describe, it, expect } from 'vitest';
import { toPredictionTableFromSchema, toPredictionTablesFromSchemas } from '../fromTableSchema';
import { predictRelations } from '../predictRelations';
import type { TableSchema } from '../../../types';

function schema(over: Partial<TableSchema> & { tableName: string }): TableSchema {
  return {
    columns: [{ name: 'id', dataType: 'integer', nullable: false }],
    primaryKeys: ['id'],
    indexes: [],
    foreignKeys: [],
    ...over,
  };
}

describe('toPredictionTableFromSchema', () => {
  it('keys the table by the caller id and keeps the bare name', () => {
    const table = toPredictionTableFromSchema('public.users', schema({ tableName: 'users' }));
    expect(table.id).toBe('public.users');
    expect(table.name).toBe('users');
  });

  it('collects the primary key and unique indexes as targetable key sets', () => {
    const table = toPredictionTableFromSchema(
      'orders',
      schema({
        tableName: 'orders',
        primaryKeys: ['id'],
        indexes: [
          { name: 'uq_code', columns: ['tenant_id', 'code'], isUnique: true, isPrimary: false },
          { name: 'idx_plain', columns: ['total'], isUnique: false, isPrimary: false },
        ],
      }),
    );
    // A non-unique index is not a key and must not become a target.
    expect(table.uniqueColumnSets).toEqual([['id'], ['tenant_id', 'code']]);
  });

  it('marks only indexed columns as indexed', () => {
    const table = toPredictionTableFromSchema(
      'orders',
      schema({
        tableName: 'orders',
        columns: [
          { name: 'id', dataType: 'integer', nullable: false },
          { name: 'user_id', dataType: 'integer', nullable: false },
        ],
        indexes: [{ name: 'idx_user', columns: ['user_id'], isUnique: false, isPrimary: false }],
      }),
    );
    const byName = new Map(table.columns.map((c) => [c.name, c]));
    expect(byName.get('user_id')!.indexed).toBe(true);
    expect(byName.get('id')!.indexed).toBe(false);
  });

  it('carries declared foreign keys so ground truth is not re-predicted', () => {
    const table = toPredictionTableFromSchema(
      'orders',
      schema({
        tableName: 'orders',
        foreignKeys: [
          {
            name: 'fk_orders_user',
            columns: ['user_id'],
            referencedTable: 'users',
            referencedColumns: ['id'],
          },
        ],
      }),
    );
    expect(table.declaredForeignKeys).toEqual([
      { columns: ['user_id'], referencedTable: 'users', referencedColumns: ['id'] },
    ]);
  });

  it('does not alias the schema arrays into the prediction input', () => {
    const source = schema({ tableName: 'orders' });
    const table = toPredictionTableFromSchema('orders', source);
    expect(table.columns).not.toBe(source.columns);
    expect(table.primaryKey).not.toBe(source.primaryKeys);
  });

  it('lets the engine infer from an ER schema end to end', () => {
    // No constraint anywhere: only the naming convention links the two, which is
    // exactly the case the ER diagram has to draw.
    const tables = toPredictionTablesFromSchemas([
      schema({
        tableName: 'users',
        columns: [{ name: 'id', dataType: 'integer', nullable: false }],
      }),
      schema({
        tableName: 'orders',
        columns: [
          { name: 'id', dataType: 'integer', nullable: false },
          { name: 'user_id', dataType: 'integer', nullable: false },
        ],
      }),
    ]);
    const found = predictRelations(tables);
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ fromTable: 'orders', toTable: 'users', tier: 'high' });
  });

  it('keys every table by name when converting a whole schema', () => {
    const tables = toPredictionTablesFromSchemas([
      schema({ tableName: 'users' }),
      schema({ tableName: 'orders' }),
    ]);
    expect(tables.map((t) => t.id)).toEqual(['users', 'orders']);
  });
});
