import { describe, expect, it, vi, beforeEach } from 'vitest';
import { emptyQueryExecState, type QueryExecState } from '../queryExecActions';
import { paneKey } from '../paneKeys';
import type { Panel } from '../panelTypes';

/**
 * Closing a query tab must drop the editor session of every pane it owns, not
 * only its `queryExec` entry: a leaked runtime session keeps the backend-side
 * connection alive and would resurrect a stale context on re-open.
 */

/**
 * Per-pane cancel, not per-panel: the runtime execution and the handle it runs
 * on both belong to the pane's own controller. The assertion records the pane
 * key the cancel was routed through, so a cancel addressed to the panel (the
 * legacy `cancel_query` + `panel.dbSessionId` shape) cannot pass as a cancel
 * addressed to the pane that was actually running.
 */
const controllerCancel = vi.hoisted(() => vi.fn((_key: string) => Promise.resolve(null)));
const controllers = vi.hoisted(() => new Map<string, { cancel: () => Promise<unknown> }>());

const registryRemove = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
/** pane keys the fake session registry reports per panel id. */
const registryKeys = vi.hoisted(() => new Map<string, string[]>());
vi.mock('../../lib/session/queryPanelSessionRoot', () => ({
  getQueryPanelSessionRegistry: () => ({
    remove: registryRemove,
    keysOfPanel: (panelId: string) => registryKeys.get(panelId) ?? [],
    peek: (key: string) => controllers.get(key),
  }),
}));

/** Register a live controller for `key`; its cancel is recorded by pane key. */
function withController(key: string): void {
  controllers.set(key, { cancel: () => controllerCancel(key) });
}

vi.mock('../activeConnectionStore', () => ({
  useActiveConnectionStore: {
    getState: () => ({
      connections: {
        'cfg-1': {
          capabilities: { supportsCancelQuery: true, supportsQueryExecutionCancel: true },
        },
        'cfg-quiet': { capabilities: { supportsCancelQuery: false } },
      },
    }),
  },
}));

const { cancelAndCleanupExec, cancelAndCleanupPaneExec } = await import('../panelExecCleanup');

function queryPanel(id: string, connectionId = 'cfg-1'): Panel {
  return {
    id,
    connectionId,
    dbSessionId: 'sess-1',
    connectionName: 'local',
    databaseType: 'postgresql',
    type: 'query',
    title: 'SQL',
    database: 'app',
    schema: 'public',
  } as Panel;
}

function runningExec(): QueryExecState {
  return { ...emptyQueryExecState(), running: true, executionId: 'exec-1' };
}

