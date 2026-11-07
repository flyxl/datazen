import { describe, it, expect } from 'vitest';
import { predictRelations, EVIDENCE_WEIGHTS } from '../predictRelations';
import { normalizeIdentifier, singularize, splitIdentifierWords, typeFamily } from '../normalize';
import type { PredictionColumn, PredictionTable } from '../types';

const col = (
  name: string,
  dataType = 'integer',
  extra: Partial<PredictionColumn> = {},
): PredictionColumn => ({
  name,
  dataType,
  ...extra,
});

function table(
  name: string,
  columns: PredictionColumn[],
  over: Partial<PredictionTable> = {},
): PredictionTable {
  return {
    id: name,
    name,
    schema: 'public',
    columns,
    primaryKey: ['id'],
    uniqueColumnSets: [],
    declaredForeignKeys: [],
    ...over,
  };
}

const users = table('users', [col('id'), col('name', 'varchar')]);
const orders = table('orders', [col('id'), col('user_id'), col('total', 'numeric')]);

describe('normalize', () => {
  it('splits camelCase, snake_case and acronyms the same way', () => {
    expect(splitIdentifierWords('UserID')).toEqual(['user', 'id']);
    expect(splitIdentifierWords('user_id')).toEqual(['user', 'id']);
    expect(splitIdentifierWords('HTTPServer')).toEqual(['http', 'server']);
    expect(normalizeIdentifier('userId')).toBe('user_id');
  });

  it('singularizes regular and irregular plurals', () => {
    expect(singularize('users')).toBe('user');
    expect(singularize('order_items')).toBe('order_item');
    expect(singularize('categories')).toBe('category');
    expect(singularize('people')).toBe('person');
  });

  it('leaves words that merely look plural alone', () => {
    // `address` is not the plural of `addres`; guessing here would invent names.
    expect(singularize('address')).toBe('address');
    expect(singularize('status')).toBe('status');
    expect(singularize('business')).toBe('business');
    expect(singularize('analysis')).toBe('analysis');
  });

  it('groups type spellings into families', () => {
    expect(typeFamily('INT')).toBe('numeric');
    expect(typeFamily('bigint')).toBe('numeric');
    expect(typeFamily('NUMERIC(10,2)')).toBe('numeric');
    expect(typeFamily('varchar(32)')).toBe('string');
    expect(typeFamily('uuid')).toBe('uuid');
    expect(typeFamily('timestamptz')).toBe('temporal');
  });
});

