import { beforeEach, describe, expect, it } from 'vitest';
import {
  BROWSE_HISTORY_MAX_ENTRIES,
  BROWSE_HISTORY_STORAGE_KEY,
  clearBrowseHistory,
  pushBrowseEntry,
  readBrowseHistory,
  relativeTimeParts,
  type StorageLike,
} from '../lib/redisBrowseHistory';

/**
 * 屏 A 第七区块的存储层（PRD §3.1 最近浏览键）。
 *
 * Pure localStorage module: no Redis round trip, no host import. The contract
 * under test is (a) connection scoping, (b) dedupe + recency ordering, (c) the
 * hard cap, (d) never throwing on garbage / disabled Web Storage, and (e) the
 * relative-time output being a *unit key + magnitude* rather than built copy.
 */

const KEY_A = 'user:1';
const KEY_B = 'user:2';
const KEY_C = 'session:9';

function fakeStorage(
  initial: Record<string, string> = {},
): StorageLike & { data: Record<string, string> } {
  const data: Record<string, string> = { ...initial };
  return {
    data,
    getItem: (key) => (key in data ? data[key] : null),
    setItem: (key, value) => {
      data[key] = value;
    },
    removeItem: (key) => {
      delete data[key];
    },
  };
}

function storedBuckets(storage: StorageLike): Record<string, unknown> {
  const raw = storage.getItem(BROWSE_HISTORY_STORAGE_KEY);
  return raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
}

const T0 = 1_700_000_000_000;

describe('readBrowseHistory', () => {
  it('is empty for an unknown connection and for an empty identity', () => {
    const storage = fakeStorage();
    pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 0 }, storage, T0);
    expect(readBrowseHistory('conn-2', storage)).toEqual([]);
    expect(readBrowseHistory('', storage)).toEqual([]);
  });

  it('degrades to an empty list instead of throwing on a corrupt payload', () => {
    const broken = fakeStorage({ [BROWSE_HISTORY_STORAGE_KEY]: '{not json' });
    expect(readBrowseHistory('conn-1', broken)).toEqual([]);

    const arrayShape = fakeStorage({ [BROWSE_HISTORY_STORAGE_KEY]: '[]' });
    expect(readBrowseHistory('conn-1', arrayShape)).toEqual([]);

    const garbageEntries = fakeStorage({
      [BROWSE_HISTORY_STORAGE_KEY]: JSON.stringify({
        'conn-1': [
          null,
          'nope',
          { key: '', dbIndex: 0, visitedAt: T0 },
          { key: KEY_B, dbIndex: 'x', visitedAt: T0 },
          { key: KEY_A, dbIndex: 0, visitedAt: 'soon' },
          { key: KEY_C, dbIndex: 3, visitedAt: T0, keyType: 'hash' },
        ],
      }),
    });
    expect(readBrowseHistory('conn-1', garbageEntries).map((entry) => entry.key)).toEqual([KEY_C]);
  });

  it('survives a storage that throws on access and a missing storage', () => {
    const hostile = {
      getItem: () => {
        throw new Error('SecurityError');
      },
      setItem: () => {
        throw new Error('SecurityError');
      },
      removeItem: () => {
        throw new Error('SecurityError');
      },
    } satisfies StorageLike;
    expect(readBrowseHistory('conn-1', hostile)).toEqual([]);
    expect(pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 0 }, hostile, T0)).toHaveLength(1);
    expect(() => clearBrowseHistory('conn-1', hostile)).not.toThrow();
    expect(readBrowseHistory('conn-1', null)).toEqual([]);
  });

  it('falls back to the ambient localStorage when no storage is injected', () => {
    globalThis.localStorage.clear();
    const storage = fakeStorage();
    pushBrowseEntry('conn-ambient', { key: KEY_A, dbIndex: 1 }, storage, T0);
    expect(readBrowseHistory('conn-ambient')).toEqual([]);
    expect(globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY)).toBeNull();

    pushBrowseEntry('conn-ambient', { key: KEY_B, dbIndex: 1 });
    expect(readBrowseHistory('conn-ambient').map((entry) => entry.key)).toEqual([KEY_B]);
    expect(globalThis.localStorage.getItem(BROWSE_HISTORY_STORAGE_KEY)).not.toBeNull();
    globalThis.localStorage.clear();
  });
});

