import { describe, expect, it } from 'vitest';
import { parseSqlParamOccurrences } from '../sql-editor/bindParams';

describe('extension-point SQL bind parameter parser', () => {
  it('leaves dialect scope filtering to the policy strategy', () => {
    expect(
      parseSqlParamOccurrences('DECLARE @value INT; SELECT @value', { enableAt: true }).map(
        (occurrence) => occurrence.name,
      ),
    ).toEqual(['value', 'value']);
  });
});
