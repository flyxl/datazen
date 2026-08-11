import { describe, expect, it } from 'vitest';
import { isSyncTargetSupported, normalizeSyncFamily, resolveSyncPairing } from '../syncPairing';

describe('syncPairing', () => {
  it('same postgresql family is direct', () => {
    expect(resolveSyncPairing('postgresql', 'cloudberry')).toEqual({
      path: 'direct',
      supported: true,
      family: 'postgresql',
    });
  });

  it('cross sql dialect uses ir', () => {
    expect(resolveSyncPairing('postgresql', 'mysql')).toEqual({
      path: 'ir',
      supported: true,
    });
  });

  it('cross category is unsupported', () => {
    expect(resolveSyncPairing('postgresql', 'mongodb').supported).toBe(false);
    expect(resolveSyncPairing('mongodb', 'redis').supported).toBe(false);
  });

  it('same kv/document types are direct', () => {
    expect(resolveSyncPairing('redis', 'redis').path).toBe('direct');
    expect(resolveSyncPairing('mongodb', 'mongodb').path).toBe('direct');
  });

  it('normalizeSyncFamily maps wire aliases', () => {
    expect(normalizeSyncFamily('mariadb')).toBe('mysql');
    expect(normalizeSyncFamily('questdb')).toBe('postgresql');
  });

  it('isSyncTargetSupported mirrors resolveSyncPairing', () => {
    expect(isSyncTargetSupported('postgresql', 'mysql')).toBe(true);
    expect(isSyncTargetSupported('postgresql', 'redis')).toBe(false);
  });
});
