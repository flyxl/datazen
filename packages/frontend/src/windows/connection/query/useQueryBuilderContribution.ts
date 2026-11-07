import { useCallback, useSyncExternalStore } from 'react';
import { sqlEditorEnhancedEP, useExtension } from '@datazen/extension-points';

const getClosedSnapshot = (): string | null => null;
const subscribeNone = (): (() => void) => () => {};

/** Reads the currently installed Pro builder and subscribes to its visibility. */
export function useQueryBuilderContribution() {
  const contribution = useExtension(sqlEditorEnhancedEP).queryBuilder;
  const subscribe = useCallback(
    (listener: () => void) => contribution?.subscribe(listener) ?? subscribeNone(),
    [contribution],
  );
  const getSnapshot = useCallback(() => contribution?.getOpenPanelId() ?? null, [contribution]);
  const openPanelId = useSyncExternalStore(subscribe, getSnapshot, getClosedSnapshot);

  return { contribution, openPanelId };
}