describe('[tester] query panel close drops the pane editor session', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    registryKeys.clear();
    controllers.clear();
    registryRemove.mockResolvedValue(undefined);
  });

  it('closes every pane session of a removed tab', () => {
    const panel = queryPanel('panel-1');
    const current = new Map<string, QueryExecState>([
      [paneKey(panel.id), runningExec()],
      [paneKey(panel.id, 'pane-b'), { ...runningExec(), executionId: 'exec-2' }],
    ]);

    withController(paneKey(panel.id));
    withController(paneKey(panel.id, 'pane-b'));

    const next = cancelAndCleanupExec([panel], current);

    expect(next.size).toBe(0);
    expect(registryRemove.mock.calls.map(([key]) => key)).toEqual([
      paneKey(panel.id),
      paneKey(panel.id, 'pane-b'),
    ]);
    expect(controllerCancel.mock.calls.map(([key]) => key)).toEqual([
      paneKey(panel.id),
      paneKey(panel.id, 'pane-b'),
    ]);
  });

  it('closes the session of a single detached pane and leaves its sibling alone', () => {
    const panel = queryPanel('panel-1');
    const sibling = paneKey(panel.id);
    const current = new Map<string, QueryExecState>([
      [sibling, runningExec()],
      [paneKey(panel.id, 'pane-b'), runningExec()],
    ]);

    withController(paneKey(panel.id, 'pane-b'));

    const next = cancelAndCleanupPaneExec(panel, 'pane-b', current);

    expect([...next.keys()]).toEqual([sibling]);
    expect(registryRemove).toHaveBeenCalledTimes(1);
    expect(registryRemove).toHaveBeenCalledWith(paneKey(panel.id, 'pane-b'));
    // The sibling pane's controller is never touched: only the pane being
    // dropped is asked to stop.
    expect(controllerCancel.mock.calls.map(([key]) => key)).toEqual([paneKey(panel.id, 'pane-b')]);
  });

  it('cancels through the pane controller, never through the panel session id', () => {
    // Regression guard for the migrated path: `sess-1` is the panel's legacy
    // connection session (the schema tree's), and an execution submitted on the
    // pane's runtime session can never be found there.
    const panel = queryPanel('panel-1');
    const key = paneKey(panel.id);
    withController(key);
    controllers.set('sess-1', { cancel: () => controllerCancel('sess-1') });
    const current = new Map<string, QueryExecState>([[key, runningExec()]]);

    cancelAndCleanupPaneExec(panel, 'main', current);

    expect(controllerCancel.mock.calls.map(([k]) => k)).toEqual([key]);
  });

  it('keeps a rejected session close from breaking the tab close', () => {
    registryRemove.mockRejectedValue(new Error('session already gone'));
    const panel = queryPanel('panel-1');
    const current = new Map<string, QueryExecState>([[paneKey(panel.id), runningExec()]]);

    expect(cancelAndCleanupExec([panel], current).size).toBe(0);
    expect(registryRemove).toHaveBeenCalledTimes(1);
  });

  it('releases sessions of every panel type but only cancels query panes', () => {
    const tablePanel = {
      id: 'panel-table',
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      connectionName: 'local',
      databaseType: 'postgresql',
      type: 'table',
      tableName: 'users',
      database: 'app',
      tableSchema: 'public',
      subTab: 'data',
    } as Panel;
    const quietPanel = queryPanel('panel-quiet', 'cfg-quiet');
    const current = new Map<string, QueryExecState>([
      [paneKey(tablePanel.id), runningExec()],
      [paneKey(quietPanel.id), runningExec()],
    ]);

    withController(paneKey(tablePanel.id));
    withController(paneKey(quietPanel.id));

    const next = cancelAndCleanupExec([tablePanel, quietPanel], current);

    expect(next.size).toBe(0);
    // The quiet panel has no cancel capability, but both sessions still close:
    // a table view owns a session too, and capability only gates cancellation.
    expect(registryRemove.mock.calls.map(([key]) => key).sort()).toEqual(
      [paneKey(tablePanel.id), paneKey(quietPanel.id)].sort(),
    );
    expect(controllerCancel).not.toHaveBeenCalled();
  });

  it('closes a never-executed pane session and returns the same map', () => {
    // The session exists from editor mount, long before any exec row: skipping
    // this close is what charged the connected-editor quota with one tab open.
    const panel = queryPanel('panel-1');
    const current = new Map<string, QueryExecState>();

    withController(paneKey(panel.id));

    expect(cancelAndCleanupPaneExec(panel, 'main', current)).toBe(current);
    expect(registryRemove).toHaveBeenCalledTimes(1);
    expect(registryRemove).toHaveBeenCalledWith(paneKey(panel.id));
    expect(controllerCancel).not.toHaveBeenCalled();
  });

  it('closes a session the exec table never heard of when the tab goes away', () => {
    const panel = queryPanel('panel-1');
    const lazy = paneKey(panel.id, 'pane-lazy');
    registryKeys.set(panel.id, [lazy]);
    const current = new Map<string, QueryExecState>();

    const next = cancelAndCleanupExec([panel], current);

    expect(next.size).toBe(0);
    expect(registryRemove.mock.calls.map(([key]) => key)).toEqual([lazy]);
  });

  it('closes each session once when both sources name the same pane', () => {
    const panel = queryPanel('panel-1');
    const key = paneKey(panel.id);
    registryKeys.set(panel.id, [key]);
    const current = new Map<string, QueryExecState>([[key, runningExec()]]);

    withController(key);

    expect(cancelAndCleanupExec([panel], current).size).toBe(0);
    expect(registryRemove.mock.calls.map(([k]) => k)).toEqual([key]);
    expect(controllerCancel).toHaveBeenCalledTimes(1);
    expect(controllerCancel.mock.calls.map(([k]) => k)).toEqual([key]);
  });
});
