import { describe, expect, it } from 'vitest';
import {
  DATA_SYNC_V1_FAMILIES,
  isSyncTargetSupported,
  normalizeSyncFamily,
  resolveSyncPairing,
} from '../syncPairing';

describe('syncPairing (Data Sync V1)', () => {
  it('same postgresql family is direct', () => {
    expect(resolveSyncPairing('postgresql', 'cloudberry')).toEqual({
      path: 'direct',
      supported: true,
      family: 'postgresql',
    });
  });

  it('mysql and mariadb are the same V1 family', () => {
    expect(resolveSyncPairing('mysql', 'mariadb')).toEqual({
      path: 'direct',
      supported: true,
      family: 'mysql',
    });
  });

  it('cross sql dialect is Transfer (IR path, not selectable)', () => {
    expect(resolveSyncPairing('postgresql', 'mysql')).toMatchObject({
      path: 'ir',
      supported: false,
    });
    expect(resolveSyncPairing('postgresql', 'mysql').reason).toMatch(/Transfer/);
  });

  it('cross category is unsupported', () => {
    expect(resolveSyncPairing('postgresql', 'mongodb').supported).toBe(false);
    expect(resolveSyncPairing('mongodb', 'redis').supported).toBe(false);
  });

  it('same kv/document types are not V1 Data Sync', () => {
    expect(resolveSyncPairing('redis', 'redis')).toMatchObject({
      path: 'direct',
      supported: false,
      family: 'redis',
    });
    expect(resolveSyncPairing('mongodb', 'mongodb').supported).toBe(false);
  });

  it('sqlite same-family is not V1', () => {
    expect(resolveSyncPairing('sqlite', 'sqlite').supported).toBe(false);
  });

  it('normalizeSyncFamily maps wire aliases', () => {
    expect(normalizeSyncFamily('mariadb')).toBe('mysql');
    expect(normalizeSyncFamily('questdb')).toBe('postgresql');
  });

  it('isSyncTargetSupported only allows V1 families', () => {
    expect(isSyncTargetSupported('postgresql', 'mysql')).toBe(false);
    expect(isSyncTargetSupported('postgresql', 'redis')).toBe(false);
    expect(isSyncTargetSupported('mysql', 'mariadb')).toBe(true);
    expect(DATA_SYNC_V1_FAMILIES).toEqual(['mysql', 'postgresql']);
  });
});
