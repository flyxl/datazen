import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { scanSql } from '../scanner';
import {
  buildStatementRanges,
  buildStatementRangesFromTokens,
  clearStatementIndexCache,
  createStatementIndexState,
  findStatementAtCursor,
  getCachedStatementIndex,
  getStatementContent,
  getStatementTextAtCursor,
  getValidStatementRanges,
  statementIndexField,
  updateStatementRangesIncremental,
} from '../statementRanges';

const fixtureDir = join(dirname(fileURLToPath(import.meta.url)), '../../__tests__/fixtures');
const benchmark20k = readFileSync(join(fixtureDir, 'benchmark20k.sql'), 'utf8');

describe('buildStatementRanges', () => {
  it('returns empty array for empty input', () => {
    expect(buildStatementRanges('')).toEqual([]);
    expect(buildStatementRanges('   \n  ')).toEqual([]);
  });

  it('builds ranges with content and delimiter offsets', () => {
    const sql = 'SELECT 1;\n\nSELECT 2;';
    const ranges = buildStatementRanges(sql);
    expect(ranges).toHaveLength(2);
    expect(ranges[0]!.contentFrom).toBeLessThan(ranges[0]!.contentTo);
    expect(getStatementContent(sql, ranges[0]!)).toBe('SELECT 1');
    expect(getStatementContent(sql, ranges[1]!)).toBe('SELECT 2');
    expect(ranges[0]!.delimiterFrom).toBe(sql.indexOf(';'));
    expect(ranges[0]!.delimiterTo).toBe(sql.indexOf(';') + 1);
  });

  it('computes firstExecutableLine skipping leading comments', () => {
    const sql = '-- header\nSELECT 1;';
    const ranges = buildStatementRanges(sql);
    expect(ranges[0]!.firstExecutableLine).toBe(2);
  });

  it('assigns firstExecutableLine = 0 for invalid non-SQL statements (e.g. "1. 输入 SELECT ...")', () => {
    const sql = '1. 输入 SELECT order_no FROM er_orders o;';
    const ranges = buildStatementRanges(sql);
    expect(ranges).toHaveLength(1);
    expect(ranges[0]!.firstExecutableLine).toBe(0);
  });

  it('assigns kind hints from statement leading keyword', () => {
    const ranges = buildStatementRanges('SELECT 1; INSERT INTO t VALUES (1);');
    expect(ranges[0]!.kindHint).toBe('select');
    expect(ranges[1]!.kindHint).toBe('insert');
  });

  it('returns degraded single range for DELIMITER scripts', () => {
    const sql = 'DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END //';
    const ranges = buildStatementRanges(sql);
    expect(ranges).toHaveLength(1);
    expect(ranges[0]!.confidence).toBe('degraded');
  });

  it('returns degraded single range for CREATE PROCEDURE', () => {
    const sql = 'CREATE PROCEDURE p AS BEGIN SELECT 1; END;';
    const ranges = buildStatementRanges(sql);
    expect(ranges).toHaveLength(1);
    expect(ranges[0]!.confidence).toBe('degraded');
  });

  it('ignores semicolons inside quotes when splitting', () => {
    const sql = "SELECT 'a;b' AS x;\nSELECT 2;";
    const ranges = buildStatementRanges(sql);
    expect(ranges).toHaveLength(2);
    expect(getStatementContent(sql, ranges[0]!)).toContain("'a;b'");
  });
});

