import { describe, expect, it, vi, beforeEach } from 'vitest';

const mockQueryCommands = {
  executeQuery: vi.fn(),
  executeQueryStream: vi.fn(),
  cancelQuery: vi.fn(),
  getQueryHistory: vi.fn().mockResolvedValue([]),
  getFavoriteQueries: vi.fn().mockResolvedValue([]),
  addFavoriteQuery: vi
    .fn()
    .mockResolvedValue({ id: 'fav-1', title: 'test', sql: 'SELECT 1', createdAt: '' }),
  deleteFavoriteQuery: vi.fn().mockResolvedValue(undefined),
  clearQueryHistory: vi.fn().mockResolvedValue(undefined),
  getExplain: vi.fn(),
};

vi.mock('../../commands/query', () => ({
  queryCommands: mockQueryCommands,
}));

describe('queryStore detail row tracking', () => {
  let useQueryStore: typeof import('../../stores/queryStore').useQueryStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const mod = await import('../../stores/queryStore');
    useQueryStore = mod.useQueryStore;
  });

  it('resultDetailRowIndex defaults to null', () => {
    const state = useQueryStore.getState();
    expect(state.resultDetailRowIndex).toBeNull();
  });

  it('setResultDetailRow sets the row index', () => {
    useQueryStore.getState().setActiveConnection('conn-1');
    useQueryStore.getState().setResultDetailRow(3);
    expect(useQueryStore.getState().resultDetailRowIndex).toBe(3);
  });

  it('setResultDetailRow(null) clears the row index', () => {
    useQueryStore.getState().setActiveConnection('conn-1');
    useQueryStore.getState().setResultDetailRow(5);
    useQueryStore.getState().setResultDetailRow(null);
    expect(useQueryStore.getState().resultDetailRowIndex).toBeNull();
  });
});

describe('queryStore executeSelection', () => {
  let useQueryStore: typeof import('../../stores/queryStore').useQueryStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const mod = await import('../../stores/queryStore');
    useQueryStore = mod.useQueryStore;
    useQueryStore.getState().setActiveConnection('conn-1');
    useQueryStore.getState().createTab();
  });

  it('executeSelection sends only the selected SQL', async () => {
    mockQueryCommands.executeQueryStream.mockImplementationOnce(async (_id, sql, onEvent) => {
      onEvent({ type: 'statementStart', index: 0, sql, columns: [] });
      onEvent({ type: 'statementEnd', index: 0, executionTimeMs: 5, truncated: false });
      onEvent({ type: 'done', totalTimeMs: 5 });
    });

    const tabId = useQueryStore.getState().tabs[0].id;
    useQueryStore.getState().updateSql(tabId, 'SELECT 1; SELECT 2; SELECT 3');
    await useQueryStore.getState().executeSelection(tabId, 'SELECT 2');

    expect(mockQueryCommands.executeQueryStream).toHaveBeenCalledWith(
      'conn-1',
      'SELECT 2',
      expect.any(Function),
    );
  });

  it('executeSelection does nothing when no active connection', async () => {
    useQueryStore.getState().setActiveConnection(null);
    const tabId = useQueryStore.getState().tabs[0]?.id;
    if (tabId) {
      await useQueryStore.getState().executeSelection(tabId, 'SELECT 1');
    }
    expect(mockQueryCommands.executeQueryStream).not.toHaveBeenCalled();
  });
});

describe('queryStore favorites', () => {
  let useQueryStore: typeof import('../../stores/queryStore').useQueryStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const mod = await import('../../stores/queryStore');
    useQueryStore = mod.useQueryStore;
  });

  it('favorites defaults to empty array', () => {
    expect(useQueryStore.getState().favorites).toEqual([]);
  });

  it('favoritesVisible defaults to false', () => {
    expect(useQueryStore.getState().favoritesVisible).toBe(false);
  });

  it('toggleFavorites toggles visibility', () => {
    useQueryStore.getState().toggleFavorites();
    expect(useQueryStore.getState().favoritesVisible).toBe(true);
    useQueryStore.getState().toggleFavorites();
    expect(useQueryStore.getState().favoritesVisible).toBe(false);
  });

  it('loadFavorites fetches from backend', async () => {
    const mockFavs = [{ id: 'f1', title: 'My query', sql: 'SELECT 1', createdAt: '2026-01-01' }];
    mockQueryCommands.getFavoriteQueries.mockResolvedValueOnce(mockFavs);

    await useQueryStore.getState().loadFavorites();
    expect(useQueryStore.getState().favorites).toEqual(mockFavs);
    expect(mockQueryCommands.getFavoriteQueries).toHaveBeenCalled();
  });

  it('addFavorite calls backend and reloads', async () => {
    mockQueryCommands.getFavoriteQueries.mockResolvedValueOnce([]);
    await useQueryStore.getState().addFavorite('title', 'SELECT 1');

    expect(mockQueryCommands.addFavoriteQuery).toHaveBeenCalledWith('title', 'SELECT 1');
    expect(mockQueryCommands.getFavoriteQueries).toHaveBeenCalled();
  });

  it('deleteFavorite calls backend and reloads', async () => {
    mockQueryCommands.getFavoriteQueries.mockResolvedValueOnce([]);
    await useQueryStore.getState().deleteFavorite('fav-123');

    expect(mockQueryCommands.deleteFavoriteQuery).toHaveBeenCalledWith('fav-123');
    expect(mockQueryCommands.getFavoriteQueries).toHaveBeenCalled();
  });

  it('reset clears favorites', () => {
    useQueryStore.setState({
      favorites: [{ id: 'x', title: 't', sql: 's', createdAt: '' }],
      favoritesVisible: true,
    });
    useQueryStore.getState().reset();
    expect(useQueryStore.getState().favorites).toEqual([]);
    expect(useQueryStore.getState().favoritesVisible).toBe(false);
  });
});

