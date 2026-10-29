import { useShallow } from 'zustand/react/shallow';
import { usePanelStore, type QueryExecState, EMPTY_QUERY_EXEC } from '../stores/panelStore';
import { paneKey } from '../stores/paneKeys';

/**
 * Execution state of one pane. `paneId` is optional and defaults to the panel's
 * own (pre-split) pane, whose key is the bare panel id — so pre-split callers
 * read exactly the entry they read before.
 */
export function useQueryExec(panelId: string, paneId?: string): QueryExecState {
  const key = paneKey(panelId, paneId);
  return usePanelStore(useShallow((s) => s.queryExec.get(key) ?? EMPTY_QUERY_EXEC));
}

export function useQueryExecField<K extends keyof QueryExecState>(
  panelId: string,
  field: K,
  paneId?: string,
): QueryExecState[K] {
  const key = paneKey(panelId, paneId);
  return usePanelStore((s) => (s.queryExec.get(key) ?? EMPTY_QUERY_EXEC)[field]);
}
