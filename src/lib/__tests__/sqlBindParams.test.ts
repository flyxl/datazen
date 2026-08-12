import { describe, expect, it } from 'vitest';
import { coerceParamValue, paramsToPayload, parseSqlParams } from '../sqlBindParams';

describe('parseSqlParams', () => {
  it('finds named and positional placeholders', () => {
    const params = parseSqlParams('SELECT * FROM t WHERE id = :uid AND n = $1');
    expect(params).toEqual([
      { name: 'uid', kind: 'named' },
      { name: '1', kind: 'positional' },
    ]);
  });

  it('ignores placeholders inside strings', () => {
    expect(parseSqlParams("SELECT ':uid' WHERE x = :uid")).toEqual([
      { name: 'uid', kind: 'named' },
    ]);
  });

  it('ignores placeholders in comments and backticks', () => {
    expect(parseSqlParams('-- :skip\nSELECT :keep /* $9 */')).toEqual([
      { name: 'keep', kind: 'named' },
    ]);
    expect(parseSqlParams('SELECT `:hid` FROM t WHERE x = :hid')).toEqual([
      { name: 'hid', kind: 'named' },
    ]);
  });

  it('dedupes repeated names', () => {
    expect(parseSqlParams('SELECT :id, :id')).toEqual([{ name: 'id', kind: 'named' }]);
  });

  it('skips colon without an identifier', () => {
    expect(parseSqlParams('SELECT 1::int, :ok')).toEqual([{ name: 'ok', kind: 'named' }]);
  });
});

describe('coerceParamValue', () => {
  it('coerces numbers, bools, and null', () => {
    expect(coerceParamValue('42')).toBe(42);
    expect(coerceParamValue('-3.5')).toBe(-3.5);
    expect(coerceParamValue('true')).toBe(true);
    expect(coerceParamValue('FALSE')).toBe(false);
    expect(coerceParamValue('null')).toBeNull();
    expect(coerceParamValue('')).toBeNull();
    expect(coerceParamValue("O'Brien")).toBe("O'Brien");
  });
});

describe('paramsToPayload', () => {
  it('maps parsed params through coerce', () => {
    const payload = paramsToPayload(
      [
        { name: 'id', kind: 'named' },
        { name: 'flag', kind: 'named' },
      ],
      { id: '7', flag: 'false' },
    );
    expect(payload).toEqual({ id: 7, flag: false });
  });

  it('uses empty string when a value is missing', () => {
    expect(paramsToPayload([{ name: 'x', kind: 'positional' }], {})).toEqual({ x: null });
  });
});
