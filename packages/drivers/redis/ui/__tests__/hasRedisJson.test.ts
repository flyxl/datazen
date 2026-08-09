import { describe, expect, it } from 'vitest';
import { hasRedisJson } from '../hasRedisJson';

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
