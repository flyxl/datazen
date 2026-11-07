/**
 * 屏 A → 屏 B key jump bridge for the Redis workbench.
 *
 * Provides `selectKey` — an imperative handle that switches databases and
 * selects a key — plus a one-shot effect that consumes dialog-related
 * pending actions (import/export, new-key) from the overview jump bridge.
 *
 * Split out of `RedisWorkbench.tsx` (D-0 composition) to keep the workbench
 * under the soft file-size budget.
 */
import { useCallback, useEffect, useRef } from 'react';
import type { RedisPendingAction } from '../overview/overviewNavigation';
import type { KeyDetailState } from './useKeyDetailState';

interface UseKeyJumpArgs {
  dbIndex: number;
  selectedDb: string | null;
  handleSelectDb: (db: string) => Promise<void>;
  detail: KeyDetailState;
  scanKeys: Array<{ key: string }>;
  scanKeysLoading: boolean;
  pendingAction?: RedisPendingAction;
  overlays: {
    setImportExportOpen: (open: boolean) => void;
    setCreateOpen: (open: boolean) => void;
  };
}

export interface KeyJumpHandle {
  selectKey: (key: string, targetDbIndex?: number) => void;
}

/**
 * Consume dialog-related pending actions from the overview jump bridge.
 * Called once on mount; subsequent renders are no-ops.
 */
export function consumePendingDialogs(
  action: RedisPendingAction | undefined,
  overlays: UseKeyJumpArgs['overlays'],
): void {
  if (!action) return;
  if (action.openImportExport) {
    overlays.setImportExportOpen(true);
  }
  if (action.openNewKey) {
    overlays.setCreateOpen(true);
  }
}

export function useKeyJump({
  dbIndex,
  selectedDb,
  handleSelectDb,
  detail,
  scanKeys,
  scanKeysLoading,
  pendingAction,
  overlays,
}: UseKeyJumpArgs): KeyJumpHandle {
  // ── Key selection via the 屏 A → 屏 B jump bridge ──

  const pendingKeyRef = useRef<{ key: string; dbIndex: number } | null>(null);

  const selectKey = useCallback(
    (key: string, targetDbIndex?: number) => {
      const targetIdx = targetDbIndex ?? dbIndex;
      const targetDb = `db${targetIdx}`;

      if (targetIdx !== dbIndex || selectedDb !== targetDb) {
        // Store the pending key; the scan effect will pick it up after the
        // db switch triggers a fresh key load.
        pendingKeyRef.current = { key, dbIndex: targetIdx };
        void handleSelectDb(targetDb);
      } else {
        // Same database — select directly.
        void detail.selectKey(key);
      }
    },
    [dbIndex, selectedDb, handleSelectDb, detail.selectKey],
  );

  // When a db switch completes and keys arrive, select the pending key.
  useEffect(() => {
    const pending = pendingKeyRef.current;
    if (!pending || scanKeysLoading) return;
    if (pending.dbIndex === dbIndex && scanKeys.some((k) => k.key === pending.key)) {
      pendingKeyRef.current = null;
      void detail.selectKey(pending.key);
    }
  }, [dbIndex, scanKeys, scanKeysLoading, detail.selectKey]);

  // ── Dialog-related pending actions (one-shot per action) ──

  const pendingActionRef = useRef(pendingAction);
  // Sync prop → ref so that updatePanel on an existing panel delivers new
  // pending actions that arrived after mount.
  useEffect(() => {
    if (pendingAction) pendingActionRef.current = pendingAction;
  }, [pendingAction]);
  useEffect(() => {
    consumePendingDialogs(pendingActionRef.current, overlays);
    pendingActionRef.current = undefined;
  }, [pendingAction]); // eslint-disable-line react-hooks/exhaustive-deps -- one-shot per action

  return { selectKey };
}
