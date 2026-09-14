import { useCallback, useEffect, useState } from 'react';
import { invokeScanKeys } from './redisInvoke';
import type { KeyEntry } from '../../../../src/types';
import type { KeyBrowserViewMode } from './KeyBrowserControls';

const PAGE_SIZE = 200;

export interface UseRedisKeyScanOptions {
  dbSessionId: string;
  dbIndex: number;
  enabled: boolean;
}

export function useRedisKeyScan({ dbSessionId, dbIndex, enabled }: UseRedisKeyScanOptions) {
  const [keys, setKeys] = useState<KeyEntry[]>([]);
  const [cursor, setCursor] = useState(0);
  const [dbSize, setDbSize] = useState(0);
  const [keysLoading, setKeysLoading] = useState(false);
  const [searchPattern, setSearchPattern] = useState('*');
  const [keyTypeFilter, setKeyTypeFilter] = useState('all');
  const [viewMode, setViewMode] = useState<KeyBrowserViewMode>('flat');
  const [withMemory, setWithMemory] = useState(false);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());

  const loadKeys = useCallback(
    async (idx: number, pattern: string, cur: number, reset: boolean) => {
      setKeysLoading(true);
      try {
        const result = await invokeScanKeys(dbSessionId, idx, pattern || '*', cur, PAGE_SIZE, {
          keyType: keyTypeFilter,
          withMemory,
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
    [dbSessionId, keyTypeFilter, withMemory],
  );

  const resetSelectionState = useCallback(() => {
    setKeys([]);
    setCursor(0);
    setDbSize(0);
  }, []);

  const refresh = useCallback(
    (pattern?: string) => {
      const p = pattern ?? searchPattern;
      setKeys([]);
      setCursor(0);
      void loadKeys(dbIndex, p, 0, true);
    },
    [dbIndex, searchPattern, loadKeys],
  );

  const loadMore = useCallback(() => {
    if (cursor !== 0) {
      void loadKeys(dbIndex, searchPattern, cursor, false);
    }
  }, [dbIndex, searchPattern, cursor, loadKeys]);

  const search = useCallback(() => {
    setKeys([]);
    setCursor(0);
    void loadKeys(dbIndex, searchPattern, 0, true);
  }, [dbIndex, searchPattern, loadKeys]);

  // Re-scan when type filter or memory option changes
  useEffect(() => {
    if (!enabled) return;
    setKeys([]);
    setCursor(0);
    void loadKeys(dbIndex, searchPattern, 0, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only when filter/memory toggles
  }, [keyTypeFilter, withMemory]);

  const toggleFolder = useCallback((path: string) => {
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  return {
    keys,
    setKeys,
    cursor,
    dbSize,
    keysLoading,
    searchPattern,
    setSearchPattern,
    keyTypeFilter,
    setKeyTypeFilter,
    viewMode,
    setViewMode,
    withMemory,
    setWithMemory,
    expandedFolders,
    loadKeys,
    resetSelectionState,
    refresh,
    loadMore,
    search,
    toggleFolder,
  };
}
