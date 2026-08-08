import { describe, expect, it } from 'vitest';
import { parseQualifiedPathParents } from '../sqlPathPrefix';

describe('parseQualifiedPathParents', () => {
  it('returns parents after a trailing dot', () => {
    const sql = 'SELECT * FROM hive.snap.';
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual(['hive', 'snap']);
  });

  it('returns parents for partial last segment', () => {
    const sql = 'SELECT * FROM hive.sn';
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual(['hive']);
  });

  it('returns empty at top-level identifier', () => {
    const sql = 'SELECT * FROM hi';
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual([]);
  });

  it('ignores dots inside strings', () => {
    const sql = "SELECT 'a.b.' FROM hive.";
    expect(parseQualifiedPathParents(sql, sql.length)).toEqual(['hive']);
  });
});
