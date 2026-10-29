import { describe, expect, it } from 'vitest';
import { parseQualifiedPathParents, resolveEnsureSegments } from '../sqlPathPrefix';

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

describe('resolveEnsureSegments', () => {
  it('uses current database for unqualified names', () => {
    expect(
      resolveEnsureSegments([], {
        currentDatabase: 'app',
        knownRoots: ['app', 'other'],
        useCurrentDatabaseRoot: true,
      }),
    ).toEqual(['app']);
  });

  it('keeps an explicit database prefix', () => {
    expect(
      resolveEnsureSegments(['other'], {
        currentDatabase: 'app',
        knownRoots: ['app', 'other'],
        useCurrentDatabaseRoot: true,
      }),
    ).toEqual(['other']);
  });

  it('prepends current database when first segment is not a known root', () => {
    expect(
      resolveEnsureSegments(['hive', 'snap'], {
        currentDatabase: 'presto',
        knownRoots: ['presto'],
        useCurrentDatabaseRoot: true,
      }),
    ).toEqual(['presto', 'hive', 'snap']);
  });

  it('does not invent a database when useCurrentDatabaseRoot is false', () => {
    expect(
      resolveEnsureSegments([], {
        currentDatabase: 'app',
        knownRoots: ['app'],
        useCurrentDatabaseRoot: false,
      }),
    ).toEqual([]);
  });
});
