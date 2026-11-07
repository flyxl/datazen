import { useEffect } from 'react';
import type { KvSlotState } from '@datazen/driver-sdk';

/**
 * Contract F-2 relay: publish this panel's selection + draft flag into the
 * host-owned `KvSlotState` atom, which the host-rendered KV slots (context bar /
 * status bar / key-props sidebar) read.
 *
 * Kept as a hook rather than inline effects (D-0) because the whole contract is
 * three rules over the same two atoms:
 *  - publishing is write-only and field-granular, so a slot that subscribes to
 *    `getSelectedKey()` re-renders on selection, not on every state change;
 *  - the dirty flag mirrors the mounted editor only — `StringEditor` publishes
 *    `false` when it unmounts and every path that drops the detail resets it, so
 *    `dirty === true` implies a draft is on screen;
 *  - exit transition: unmount / session swap clears both, so a later mount never
 *    inherits a key that is no longer rendered or a draft that no longer exists.
 */

export function useKvSlotRelay(
  kvSlotState: KvSlotState | undefined,
  dbSessionId: string,
  selectedKey: string | null,
  editorDirty: boolean,
): void {
  useEffect(() => {
    kvSlotState?.selectKey(selectedKey);
  }, [kvSlotState, selectedKey]);

  useEffect(() => {
    kvSlotState?.setDirty(editorDirty);
  }, [kvSlotState, editorDirty]);

  useEffect(() => {
    if (!kvSlotState) return;
    return () => {
      kvSlotState.selectKey(null);
      kvSlotState.setDirty(false);
    };
  }, [kvSlotState, dbSessionId]);
}
