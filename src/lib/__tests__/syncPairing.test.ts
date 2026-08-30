import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearSyncPairingCache,
  isSyncTargetSupported,
  normalizeSyncFamily,
  resolveSyncPairing,
} from '../syncPairing';

const classifyDataSyncPairMock = vi.fn();

vi.mock('../../commands/sync', () => ({
  syncCommands: {
    classifyDataSyncPair: (...args: unknown[]) => classifyDataSyncPairMock(...args),
  },
}));

function mockPairing(
  sourceType: string,
  targetType: string,
  view: {
    path: string;
    supported: boolean;
    family?: string | null;
    reason?: string | null;
  },
) {
  classifyDataSyncPairMock.mockImplementation(
    async (source: string, target: string) => {
      if (source === sourceType && target === targetType) return view;
      if (source === targetType && target === sourceType) return view;
      return { path: 'unsupported', supported: false, family: null, reason: null };
    },
  );
}

describe('syncPairing (Data Sync V1 via IPC)', () => {
  beforeEach(() => {
    clearSyncPairingCache();
    classifyDataSyncPairMock.mockReset();
  });

  it('same postgresql family is direct', async () => {
    mockPairing('postgresql', 'cloudberry', {
      path: 'direct',
      supported: true,
      family: 'postgresql',
    });
    await expect(resolveSyncPairing('postgresql', 'cloudberry')).resolves.toEqual({
      path: 'direct',
      supported: true,
      family: 'postgresql',
    });
    expect(classifyDataSyncPairMock).toHaveBeenCalledWith('postgresql', 'cloudberry');
  });

  it('mysql and mariadb are the same V1 family', async () => {
    mockPairing('mysql', 'mariadb', {
      path: 'direct',
      supported: true,
      family: 'mysql',
    });
    await expect(resolveSyncPairing('mysql', 'mariadb')).resolves.toEqual({
      path: 'direct',
      supported: true,
      family: 'mysql',
    });
  });

  it('cross sql dialect is Transfer (IR path, not selectable)', async () => {
    mockPairing('postgresql', 'mysql', {
      path: 'ir',
      supported: false,
      reason: 'heterogeneous pair postgresql → mysql is Data Transfer, not Data Synchronization',
    });
    await expect(resolveSyncPairing('postgresql', 'mysql')).resolves.toMatchObject({
      path: 'ir',
      supported: false,
    });
    expect((await resolveSyncPairing('postgresql', 'mysql')).reason).toMatch(/Transfer/);
  });

  it('cross category is unsupported', async () => {
    classifyDataSyncPairMock.mockImplementation(async (source: string, target: string) => {
      if (
        (source === 'postgresql' && target === 'mongodb') ||
        (source === 'mongodb' && target === 'redis')
      ) {
        return { path: 'unsupported', supported: false, family: null, reason: 'cross category' };
      }
      return { path: 'unsupported', supported: false };
    });
    expect(await isSyncTargetSupported('postgresql', 'mongodb')).toBe(false);
    expect(await isSyncTargetSupported('mongodb', 'redis')).toBe(false);
  });

  it('same kv/document types are not V1 Data Sync', async () => {
    classifyDataSyncPairMock.mockImplementation(async (source: string, target: string) => {
      if (source === 'redis' && target === 'redis') {
        return {
          path: 'direct',
          supported: false,
          family: 'redis',
          reason: "Data Synchronization for family 'redis' is not available in V1",
        };
      }
      if (source === 'mongodb' && target === 'mongodb') {
        return {
          path: 'direct',
          supported: false,
          family: 'mongodb',
          reason: "Data Synchronization for family 'mongodb' is not available in V1",
        };
      }
      return { path: 'unsupported', supported: false };
    });
    await expect(resolveSyncPairing('redis', 'redis')).resolves.toMatchObject({
      path: 'direct',
      supported: false,
      family: 'redis',
    });
    expect(await isSyncTargetSupported('mongodb', 'mongodb')).toBe(false);
  });

  it('sqlite same-family is not V1', async () => {
    mockPairing('sqlite', 'sqlite', {
      path: 'direct',
      supported: false,
      family: 'sqlite',
      reason: "Data Synchronization for family 'sqlite' is not available in V1",
    });
    expect(await isSyncTargetSupported('sqlite', 'sqlite')).toBe(false);
  });

  it('redis, mongodb, and kiwi boundaries delegate to backend IPC', async () => {
    classifyDataSyncPairMock.mockImplementation(async (source: string, target: string) => {
      if (source === 'redis' && target === 'mysql') {
        return { path: 'unsupported', supported: false, family: null, reason: 'cross category' };
      }
      if (source === 'kiwi' && target === 'postgresql') {
        return { path: 'unsupported', supported: false, family: null, reason: 'other category' };
      }
      return { path: 'unsupported', supported: false };
    });
    expect(await isSyncTargetSupported('redis', 'mysql')).toBe(false);
    expect(await isSyncTargetSupported('kiwi', 'postgresql')).toBe(false);
  });

  it('normalizeSyncFamily maps wire aliases (Transfer UI helper)', () => {
    expect(normalizeSyncFamily('mariadb')).toBe('mysql');
    expect(normalizeSyncFamily('questdb')).toBe('postgresql');
  });

  it('caches IPC results for repeated lookups', async () => {
    classifyDataSyncPairMock.mockResolvedValue({
      path: 'direct',
      supported: true,
      family: 'mysql',
    });
    await resolveSyncPairing('mysql', 'mariadb');
    await resolveSyncPairing('mysql', 'mariadb');
    expect(classifyDataSyncPairMock).toHaveBeenCalledTimes(1);
  });
});
