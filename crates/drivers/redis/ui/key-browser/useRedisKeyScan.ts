import { useCallback, useEffect, useState } from 'react';
import { invokeScanKeys } from '../shared/redisInvoke';
import type { KeyEntry } from '@datazen/driver-sdk';

const PAGE_SIZE = 200;

export interface UseRedisKeyScanOptions {
  dbSessionId: string;
  dbIndex: number;
  enabled: boolean;
}

/**
 * Flat key list of the selected database (`scan_keys`), kept beside — not
 * inside — the hierarchical tree: the tree drives `list_children` per prefix
 * ({@link useKeyTree}), while this list is what "select all loaded", the value
 * search scope and the console completion feed on.
 *
 * `appliedPattern` is the pattern the current cursor belongs to. `searchPattern`
 * is only what the user has typed, so continuing an old cursor with it (the bug
 * this separates) would silently re-scan a different pattern mid-page.
 */
export function useRedisKeyScan({ dbSessionId, dbIndex, enabled }: UseRedisKeyScanOptions) {
  const [keys, setKeys] = useState<KeyEntry[]>([]);
  const [cursor, setCursor] = useState(0);
  const [dbSize, setDbSize] = useState(0);
  const [keysLoading, setKeysLoading] = useState(false);
  const [searchPattern, setSearchPattern] = useState('*');
  const [appliedPattern, setAppliedPattern] = useState('*');
  const [keyTypeFilter, setKeyTypeFilter] = useState('all');
  const [withMemory, setWithMemory] = useState(false);
  const [noTtlOnly, setNoTtlOnly] = useState(false);

  const loadKeys = useCallback(
    async (idx: number, pattern: string, cur: number, reset: boolean) => {
      setKeysLoading(true);
      try {
        const result = await invokeScanKeys(dbSessionId, idx, pattern || '*', cur, PAGE_SIZE, {
          keyType: keyTypeFilter,
          withMemory,
          noTtlOnly,
        });
        if (reset) {
          setKeys(result.keys);
        } else {
          setKeys((prev) => [...prev, ...result.keys]);
        }
        setCursor(result.cursor);
        setDbSize(result.dbSize);
      } catch (e) {
        console.error('scan_keys failed:', e);
      } finally {
        setKeysLoading(false);
      }
    },
    [dbSessionId, keyTypeFilter, withMemory, noTtlOnly],
  );

  /** Forget everything about the current db (db switch). */
  const resetSelectionState = useCallback(() => {
    setKeys([]);
    setCursor(0);
    setDbSize(0);
    setAppliedPattern('*');
  }, []);

  /** Restart the scan from cursor 0, optionally with a freshly resolved pattern. */
  const refresh = useCallback(
    (pattern?: string) => {
      const p = pattern ?? appliedPattern;
      setAppliedPattern(p);
      setKeys([]);
      setCursor(0);
      void loadKeys(dbIndex, p, 0, true);
    },
    [dbIndex, appliedPattern, loadKeys],
  );

  const loadMore = useCallback(() => {
    if (cursor !== 0) {
      void loadKeys(dbIndex, appliedPattern, cursor, false);
    }
  }, [dbIndex, appliedPattern, cursor, loadKeys]);

  /** Apply the search row: resolve pattern, restart scan from zero. */
  const search = useCallback(
    (pattern?: string) => {
      refresh(pattern ?? searchPattern);
    },
    [refresh, searchPattern],
  );

  // Re-scan when type filter, memory option, or no-expiry filter changes
  useEffect(() => {
    if (!enabled) return;
    setKeys([]);
    setCursor(0);
    void loadKeys(dbIndex, appliedPattern, 0, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only when filter/memory/no-ttl toggles
  }, [keyTypeFilter, withMemory, noTtlOnly]);

  return {
    keys,
    setKeys,
    cursor,
    dbSize,
    keysLoading,
    searchPattern,
    setSearchPattern,
    appliedPattern,
    keyTypeFilter,
    setKeyTypeFilter,
    withMemory,
    setWithMemory,
    noTtlOnly,
    setNoTtlOnly,
    loadKeys,
    resetSelectionState,
    refresh,
    loadMore,
    search,
  };
}

/** Everything 屏 B reads off the flat scan — passed whole to `KeyTreePane`. */
export type KeyScanApi = ReturnType<typeof useRedisKeyScan>;
