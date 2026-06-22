import { describe, expect, it, vi, beforeEach } from 'vitest';

const mockQueryCommands = {
  executeQuery: vi.fn(),
  cancelQuery: vi.fn(),
  getQueryHistory: vi.fn().mockResolvedValue([]),
  getFavoriteQueries: vi.fn().mockResolvedValue([]),
  addFavoriteQuery: vi.fn().mockResolvedValue({ id: 'fav-1', title: 'test', sql: 'SELECT 1', createdAt: '' }),
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
    useQueryStore.getState().setResultDetailRow(3);
    expect(useQueryStore.getState().resultDetailRowIndex).toBe(3);
  });

  it('setResultDetailRow(null) clears the row index', () => {
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
    useQueryStore.getState().setConnectionId('conn-1');
    useQueryStore.getState().createTab();
  });

  it('executeSelection sends only the selected SQL', async () => {
    mockQueryCommands.executeQuery.mockResolvedValueOnce({
      results: [{ sql: 'SELECT 1', columns: [], rows: [], executionTimeMs: 5 }],
      totalTimeMs: 5,
    });

    const tabId = useQueryStore.getState().tabs[0].id;
    useQueryStore.getState().updateSql(tabId, 'SELECT 1; SELECT 2; SELECT 3');
    await useQueryStore.getState().executeSelection(tabId, 'SELECT 2');

    expect(mockQueryCommands.executeQuery).toHaveBeenCalledWith('conn-1', 'SELECT 2');
  });

  it('executeSelection does nothing when connectionId is null', async () => {
    useQueryStore.getState().setConnectionId(null);
    const tabId = useQueryStore.getState().tabs[0].id;
    await useQueryStore.getState().executeSelection(tabId, 'SELECT 1');

    expect(mockQueryCommands.executeQuery).not.toHaveBeenCalled();
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
    useQueryStore.setState({ favorites: [{ id: 'x', title: 't', sql: 's', createdAt: '' }], favoritesVisible: true });
    useQueryStore.getState().reset();
    expect(useQueryStore.getState().favorites).toEqual([]);
    expect(useQueryStore.getState().favoritesVisible).toBe(false);
  });
});
