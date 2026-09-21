import { describe, it, expect } from 'vitest';
import { toPredictionTable, toPredictionTables } from '../fromRelationMetadata';
import type { EditorRelationMetadata } from '../../relationMetadata/types';

function relation(over: Partial<EditorRelationMetadata> = {}): EditorRelationMetadata {
  return {
    key: 'public.orders' as EditorRelationMetadata['key'],
    identity: {
      namespacePath: [{ name: 'public', quoted: false }],
      name: { name: 'orders', quoted: false },
    },
    kind: 'table',
    columns: [
      { name: 'id', dataType: 'integer', nullable: false },
      { name: 'user_id', dataType: 'integer', nullable: false },
    ],
    primaryKey: ['id'],
    indexes: [],
    foreignKeys: [],
    loadedAt: 1,
    ...over,
  };
}

describe('toPredictionTable', () => {
  it('carries the bare table name and schema', () => {
    const table = toPredictionTable('public.orders', relation(), 'public');
    expect(table.id).toBe('public.orders');
    expect(table.name).toBe('orders');
    expect(table.schema).toBe('public');
  });

  it('marks only indexed columns as indexed', () => {
    const table = toPredictionTable(
      'orders',
      relation({
        columns: [
          { name: 'id', dataType: 'integer', nullable: false },
          { name: 'user_id', dataType: 'integer', nullable: false },
          { name: 'code', dataType: 'varchar', nullable: false },
        ],
        indexes: [
          { name: 'idx_user', columns: ['user_id'], isUnique: false },
          { name: 'uq_code', columns: ['code'], isUnique: true },
        ],
      }),
    );
    const byName = new Map(table.columns.map((c) => [c.name, c]));
    expect(byName.get('user_id')!.indexed).toBe(true);
    expect(byName.get('code')!.indexed).toBe(true);
    expect(byName.get('id')!.indexed).toBe(false);
  });

  it('collects the primary key and unique indexes as targetable key sets', () => {
    const table = toPredictionTable(
      'orders',
      relation({
        primaryKey: ['id'],
        indexes: [
          { name: 'uq_code', columns: ['tenant_id', 'code'], isUnique: true },
          { name: 'idx_plain', columns: ['total'], isUnique: false },
        ],
      }),
    );
    // Non-unique indexes are not keys and must not become targets.
    expect(table.uniqueColumnSets).toEqual([['id'], ['tenant_id', 'code']]);
  });

  it('omits the key set when there is no primary key', () => {
    const table = toPredictionTable('orders', relation({ primaryKey: [] }));
    expect(table.uniqueColumnSets).toEqual([]);
    expect(table.primaryKey).toEqual([]);
  });

  it('carries declared foreign keys so ground truth is not re-predicted', () => {
    const table = toPredictionTable(
      'orders',
      relation({
        foreignKeys: [
          { columns: ['user_id'], referencedTable: 'users', referencedColumns: ['id'] },
        ],
      }),
    );
    expect(table.declaredForeignKeys).toEqual([
      { columns: ['user_id'], referencedTable: 'users', referencedColumns: ['id'] },
    ]);
  });

  it('does not alias the metadata arrays into the prediction input', () => {
    // The caller mutates neither, but sharing references would make a later
    // in-place sort of the input silently reorder the cache.
    const source = relation();
    const table = toPredictionTable('orders', source);
    expect(table.columns).not.toBe(source.columns);
    expect(table.primaryKey).not.toBe(source.primaryKey);
  });
});

describe('toPredictionTables', () => {
  it('keys each table by its caller identity and reads the schema per identity', () => {
    const relations = new Map<string, EditorRelationMetadata>([
      ['public.orders', relation()],
      [
        'sales.users',
        relation({
          identity: {
            namespacePath: [{ name: 'sales', quoted: false }],
            name: { name: 'users', quoted: false },
          },
        }),
      ],
    ]);
    const tables = toPredictionTables(relations, (id) =>
      id.startsWith('sales.') ? 'sales' : 'public',
    );
    expect(tables.map((t) => `${t.id}@${t.schema}`)).toEqual([
      'public.orders@public',
      'sales.users@sales',
    ]);
  });

  it('leaves the schema undefined when the caller has none', () => {
    const tables = toPredictionTables(new Map([['orders', relation()]]));
    expect(tables[0]!.schema).toBeUndefined();
  });
});
