import { describe, expect, it, vi, beforeEach } from 'vitest';
import { emptyQueryExecState, type QueryExecState } from '../queryExecActions';
import { paneKey } from '../paneKeys';
import type { Panel } from '../panelTypes';

/**
 * Closing a query tab must drop the editor session of every pane it owns, not
 * only its `queryExec` entry: a leaked runtime session keeps the backend-side
 * connection alive and would resurrect a stale context on re-open.
 */

const cancelQuery = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock('../../commands/query', () => ({
  queryCommands: { cancelQuery: (...args: unknown[]) => cancelQuery(...args) },
}));

const registryRemove = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock('../../lib/session/queryPanelSessionRoot', () => ({
  getQueryPanelSessionRegistry: () => ({ remove: registryRemove }),
}));

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
    registryRemove.mockResolvedValue(undefined);
  });

  it('closes every pane session of a removed tab', () => {
    const panel = queryPanel('panel-1');
    const current = new Map<string, QueryExecState>([
      [paneKey(panel.id), runningExec()],
      [paneKey(panel.id, 'pane-b'), { ...runningExec(), executionId: 'exec-2' }],
    ]);

    const next = cancelAndCleanupExec([panel], current);

    expect(next.size).toBe(0);
    expect(registryRemove.mock.calls.map(([key]) => key)).toEqual([
      paneKey(panel.id),
      paneKey(panel.id, 'pane-b'),
    ]);
    expect(cancelQuery.mock.calls.map(([, executionId]) => executionId)).toEqual([
      'exec-1',
      'exec-2',
    ]);
  });

  it('closes the session of a single detached pane and leaves its sibling alone', () => {
    const panel = queryPanel('panel-1');
    const sibling = paneKey(panel.id);
    const current = new Map<string, QueryExecState>([
      [sibling, runningExec()],
      [paneKey(panel.id, 'pane-b'), runningExec()],
    ]);

    const next = cancelAndCleanupPaneExec(panel, 'pane-b', current);

    expect([...next.keys()]).toEqual([sibling]);
    expect(registryRemove).toHaveBeenCalledTimes(1);
    expect(registryRemove).toHaveBeenCalledWith(paneKey(panel.id, 'pane-b'));
    expect(cancelQuery).toHaveBeenCalledTimes(1);
  });

  it('keeps a rejected session close from breaking the tab close', () => {
    registryRemove.mockRejectedValue(new Error('session already gone'));
    const panel = queryPanel('panel-1');
    const current = new Map<string, QueryExecState>([[paneKey(panel.id), runningExec()]]);

    expect(cancelAndCleanupExec([panel], current).size).toBe(0);
    expect(registryRemove).toHaveBeenCalledTimes(1);
  });

  it('skips non-query panels and drivers without cancel capability', () => {
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

    const next = cancelAndCleanupExec([tablePanel, quietPanel], current);

    expect([...next.keys()]).toEqual([paneKey(tablePanel.id)]);
    // The quiet panel has no cancel capability, but its session still closes.
    expect(registryRemove).toHaveBeenCalledTimes(1);
    expect(registryRemove).toHaveBeenCalledWith(paneKey(quietPanel.id));
    expect(cancelQuery).not.toHaveBeenCalled();
  });

  it('returns the same map when the pane owns no exec state', () => {
    const panel = queryPanel('panel-1');
    const current = new Map<string, QueryExecState>();

    expect(cancelAndCleanupPaneExec(panel, 'main', current)).toBe(current);
    expect(registryRemove).not.toHaveBeenCalled();
  });
});