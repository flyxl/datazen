import { describe, expect, it } from 'vitest';
import { getDriverIconMap } from '../databaseTypes';

describe('getDriverIconMap', () => {
  it('exposes db.* keys for built-in SQL/KV types', () => {
    const map = getDriverIconMap();
    expect(map['db.postgresql']).toMatch(/postgresql/i);
    expect(map['db.mysql']).toBeTruthy();
    expect(map['db.redis']).toBeTruthy();
  });
});