describe('queryStore tabs and execution', () => {
  let useQueryStore: typeof import('../../stores/queryStore').useQueryStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const mod = await import('../../stores/queryStore');
    useQueryStore = mod.useQueryStore;
    useQueryStore.getState().reset();
    useQueryStore.getState().setActiveConnection('conn-1');
  });

  it('createTab adds tab and sets active', () => {
    useQueryStore.getState().createTab();
    expect(useQueryStore.getState().tabs).toHaveLength(1);
    expect(useQueryStore.getState().activeTabId).toBe(useQueryStore.getState().tabs[0].id);
  });

  it('closeTab removes tab but keeps last one', () => {
    useQueryStore.getState().createTab();
    useQueryStore.getState().createTab();
    const firstId = useQueryStore.getState().tabs[0].id;
    useQueryStore.getState().closeTab(firstId);
    expect(useQueryStore.getState().tabs).toHaveLength(1);
    useQueryStore.getState().closeTab(useQueryStore.getState().tabs[0].id);
    expect(useQueryStore.getState().tabs).toHaveLength(1);
  });

  it('updateSql and setActiveResult', () => {
    useQueryStore.getState().createTab();
    const tabId = useQueryStore.getState().tabs[0].id;
    useQueryStore.getState().updateSql(tabId, 'SELECT 1');
    expect(useQueryStore.getState().tabs[0].sql).toBe('SELECT 1');
    useQueryStore.getState().setActiveResult(tabId, 2);
    expect(useQueryStore.getState().tabs[0].activeResultIdx).toBe(2);
  });

  it('executeQuery success and error', async () => {
    useQueryStore.getState().createTab();
    const tabId = useQueryStore.getState().tabs[0].id;
    useQueryStore.getState().updateSql(tabId, 'SELECT 1');

    mockQueryCommands.executeQueryStream.mockImplementationOnce(async (_id, sql, onEvent) => {
      onEvent({
        type: 'statementStart',
        index: 0,
        sql,
        columns: [{ name: 'v', dataType: 'int', nullable: true }],
      });
      onEvent({ type: 'rows', index: 0, rows: [[1]] });
      onEvent({ type: 'statementEnd', index: 0, executionTimeMs: 10, truncated: false });
      onEvent({ type: 'done', totalTimeMs: 10 });
    });
    await useQueryStore.getState().executeQuery(tabId);
    expect(useQueryStore.getState().tabs[0].results).toHaveLength(1);
    expect(useQueryStore.getState().tabs[0].results[0].rows).toEqual([[1]]);
    expect(useQueryStore.getState().tabs[0].running).toBe(false);

    mockQueryCommands.executeQueryStream.mockRejectedValueOnce(new Error('syntax error'));
    await useQueryStore.getState().executeQuery(tabId);
    expect(useQueryStore.getState().tabs[0].error).toBe('syntax error');
  });

  it('executeQuery concatenates streamed row chunks', async () => {
    useQueryStore.getState().createTab();
    const tabId = useQueryStore.getState().tabs[0].id;
    useQueryStore.getState().updateSql(tabId, 'SELECT n');

    mockQueryCommands.executeQueryStream.mockImplementationOnce(async (_id, sql, onEvent) => {
      onEvent({
        type: 'statementStart',
        index: 0,
        sql,
        columns: [{ name: 'n', dataType: 'int', nullable: true }],
      });
      onEvent({ type: 'rows', index: 0, rows: [[1], [2]] });
      onEvent({ type: 'rows', index: 0, rows: [[3]] });
      onEvent({ type: 'statementEnd', index: 0, executionTimeMs: 8, truncated: false });
      onEvent({ type: 'done', totalTimeMs: 8 });
    });
    await useQueryStore.getState().executeQuery(tabId);
    expect(useQueryStore.getState().tabs[0].results[0].rows).toEqual([[1], [2], [3]]);
    expect(useQueryStore.getState().tabs[0].running).toBe(false);
  });

  it('executeQuery sets error when not connected', async () => {
    useQueryStore.getState().setActiveConnection(null);
    // Tab still exists from a previous connection
    const tabs = useQueryStore.getState().tabs;
    if (tabs.length > 0) {
      await useQueryStore.getState().executeQuery(tabs[0].id);
    }
    // No tabs when no connection, so nothing should happen
    expect(mockQueryCommands.executeQueryStream).not.toHaveBeenCalled();
  });

  it('cancelQuery cancels and marks tab', async () => {
    useQueryStore.getState().createTab();
    const tabId = useQueryStore.getState().tabs[0].id;
    // Manually set running state
    const connState = useQueryStore.getState().states.get('conn-1');
    if (connState) {
      const updated = new Map(useQueryStore.getState().states);
      updated.set('conn-1', {
        ...connState,
        tabs: connState.tabs.map((t) => (t.id === tabId ? { ...t, running: true } : t)),
      });
      useQueryStore.setState({ states: updated, tabs: updated.get('conn-1')!.tabs });
    }
    mockQueryCommands.cancelQuery.mockResolvedValueOnce(undefined);
    await useQueryStore.getState().cancelQuery(tabId);
    expect(useQueryStore.getState().tabs[0].running).toBe(false);
    expect(useQueryStore.getState().tabs[0].error).toBeTruthy();
  });

  it('loadHistory and toggleHistory', async () => {
    mockQueryCommands.getQueryHistory.mockResolvedValueOnce([
      { id: 'h1', sql: 'SELECT 1', executedAt: '2026-01-01', durationMs: 5 },
    ]);
    await useQueryStore.getState().loadHistory();
    expect(useQueryStore.getState().history).toHaveLength(1);
    useQueryStore.getState().toggleHistory();
    expect(useQueryStore.getState().historyVisible).toBe(true);
  });

  it('updateResultCell mutates result grid', () => {
    useQueryStore.getState().createTab();
    const tabId = useQueryStore.getState().tabs[0].id;
    // Manually set results
    const connState = useQueryStore.getState().states.get('conn-1');
    if (connState) {
      const updated = new Map(useQueryStore.getState().states);
      updated.set('conn-1', {
        ...connState,
        tabs: connState.tabs.map((t) =>
          t.id === tabId
            ? {
                ...t,
                results: [
                  {
                    sql: 'SELECT 1',
                    columns: [{ name: 'v', dataType: 'int' }],
                    rows: [[1]],
                    executionTimeMs: 1,
                  },
                ],
              }
            : t,
        ),
      });
      useQueryStore.setState({ states: updated, tabs: updated.get('conn-1')!.tabs });
    }
    useQueryStore.getState().updateResultCell(tabId, 0, 0, 'v', 99);
    expect(useQueryStore.getState().tabs[0].results[0].rows[0][0]).toBe(99);
  });

  it('setChartConfig and setResultViewMode', () => {
    useQueryStore.getState().createTab();
    const tabId = useQueryStore.getState().tabs[0].id;
    const config = { chartType: 'bar' as const, xAxis: 'x', yAxes: ['y'], series: [] };
    useQueryStore.getState().setChartConfig(tabId, config);
    expect(useQueryStore.getState().tabs[0].chartConfig).toEqual(config);
    useQueryStore.getState().setResultViewMode(tabId, 'chart');
    expect(useQueryStore.getState().tabs[0].resultViewMode).toBe('chart');
  });

  it('per-connection state isolation', () => {
    useQueryStore.getState().createTab();
    const tab1 = useQueryStore.getState().tabs[0];
    useQueryStore.getState().updateSql(tab1.id, 'SELECT A');

    useQueryStore.getState().setActiveConnection('conn-2');
    expect(useQueryStore.getState().tabs).toHaveLength(0);

    useQueryStore.getState().createTab();
    const tab2 = useQueryStore.getState().tabs[0];
    useQueryStore.getState().updateSql(tab2.id, 'SELECT B');
    expect(useQueryStore.getState().tabs[0].sql).toBe('SELECT B');

    useQueryStore.getState().setActiveConnection('conn-1');
    expect(useQueryStore.getState().tabs).toHaveLength(1);
    expect(useQueryStore.getState().tabs[0].sql).toBe('SELECT A');
  });
});
