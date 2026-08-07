import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: vi.fn().mockResolvedValue(['testdb']),
    useDatabase: vi.fn().mockResolvedValue(undefined),
    getTables: vi.fn().mockResolvedValue([
      { name: 'users', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'products', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'orders', tableType: 'TABLE', schema: 'public', rowCount: null },
    ]),
    getColumns: vi.fn().mockResolvedValue(['id', 'name']),
  },
}));

describe('computeIsMultiDatabase / resolvePreferredDatabase', () => {
  it('is multi only when capability and length > 1', async () => {
    const { computeIsMultiDatabase, resolvePreferredDatabase } = await import('../schemaStore');
    expect(computeIsMultiDatabase(true, 2)).toBe(true);
    expect(computeIsMultiDatabase(true, 1)).toBe(false);
    expect(computeIsMultiDatabase(true, 0)).toBe(false);
    expect(computeIsMultiDatabase(false, 5)).toBe(false);
    expect(computeIsMultiDatabase(undefined, 5)).toBe(false);

    expect(resolvePreferredDatabase(['a', 'b'], 'b')).toBe('b');
    expect(resolvePreferredDatabase(['a', 'b'], 'missing')).toBe('a');
    expect(resolvePreferredDatabase(['a', 'b'])).toBe('a');
    expect(resolvePreferredDatabase([])).toBeNull();
  });
});

describe('schemaStore.loadForConnection isMultiDatabase', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;
  let databaseCommands: typeof import('../../commands/database').databaseCommands;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    const cmdMod = await import('../../commands/database');
    databaseCommands = cmdMod.databaseCommands;
    useSchemaStore.getState().reset();
  });

  it('sets isMultiDatabase true for mysql with multiple databases', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_c']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
      preferredDatabase: 'db_b',
    });

    const state = useSchemaStore.getState();
    expect(state.isMultiDatabase).toBe(true);
    expect(state.databases).toEqual(['db_a', 'db_b', 'db_c']);
    expect(state.currentDatabase).toBe('db_b');
    expect(databaseCommands.getTables).not.toHaveBeenCalled();
  });

  it('sets isMultiDatabase false for mysql with a single database', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['only_db']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
    expect(useSchemaStore.getState().currentDatabase).toBe('only_db');
  });

  it('sets isMultiDatabase true for postgresql with multiple databases', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db1', 'db2']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'postgresql',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
    expect(useSchemaStore.getState().databases).toEqual(['db1', 'db2']);
    expect(useSchemaStore.getState().currentDatabase).toBe('db1');
  });

  it('falls back to first database when preferred is missing', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['alpha', 'beta']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mariadb',
      preferredDatabase: 'nope',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().currentDatabase).toBe('alpha');
    expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
  });
});

describe('schemaStore.loadTables', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;
  let databaseCommands: typeof import('../../commands/database').databaseCommands;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    const cmdMod = await import('../../commands/database');
    databaseCommands = cmdMod.databaseCommands;
    useSchemaStore.setState({ connectionId: 'test-conn' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not call getColumns during loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    expect(databaseCommands.getTables).toHaveBeenCalledOnce();
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
  });

  it('calls useDatabase before getTables', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    expect(databaseCommands.useDatabase).toHaveBeenCalledWith('test-conn', 'testdb');
    expect(databaseCommands.getTables).toHaveBeenCalledWith('test-conn', 'testdb');
  });

  it('populates tables but leaves columnMap empty after loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    const state = useSchemaStore.getState();
    expect(state.tables).toHaveLength(3);
    expect(Object.keys(state.columnMap)).toHaveLength(0);
  });
});

describe('schemaStore.loadColumnMap', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;
  let databaseCommands: typeof import('../../commands/database').databaseCommands;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    const cmdMod = await import('../../commands/database');
    databaseCommands = cmdMod.databaseCommands;
    useSchemaStore.setState({ connectionId: 'test-conn' });
  });

  it('loads columns for all tables sequentially when called', async () => {
    await useSchemaStore.getState().loadTables('testdb');
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();

    await useSchemaStore.getState().loadColumnMap();

    const state = useSchemaStore.getState();
    expect(databaseCommands.getColumns).toHaveBeenCalledTimes(3);
    expect(state.columnMap).toEqual({
      users: ['id', 'name'],
      products: ['id', 'name'],
      orders: ['id', 'name'],
    });
  });

  it('does nothing when connectionId is null', async () => {
    useSchemaStore.setState({ connectionId: null });
    await useSchemaStore.getState().loadColumnMap();
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
  });
});
