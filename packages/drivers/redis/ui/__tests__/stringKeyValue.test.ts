import { describe, expect, it } from 'vitest';
import {
  initialStringEditorValue,
  looksLikeJsonText,
  tryPrettyJson,
  unwrapStringKeyValue,
} from '../stringKeyValue';

describe('unwrapStringKeyValue', () => {
  it('unwraps { value } payload from get_key_detail', () => {
    expect(
      unwrapStringKeyValue({ value: '{"name":"张三","level":"vip"}' }),
    ).toBe('{"name":"张三","level":"vip"}');
  });

  it('passes through raw strings', () => {
    expect(unwrapStringKeyValue('hello')).toBe('hello');
  });
});

describe('tryPrettyJson / looksLikeJsonText', () => {
  it('pretty-prints objects and arrays', () => {
    expect(looksLikeJsonText('{"a":1}')).toBe(true);
    expect(tryPrettyJson('{"name":"张三","level":"vip"}')).toBe(
      '{\n  "name": "张三",\n  "level": "vip"\n}',
    );
    expect(tryPrettyJson('[1,2]')).toBe('[\n  1,\n  2\n]');
  });

  it('returns null for plain text and invalid JSON', () => {
    expect(looksLikeJsonText('hello')).toBe(false);
    expect(tryPrettyJson('hello')).toBeNull();
    expect(tryPrettyJson('{not json')).toBeNull();
    expect(tryPrettyJson('42')).toBeNull();
  });
});

describe('initialStringEditorValue', () => {
  it('pretty-prints JSON stored in the string wrapper', () => {
    expect(
      initialStringEditorValue({ value: '{"name":"张三","level":"vip"}' }),
    ).toBe('{\n  "name": "张三",\n  "level": "vip"\n}');
  });

  it('keeps non-JSON strings as-is', () => {
    expect(initialStringEditorValue({ value: 'plain' })).toBe('plain');
  });
});
