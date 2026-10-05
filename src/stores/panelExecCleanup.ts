import { queryCommands } from '../commands/query';
import { getCancelCapability } from '../lib/queryExecutionViewModel';
import { getQueryPanelSessionRegistry } from '../lib/session/queryPanelSessionRoot';
import { useActiveConnectionStore } from './activeConnectionStore';
import { paneKey, paneKeysOfPanel } from './paneKeys';
import type { QueryExecState } from './queryExecActions';
import type { Panel } from './panelTypes';

/**
 * Cancel + cleanup of `queryExec` entries, one pane at a time or a whole tab at
 * a time.
 *
 * Both live outside `panelStore` on purpose: closing a tab must drop **every**
 * pane it owns, and that is precisely the step that is easy to get half-right
 * once a panel can hold several panes. Keeping it here gives the rule one
 * obvious home and one obvious test.
 */

/** Best-effort cancel of a pane's still-running execution. Never throws. */
function cancelPaneExec(panel: Panel, exec: QueryExecState | undefined): void {
  if (!exec?.running || !exec.executionId) return;
  const capabilities =
    useActiveConnectionStore.getState().connections[panel.connectionId]?.capabilities;
  if (getCancelCapability(capabilities) !== 'supported') return;
  queryCommands.cancelQuery(panel.dbSessionId, exec.executionId).catch(() => {});
}

/**
 * Drop a single pane: cancel its running execution, then delete its state.
 * Returns the same map when the pane has no entry, so no needless re-render.
 */
export function cancelAndCleanupPaneExec(
  panel: Panel,
  paneId: string,
  currentExec: Map<string, QueryExecState>,
): Map<string, QueryExecState> {
  const key = paneKey(panel.id, paneId);
  if (!currentExec.has(key)) return currentExec;
  cancelPaneExec(panel, currentExec.get(key));
  // 关闭旅程：同一个 pane key 的编辑器会话随 pane 一并关闭（fire-and-forget）。
  void getQueryPanelSessionRegistry().remove(key).catch(() => {});
  const nextExec = new Map(currentExec);
  nextExec.delete(key);
  return nextExec;
}

/**
 * Drop `panelsToRemove` entirely: every pane of every removed query panel is
 * cancelled if still running and deleted, so no exec state — and no SQL or
 * result set — survives the tab that owned it. Non-query panels own no exec
 * state and are skipped.
 */
export function cancelAndCleanupExec(
  panelsToRemove: Panel[],
  currentExec: Map<string, QueryExecState>,
): Map<string, QueryExecState> {
  const nextExec = new Map(currentExec);
  for (const panel of panelsToRemove) {
    if (panel.type !== 'query') continue;
    for (const key of paneKeysOfPanel(currentExec, panel.id)) {
      cancelPaneExec(panel, currentExec.get(key));
      void getQueryPanelSessionRegistry().remove(key).catch(() => {});
      nextExec.delete(key);
    }
  }
  return nextExec;
}
