import { describe, expect, it } from 'vitest';
import { findDeclaredAtVars, parseSqlParams } from '../sql-editor/bindParams';

describe('extension-point SQL bind parameter parser', () => {
  it('treats T-SQL function signature parameters as local variables', () => {
    const sql = `CREATE FUNCTION [dbo].[fn_NormalizeCode]
(
    @value NVARCHAR(64)
)
RETURNS NVARCHAR(64)
AS
BEGIN
    RETURN UPPER(LTRIM(RTRIM(@value)));
END;`;

    expect(findDeclaredAtVars(sql)).toEqual(new Set(['value']));
    expect(parseSqlParams(sql)).toEqual([]);
    expect(parseSqlParams(`${sql}\nSELECT @value`)).toEqual([
      {
        name: 'value',
        kind: 'named',
        syntax: 'at',
        stableId: 'named:value',
      },
    ]);
  });

  it('keeps ordinary T-SQL @name placeholders enabled', () => {
    expect(parseSqlParams('SELECT @value')).toEqual([
      {
        name: 'value',
        kind: 'named',
        syntax: 'at',
        stableId: 'named:value',
      },
    ]);
  });
});
