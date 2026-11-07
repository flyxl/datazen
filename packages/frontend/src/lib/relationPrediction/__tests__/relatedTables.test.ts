import { describe, it, expect } from 'vitest';
import {
  DECLARED_RELATION_BOOST,
  PREDICTED_RELATION_BOOST,
  relatedTableBoostMap,
  relatedTableHints,
} from '../relatedTables';
import type { PredictionTable } from '../types';

function table(over: Partial<PredictionTable> = {}): PredictionTable {
  return {
    id: 'orders',
    name: 'orders',
    schema: 'public',
    columns: [
      { name: 'id', dataType: 'integer' },
      { name: 'user_id', dataType: 'integer' },
    ],
    primaryKey: ['id'],
    uniqueColumnSets: [],
    declaredForeignKeys: [],
    ...over,
  };
}

const users: PredictionTable = {
  id: 'users',
  name: 'users',
  schema: 'public',
  columns: [{ name: 'id', dataType: 'integer' }],
  primaryKey: ['id'],
  uniqueColumnSets: [],
  declaredForeignKeys: [],
};

describe('relatedTableHints', () => {
  it('returns nothing for no loaded tables', () => {
    expect(relatedTableHints([])).toEqual([]);
  });

  it('surfaces a declared foreign key target', () => {
    const orders = table({
      declaredForeignKeys: [
        { columns: ['user_id'], referencedTable: 'users', referencedColumns: ['id'] },
      ],
    });
    expect(relatedTableHints([orders, users])).toEqual([
      { name: 'users', origin: 'declared', boost: DECLARED_RELATION_BOOST, via: 'orders' },
    ]);
  });

  it('never suggests a table as related to itself', () => {
    const selfRef = table({
      declaredForeignKeys: [
        { columns: ['manager_id'], referencedTable: 'orders', referencedColumns: ['id'] },
      ],
    });
    expect(relatedTableHints([selfRef])).toEqual([]);
  });

  it('nudges towards a predicted target', () => {
    // No constraint anywhere — only the naming convention links the two.
    const hints = relatedTableHints([table(), users]);
    expect(hints).toEqual([
      { name: 'users', origin: 'predicted', boost: PREDICTED_RELATION_BOOST, via: 'orders' },
    ]);
  });

  it('prefers the declared reason when both apply', () => {
    const orders = table({
      declaredForeignKeys: [
        { columns: ['user_id'], referencedTable: 'users', referencedColumns: ['id'] },
      ],
    });
    const hints = relatedTableHints([orders, users]);
    expect(hints).toHaveLength(1);
    expect(hints[0]!.origin).toBe('declared');
  });

  it('drops an ambiguous prediction instead of ranking a coin flip first', () => {
    // A legacy `person` and a newer `people` both answer to `person_id`.
    const legacy: PredictionTable = {
      id: 'person',
      name: 'person',
      schema: 'public',
      columns: [{ name: 'id', dataType: 'integer' }],
      primaryKey: ['id'],
      uniqueColumnSets: [],
      declaredForeignKeys: [],
    };
    const modern: PredictionTable = { ...legacy, id: 'people', name: 'people' };
    const rows = table({
      id: 'rows',
      name: 'rows',
      columns: [
        { name: 'id', dataType: 'integer' },
        { name: 'person_id', dataType: 'integer' },
      ],
    });
    expect(relatedTableHints([legacy, modern, rows])).toEqual([]);
  });

  it('orders the strongest hint first', () => {
    const orders = table({
      declaredForeignKeys: [
        { columns: ['user_id'], referencedTable: 'users', referencedColumns: ['id'] },
      ],
    });
    const invoices = table({
      id: 'invoices',
      name: 'invoices',
      columns: [
        { name: 'id', dataType: 'integer' },
        { name: 'order_id', dataType: 'integer' },
      ],
    });
    const hints = relatedTableHints([orders, users, invoices]);
    // `users` is declared and must outrank anything inferred.
    expect(hints[0]).toMatchObject({ name: 'users', origin: 'declared' });
    // `invoices.order_id` names `orders`, so the inference points back at the
    // already-loaded `orders` — the caller decides whether that is worth showing.
    expect(hints).toContainEqual({
      name: 'orders',
      origin: 'predicted',
      boost: PREDICTED_RELATION_BOOST,
      via: 'invoices',
    });
  });
});

describe('relatedTableBoostMap', () => {
  it('folds names so quoting differences resolve to one entry', () => {
    const map = relatedTableBoostMap(
      [{ name: 'Users', origin: 'declared', boost: 20, via: 'orders' }],
      (name) => name.toLowerCase(),
    );
    expect(map.get('users')).toMatchObject({ name: 'Users' });
  });

  it('keeps the strongest hint per table', () => {
    const map = relatedTableBoostMap(
      [
        { name: 'users', origin: 'predicted', boost: 12, via: 'a' },
        { name: 'users', origin: 'declared', boost: 20, via: 'b' },
        { name: 'users', origin: 'predicted', boost: 12, via: 'c' },
      ],
      (name) => name,
    );
    expect(map.get('users')).toMatchObject({ boost: 20, via: 'b' });
    expect(map.size).toBe(1);
  });
});
