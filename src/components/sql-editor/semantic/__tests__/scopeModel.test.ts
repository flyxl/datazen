import { describe, expect, it } from 'vitest';
import { getDialectAdapter } from '../dialectAdapter';
import { buildSemanticModel, findScopeAtCursor, getScopeChain } from '../scopeModel';

describe('buildSemanticModel', () => {
  it('parses simple SELECT FROM with implicit alias', () => {
    const sql = 'SELECT id FROM users';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.scopes).toHaveLength(1);
    const rels = model.scopes[0]!.relations;
    expect(rels.some((r) => r.relation.name.name === 'users')).toBe(true);
  });

  it('parses explicit AS alias', () => {
    const sql = 'SELECT u.id FROM users AS u';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const rel = model.scopes[0]!.relations.find((r) => r.alias === 'u');
    expect(rel?.relation.name.name).toBe('users');
  });

  it('parses quoted identifiers', () => {
    const sql = 'SELECT 1 FROM "Schema"."Table" t';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const rel = model.scopes[0]!.relations[0];
    expect(rel?.relation.namespacePath[0]?.name).toBe('Schema');
    expect(rel?.relation.name.name).toBe('Table');
    expect(rel?.alias).toBe('t');
  });

  it('parses WITH CTE and exposes cte binding', () => {
    const sql = 'WITH cte AS (SELECT 1 AS x) SELECT * FROM cte';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const root = model.scopes[0]!;
    expect(root.ctes.some((c) => c.name === 'cte')).toBe(true);
    expect(root.relations.some((r) => r.sourceKind === 'cte' && r.alias === 'cte')).toBe(true);
  });

  it('parses CTE with explicit column list', () => {
    const sql = 'WITH cte(a, b) AS (SELECT 1, 2) SELECT a FROM cte';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.scopes[0]!.ctes[0]?.columnNames).toEqual(['a', 'b']);
  });

  it('parses JOIN relations', () => {
    const sql = 'SELECT * FROM a JOIN b ON a.id = b.a_id';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const names = model.scopes[0]!.relations.map((r) => r.relation.name.name);
    expect(names).toContain('a');
    expect(names).toContain('b');
  });

  it('parses multi-part qualified names', () => {
    const sql = 'SELECT * FROM db.schema.table';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const rel = model.scopes[0]!.relations[0];
    expect(rel?.relation.namespacePath.map((s) => s.name)).toEqual(['db', 'schema']);
    expect(rel?.relation.name.name).toBe('table');
  });

  it('parses INSERT INTO target', () => {
    const sql = 'INSERT INTO schema.users (id) VALUES (1)';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.scopes[0]!.kind).toBe('insert');
    expect(model.scopes[0]!.relations[0]?.relation.name.name).toBe('users');
  });

  it('parses UPDATE target with alias', () => {
    const sql = 'UPDATE users u SET name = 1';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.scopes[0]!.kind).toBe('update');
    expect(model.scopes[0]!.relations[0]?.alias).toBe('u');
  });

  it('parses DELETE FROM target', () => {
    const sql = 'DELETE FROM users WHERE id = 1';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.scopes[0]!.kind).toBe('delete');
    expect(model.scopes[0]!.relations[0]?.relation.name.name).toBe('users');
  });

  it('extracts projection aliases', () => {
    const sql = 'SELECT count(*) AS cnt, name n FROM t';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const aliases = model.scopes[0]!.projectionAliases.map((a) => a.alias);
    expect(aliases).toContain('cnt');
    expect(aliases).toContain('n');
  });

  it('detects cursor intent for FROM clause', () => {
    const sql = 'SELECT * FROM ';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('relation');
  });

  it('detects qualified column intent', () => {
    const sql = 'SELECT u.';
    const cursor = sql.length;
    const model = buildSemanticModel(sql, cursor, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('qualified_column');
    expect(model.cursorIntent.qualifierParts).toContain('u');
  });

  it('detects join target intent', () => {
    const sql = 'SELECT * FROM a JOIN ';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('join_target');
  });

  it('detects insert values intent', () => {
    const sql = 'INSERT INTO t (a) VALUES (';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('insert_values');
  });

  it('detects function call intent', () => {
    const sql = 'SELECT count(';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('function_call');
  });

  it('detects projection intent', () => {
    const sql = 'SELECT ';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('projection');
  });

  it('detects projection intent when typing partial identifier after SELECT', () => {
    // Bug case: typing "SELECT ord" must still be projection (not unknown),
    // so completion returns columns instead of table names.
    const sql = 'SELECT ord';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('projection');
  });

  it('detects projection intent when typing after WHERE', () => {
    const sql = 'SELECT * FROM t WHERE nam';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('projection');
  });

  it('detects relation intent when typing partial identifier after FROM', () => {
    const sql = 'SELECT * FROM use';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).toBe('relation');
  });

  it('does NOT treat as relation intent when table is complete and typing subsequent clause (e.g. WHE)', () => {
    const sql = 'SELECT * FROM er_customers WHE';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.cursorIntent.kind).not.toBe('relation');
  });

  it('returns partial model for incomplete SQL without throwing', () => {
    const sql = 'SELECT * FROM (SELECT 1';
    expect(() => buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' })).not.toThrow();
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.diagnostics.some((d) => d.message.includes('parentheses'))).toBe(true);
  });

  it('handles mysql backtick quoting', () => {
    const sql = 'SELECT * FROM `db`.`tbl` x';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'mysql' });
    const rel = model.scopes[0]!.relations[0];
    expect(rel?.relation.namespacePath[0]?.name).toBe('db');
    expect(rel?.alias).toBe('x');
  });

  it('handles mssql bracket quoting', () => {
    const sql = 'SELECT * FROM [dbo].[Users] u';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'mssql' });
    const rel = model.scopes[0]!.relations[0];
    expect(rel?.relation.namespacePath[0]?.name).toBe('dbo');
    expect(rel?.alias).toBe('u');
  });
  it('parses FROM subquery into a nested scope', () => {
    const sql = 'SELECT * FROM (SELECT id FROM users) u';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.scopes).toHaveLength(2);
    const sub = model.scopes.find((s) => s.kind === 'subquery');
    expect(sub).toBeDefined();
    expect(sub?.parentId).toBe(model.scopes[0]!.id);
    const rel = model.scopes[0]!.relations.find((r) => r.sourceKind === 'subquery');
    expect(rel?.alias).toBe('u');
    expect(rel?.relation.name.name).toBe(sub?.id);
  });
});

