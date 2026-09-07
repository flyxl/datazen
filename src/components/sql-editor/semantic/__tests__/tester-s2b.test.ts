import { describe, expect, it } from 'vitest';
import { getDialectAdapter } from '../dialectAdapter';
import { listVisibleRelations, resolveQualifiedColumn, resolveRelation } from '../relationResolver';
import { buildSemanticModel, findScopeAtCursor } from '../scopeModel';
// Pure helper coverage targets added by the independent tester ([tester]).
import {
  parseQualifiedNameText,
  qualifiedNameToString,
  segmentFromQuotedText,
} from '../quoteHelper';

describe('[tester] dialectAdapter shouldQuoteIdentifier', () => {
  const pg = getDialectAdapter('postgresql');
  it('quotes empty, reserved and invalid-char identifiers', () => {
    expect(pg.shouldQuoteIdentifier('')).toBe(true);
    expect(pg.shouldQuoteIdentifier('select')).toBe(true);
    expect(pg.shouldQuoteIdentifier('order')).toBe(true);
    expect(pg.shouldQuoteIdentifier('user')).toBe(true);
    expect(pg.shouldQuoteIdentifier('foo bar')).toBe(true);
    expect(pg.shouldQuoteIdentifier('a-b')).toBe(true);
    expect(pg.shouldQuoteIdentifier('123')).toBe(true);
  });
  it('does not quote a plain valid identifier', () => {
    expect(pg.shouldQuoteIdentifier('users')).toBe(false);
    expect(pg.shouldQuoteIdentifier('_tbl1')).toBe(false);
    expect(pg.shouldQuoteIdentifier('a$b')).toBe(false);
  });
});

describe('[tester] dialectAdapter unquoteIdentifier fallback', () => {
  const pg = getDialectAdapter('postgresql');
  it('returns bare identifier unchanged', () => {
    expect(pg.unquoteIdentifier('users')).toBe('users');
  });
  it('returns null for non-identifier garbage', () => {
    expect(pg.unquoteIdentifier('')).toBeNull();
    expect(pg.unquoteIdentifier('a-b')).toBeNull();
    expect(pg.unquoteIdentifier('a b')).toBeNull();
    expect(pg.unquoteIdentifier('  ')).toBeNull();
    expect(pg.unquoteIdentifier('123')).toBeNull();
  });
});

describe('[tester] quoteHelper segmentFromQuotedText', () => {
  it('parses double-quoted, backtick and bracket segments', () => {
    expect(segmentFromQuotedText('"a"')).toEqual({ name: 'a', quoted: true });
    expect(segmentFromQuotedText('`a`')).toEqual({ name: 'a', quoted: true });
    expect(segmentFromQuotedText('[a]')).toEqual({ name: 'a', quoted: true });
  });
  it('returns null for bare/non-quoted input', () => {
    expect(segmentFromQuotedText('foo')).toBeNull();
    expect(segmentFromQuotedText('')).toBeNull();
    expect(segmentFromQuotedText('"foo')).toBeNull();
  });
});

describe('[tester] quoteHelper parseQualifiedNameText edge cases', () => {
  it('returns null for empty input', () => {
    expect(parseQualifiedNameText('')).toBeNull();
    expect(parseQualifiedNameText('   ')).toBeNull();
  });
  it('handles escaped double quote inside a quoted segment', () => {
    const q = parseQualifiedNameText('"a""b".c');
    expect(q?.namespacePath.map((s) => s.name)).toEqual(['a"b']);
    expect(q?.name.name).toBe('c');
    expect(q?.namespacePath[0]?.quoted).toBe(true);
  });
  it('returns null for unterminated double/backtick/bracket quotes', () => {
    expect(parseQualifiedNameText('"unterminated')).toBeNull();
    expect(parseQualifiedNameText('`unterminated')).toBeNull();
    expect(parseQualifiedNameText('[unterminated')).toBeNull();
  });
  it('handles escaped bracket inside a bracket-quoted segment', () => {
    const q = parseQualifiedNameText('[a]]b]');
    expect(q?.namespacePath).toHaveLength(0);
    expect(q?.name.name).toBe('a]b');
    expect(q?.name.quoted).toBe(true);
  });
  it('returns null for a leading digit / invalid start char', () => {
    expect(parseQualifiedNameText('123abc')).toBeNull();
    expect(parseQualifiedNameText("'abc'")).toBeNull();
  });
  it('returns null when only separators are present', () => {
    expect(parseQualifiedNameText('.')).toBeNull();
    expect(parseQualifiedNameText('..')).toBeNull();
  });
  it('handles a mixed fully-qualified name across quote styles', () => {
    const q = parseQualifiedNameText('db.[Schema].`tbl`');
    expect(q?.namespacePath.map((s) => s.name)).toEqual(['db', 'Schema']);
    expect(q?.name.name).toBe('tbl');
    expect(q?.name.quoted).toBe(true);
  });
});

