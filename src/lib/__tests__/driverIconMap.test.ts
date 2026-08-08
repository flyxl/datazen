import { describe, expect, it } from 'vitest';
import { getDriverIconMap } from '../databaseTypes';

describe('getDriverIconMap', () => {
  it('exposes db.* keys from generated DRIVER_ICON_ENTRIES', () => {
    const map = getDriverIconMap();
    expect(map['db.postgresql']).toMatch(/postgresql/i);
    expect(map['db.mysql']).toBeTruthy();
    expect(map['db.mariadb']).toBeTruthy();
    expect(map['db.redis']).toBeTruthy();
    expect(map['db.mongodb']).toBeTruthy();
    expect(map['db.clickhouse']).toBeTruthy();
    // protocol reuse → parent icon file
    expect(map['db.doris']).toBeTruthy();
    expect(map['db.questdb']).toBeTruthy();
  });
});
