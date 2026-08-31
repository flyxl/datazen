import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { ConnectionConfig } from '../../types';

const mockConnectionCommands = {
  getConnections: vi.fn(),
  saveConnection: vi.fn(),
  deleteConnection: vi.fn(),
  testConnection: vi.fn(),
  getGroups: vi.fn(),
  saveGroups: vi.fn(),
};

vi.mock('../../commands/connection', () => ({
  connectionCommands: mockConnectionCommands,
}));

vi.mock('../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

function makeConn(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: 'conn-1',
    name: 'Test DB',
    databaseType: 'postgresql',
    host: 'localhost',
    port: 5432,
    ...overrides,
  };
}

describe('filterConnections / groupConnections', () => {
  it('filterConnections filters by group and search', async () => {
    const { filterConnections, groupConnections } = await import('../connectionStore');
    const conns = [
      makeConn({ id: '1', name: 'Alpha', group: 'prod', host: 'a.example.com' }),
      makeConn({ id: '2', name: 'Beta', group: 'dev', database: 'mydb' }),
      makeConn({ id: '3', name: 'Gamma', group: 'prod' }),
    ];
    expect(filterConnections(conns, 'prod', '')).toHaveLength(2);
    expect(filterConnections(conns, null, 'mydb')).toHaveLength(1);
    expect(filterConnections(conns, 'prod', 'alpha')).toHaveLength(1);

    const grouped = groupConnections(conns, ['prod', 'dev'], '');
    expect(grouped.find((g) => g.group === 'prod')?.connections).toHaveLength(2);
    expect(grouped.find((g) => g.group === 'dev')?.connections).toHaveLength(1);
  });

  it('sortConnectionsInGroup puts pinned connections first', async () => {
    const { sortConnectionsInGroup } = await import('../connectionStore');
    const sorted = sortConnectionsInGroup([
      makeConn({ id: '1', name: 'Beta' }),
      makeConn({ id: '2', name: 'Alpha', pinned: true }),
      makeConn({ id: '3', name: 'Gamma' }),
    ]);
    expect(sorted.map((c) => c.id)).toEqual(['2', '1', '3']);
  });

  it('groupConnections preserves the persisted order for unpinned connections', async () => {
    const { groupConnections } = await import('../connectionStore');
    const grouped = groupConnections(
      [
        makeConn({ id: '2', name: 'Alpha', group: 'work' }),
        makeConn({ id: '1', name: 'Zulu', group: 'work' }),
      ],
      ['work'],
      '',
    );
    expect(grouped.find((group) => group.group === 'work')?.connections.map((c) => c.id)).toEqual([
      '2',
      '1',
    ]);
  });
});

