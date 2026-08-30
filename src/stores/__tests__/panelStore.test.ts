import { describe, expect, it, beforeEach, vi } from 'vitest';

vi.mock('../../locales/t', () => ({
  t: (key: string) => key,
}));

const mockGetQueryHistory = vi.fn().mockResolvedValue([]);
const mockGetFavoriteQueries = vi.fn().mockResolvedValue([]);
const mockAddFavoriteQuery = vi.fn().mockResolvedValue(undefined);
const mockDeleteFavoriteQuery = vi.fn().mockResolvedValue(undefined);
const mockExecuteQueryStream = vi.fn().mockResolvedValue(undefined);
const mockCancelQuery = vi.fn().mockResolvedValue(undefined);

const activeConnectionState = {
  connections: {
    'cfg-1': {
      capabilities: {
        supportsCancelQuery: true,
        supportsExplain: true,
        supportsStreamingResults: true,
      },
    },
  },
};

vi.mock('../../commands/query', () => ({
  queryCommands: {
    getQueryHistory: (...args: unknown[]) => mockGetQueryHistory(...args),
    getFavoriteQueries: (...args: unknown[]) => mockGetFavoriteQueries(...args),
    addFavoriteQuery: (...args: unknown[]) => mockAddFavoriteQuery(...args),
    deleteFavoriteQuery: (...args: unknown[]) => mockDeleteFavoriteQuery(...args),
    executeQueryStream: (...args: unknown[]) => mockExecuteQueryStream(...args),
    cancelQuery: (...args: unknown[]) => mockCancelQuery(...args),
    executeQuery: vi.fn().mockResolvedValue({ results: [], totalTimeMs: 10 }),
  },
}));

vi.mock('../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: {
    getState: () => activeConnectionState,
  },
}));

