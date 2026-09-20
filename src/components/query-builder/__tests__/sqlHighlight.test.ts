/**
 * The preview highlighter is a hand-rolled tokenizer, so the properties that
 * matter are the boring ones:
 *  - it is lossless (concatenated tokens === input, for messy input too),
 *  - it classifies the tokens a SQL preview is judged on,
 *  - it never throws, including on half-written statements.
 */
import { describe, expect, it } from 'vitest';
import { SQL_KEYWORDS, tokenizeSql } from '../sqlHighlight';

const text = (sql: string) =>
  tokenizeSql(sql)
    .map((t) => t.text)
    .join('');
const kinds = (sql: string) => tokenizeSql(sql).map((t) => t.kind);

describe('tokenizeSql', () => {
  it('is lossless for realistic statements', () => {
    const sql = [
      '-- online customers',
      'SELECT c.id, c.name FROM "customer" c',
      'JOIN orders o ON o.customer_id = c.id',
      "WHERE c.status IN ('online', 'store') AND o.total >= 2000",
      'ORDER BY c.name ASC LIMIT 50 OFFSET 10;',
    ].join('\n');
    expect(text(sql)).toBe(sql);
  });

  it('is lossless for messy input', () => {
    const cases = [
      '',
      '   ',
      "SELECT 'unterminated",
      'SELECT "unterminated',
      'SELECT `tick',
      'SELECT [bracket',
      '/* unterminated block comment',
      'SELECT 1 -- trailing',
      'SELECT @#$%^& 1',
      'SELECT\n\t1  ,  2',
      'SELECT 3.14, 42, .5',
    ];
    for (const sql of cases) {
      expect(text(sql), `${JSON.stringify(sql)} must round-trip`).toBe(sql);
    }
  });

  it('classifies keywords case-insensitively but leaves quoting intact', () => {
    expect(kinds('select')).toEqual(['keyword']);
    expect(kinds('SeLeCt')).toEqual(['keyword']);
    expect(kinds('selectivity')).toEqual(['identifier']);
    expect(kinds("'select'")).toEqual(['string']);
    expect(kinds('"select"')).toEqual(['identifier']);
    expect(kinds('`select`')).toEqual(['identifier']);
    expect(kinds('[select]')).toEqual(['identifier']);
  });

  it('separates comments, strings, numbers and punctuation', () => {
    const tokens = tokenizeSql("-- c\nSELECT a, 12, 'x' /* b */");
    expect(tokens.filter((t) => t.kind === 'comment').map((t) => t.text)).toEqual([
      '-- c',
      '/* b */',
    ]);
    expect(tokens.filter((t) => t.kind === 'number').map((t) => t.text)).toEqual(['12']);
    expect(tokens.filter((t) => t.kind === 'string').map((t) => t.text)).toEqual(["'x'"]);
    expect(tokens.some((t) => t.kind === 'punct' && t.text === ',')).toBe(true);
  });

  it('escapes quoted literals without splitting them', () => {
    expect(kinds("'it''s'")).toEqual(['string']);
    expect(kinds('"we""ird"')).toEqual(['identifier']);
  });

  it('treats an unquoted identifier as an identifier until it is a keyword', () => {
    expect(kinds('customer_id')).toEqual(['identifier']);
    expect(kinds('left')).toEqual(['keyword']);
    expect(SQL_KEYWORDS.has('LEFT')).toBe(true);
  });
});
