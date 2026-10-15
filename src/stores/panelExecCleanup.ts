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
/**
 * Whether this panel's driver can cancel a running statement precisely.
 *
 * Shared so the close flow can tell "there is nothing to cancel" (the session
 * close is then the cleanup) from "a cancel was requested and never confirmed".
 */
export function canCancelPane(panel: Panel): boolean {
  const capabilities =
    useActiveConnectionStore.getState().connections[panel.connectionId]?.capabilities;
  return getCancelCapability(capabilities) === 'supported';
}

/** The slice of the panel store `requestPaneCancel` needs; keeps this module store-agnostic. */
export interface PaneCancelStore {
  cancelQuery: (panelId: string, paneId?: string) => Promise<void>;
  queryExec: Map<string, QueryExecState>;
}

/**
 * Ask the runtime to stop one pane's statement, reporting whether the request was
 * *delivered*: `cancelQuery` folds "no editor session to cancel through" and "the
 * runtime refused" into `cancelState: 'failed'`, and only an accepted request is
 * worth waiting for (see `runningExecutionClose`).
 */
export async function requestPaneCancel(
  getStore: () => PaneCancelStore,
  panelId: string,
  paneId: string,
): Promise<boolean> {
  await getStore().cancelQuery(panelId, paneId);
  const exec = getStore().queryExec.get(paneKey(panelId, paneId));
  return exec != null && (exec.cancelState === 'requested' || !exec.running);
}

function cancelPaneExec(panel: Panel, exec: QueryExecState | undefined): void {
  if (!exec?.running || !exec.executionId) return;
  if (!canCancelPane(panel)) return;
  queryCommands.cancelQuery(panel.dbSessionId, exec.executionId).catch(() => {});
}

/**
 * Every pane key of `panelId` that may own an editor session.
 *
 * The union of two sources, because neither alone is complete:
 *
 * * the `queryExec` table knows panes that *executed*;
 * * the session registry knows panes whose editor *mounted* — a pane opens its
 *   session before it is ever executed, so a never-executed pane holds a
 *   runtime session (and a host-side connected-editor permit) with no exec row.
 *
 * Enumerating only the exec table leaked exactly those sessions: the pane was
 * destroyed, its controller stayed in the registry, and the quota was charged
 * until the window died.
 */
export function sessionKeysOfPanel(
  currentExec: Map<string, QueryExecState>,
  panelId: string,
): string[] {
  const keys = new Set<string>(paneKeysOfPanel(currentExec, panelId));
  for (const key of getQueryPanelSessionRegistry().keysOfPanel(panelId)) keys.add(key);
  return [...keys];
}

/**
 * Drop a single pane: cancel its running execution, release its editor session
 * (executed or not), then delete its exec state. Returns the same map when the
 * pane has no exec entry, so no needless re-render.
 */
export function cancelAndCleanupPaneExec(
  panel: Panel,
  paneId: string,
  currentExec: Map<string, QueryExecState>,
): Map<string, QueryExecState> {
  const key = paneKey(panel.id, paneId);
  const exec = currentExec.get(key);
  if (exec) cancelPaneExec(panel, exec);
  // 关闭旅程：同一个 pane key 的编辑器会话随 pane 一并关闭（fire-and-forget）。
  // 这一句不能挂在「有没有 exec 行」上——会话在编辑器挂载时就开好了。
  void getQueryPanelSessionRegistry()
    .remove(key)
    .catch(() => {});
  if (!exec) return currentExec;
  const nextExec = new Map(currentExec);
  nextExec.delete(key);
  return nextExec;
}

/**
 * Drop `panelsToRemove` entirely: every pane of every removed panel is
 * cancelled if still running and deleted, so no exec state — and no SQL or
 * result set — survives the tab that owned it. Session release runs for every
 * panel type (a table view owns a session too); exec state only exists for
 * query panels.
 */
export function cancelAndCleanupExec(
  panelsToRemove: Panel[],
  currentExec: Map<string, QueryExecState>,
): Map<string, QueryExecState> {
  const nextExec = new Map(currentExec);
  for (const panel of panelsToRemove) {
    for (const key of sessionKeysOfPanel(currentExec, panel.id)) {
      if (panel.type === 'query') cancelPaneExec(panel, currentExec.get(key));
      void getQueryPanelSessionRegistry()
        .remove(key)
        .catch(() => {});
      nextExec.delete(key);
    }
  }
  return nextExec;
}