describe('predictRelations', () => {
  it('infers the conventional {table}_id relationship', () => {
    const found = predictRelations([users, orders]);
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({
      fromTable: 'orders',
      toTable: 'users',
      columnPairs: [{ left: 'user_id', right: 'id' }],
      tier: 'high',
      ambiguous: false,
    });
  });

  it('accepts the un-singularized table name too', () => {
    const orderLines = table('order_lines', [col('id'), col('orders_id')]);
    const found = predictRelations([orders, orderLines]);
    expect(found.some((c) => c.columnPairs[0]!.left === 'orders_id')).toBe(true);
  });

  it('matches a prefixed table name from an unprefixed column', () => {
    // `app_user` is referenced by `user_id`; the table carries a prefix the
    // column drops, which is how most ORM-managed schemas are named.
    const appUsers = table('app_users', [col('id')]);
    const sessions = table('sessions', [col('id'), col('user_id')]);
    const found = predictRelations([appUsers, sessions]);
    expect(found).toHaveLength(1);
    expect(found[0]!.evidence.map((e) => e.code)).toContain('name-matches-table-stem');
    // Weaker than an exact match, so the score stays below the exact-name case.
    expect(found[0]!.score).toBeLessThan(0.9);
  });

  it('flags a prefix collision as ambiguous rather than choosing', () => {
    // `order_status` and `user_status` both end in `status`, so `status_id`
    // names both equally well.
    const orderStatus = table('order_status', [col('id')]);
    const userStatus = table('user_status', [col('id')]);
    const rows = table('rows', [col('id'), col('status_id')]);
    const found = predictRelations([orderStatus, userStatus, rows]);
    expect(found).toHaveLength(2);
    expect(found.every((c) => c.ambiguous)).toBe(true);
    expect(found.every((c) => c.tier === 'medium')).toBe(true);
  });

  it('never relates two tables through their generic id column alone', () => {
    // Every table's surrogate key is `id`; matching on it would relate
    // everything to everything. Only a column that *names* the target counts.
    const plainOrders = table('orders', [col('id'), col('total', 'numeric')]);
    expect(predictRelations([users, plainOrders])).toEqual([]);
  });

  it('returns nothing for a single table', () => {
    expect(predictRelations([users])).toEqual([]);
  });

  it('skips columns already covered by a declared foreign key', () => {
    const declared = table('orders', [col('id'), col('user_id')], {
      declaredForeignKeys: [
        { columns: ['user_id'], referencedTable: 'users', referencedColumns: ['id'] },
      ],
    });
    expect(predictRelations([users, declared])).toEqual([]);
  });

  it('rejects a type family mismatch however good the name is', () => {
    // MySQL would compare '1' = 1 through an implicit cast, so the name alone
    // must not be enough.
    const stringFk = table('orders', [col('id'), col('user_id', 'varchar(32)')]);
    expect(predictRelations([users, stringFk])).toEqual([]);
  });

  it('accepts different numeric widths', () => {
    const bigUsers = table('users', [col('id', 'bigint')]);
    const found = predictRelations([bigUsers, orders]);
    expect(found).toHaveLength(1);
    expect(found[0]!.evidence.map((e) => e.code)).toContain('type-compatible');
    expect(found[0]!.evidence.map((e) => e.code)).not.toContain('type-exact');
  });

  it('will not target a non-key column', () => {
    // `orders.name` matches `users.name` by name, but `name` is not a key.
    const named = table('orders', [col('id'), col('name', 'varchar')]);
    expect(predictRelations([users, named])).toEqual([]);
  });

  it('targets a unique index that is not the primary key', () => {
    const accounts = table('accounts', [col('id'), col('account_no', 'varchar')], {
      uniqueColumnSets: [['account_no']],
    });
    const invoices = table('invoices', [col('id'), col('account_no', 'varchar')]);
    const found = predictRelations([accounts, invoices]);
    expect(found).toHaveLength(1);
    expect(found[0]!.evidence.map((e) => e.code)).toContain('target-is-unique');
  });

  it('folds a composite key into one relationship with both pairs', () => {
    const parents = table('parents', [col('id'), col('no')], {
      primaryKey: ['id', 'no'],
    });
    const children = table('children', [col('id'), col('parent_id'), col('parent_no')]);
    const found = predictRelations([parents, children]);
    expect(found).toHaveLength(1);
    expect(found[0]!.columnPairs).toEqual([
      { left: 'parent_id', right: 'id' },
      { left: 'parent_no', right: 'no' },
    ]);
  });

  it('keeps two separate references to one table apart', () => {
    // `billing_address_id` and `shipping_address_id` are two relationships to
    // one table; merging them would invent an AND that matches nothing.
    const addresses = table('addresses', [col('id')]);
    const ordersWithTwo = table('orders', [
      col('id'),
      col('billing_address_id'),
      col('shipping_address_id'),
    ]);
    const found = predictRelations([addresses, ordersWithTwo]);
    expect(found).toHaveLength(2);
    expect(found.every((c) => c.columnPairs.length === 1)).toBe(true);
  });

  it('refuses to choose when two tables match equally well', () => {
    // A legacy `person` table and a newer `people` one both singularize to
    // `person`, so `person_id` names both equally well.
    const legacy = table('person', [col('id')]);
    const modern = table('people', [col('id')]);
    const rows = table('rows', [col('id'), col('person_id')]);
    const found = predictRelations([legacy, modern, rows]);
    expect(found.length).toBeGreaterThan(0);
    // Neither may be applied automatically.
    expect(found.every((c) => c.ambiguous)).toBe(true);
    expect(found.every((c) => c.tier === 'medium')).toBe(true);
    expect(found[0]!.evidence.map((e) => e.code)).toContain('ambiguous');
  });

  it('does not cross schemas by default', () => {
    const otherSchema = table('users', [col('id')], { id: 'sales.users', schema: 'sales' });
    expect(predictRelations([otherSchema, orders])).toEqual([]);
    expect(predictRelations([otherSchema, orders], { allowCrossSchema: true })).toHaveLength(1);
  });

  it('does not relate a table to itself', () => {
    const employees = table('employees', [col('id'), col('employee_id')]);
    expect(predictRelations([employees])).toEqual([]);
  });

  it('drops a partial composite match rather than emitting half a predicate', () => {
    // Only one of the two key columns is present, so the ON clause would be
    // incomplete.
    const parents = table('parents', [col('id'), col('no')], { primaryKey: ['id', 'no'] });
    const children = table('children', [col('id'), col('parent_id')]);
    expect(predictRelations([parents, children])).toEqual([]);
  });

  it('produces a deterministic id and stable ordering', () => {
    const first = predictRelations([users, orders]);
    const second = predictRelations([orders, users]);
    expect(second).toEqual(first);
  });

  it('raises the score when the source column is indexed', () => {
    const plain = predictRelations([users, orders]);
    const indexed = predictRelations([
      users,
      table('orders', [col('id'), col('user_id', 'integer', { indexed: true })]),
    ]);
    expect(indexed[0]!.score).toBe(plain[0]!.score + EVIDENCE_WEIGHTS.sourceIndexed);
  });

  it('honours a custom high threshold', () => {
    // Pin the score rather than assuming it, so the test fails loudly if the
    // weights move.
    const score = predictRelations([users, orders])[0]!.score;
    expect(score).toBeCloseTo(
      EVIDENCE_WEIGHTS.targetIsPrimaryKey +
        EVIDENCE_WEIGHTS.nameMatchesTable +
        EVIDENCE_WEIGHTS.typeExact,
      3,
    );
    const raised = predictRelations([users, orders], { highThreshold: score + 0.01 });
    expect(raised[0]!.tier).toBe('medium');
  });

  it('explains every candidate with the evidence that produced it', () => {
    const found = predictRelations([users, orders]);
    const codes = found[0]!.evidence.map((e) => e.code);
    expect(codes).toContain('name-matches-table-stem');
    expect(codes).toContain('target-is-primary-key');
    expect(codes).toContain('type-exact');
    expect(found[0]!.evidence.every((e) => e.detail.length > 0)).toBe(true);
  });

  it('lets a keyless table be a source but never a target', () => {
    // Without a key there is nothing to point at, so it cannot be referenced —
    // but its own columns may still reference a keyed table.
    const noPk = table('views_like', [col('id'), col('user_id')], { primaryKey: [] });
    const found = predictRelations([users, noPk]);
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ fromTable: 'views_like', toTable: 'users' });
  });

  it('does not treat a unique index over the primary key as a second target', () => {
    const usersWithUniquePk = table('users', [col('id')], { uniqueColumnSets: [['id']] });
    const found = predictRelations([usersWithUniquePk, orders]);
    expect(found).toHaveLength(1);
    expect(found[0]!.evidence.map((e) => e.code)).toContain('target-is-primary-key');
    expect(found[0]!.evidence.map((e) => e.code)).not.toContain('target-is-unique');
  });
});
