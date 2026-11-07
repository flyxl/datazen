import { useCallback, useState } from 'react';

/**
 * Checkbox selection set of the key tree (屏 B left column).
 *
 * Owns only *which* keys are checked; what an action does with them stays with
 * the dialogs that own it. Carved out of `RedisWorkbench.tsx` (D-0) because
 * R1's header action group (select-all / clear) and I-9's `⌘A`/`Esc` both need
 * the same mutators.
 *
 * Exit transitions are explicit: every path that drops the mounted detail
 * (db switch, refresh, key deletion) also calls {@link clearSelection}, so a
 * checked-but-invisible key can never linger in the batch payload.
 */

export interface KeySelection {
  selectedKeys: Set<string>;
  selectionCount: number;
  toggleKey: (key: string, checked: boolean) => void;
  toggleKeys: (keys: string[], checked: boolean) => void;
  /** Replace the set with exactly `keys` (header "select all loaded"). */
  selectMany: (keys: string[]) => void;
  clearSelection: () => void;
  /** Drop the given keys from the set (I-8: only the ones that succeeded). */
  removeKeys: (keys: string[]) => void;
  /** Functional update — the seam `KeyWorkbenchDialogs` uses for rename/delete. */
  update: (updater: (prev: Set<string>) => Set<string>) => void;
  has: (key: string) => boolean;
}

export function useKeySelection(): KeySelection {
  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(() => new Set<string>());

  const toggleKey = useCallback((key: string, checked: boolean) => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      if (checked) next.add(key);
      else next.delete(key);
      return next;
    });
  }, []);

  const toggleKeys = useCallback((keysToToggle: string[], checked: boolean) => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      for (const key of keysToToggle) {
        if (checked) next.add(key);
        else next.delete(key);
      }
      return next;
    });
  }, []);

  const selectMany = useCallback((keys: string[]) => {
    setSelectedKeys(new Set(keys));
  }, []);

  const clearSelection = useCallback(() => {
    setSelectedKeys((prev) => (prev.size === 0 ? prev : new Set<string>()));
  }, []);

  const removeKeys = useCallback((keys: string[]) => {
    setSelectedKeys((prev) => {
      if (prev.size === 0) return prev;
      const next = new Set(prev);
      for (const key of keys) next.delete(key);
      return next.size === prev.size ? prev : next;
    });
  }, []);

  const has = useCallback((key: string) => selectedKeys.has(key), [selectedKeys]);

  const update = useCallback((updater: (prev: Set<string>) => Set<string>) => {
    setSelectedKeys((prev) => updater(prev));
  }, []);

  return {
    selectedKeys,
    selectionCount: selectedKeys.size,
    toggleKey,
    toggleKeys,
    selectMany,
    clearSelection,
    removeKeys,
    update,
    has,
  };
}
