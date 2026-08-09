import { describe, expect, it } from 'vitest';
import {
  hasRedisJson,
  isJsonKeyType,
  looksLikeJsonModuleDetail,
} from '../hasRedisJson';

describe('hasRedisJson', () => {
  it('detects ReJSON and RedisJSON module names', () => {
    expect(hasRedisJson(['ReJSON'])).toBe(true);
    expect(hasRedisJson(['redisjson'])).toBe(true);
    expect(hasRedisJson(['RedisJSON', 'search'])).toBe(true);
  });

  it('returns false when module list is empty or unrelated', () => {
    expect(hasRedisJson([])).toBe(false);
    expect(hasRedisJson(['search', 'timeseries'])).toBe(false);
  });
});

describe('isJsonKeyType', () => {
  it('detects ReJSON-RL and json types', () => {
    expect(isJsonKeyType('ReJSON-RL')).toBe(true);
    expect(isJsonKeyType('json')).toBe(true);
    expect(isJsonKeyType('string')).toBe(false);
  });
});

describe('looksLikeJsonModuleDetail', () => {
  it('detects module type from unsupported value bucket', () => {
    expect(
      looksLikeJsonModuleDetail({
        keyType: 'ReJSON-RL',
        value: { raw: '(unsupported or module type) ReJSON-RL' },
      }),
    ).toBe(true);
    expect(
      looksLikeJsonModuleDetail({
        keyType: 'module',
        value: { raw: '(unsupported or module type) ReJSON-RL' },
      }),
    ).toBe(true);
    expect(
      looksLikeJsonModuleDetail({
        keyType: 'string',
        value: { value: 'hello' },
      }),
    ).toBe(false);
  });
});