describe('findStatementAtCursor / getStatementTextAtCursor', () => {
  it('selects statement by cursor position among multiple statements', () => {
    const sql = 'SELECT id FROM users;\n\nSELECT name FROM orders;\n\nDELETE FROM logs;';
    expect(getStatementTextAtCursor(sql, 5)).toBe('SELECT id FROM users;');
    expect(getStatementTextAtCursor(sql, sql.indexOf('orders'))).toBe('SELECT name FROM orders;');
    expect(getStatementTextAtCursor(sql, sql.indexOf('logs'))).toBe('DELETE FROM logs;');
  });

  it('returns first statement when cursor is before all statements', () => {
    const sql = 'SELECT 1; SELECT 2;';
    expect(getStatementTextAtCursor(sql, 0)).toBe('SELECT 1;');
  });

  it('returns last statement when cursor is after all statements', () => {
    const sql = 'SELECT 1; SELECT 2;';
    expect(getStatementTextAtCursor(sql, sql.length)).toBe('SELECT 2;');
  });

  it('picks left neighbor when cursor is in whitespace between statements', () => {
    const sql = 'SELECT 1;\n\nSELECT 2;';
    const gap = sql.indexOf('\n\n') + 1;
    expect(getStatementTextAtCursor(sql, gap)).toBe('SELECT 1;');
  });

  it('includes leading comments in physical statement text', () => {
    const sql = '-- comment ; with semicolon\nSELECT 1;';
    expect(getStatementTextAtCursor(sql, 5)).toBe('-- comment ; with semicolon\nSELECT 1;');
  });

  it('returns empty for whitespace-only scripts', () => {
    expect(getStatementTextAtCursor('   \n  ', 2)).toBe('');
    expect(getStatementTextAtCursor('', 0)).toBe('');
  });
});

describe('incremental index', () => {
  it('matches full rescan after incremental edit', () => {
    const sql = 'SELECT 1;\nSELECT 2;';
    const initial = buildStatementRanges(sql);
    const edited = 'SELECT 1;\nSELECT 9;';
    const incremental = updateStatementRangesIncremental(edited, initial, edited.indexOf('9'));
    const full = buildStatementRanges(edited);
    expect(incremental).toEqual(full);
  });

  it('falls back to full scan when incremental merge diverges', () => {
    const sql = 'SELECT 1;\nSELECT 2;';
    const initial = buildStatementRanges(sql);
    const edited = "SELECT 'open; SELECT 2;";
    const incremental = updateStatementRangesIncremental(edited, initial, 8);
    expect(incremental).toEqual(buildStatementRanges(edited));
  });
});

describe('statement index cache and CodeMirror field', () => {
  it('stores and retrieves cached snapshots by revision', () => {
    clearStatementIndexCache();
    const snapshot = createStatementIndexState('SELECT 1;', 1, 'panel-1', 'postgres');
    const cached = getCachedStatementIndex({
      docIdentity: 'panel-1',
      dialectId: 'postgres',
      revision: 1,
    });
    expect(cached?.ranges).toEqual(snapshot.ranges);
    clearStatementIndexCache();
  });

  it('updates ranges through CodeMirror StateField on doc change', () => {
    const field = statementIndexField('panel-2', 'standard');
    let state = EditorState.create({ doc: 'SELECT 1;\nSELECT 2;', extensions: [field] });
    const digitPos = state.doc.toString().indexOf('2');
    state = state.update({ changes: { from: digitPos, to: digitPos + 1, insert: '9' } }).state;
    const index = state.field(field);
    expect(getValidStatementRanges(index.ranges)).toHaveLength(2);
    expect(getStatementContent(state.doc.toString(), index.ranges[1]!)).toBe('SELECT 9');
  });
});

describe('20k fixture performance smoke', () => {
  const boundaryScan = { includeOther: false, includeText: false } as const;
  const perfBudgetMs = process.env.V8_COVERAGE || process.env.NODE_V8_COVERAGE ? 60_000 : 10_000;

  it('builds ranges for 20k-line fixture', { timeout: 30_000 }, () => {
    const start = performance.now();
    const { tokens } = scanSql(benchmark20k, boundaryScan);
    const ranges = buildStatementRangesFromTokens(benchmark20k, tokens);
    const elapsed = performance.now() - start;
    expect(ranges.length).toBeGreaterThan(1000);
    expect(elapsed).toBeLessThan(perfBudgetMs);
  });

  it(
    'finds statement at cursor in 20k fixture without rescanning tokens',
    { timeout: 30_000 },
    () => {
      const { tokens } = scanSql(benchmark20k, boundaryScan);
      const ranges = buildStatementRangesFromTokens(benchmark20k, tokens);
      const offset = Math.floor(benchmark20k.length / 2);
      const start = performance.now();
      const range = findStatementAtCursor(benchmark20k, offset, ranges, tokens);
      const elapsed = performance.now() - start;
      expect(range).not.toBeNull();
      expect(elapsed).toBeLessThan(50);
    },
  );
});