describe('pushBrowseEntry', () => {
  let storage: StorageLike & { data: Record<string, string> };

  beforeEach(() => {
    storage = fakeStorage();
  });

  it('keeps the newest entry first and dedupes by db + key', () => {
    pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 0, keyType: 'string' }, storage, T0);
    pushBrowseEntry('conn-1', { key: KEY_B, dbIndex: 0 }, storage, T0 + 1000);
    const after = pushBrowseEntry(
      'conn-1',
      { key: KEY_A, dbIndex: 0, keyType: 'hash' },
      storage,
      T0 + 2000,
    );

    expect(after.map((entry) => entry.key)).toEqual([KEY_A, KEY_B]);
    expect(after[0].visitedAt).toBe(T0 + 2000);
    expect(after[0].keyType).toBe('hash');
    expect(after).toHaveLength(2);
  });

  it('treats the same key name in another logical database as a new entry', () => {
    pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 0 }, storage, T0);
    const after = pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 7 }, storage, T0 + 1000);
    expect(after.map((entry) => entry.dbIndex)).toEqual([7, 0]);
  });

  it('scopes buckets per connection', () => {
    pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 0 }, storage, T0);
    pushBrowseEntry('conn-2', { key: KEY_B, dbIndex: 0 }, storage, T0 + 500);
    expect(Object.keys(storedBuckets(storage)).sort()).toEqual(['conn-1', 'conn-2']);
    expect(readBrowseHistory('conn-1', storage).map((entry) => entry.key)).toEqual([KEY_A]);
    expect(readBrowseHistory('conn-2', storage).map((entry) => entry.key)).toEqual([KEY_B]);
  });

  it('caps the list at the declared length, dropping the oldest', () => {
    for (let i = 0; i < BROWSE_HISTORY_MAX_ENTRIES + 5; i += 1) {
      pushBrowseEntry('conn-1', { key: `k${i}`, dbIndex: 0 }, storage, T0 + i * 1000);
    }
    const after = readBrowseHistory('conn-1', storage);
    expect(after).toHaveLength(BROWSE_HISTORY_MAX_ENTRIES);
    expect(after[0].key).toBe(`k${BROWSE_HISTORY_MAX_ENTRIES + 4}`);
    expect(after.at(-1)?.key).toBe('k5');
  });

  it('normalises a nonsensical db index / timestamp instead of storing NaN', () => {
    const after = pushBrowseEntry(
      'conn-1',
      { key: KEY_A, dbIndex: Number.NaN },
      storage,
      Number.NaN,
    );
    expect(after[0].dbIndex).toBe(0);
    expect(Number.isFinite(after[0].visitedAt)).toBe(true);
  });

  it('ignores an empty key and records nothing', () => {
    const after = pushBrowseEntry('conn-1', { key: '', dbIndex: 0 }, storage, T0);
    expect(after).toEqual([]);
    expect(storage.data[BROWSE_HISTORY_STORAGE_KEY]).toBeUndefined();
  });
});

describe('clearBrowseHistory', () => {
  it('removes the storage slot once the last bucket is gone', () => {
    const storage = fakeStorage();
    pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 0 }, storage, T0);
    clearBrowseHistory('conn-1', storage);
    expect(readBrowseHistory('conn-1', storage)).toEqual([]);
    expect(storage.data[BROWSE_HISTORY_STORAGE_KEY]).toBeUndefined();
  });

  it('leaves other connections intact', () => {
    const storage = fakeStorage();
    pushBrowseEntry('conn-1', { key: KEY_A, dbIndex: 0 }, storage, T0);
    pushBrowseEntry('conn-2', { key: KEY_B, dbIndex: 0 }, storage, T0);
    clearBrowseHistory('conn-1', storage);
    expect(readBrowseHistory('conn-1', storage)).toEqual([]);
    expect(readBrowseHistory('conn-2', storage).map((entry) => entry.key)).toEqual([KEY_B]);
    expect(Object.keys(storedBuckets(storage))).toEqual(['conn-2']);
  });
});

describe('relativeTimeParts', () => {
  it('returns a unit key plus magnitude, never built copy', () => {
    expect(relativeTimeParts(T0 - 5000, T0)).toEqual({
      value: 5,
      unitKey: 'redis.overview.timeAgo.seconds',
    });
    expect(relativeTimeParts(T0 - 5 * 60_000, T0)).toEqual({
      value: 5,
      unitKey: 'redis.overview.timeAgo.minutes',
    });
    expect(relativeTimeParts(T0 - 3 * 3_600_000, T0)).toEqual({
      value: 3,
      unitKey: 'redis.overview.timeAgo.hours',
    });
    expect(relativeTimeParts(T0 - 2 * 86_400_000, T0)).toEqual({
      value: 2,
      unitKey: 'redis.overview.timeAgo.days',
    });
  });

  it('clamps a future timestamp to one second instead of a negative value', () => {
    expect(relativeTimeParts(T0 + 10 * 60_000, T0)).toEqual({
      value: 1,
      unitKey: 'redis.overview.timeAgo.seconds',
    });
  });

  it('truncates sub-minute magnitudes so the row does not flicker at 0', () => {
    expect(relativeTimeParts(T0 - 400, T0).value).toBe(1);
  });
});
