/**
 * [tester] sql-s2-a boundary and coverage gap tests — added by track tester agent.
 */
import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { scanSql } from '../scanner';
import {
  buildStatementRanges,
  clearStatementIndexCache,
  createStatementIndexState,
  findStatementAtCursor,
  getCachedStatementIndex,
  getStatementTextAtCursor,
  setCachedStatementIndex,
  statementIndexField,
  updateStatementRangesIncremental,
} from '../statementRanges';
import { isQuoteOrCommentKind, isStringKind, SqlTokenKind } from '../tokens';

describe('[tester] token helper coverage', () => {
  it('classifies string and comment kinds', () => {
    expect(isStringKind(SqlTokenKind.SingleQuoted)).toBe(true);
    expect(isStringKind(SqlTokenKind.Semicolon)).toBe(false);
    expect(isQuoteOrCommentKind(SqlTokenKind.LineComment)).toBe(true);
    expect(isQuoteOrCommentKind(SqlTokenKind.Other)).toBe(false);
  });
});

describe('[tester] scanner boundary cases', () => {
  it('tokenizes SQL containing CJK identifiers and string literals', () => {
    const sql = "SELECT '中文'; 列名 FROM 表;";
    const { tokens } = scanSql(sql);
    expect(tokens.some((t) => t.text.includes('中文'))).toBe(true);
    expect(tokens.filter((t) => t.kind === SqlTokenKind.Semicolon)).toHaveLength(2);
  });

  it('treats ideographic full-width space as whitespace between statements', () => {
    const sql = 'SELECT 1;\u3000SELECT 2;';
    const ranges = buildStatementRanges(sql);
    expect(ranges).toHaveLength(2);
  });

  it('tolerates unclosed single-quoted strings without throwing', () => {
    const { tokens } = scanSql("SELECT 'never closed; SELECT 2;");
    expect(tokens.some((t) => t.kind === SqlTokenKind.SingleQuoted)).toBe(true);
    expect(() => buildStatementRanges("SELECT 'never closed; SELECT 2;")).not.toThrow();
  });

  it('tolerates nested block comments with semicolons', () => {
    const sql = 'SELECT 1; /* outer ; /* inner ; */ still comment */ SELECT 2;';
    const ranges = buildStatementRanges(sql);
    expect(ranges.length).toBeGreaterThanOrEqual(2);
  });
});

describe('[tester] statement range kind hints and cursor edges', () => {
  it('assigns kind hints for update/delete/merge/ddl/transaction statements', () => {
    expect(buildStatementRanges('UPDATE t SET a=1;')[0]!.kindHint).toBe('update');
    expect(buildStatementRanges('DELETE FROM t;')[0]!.kindHint).toBe('delete');
    expect(buildStatementRanges('MERGE INTO t USING s ON 1=1;')[0]!.kindHint).toBe('merge');
    expect(buildStatementRanges('DROP TABLE t;')[0]!.kindHint).toBe('ddl');
    expect(buildStatementRanges('BEGIN; SELECT 1; COMMIT;')[0]!.kindHint).toBe('transaction');
  });

  it('returns null when cursor sits in a comment-only script', () => {
    const sql = '-- only a comment';
    expect(findStatementAtCursor(sql, 2)).toBeNull();
  });

  it('returns first statement when cursor is before all statements', () => {
    const sql = '   SELECT 1; SELECT 2;';
    const range = findStatementAtCursor(sql, 1);
    expect(range?.index).toBe(0);
  });

  it('returns last statement when cursor is after trailing content', () => {
    const sql = 'SELECT 1; SELECT 2;   ';
    const range = findStatementAtCursor(sql, sql.length - 1);
    expect(range?.index).toBe(1);
  });

  it('selects left neighbor in whitespace gap between statements', () => {
    const sql = 'SELECT 1;\n\nSELECT 2;';
    const gap = sql.indexOf('\n\n') + 1;
    expect(findStatementAtCursor(sql, gap)?.index).toBe(0);
  });

  it('falls back to first valid range from getStatementTextAtCursor when find returns empty trim', () => {
    const sql = '/* x */ SELECT 1;';
    expect(getStatementTextAtCursor(sql, sql.length)).toContain('SELECT 1');
  });

  it('returns trimmed script when no valid ranges exist but source is non-empty', () => {
    expect(getStatementTextAtCursor('   ;   ', 2)).toBe(';');
  });
});

describe('[tester] incremental index and cache paths', () => {
  it('rebuilds from scratch when previous ranges are empty', () => {
    const sql = 'SELECT 1;';
    expect(updateStatementRangesIncremental(sql, [], 0)).toEqual(buildStatementRanges(sql));
  });

  it('rebuilds from scratch when changeFrom is zero', () => {
    const sql = 'SELECT 1;';
    const initial = buildStatementRanges(sql);
    expect(updateStatementRangesIncremental(sql, initial, 0)).toEqual(buildStatementRanges(sql));
  });

  it('falls back to full scan when incremental merge diverges from full rescan', () => {
    const sql = 'SELECT 1;\nSELECT 2;';
    const initial = buildStatementRanges(sql);
    const edited = "SELECT 'open; SELECT 2;";
    const result = updateStatementRangesIncremental(edited, initial, 8);
    expect(result).toEqual(buildStatementRanges(edited));
  });

  it('evicts oldest cache entry when limit exceeded', () => {
    clearStatementIndexCache();
    for (let i = 0; i < 33; i += 1) {
      setCachedStatementIndex(
        { docIdentity: `doc-${i}`, dialectId: 'standard', revision: 0 },
        { revision: 0, ranges: [], sourceLength: 0 },
      );
    }
    expect(
      getCachedStatementIndex({ docIdentity: 'doc-0', dialectId: 'standard', revision: 0 }),
    ).toBeUndefined();
    expect(
      getCachedStatementIndex({ docIdentity: 'doc-32', dialectId: 'standard', revision: 0 }),
    ).toBeDefined();
    clearStatementIndexCache();
  });

  it('does not rebuild index on selection-only transactions', () => {
    const field = statementIndexField('panel-sel', 'standard');
    let state = EditorState.create({ doc: 'SELECT 1;', extensions: [field] });
    const before = state.field(field);
    state = state.update({ selection: { anchor: 3 } }).state;
    const after = state.field(field);
    expect(after.ranges).toEqual(before.ranges);
    expect(after.revision).toBe(before.revision);
  });

  it('creates cached snapshot via createStatementIndexState', () => {
    clearStatementIndexCache();
    createStatementIndexState('SELECT 1;', 5, 'panel-cache', 'mysql');
    expect(
      getCachedStatementIndex({ docIdentity: 'panel-cache', dialectId: 'mysql', revision: 5 }),
    ).toBeDefined();
    clearStatementIndexCache();
  });
});

describe('[tester] degraded vs exact splitting', () => {
  it('splits BEGIN blocks normally when not a stored procedure script', () => {
    const sql = 'BEGIN; SELECT 1; COMMIT;\nSELECT 2;';
    const ranges = buildStatementRanges(sql);
    expect(ranges.length).toBeGreaterThan(1);
    expect(ranges.every((r) => r.confidence === 'exact')).toBe(true);
  });
});
