import { describe, expect, it } from 'vitest';
import type { SqlParameterOccurrence } from '@datazen/driver-sdk';
import { findDeclaredAtVars, sqlserverSqlParameterStrategy } from '../sqlParameters';

function atOccurrences(sql: string, names: string[]): SqlParameterOccurrence[] {
  const occurrences: SqlParameterOccurrence[] = [];
  let searchFrom = 0;
  for (const name of names) {
    const token = `@${name}`;
    const from = sql.indexOf(token, searchFrom);
    if (from < 0) throw new Error(`Missing test token ${token}`);
    const to = from + token.length;
    occurrences.push({
      from,
      to,
      id: `named:${name}`,
      token,
      syntax: 'at',
      name,
    });
    searchFrom = to;
  }
  return occurrences;
}

describe('SQL Server SQL parameter strategy', () => {
  it('excludes T-SQL function signature parameters only within the routine', () => {
    const sql = `CREATE FUNCTION [dbo].[fn_NormalizeCode]
(
    @value NVARCHAR(64)
)
RETURNS NVARCHAR(64)
AS
BEGIN
    RETURN UPPER(LTRIM(RTRIM(@value)));
END;
SELECT @value;`;
    const candidates = atOccurrences(sql, ['value', 'value', 'value']);

    expect(findDeclaredAtVars(sql)).toEqual(new Set(['value']));
    expect(sqlserverSqlParameterStrategy.filterOccurrences?.(sql, candidates)).toEqual([
      candidates[2],
    ]);
  });

  it('excludes DECLARE and SET variables while preserving unrelated bind names', () => {
    const sql = 'DECLARE @limit INT; SET @count = 0; SELECT @limit, @count, @filter;';
    const candidates = atOccurrences(sql, ['limit', 'count', 'limit', 'count', 'filter']);

    expect(findDeclaredAtVars(sql)).toEqual(new Set(['limit', 'count']));
    expect(sqlserverSqlParameterStrategy.filterOccurrences?.(sql, candidates)).toEqual([
      candidates[4],
    ]);
  });

  it('keeps an ordinary @name placeholder enabled', () => {
    const sql = 'SELECT @value';
    const candidates = atOccurrences(sql, ['value']);
    expect(sqlserverSqlParameterStrategy.filterOccurrences?.(sql, candidates)).toEqual(candidates);
  });
});