describe('panelStore', () => {
  let usePanelStore: typeof import('../panelStore').usePanelStore;
  let nextPanelId: typeof import('../panelStore').nextPanelId;
  let useSchemaStore: typeof import('../schemaStore').useSchemaStore;
  type Panel = import('../panelStore').Panel;
  type TablePanel = import('../panelStore').TablePanel;

  const base = {
    connectionId: 'cfg-1',
    dbSessionId: 'sess-1',
    connectionName: 'TestDB',
    databaseType: 'postgresql' as const,
  };

  function makeTable(name: string): TablePanel {
    return {
      ...base,
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: name,
      subTab: 'data',
    };
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    activeConnectionState.connections['cfg-1'].capabilities.supportsCancelQuery = true;
    const mod = await import('../panelStore');
    usePanelStore = mod.usePanelStore;
    nextPanelId = mod.nextPanelId;
    const schemaMod = await import('../schemaStore');
    useSchemaStore = schemaMod.useSchemaStore;
    usePanelStore.setState({ panels: [], activePanelId: null, queryExec: new Map() });
  });

  /** Seed schemaStore with a currentDatabase for a runtime session id. */
  function seedCurrentDatabase(
    dbSessionId: string | null,
    database: string | null,
    schema: string | null = null,
  ) {
    const schemas = new Map();
    if (dbSessionId && database) {
      schemas.set(dbSessionId, { currentDatabase: database, currentSchema: schema });
    }
    useSchemaStore.setState({ activeDbSessionId: dbSessionId, schemas });
  }

  // ── Query execution carries the panel's database (F1 BUG-001) ──

  it('executeQuery forwards schemaStore currentDatabase of the panel session', async () => {
    seedCurrentDatabase('sess-1', 'db_b');
    const panel: Panel = { ...base, type: 'query', id: nextPanelId('qry'), title: 'Q1' };
    usePanelStore.getState().addPanel(panel);
    usePanelStore.getState().updateSql(panel.id, 'SELECT DATABASE()');

    await usePanelStore.getState().executeQuery(panel.id);

    expect(mockExecuteQueryStream).toHaveBeenCalledWith(
      'sess-1',
      'SELECT DATABASE()',
      expect.any(Function),
      { database: 'db_b', schema: null },
    );
  });

  it('executeQuery falls back to null when no schema entry exists', async () => {
    seedCurrentDatabase(null, null);
    const panel: Panel = { ...base, type: 'query', id: nextPanelId('qry'), title: 'Q1' };
    usePanelStore.getState().addPanel(panel);
    usePanelStore.getState().updateSql(panel.id, 'SELECT 1');

    await usePanelStore.getState().executeQuery(panel.id);

    expect(mockExecuteQueryStream).toHaveBeenCalledWith(
      'sess-1',
      'SELECT 1',
      expect.any(Function),
      { database: null, schema: null },
    );
  });

  it('executeQuery forwards the F7 currentSchema of the panel session (PG)', async () => {
    seedCurrentDatabase('sess-1', 'db_b', 'sales');
    const panel: Panel = { ...base, type: 'query', id: nextPanelId('qry'), title: 'Q1' };
    usePanelStore.getState().addPanel(panel);
    usePanelStore.getState().updateSql(panel.id, 'SELECT * FROM users');

    await usePanelStore.getState().executeQuery(panel.id);

    expect(mockExecuteQueryStream).toHaveBeenCalledWith(
      'sess-1',
      'SELECT * FROM users',
      expect.any(Function),
      { database: 'db_b', schema: 'sales' },
    );
  });

  // ── addPanel ─────────────────────────────────────────────────

  it('addPanel appends panel and activates it by default', () => {
    const panel = makeTable('users');
    usePanelStore.getState().addPanel(panel);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(panel.id);
    expect(state.activePanelId).toBe(panel.id);
  });

  it('addPanel with activate=false does not change activePanelId', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(2);
    expect(state.activePanelId).toBe(p1.id);
  });

  // ── removePanel ──────────────────────────────────────────────

  it('removePanel removes panel and adjusts active', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    usePanelStore.getState().removePanel(p2.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.activePanelId).toBe(p1.id);
  });

  it('removePanel selects next panel when removing active', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);
    usePanelStore.getState().setActivePanel(p2.id);

    usePanelStore.getState().removePanel(p2.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(2);
    expect(state.activePanelId).toBe(p3.id);
  });

  it('removePanel selects previous panel when removing last', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    usePanelStore.getState().removePanel(p2.id);

    expect(usePanelStore.getState().activePanelId).toBe(p1.id);
  });

  it('removePanel sets active to null when last panel removed', () => {
    const p1 = makeTable('users');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().removePanel(p1.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.activePanelId).toBeNull();
  });

  // ── setActivePanel ───────────────────────────────────────────

  it('setActivePanel updates activePanelId', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    usePanelStore.getState().setActivePanel(p2.id);
    expect(usePanelStore.getState().activePanelId).toBe(p2.id);
  });

  // ── updatePanel ──────────────────────────────────────────────

  it('updatePanel merges partial data into panel', () => {
    const p1 = makeTable('users');
    usePanelStore.getState().addPanel(p1);

    usePanelStore.getState().updatePanel(p1.id, { subTab: 'structure' });

    const updated = usePanelStore.getState().panels[0] as TablePanel;
    expect(updated.subTab).toBe('structure');
    expect(updated.tableName).toBe('users');
  });

  it('updatePanel with structureEditing', () => {
    const p1 = makeTable('users');
    usePanelStore.getState().addPanel(p1);

    usePanelStore.getState().updatePanel(p1.id, {
      subTab: 'structure',
      structureEditing: true,
    } as Partial<Panel>);

    const updated = usePanelStore.getState().panels[0] as TablePanel;
    expect(updated.structureEditing).toBe(true);
  });

  // ── removeAllForConnection ───────────────────────────────────

  it('removeAllForConnection removes all panels for a connectionId', () => {
    const p1 = makeTable('users');
    const p2: Panel = {
      ...base,
      connectionId: 'cfg-2',
      dbSessionId: 'sess-2',
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: 'other',
      subTab: 'data',
    };
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    usePanelStore.getState().removeAllForConnection('cfg-1');

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].connectionId).toBe('cfg-2');
    expect(state.activePanelId).toBe(p2.id);
  });

  it('removeAllForConnection sets active to last remaining', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    usePanelStore.getState().removeAllForConnection('cfg-1');

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.activePanelId).toBeNull();
  });

  // ── closeOtherPanels ─────────────────────────────────────────

  it('closeOtherPanels keeps only the specified panel', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closeOtherPanels(p2.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(p2.id);
    expect(state.activePanelId).toBe(p2.id);
  });

  // ── closeAllPanels ───────────────────────────────────────────

  it('closeAllPanels removes all panels', () => {
    usePanelStore.getState().addPanel(makeTable('users'));
    usePanelStore.getState().addPanel(makeTable('orders'), false);

    usePanelStore.getState().closeAllPanels();

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.activePanelId).toBeNull();
  });

  // ── closePanelsToTheRight ────────────────────────────────────

  it('closePanelsToTheRight removes panels after the specified one', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closePanelsToTheRight(p1.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(p1.id);
  });

  it('closePanelsToTheRight adjusts active when active is removed', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3);

    usePanelStore.getState().closePanelsToTheRight(p1.id);

    expect(usePanelStore.getState().activePanelId).toBe(p1.id);
  });

  // ── closePanelsToTheLeft ─────────────────────────────────────

  it('closePanelsToTheLeft removes panels before the specified one', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closePanelsToTheLeft(p3.id);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].id).toBe(p3.id);
  });

  it('closePanelsToTheLeft adjusts active when active is removed', () => {
    const p1 = makeTable('users');
    const p2 = makeTable('orders');
    const p3 = makeTable('products');
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);
    usePanelStore.getState().addPanel(p3, false);

    usePanelStore.getState().closePanelsToTheLeft(p3.id);

    expect(usePanelStore.getState().activePanelId).toBe(p3.id);
  });

  // ── nextPanelId ──────────────────────────────────────────────

  it('nextPanelId generates unique IDs with prefix', () => {
    const id1 = nextPanelId('tbl');
    const id2 = nextPanelId('tbl');
    const id3 = nextPanelId('query');

    expect(id1).toMatch(/^panel-tbl-/);
    expect(id2).toMatch(/^panel-tbl-/);
    expect(id3).toMatch(/^panel-query-/);
    expect(id1).not.toBe(id2);
  });

  // ── Cross-connection scenarios ───────────────────────────────

  it('panels from different connections coexist', () => {
    const p1 = makeTable('users');
    const p2: Panel = {
      connectionId: 'cfg-2',
      dbSessionId: 'sess-2',
      connectionName: 'OtherDB',
      databaseType: 'mysql' as any,
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: 'other',
      subTab: 'data',
    };
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(2);
    expect(state.panels[0].connectionId).toBe('cfg-1');
    expect(state.panels[1].connectionId).toBe('cfg-2');
    expect(state.activePanelId).toBe(p2.id);
  });

  it('removeAllForConnection preserves other connections panels', () => {
    const p1 = makeTable('users');
    const p2: Panel = {
      connectionId: 'cfg-2',
      dbSessionId: 'sess-2',
      connectionName: 'OtherDB',
      databaseType: 'mysql' as any,
      type: 'table',
      id: nextPanelId('tbl'),
      tableName: 'other',
      subTab: 'data',
    };
    usePanelStore.getState().addPanel(p1);
    usePanelStore.getState().addPanel(p2, false);

    usePanelStore.getState().removeAllForConnection('cfg-1');

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].connectionId).toBe('cfg-2');
  });

  // ── queryExec lifecycle ────────────────────────────────────────

  it('addPanel creates queryExec entry for query panels', () => {
    const panel: Panel = {
      ...base,
      type: 'query',
      id: nextPanelId('qry'),
      title: 'Query 1',
    };
    usePanelStore.getState().addPanel(panel);
    const exec = usePanelStore.getState().queryExec.get(panel.id);
    expect(exec).toBeDefined();
    expect(exec!.sql).toBe('');
    expect(exec!.running).toBe(false);
  });

  it('addPanel does not create queryExec entry for table panels', () => {
    const panel = makeTable('users');
    usePanelStore.getState().addPanel(panel);
    expect(usePanelStore.getState().queryExec.has(panel.id)).toBe(false);
  });

  it('removePanel cleans up queryExec entry', () => {
    const panel: Panel = {
      ...base,
      type: 'query',
      id: nextPanelId('qry'),
      title: 'Query 1',
    };
    usePanelStore.getState().addPanel(panel);
    expect(usePanelStore.getState().queryExec.has(panel.id)).toBe(true);

    usePanelStore.getState().removePanel(panel.id);
    expect(usePanelStore.getState().queryExec.has(panel.id)).toBe(false);
  });

  it('closeAllPanels cleans up all queryExec entries', () => {
    const q1: Panel = { ...base, type: 'query', id: nextPanelId('qry'), title: 'Q1' };
    const q2: Panel = { ...base, type: 'query', id: nextPanelId('qry'), title: 'Q2' };
    usePanelStore.getState().addPanel(q1);
    usePanelStore.getState().addPanel(q2, false);

    usePanelStore.getState().closeAllPanels();
    expect(usePanelStore.getState().queryExec.size).toBe(0);
  });

  it('updateSql updates queryExec sql field', () => {
    const panel: Panel = { ...base, type: 'query', id: nextPanelId('qry'), title: 'Q1' };
    usePanelStore.getState().addPanel(panel);

    usePanelStore.getState().updateSql(panel.id, 'SELECT 1');
    expect(usePanelStore.getState().queryExec.get(panel.id)!.sql).toBe('SELECT 1');
  });

  it('removeAllForConnection cleans up queryExec for that connection', () => {
    const q1: Panel = { ...base, type: 'query', id: nextPanelId('qry'), title: 'Q1' };
    const q2: Panel = {
      ...base,
      connectionId: 'cfg-2',
      dbSessionId: 'sess-2',
      type: 'query',
      id: nextPanelId('qry'),
      title: 'Q2',
    };
    usePanelStore.getState().addPanel(q1);
    usePanelStore.getState().addPanel(q2, false);

    usePanelStore.getState().removeAllForConnection('cfg-1');
    expect(usePanelStore.getState().queryExec.has(q1.id)).toBe(false);
    expect(usePanelStore.getState().queryExec.has(q2.id)).toBe(true);
  });

  // ── Redis panel ──────────────────────────────────────────────

  it('supports redis-db panel type', () => {
    const redisPanel: Panel = {
      connectionId: 'cfg-redis',
      dbSessionId: 'sess-redis',
      connectionName: 'Redis',
      databaseType: 'redis' as any,
      type: 'redis-db',
      id: nextPanelId('redis'),
      dbName: 'db0',
    };
    usePanelStore.getState().addPanel(redisPanel);

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].type).toBe('redis-db');
    expect(state.activePanelId).toBe(redisPanel.id);
  });

  // ── History / Favorites async actions ─────────────────────────

  function makeQueryPanel(title: string): Panel {
    return {
      ...base,
      type: 'query',
      id: nextPanelId('qry'),
      title,
    };
  }

  it('loadHistory calls IPC and sets queryHistory', async () => {
    const history = [
      {
        id: 'h1',
        connectionId: 'cfg-1',
        database: 'db',
        sql: 'SELECT 1',
        executedAt: '',
        executionTimeMs: 10,
        success: true,
      },
    ];
    mockGetQueryHistory.mockResolvedValueOnce(history);

    await usePanelStore.getState().loadHistory('cfg-1');

    expect(mockGetQueryHistory).toHaveBeenCalledWith(1000, 'cfg-1');
    expect(usePanelStore.getState().queryHistory).toEqual(history);
  });

  it('loadFavorites calls IPC and sets queryFavorites', async () => {
    const favorites = [
      { id: 'f1', connectionId: 'cfg-1', title: 'Fav', sql: 'SELECT 1', createdAt: '' },
    ];
    mockGetFavoriteQueries.mockResolvedValueOnce(favorites);

    await usePanelStore.getState().loadFavorites('cfg-1');

    expect(mockGetFavoriteQueries).toHaveBeenCalledWith('cfg-1');
    expect(usePanelStore.getState().queryFavorites).toEqual(favorites);
  });

  it('addFavorite calls IPC then reloads favorites', async () => {
    const favorites = [
      { id: 'f1', connectionId: 'cfg-1', title: 'My Fav', sql: 'SELECT 1', createdAt: '' },
    ];
    mockGetFavoriteQueries.mockResolvedValueOnce(favorites);

    await usePanelStore.getState().addFavorite('My Fav', 'SELECT 1', 'cfg-1');

    expect(mockAddFavoriteQuery).toHaveBeenCalledWith('cfg-1', 'My Fav', 'SELECT 1');
    expect(mockGetFavoriteQueries).toHaveBeenCalledWith('cfg-1');
    expect(usePanelStore.getState().queryFavorites).toEqual(favorites);
  });

  it('deleteFavorite calls IPC then reloads favorites', async () => {
    const panel = makeQueryPanel('Q1');
    usePanelStore.getState().addPanel(panel);
    mockGetFavoriteQueries.mockResolvedValueOnce([]);

    await usePanelStore.getState().deleteFavorite('f1');

    expect(mockDeleteFavoriteQuery).toHaveBeenCalledWith('f1');
    expect(mockGetFavoriteQueries).toHaveBeenCalledWith('cfg-1');
  });

  it('toggleHistory flips historyVisible', () => {
    expect(usePanelStore.getState().historyVisible).toBe(false);
    usePanelStore.getState().toggleHistory();
    expect(usePanelStore.getState().historyVisible).toBe(true);
    usePanelStore.getState().toggleHistory();
    expect(usePanelStore.getState().historyVisible).toBe(false);
  });

  it('openQueryHistory shows drawer, hides favorites, and loads filtered history', async () => {
    const history = [{ id: 'h1', sql: 'SELECT 1' }];
    mockGetQueryHistory.mockResolvedValueOnce(history);
    usePanelStore.setState({ favoritesVisible: true, historyVisible: false });

    await usePanelStore.getState().openQueryHistory('cfg-1');

    expect(mockGetQueryHistory).toHaveBeenCalledWith(1000, 'cfg-1');
    const state = usePanelStore.getState();
    expect(state.historyVisible).toBe(true);
    expect(state.favoritesVisible).toBe(false);
    expect(state.queryHistory).toEqual(history);
  });

  it('toggleFavorites flips favoritesVisible', () => {
    expect(usePanelStore.getState().favoritesVisible).toBe(false);
    usePanelStore.getState().toggleFavorites();
    expect(usePanelStore.getState().favoritesVisible).toBe(true);
    usePanelStore.getState().toggleFavorites();
    expect(usePanelStore.getState().favoritesVisible).toBe(false);
  });

  // ── cancelQuery ────────────────────────────────────────────────

  it('requests cancellation without claiming the query already stopped', async () => {
    const panel = makeQueryPanel('Q1');
    usePanelStore.getState().addPanel(panel);
    usePanelStore.setState((s) => ({
      queryExec: new Map(s.queryExec).set(panel.id, {
        ...s.queryExec.get(panel.id)!,
        running: true,
      }),
    }));

    await usePanelStore.getState().cancelQuery(panel.id);

    expect(mockCancelQuery).toHaveBeenCalledWith('sess-1');
    const exec = usePanelStore.getState().queryExec.get(panel.id)!;
    expect(exec.running).toBe(true);
    expect(exec.cancelState).toBe('requested');
    expect(exec.cancelError).toBeNull();
  });

  it('does not call cancel for a driver that does not support it', async () => {
    activeConnectionState.connections['cfg-1'].capabilities.supportsCancelQuery = false;
    const panel = makeQueryPanel('Q1');
    usePanelStore.getState().addPanel(panel);
    usePanelStore.setState((s) => ({
      queryExec: new Map(s.queryExec).set(panel.id, {
        ...s.queryExec.get(panel.id)!,
        running: true,
      }),
    }));

    await usePanelStore.getState().cancelQuery(panel.id);

    expect(mockCancelQuery).not.toHaveBeenCalled();
    expect(usePanelStore.getState().queryExec.get(panel.id)!.running).toBe(true);
  });

  // ── setActiveResult / setResultDetailRow / setChartConfig / setResultViewMode ──

  it('setActiveResult updates activeResultIdx', () => {
    const panel = makeQueryPanel('Q1');
    usePanelStore.getState().addPanel(panel);
    usePanelStore.getState().setActiveResult(panel.id, 2);
    expect(usePanelStore.getState().queryExec.get(panel.id)!.activeResultIdx).toBe(2);
  });

  it('setResultDetailRow updates resultDetailRowIndex', () => {
    const panel = makeQueryPanel('Q1');
    usePanelStore.getState().addPanel(panel);
    usePanelStore.getState().setResultDetailRow(panel.id, 5);
    expect(usePanelStore.getState().queryExec.get(panel.id)!.resultDetailRowIndex).toBe(5);
  });

  it('setResultViewMode updates resultViewMode', () => {
    const panel = makeQueryPanel('Q1');
    usePanelStore.getState().addPanel(panel);
    usePanelStore.getState().setResultViewMode(panel.id, 'chart');
    expect(usePanelStore.getState().queryExec.get(panel.id)!.resultViewMode).toBe('chart');
  });

  // ── reset ──────────────────────────────────────────────────────

  it('reset clears all state', () => {
    const panel = makeQueryPanel('Q1');
    usePanelStore.getState().addPanel(panel);
    usePanelStore.setState({ queryHistory: [{ id: 'h1' }] as any, historyVisible: true });

    usePanelStore.getState().reset();

    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.activePanelId).toBeNull();
    expect(state.queryExec.size).toBe(0);
    expect(state.queryHistory).toHaveLength(0);
    expect(state.queryFavorites).toHaveLength(0);
    expect(state.historyVisible).toBe(false);
    expect(state.favoritesVisible).toBe(false);
  });
});
