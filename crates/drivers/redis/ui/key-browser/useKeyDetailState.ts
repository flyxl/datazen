import { useCallback, useState } from 'react';
import { invokeGetKey } from '../shared/redisInvoke';
import { requestDraftLeave } from '../shared/draftGuard';
import type { KeyDetail } from '../shared/types';

/**
 * The mounted key detail of 屏 B's right column: which key is open, its loaded
 * `KeyDetail`, whether it is still fetching, and the editor's dirty draft flag.
 *
 * Split out of `RedisWorkbench.tsx` (D-0). The dirty invariant this hook keeps:
 * `editorDirty` can only be true while a detail with an unsaved draft is
 * mounted — every path that drops the detail goes through {@link clearDetail},
 * which resets the flag together with the key.
 *
 * {@link selectKey} is also the **I-1 draft-gate choke point** for every
 * selection move. The guard is not a caller concern: whether a move is dangerous
 * is a question only this hook can answer, because it owns the two values the
 * answer depends on (`selectedKey` and `keyDetail.key`). Three cases:
 *
 *  - **consistent same-key** (`key === selectedKey === keyDetail.key`): a plain
 *    in-place refetch — tree re-click, header refresh, post-write reload, dialog
 *    TTL/PERSIST refetch. Nothing is dropped, so there is nothing to ask and the
 *    refetch stays ask-free.
 *  - **cross-key** (`key !== selectedKey`): switching keys unmounts the editor
 *    and takes the draft with it ⇒ ask before touching any state.
 *  - **stale gap** (`key === selectedKey` but `keyDetail.key` differs, BUG-007):
 *    the two can disagree after a retarget/rename. This refetch is *not* the
 *    in-place one — it flips `loading`, remounts the editor and would silently
 *    wipe the draft ⇒ ask here too.
 *
 * Both asks happen **before** `setSelectedKey`, so a 继续编辑 answer leaves the
 * selection exactly where it was. That ordering is load-bearing (BUG-008): moving
 * the selection first split the displayed name from the key the editor would
 * write to, and the next 保存 silently wrote the OLD key.
 */

export interface KeyDetailState {
  selectedKey: string | null;
  keyDetail: KeyDetail | null;
  detailLoading: boolean;
  editorDirty: boolean;
  setEditorDirty: (dirty: boolean) => void;
  /**
   * Point the right column at `key`, loading its detail (keeps the draft flag).
   * Passes the I-1 leave gate first when the move would drop a live draft;
   * resolves without doing anything if the user answers 继续编辑.
   */
  selectKey: (key: string) => Promise<void>;
  /** Rename follow-up: keep the column mounted but re-point it at `newKey`. */
  retargetKey: (newKey: string) => void;
  /** Drop the mounted detail and its draft (also the unmount/db-switch path). */
  clearDetail: () => void;
  /**
   * {@link clearDetail} behind the I-1 leave gate — the panel-close and
   * dialog-side clear paths, which the *user* triggers while a draft is live.
   * A 继续编辑 answer cancels the close and the detail stays mounted.
   */
  clearDetailGuarded: () => void;
}

export function useKeyDetailState(dbSessionId: string, dbIndex: number): KeyDetailState {
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [keyDetail, setKeyDetail] = useState<KeyDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [editorDirty, setEditorDirty] = useState(false);

  const selectKey = useCallback(
    async (key: string) => {
      const sameKey = key === selectedKey;
      const consistent = sameKey && keyDetail?.key === key;
      // BUG-007: a same-key refetch across the stale gap is NOT the in-place one
      // below — it would remount the editor and wipe the draft.
      if (sameKey && !consistent) {
        if (!(await requestDraftLeave())) return;
      } else if (!sameKey) {
        // I-1 切键: the editor unmounts with the old key, draft included.
        if (!(await requestDraftLeave())) return;
      }
      /*
       * BUG-001 (in-place rule): refetching the detail that is ALREADY mounted
       * must not flip `detailLoading` — the loading branch replaces the editor,
       * and its unmount cleanup wipes the draft silently. Same-key refetches
       * stay in place; a fresh selection still reports loading exactly as before.
       */
      const inPlace = consistent;
      setSelectedKey(key);
      if (!inPlace) setDetailLoading(true);
      try {
        const detail = await invokeGetKey(dbSessionId, dbIndex, key);
        setKeyDetail(detail);
      } catch (e) {
        console.error('get_key failed:', e);
        // In-place failure keeps the mounted editor (and its draft) intact.
        if (!inPlace) setKeyDetail(null);
      } finally {
        if (!inPlace) setDetailLoading(false);
      }
    },
    [dbSessionId, dbIndex, selectedKey, keyDetail],
  );

  const retargetKey = useCallback((newKey: string) => {
    setSelectedKey(newKey);
  }, []);

  const clearDetail = useCallback(() => {
    setSelectedKey(null);
    setKeyDetail(null);
    setEditorDirty(false);
  }, []);

  const clearDetailGuarded = useCallback(() => {
    void (async () => {
      if (!(await requestDraftLeave())) return;
      clearDetail();
    })();
  }, [clearDetail]);

  return {
    selectedKey,
    keyDetail,
    detailLoading,
    editorDirty,
    setEditorDirty,
    selectKey,
    retargetKey,
    clearDetail,
    clearDetailGuarded,
  };
}