describe('nested scope shadowing', () => {
  it('places cursor inside a subquery in the innermost scope', () => {
    const sql = 'SELECT * FROM (SELECT id FROM users) u';
    const cursor = sql.indexOf('users') + 3;
    const model = buildSemanticModel(sql, cursor, { dialectId: 'postgresql' });
    const scope = findScopeAtCursor(model.scopes, cursor);
    expect(scope?.kind).toBe('subquery');
  });

  it('keeps a cursor after the subquery in the outer scope', () => {
    const sql = 'SELECT * FROM (SELECT id FROM users) u';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const scope = findScopeAtCursor(model.scopes, sql.length);
    expect(scope?.kind).toBe('select');
  });
});

describe('findScopeAtCursor', () => {
  it('returns innermost scope containing cursor', () => {
    const sql = 'SELECT 1';
    const model = buildSemanticModel(sql, 5, { dialectId: 'postgresql' });
    const scope = findScopeAtCursor(model.scopes, 5);
    expect(scope?.id).toBe(model.scopes[0]?.id);
  });
});

describe('getScopeChain', () => {
  it('walks parent chain', () => {
    const sql = 'SELECT 1';
    const model = buildSemanticModel(sql, 5, { dialectId: 'postgresql' });
    const chain = getScopeChain(model.scopes, model.scopes[0]!.id);
    expect(chain).toHaveLength(1);
  });
});

describe('dialect folding in scope model', () => {
  it('uses adapter fold for oracle identifiers', () => {
    const adapter = getDialectAdapter('oracle');
    expect(adapter.foldUnquotedIdentifier('mytab')).toBe('MYTAB');
  });

  it('handles multi-statement document: detects correct statement and cursorIntent in second SQL', () => {
    const doc = 'SELECT * FROM er_customers;\nSELECT * FROM er_orders o LEFT JOIN ';
    const cursor = doc.length;
    const model = buildSemanticModel(doc, cursor, { dialectId: 'postgresql' });
    console.log('Multi-statement test model:', {
      statementIndex: model.statement.index,
      from: model.statement.from,
      to: model.statement.to,
      cursorIntent: model.cursorIntent.kind,
      scopes: model.scopes.map((s) => ({ id: s.id, from: s.range.from, to: s.range.to })),
    });
    expect(model.statement.index).toBe(1);
    expect(model.cursorIntent.kind).toBe('join_target');
  });

  it('handles windowing bounds without throwing', () => {
    const lines = Array.from({ length: 1200 }, (_, i) => `SELECT ${i} FROM table_${i};`);
    const doc = lines.join('\n');
    const midPos = Math.floor(doc.length / 2);
    const model = buildSemanticModel(doc.slice(0, 500), 20, { dialectId: 'postgresql' });
    expect(model).toBeDefined();
    expect(midPos).toBeGreaterThan(0);
  });
});
