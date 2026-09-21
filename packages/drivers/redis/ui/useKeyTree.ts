import { useCallback, useEffect, useRef, useState } from 'react';
import { invokeListChildren, type ChildEntry } from './redisInvoke';

const PAGE_SIZE = 500;

interface LevelState {
  children: ChildEntry[];
  cursor: number;
  done: boolean;
  loading: boolean;
}

const EMPTY_LEVEL: LevelState = { children: [], cursor: 0, done: false, loading: false };

export interface UseKeyTreeOptions {
  dbSessionId: string;
  dbIndex: number;
  enabled: boolean;
  noTtlOnly?: boolean;
  keyType?: string;
}

/**
 * Server-driven hierarchical key tree. Each expanded folder prefix owns one
 * `list_children` level (leaf keys + virtual folders) with its own SCAN cursor.
 */
export function useKeyTree({
  dbSessionId,
  dbIndex,
  enabled,
  noTtlOnly = false,
  keyType = 'all',
}: UseKeyTreeOptions) {
  const [levels, setLevels] = useState<Record<string, LevelState>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [rootLoading, setRootLoading] = useState(false);
  // Guards against out-of-order responses when a prefix refetches quickly.
  const reqSeq = useRef<Record<string, number>>({});

  const fetchLevel = useCallback(
    async (prefix: string, reset: boolean) => {
      const seq = (reqSeq.current[prefix] ?? 0) + 1;
      reqSeq.current[prefix] = seq;
      const isRoot = prefix === '';
      if (isRoot) setRootLoading(true);
      setLevels((prev) => ({
        ...prev,
        [prefix]: {
          ...(reset ? EMPTY_LEVEL : (prev[prefix] ?? EMPTY_LEVEL)),
          loading: true,
        },
      }));
      try {
        const cur = reset ? 0 : (levels[prefix]?.cursor ?? 0);
        const result = await invokeListChildren(dbSessionId, dbIndex, prefix, cur, PAGE_SIZE, {
          noTtlOnly,
          keyType,
        });
        if (reqSeq.current[prefix] !== seq) return; // stale response
        setLevels((prev) => {
          const existing = reset ? [] : (prev[prefix]?.children ?? []);
          return {
            ...prev,
            [prefix]: {
              children: [...existing, ...result.children],
              cursor: result.cursor,
              done: result.cursor === 0,
              loading: false,
            },
          };
        });
      } catch (e) {
        console.error('list_children failed:', e);
        if (reqSeq.current[prefix] === seq) {
          setLevels((prev) => ({
            ...prev,
            [prefix]: { ...(prev[prefix] ?? EMPTY_LEVEL), loading: false, done: true },
          }));
        }
      } finally {
        if (isRoot) setRootLoading(false);
      }
    },
    // levels intentionally excluded: we read the latest via setLevels closure guard
    [dbSessionId, dbIndex, noTtlOnly, keyType],
  );

  // Reset + load root when the tree becomes active or filters change.
  const loadRoot = useCallback(() => {
    reqSeq.current = {};
    setLevels({});
    void fetchLevel('', true);
  }, [fetchLevel]);

  useEffect(() => {
    if (!enabled) return;
    loadRoot();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- re-run only on filter/db changes
  }, [enabled, dbSessionId, dbIndex, noTtlOnly, keyType]);

  const toggleFolder = useCallback(
    (prefix: string) => {
      setExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(prefix)) {
          next.delete(prefix);
        } else {
          next.add(prefix);
          if (!levels[prefix]) void fetchLevel(prefix, true);
        }
        return next;
      });
    },
    [fetchLevel, levels],
  );

  const loadMore = useCallback(
    (prefix: string) => {
      const level = levels[prefix];
      if (level && !level.done && level.cursor !== 0) void fetchLevel(prefix, false);
    },
    [fetchLevel, levels],
  );

  /** Re-fetch every currently-expanded level (and root) in place. */
  const refresh = useCallback(() => {
    reqSeq.current = {};
    setLevels({});
    void fetchLevel('', true);
    for (const prefix of expanded) {
      if (prefix !== '') void fetchLevel(prefix, true);
    }
  }, [expanded, fetchLevel]);

  return {
    levels,
    expanded,
    rootLoading,
    toggleFolder,
    loadMore,
    refresh,
    loadRoot,
  };
}