describe('connectionStore actions', () => {
  let useConnectionStore: typeof import('../connectionStore').useConnectionStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const mod = await import('../connectionStore');
    useConnectionStore = mod.useConnectionStore;
    useConnectionStore.setState({
      connections: [],
      groups: [],
      selectedGroup: null,
      searchQuery: '',
      loading: false,
      connectionsLoaded: false,
      error: null,
    });
  });

  it('fetchConnections loads as-is without group write-backs', async () => {
    const conns = [makeConn({ group: '开发环境' })];
    mockConnectionCommands.getConnections.mockResolvedValueOnce(conns);
    await useConnectionStore.getState().fetchConnections();
    expect(useConnectionStore.getState().connections).toEqual(conns);
    expect(mockConnectionCommands.saveConnection).not.toHaveBeenCalled();
    expect(useConnectionStore.getState().loading).toBe(false);
    expect(useConnectionStore.getState().connectionsLoaded).toBe(true);

    mockConnectionCommands.getConnections.mockRejectedValueOnce(new Error('network'));
    await useConnectionStore.getState().fetchConnections();
    expect(useConnectionStore.getState().error).toBe('network');
    expect(useConnectionStore.getState().connectionsLoaded).toBe(true);
  });

  it('fetchGroups loads groups as-is without write-backs', async () => {
    mockConnectionCommands.getGroups.mockResolvedValueOnce(['开发环境', 'E2E 测试']);
    await useConnectionStore.getState().fetchGroups();
    expect(useConnectionStore.getState().groups).toEqual(['开发环境', 'E2E 测试']);
    expect(mockConnectionCommands.saveGroups).not.toHaveBeenCalled();
  });

  it('saveConnection refreshes and emits on success', async () => {
    mockConnectionCommands.saveConnection.mockResolvedValue(undefined);
    mockConnectionCommands.getConnections.mockResolvedValue([makeConn()]);
    mockConnectionCommands.getGroups.mockResolvedValue([]);
    await useConnectionStore.getState().saveConnection(makeConn());
    expect(mockConnectionCommands.saveConnection).toHaveBeenCalled();
    expect(useConnectionStore.getState().connections).toHaveLength(1);
  });

  it('saveConnection sets error on failure', async () => {
    mockConnectionCommands.saveConnection.mockRejectedValueOnce(new Error('save fail'));
    await useConnectionStore.getState().saveConnection(makeConn());
    expect(useConnectionStore.getState().error).toBe('save fail');
  });

  it('duplicateConnection creates a copy', async () => {
    useConnectionStore.setState({ connections: [makeConn({ id: 'orig', name: 'Original' })] });
    mockConnectionCommands.saveConnection.mockResolvedValue(undefined);
    mockConnectionCommands.getConnections.mockResolvedValue([]);
    mockConnectionCommands.getGroups.mockResolvedValue([]);
    await useConnectionStore.getState().duplicateConnection('orig');
    expect(mockConnectionCommands.saveConnection).toHaveBeenCalledWith(
      expect.objectContaining({ name: expect.stringContaining('Original') }),
    );
  });

  it('duplicateConnection no-ops when source missing', async () => {
    await useConnectionStore.getState().duplicateConnection('missing');
    expect(mockConnectionCommands.saveConnection).not.toHaveBeenCalled();
  });

  it('deleteConnection success and error', async () => {
    mockConnectionCommands.deleteConnection.mockResolvedValue(undefined);
    mockConnectionCommands.getConnections.mockResolvedValue([]);
    mockConnectionCommands.getGroups.mockResolvedValue([]);
    await useConnectionStore.getState().deleteConnection('conn-1');
    expect(mockConnectionCommands.deleteConnection).toHaveBeenCalledWith('conn-1');

    mockConnectionCommands.deleteConnection.mockRejectedValueOnce(new Error('del fail'));
    await useConnectionStore.getState().deleteConnection('conn-1');
    expect(useConnectionStore.getState().error).toBe('del fail');
  });

  it('testConnection delegates to commands', async () => {
    mockConnectionCommands.testConnection.mockResolvedValue({ version: '15' });
    const info = await useConnectionStore.getState().testConnection(makeConn());
    expect(info).toEqual({ version: '15' });
  });

  it('addGroup skips empty and duplicates', async () => {
    useConnectionStore.setState({ groups: ['existing'] });
    await useConnectionStore.getState().addGroup('  ');
    expect(mockConnectionCommands.saveGroups).not.toHaveBeenCalled();
    await useConnectionStore.getState().addGroup('existing');
    expect(mockConnectionCommands.saveGroups).not.toHaveBeenCalled();
    await useConnectionStore.getState().addGroup('new-group');
    expect(useConnectionStore.getState().groups).toContain('new-group');
  });

  it('renameGroup updates connections in group', async () => {
    useConnectionStore.setState({
      groups: ['old'],
      connections: [makeConn({ id: 'c1', group: 'old' })],
    });
    mockConnectionCommands.saveGroups.mockResolvedValue(undefined);
    mockConnectionCommands.saveConnection.mockResolvedValue(undefined);
    mockConnectionCommands.getConnections.mockResolvedValue([]);
    await useConnectionStore.getState().renameGroup('old', 'new');
    expect(mockConnectionCommands.saveConnection).toHaveBeenCalledWith(
      expect.objectContaining({ group: 'new' }),
    );
  });

  it('deleteGroup clears group from connections', async () => {
    useConnectionStore.setState({
      groups: ['gone'],
      connections: [makeConn({ id: 'c1', group: 'gone' })],
    });
    mockConnectionCommands.saveGroups.mockResolvedValue(undefined);
    mockConnectionCommands.saveConnection.mockResolvedValue(undefined);
    mockConnectionCommands.getConnections.mockResolvedValue([]);
    await useConnectionStore.getState().deleteGroup('gone');
    expect(useConnectionStore.getState().groups).not.toContain('gone');
    expect(useConnectionStore.getState().selectedGroup).toBeNull();
  });

  it('moveConnectionToGroup updates connection', async () => {
    useConnectionStore.setState({ connections: [makeConn({ id: 'c1' })] });
    mockConnectionCommands.saveConnection.mockResolvedValue(undefined);
    mockConnectionCommands.getConnections.mockResolvedValue([]);
    await useConnectionStore.getState().moveConnectionToGroup('c1', 'staging');
    expect(mockConnectionCommands.saveConnection).toHaveBeenCalledWith(
      expect.objectContaining({ group: 'staging' }),
    );
  });

  it('setSelectedGroup and setSearchQuery', () => {
    useConnectionStore.getState().setSelectedGroup('prod');
    useConnectionStore.getState().setSearchQuery('alpha');
    expect(useConnectionStore.getState().selectedGroup).toBe('prod');
    expect(useConnectionStore.getState().searchQuery).toBe('alpha');
  });
});