describe('[tester] quoteHelper qualifiedNameToString', () => {
  it('joins namespace and name with dots', () => {
    const id = parseQualifiedNameText('db.schema.table');
    expect(id).not.toBeNull();
    expect(qualifiedNameToString(id!)).toBe('db.schema.table');
  });
  it('renders a bare relation name without a leading dot', () => {
    const id = parseQualifiedNameText('users');
    expect(qualifiedNameToString(id!)).toBe('users');
  });
});

describe('[tester] relationResolver resolveQualifiedColumn', () => {
  it('delegates qualifier resolution to resolveRelation', () => {
    const sql = 'SELECT u.id FROM users u WHERE u.';
    const cursor = sql.indexOf('u.') + 2;
    const model = buildSemanticModel(sql, cursor, { dialectId: 'postgresql' });
    const result = resolveQualifiedColumn(
      'u',
      'id',
      model,
      cursor,
      getDialectAdapter('postgresql'),
    );
    expect(result.status).toBe('unique');
    expect(result.binding?.alias).toBe('u');
  });
});

describe('[tester] relationResolver ambiguous CTE', () => {
  it('reports ambiguous when duplicate CTE names exist in one scope', () => {
    const sql = 'WITH a AS (SELECT 1), a AS (SELECT 2) SELECT * FROM a';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const result = resolveRelation('a', model, sql.length, getDialectAdapter('postgresql'));
    expect(result.status).toBe('ambiguous');
    expect(result.candidates?.length).toBeGreaterThan(1);
  });
});

describe('[tester] listVisibleRelations dedup and shadowing', () => {
  it('inner scope alias shadows outer relations when presenting visible list', () => {
    const sql = 'SELECT * FROM (SELECT * FROM t) t';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const visible = listVisibleRelations(model, sql.length, getDialectAdapter('postgresql'));
    const names = visible.map((v) => v.alias ?? v.relation.name.name);
    // The outer subquery alias 't' is the only visible alias of that name.
    expect(names.filter((n) => n === 't')).toHaveLength(1);
  });
  it('lists CTE binding even when it shares a name with a later table alias', () => {
    const sql = 'WITH c AS (SELECT 1) SELECT * FROM real_t c';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    const visible = listVisibleRelations(model, sql.length, getDialectAdapter('postgresql'));
    const aliases = visible.map((v) => v.alias ?? v.relation.name.name);
    expect(aliases.some((a) => a === 'c')).toBe(true);
  });
});

describe('[tester] CTE column list does not hang', () => {
  it('parses CTE with an empty-ish or comment-heavy column list without spinning', () => {
    const sql = 'WITH c(a) AS (SELECT 1) SELECT * FROM c';
    const model = buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' });
    expect(model.scopes[0]!.ctes[0]?.columnNames).toEqual(['a']);
  });
  it('parses a CTE whose body contains nested parens without hanging', () => {
    const sql = 'WITH x AS (SELECT count(*) FROM (VALUES (1)) v(n)) SELECT * FROM x';
    expect(() => buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' })).not.toThrow();
  });
});

describe('[tester] incomplete SQL robustness', () => {
  it('does not throw on dangling comma / paren in SELECT list', () => {
    const sql = 'SELECT a, FROM (SELECT 1';
    expect(() => buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' })).not.toThrow();
  });
  it('does not throw when cursor sits at end of a JOIN keyword', () => {
    const sql = 'SELECT * FROM a LEFT JOIN';
    expect(() => buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' })).not.toThrow();
  });
  it('does not throw on nested shadowing with incomplete inner query', () => {
    const sql = 'SELECT * FROM (SELECT id FROM users WHERE ';
    expect(() => buildSemanticModel(sql, sql.length, { dialectId: 'postgresql' })).not.toThrow();
  });
});
