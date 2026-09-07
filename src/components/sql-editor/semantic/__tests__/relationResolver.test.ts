import { describe, expect, it } from 'vitest';
import { getDialectAdapter } from '../dialectAdapter';
import {
  resolveQualifiedRelation,
  resolveRelation,
  listVisibleRelations,
} from '../relationResolver';
import { buildSemanticModel } from '../scopeModel';

describe('resolveRelation', () => {
  it('resolves unique alias', () => {
    const sql = 'SELECT u.id FROM users u WHERE u.';
    const cursor = sql.indexOf('u.') + 2;
    const model = buildSemanticModel(sql, cursor, { dialectId: 'postgresql' });
    const result = resolveRelation('u', model, cursor, getDialectAdapter('postgresql'));
    expect(result.status).toBe('unique');
    expect(result.binding?.alias).toBe('u');
  });

  it('resolves table name without alias', () => {
    const sql = 'SELECT * FROM orders WHERE orders.';
    const cursor = sql.length;
    const model = buildSemanticModel(sql, cursor, { dialectId: 'postgresql' });
    const result = resolveRelation('orders', model, cursor, getDialectAdapter('postgresql'));
    expect(result.status).toBe('unique');
    expect(result.binding?.relation.name.name).toBe('orders');
  });

  it('resolves CTE by name', () => {
    const sql = 'WITH stats AS (SELECT 1 n) SELECT * FROM stats s WHERE s.';
    const cursor = sql.indexOf('s.') + 2;
    const model = buildSemanticModel(sql, cursor, { dialectId: 'postgresql' });
    const result = resolveRelation('stats', model, cursor, getDialectAdapter('postgresql'));
    expect(result.status).toBe('unique');
    expect(result.binding?.sourceKind).toBe('cte');
  });

  it('returns ambiguous for duplicate unaliased same-name tables in one scope', () => {
    const sql = 'SELECT * FROM t, t';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const result = resolveRelation('t', model, sql.length, getDialectAdapter('postgresql'));
    expect(result.status).toBe('ambiguous');
    expect(result.candidates?.length).toBeGreaterThan(1);
  });

  it('returns unresolved for unknown alias', () => {
    const sql = 'SELECT * FROM users';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const result = resolveRelation('missing', model, sql.length, getDialectAdapter('postgresql'));
    expect(result.status).toBe('unresolved');
  });

  it('handles incomplete SQL without throwing', () => {
    const sql = 'SELECT * FROM users u JOIN ';
    expect(() => {
      const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
      resolveRelation('u', model, sql.length, getDialectAdapter('postgresql'));
    }).not.toThrow();
  });

  it('resolves schema-qualified relation', () => {
    const sql = 'SELECT * FROM public.users u';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const adapter = getDialectAdapter('postgresql');
    const result = resolveRelation('u', model, sql.length, adapter);
    expect(result.status).toBe('unique');
    expect(result.binding?.relation.namespacePath[0]?.name).toBe('public');
  });

  it('case-insensitive match for postgresql', () => {
    const sql = 'SELECT * FROM Users U';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const result = resolveRelation('u', model, sql.length, getDialectAdapter('postgresql'));
    expect(result.status).toBe('unique');
  });
});

describe('resolveQualifiedRelation', () => {
  it('matches fully qualified binding', () => {
    const sql = 'SELECT * FROM db.schema.table';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const adapter = getDialectAdapter('postgresql');
    const qualified = adapter.parseQualifiedName('db.schema.table');
    expect(qualified).not.toBeNull();
    const result = resolveQualifiedRelation(qualified!, model, sql.length, adapter);
    expect(result.status).toBe('unique');
  });
});

describe('listVisibleRelations', () => {
  it('lists relations with shadowing (later alias wins)', () => {
    const sql = 'WITH t AS (SELECT 1) SELECT * FROM real_t t';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const visible = listVisibleRelations(model, sql.length, getDialectAdapter('postgresql'));
    const aliases = visible.map((v) => v.alias ?? v.relation.name.name);
    expect(aliases.some((a) => a === 't')).toBe(true);
  });

  it('includes CTE and table bindings', () => {
    const sql = 'WITH c AS (SELECT 1) SELECT * FROM users u JOIN c ON true';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const visible = listVisibleRelations(model, sql.length, getDialectAdapter('postgresql'));
    expect(visible.length).toBeGreaterThanOrEqual(2);
  });
});

describe('nested subquery resolution', () => {
  it('resolves a subquery alias uniquely in the outer scope', () => {
    const sql = 'SELECT * FROM (SELECT id FROM users) u';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const result = resolveRelation('u', model, sql.length, getDialectAdapter('postgresql'));
    expect(result.status).toBe('unique');
    expect(result.binding?.sourceKind).toBe('subquery');
  });

  it('inner scope shadows an outer alias during resolution', () => {
    const sql = 'SELECT * FROM (SELECT * FROM t) t';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    // Cursor inside the subquery body: resolving 't' finds the inner table,
    // which correctly shadows the outer subquery alias of the same name.
    const innerPos = sql.indexOf('t)');
    const result = resolveRelation('t', model, innerPos, getDialectAdapter('postgresql'));
    expect(result.status).toBe('unique');
    expect(result.binding?.sourceKind).toBe('table');
  });
});

describe('self-join resolution', () => {
  it('resolves distinct aliases for self join', () => {
    const sql = 'SELECT * FROM employees e1 JOIN employees e2 ON e1.id = e2.manager_id';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const e1 = resolveRelation('e1', model, sql.length, getDialectAdapter('postgresql'));
    const e2 = resolveRelation('e2', model, sql.length, getDialectAdapter('postgresql'));
    expect(e1.status).toBe('unique');
    expect(e2.status).toBe('unique');
    expect(e1.binding?.alias).toBe('e1');
    expect(e2.binding?.alias).toBe('e2');
  });
});
