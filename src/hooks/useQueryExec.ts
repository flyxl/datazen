import { useShallow } from 'zustand/react/shallow';
import { usePanelStore, type QueryExecState, EMPTY_QUERY_EXEC } from '../stores/panelStore';

export function useQueryExec(panelId: string): QueryExecState {
  return usePanelStore(useShallow((s) => s.queryExec.get(panelId) ?? EMPTY_QUERY_EXEC));
}

export function useQueryExecField<K extends keyof QueryExecState>(
  panelId: string,
  field: K,
): QueryExecState[K] {
  return usePanelStore((s) => (s.queryExec.get(panelId) ?? EMPTY_QUERY_EXEC)[field]);
}
