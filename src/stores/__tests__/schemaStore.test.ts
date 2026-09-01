import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: vi.fn().mockResolvedValue(['testdb']),
    getTables: vi.fn().mockResolvedValue([
      { name: 'users', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'products', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'orders', tableType: 'TABLE', schema: 'public', rowCount: null },
    ]),
    getColumns: vi.fn().mockResolvedValue(['id', 'name']),
  },
}));

describe('computeIsMultiDatabase / resolvePreferredDatabase / resolveVisibleDatabases', () => {
  it('is multi only when capability and length > 1', async () => {
    const { computeIsMultiDatabase, resolvePreferredDatabase, resolveVisibleDatabases } =
      await import('../schemaStore');
    expect(computeIsMultiDatabase(true, 2)).toBe(true);
    expect(computeIsMultiDatabase(true, 1)).toBe(false);
    expect(computeIsMultiDatabase(true, 0)).toBe(false);
    expect(computeIsMultiDatabase(false, 5)).toBe(false);
    expect(computeIsMultiDatabase(undefined, 5)).toBe(false);

    expect(resolvePreferredDatabase(['a', 'b'], 'b')).toBe('b');
    expect(resolvePreferredDatabase(['a', 'b'], 'missing')).toBe('a');
    expect(resolvePreferredDatabase(['a', 'b'])).toBe('a');
    expect(resolvePreferredDatabase([])).toBeNull();

    expect(resolveVisibleDatabases(['a', 'b', 'c'], 'b')).toEqual({
      databases: ['b'],
      preferred: 'b',
      lockedToConfigured: true,
    });
    expect(resolveVisibleDatabases(['a', 'b'], undefined)).toEqual({
      databases: ['a', 'b'],
      preferred: 'a',
      lockedToConfigured: false,
    });
    expect(resolveVisibleDatabases(['a', 'b'], '  ')).toEqual({
      databases: ['a', 'b'],
      preferred: 'a',
      lockedToConfigured: false,
    });
    // Kiwi-style: preferred is instance domain, not in logical DB list → do not lock
    expect(resolveVisibleDatabases(['app_db', 'other'], 'afi-ph-useraccount-dbreader.aku')).toEqual(
      {
        databases: ['app_db', 'other'],
        preferred: 'app_db',
        lockedToConfigured: false,
      },
    );
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

  it('locks to configured database and disables multi-db session', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_c']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
      preferredDatabase: 'db_b',
    });

    const state = useSchemaStore.getState();
    expect(state.isMultiDatabase).toBe(false);
    expect(state.databases).toEqual(['db_b']);
    expect(state.currentDatabase).toBe('db_b');
    expect(databaseCommands.getTables).not.toHaveBeenCalled();
  });

  it('lists all databases when none configured (mysql)', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_c']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    const state = useSchemaStore.getState();
    expect(state.isMultiDatabase).toBe(true);
    expect(state.databases).toEqual(['db_a', 'db_b', 'db_c']);
    expect(state.currentDatabase).toBe('db_a');
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

  it('falls back to listing all when preferred is empty string', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['alpha', 'beta']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mariadb',
      preferredDatabase: '   ',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().databases).toEqual(['alpha', 'beta']);
    expect(useSchemaStore.getState().currentDatabase).toBe('alpha');
    expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
  });

  it('does not lock when configured database is absent from server list (e.g. Kiwi domain)', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['alpha', 'beta']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mariadb',
      preferredDatabase: 'nope',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().databases).toEqual(['alpha', 'beta']);
    expect(useSchemaStore.getState().currentDatabase).toBe('alpha');
    expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
  });

  it('refresh after creating a new DB preserves locked database when preferred is passed', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
      preferredDatabase: 'db_a',
    });

    expect(useSchemaStore.getState().databases).toEqual(['db_a']);
    expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    expect(useSchemaStore.getState().isMultiDatabase).toBe(false);

    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_new']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
      preferredDatabase: 'db_a',
    });

    expect(useSchemaStore.getState().databases).toEqual(['db_a']);
    expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
    expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
  });

  it('refresh after creating a new DB shows all DBs when no preferred', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().databases).toEqual(['db_a', 'db_b']);
    expect(useSchemaStore.getState().isMultiDatabase).toBe(true);

    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_new']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().databases).toEqual(['db_a', 'db_b', 'db_new']);
    expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
  });

  it('seeds top-level database branches only when multi-db', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db1', 'db2']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'postgresql',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
    expect(useSchemaStore.getState().namespaceTree).toEqual({ db1: {}, db2: {} });
  });

  it('does not seed database names as top-level branches for single-db', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['only_db']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'postgresql',
      skipLoadTables: true,
    });

    expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
    expect(useSchemaStore.getState().namespaceTree).toEqual({});
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
    useSchemaStore.setState({ dbSessionId: 'test-conn' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not call getColumns during loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    expect(databaseCommands.getTables).toHaveBeenCalledOnce();
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
  });

  it('loads tables for the pinned database without a use_database IPC (F1)', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    expect(databaseCommands.getTables).toHaveBeenCalledWith('test-conn', 'testdb');
  });

  it('populates tables but leaves columnMap empty after loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb');

    const state = useSchemaStore.getState();
    expect(state.tables).toHaveLength(3);
    expect(Object.keys(state.columnMap)).toHaveLength(0);
  });

  it('setLoadedTables partitions views and clears columnMap', async () => {
    useSchemaStore.setState({
      columnMap: { old: ['a'] },
      currentDatabase: 'other',
    });
    useSchemaStore.getState().setLoadedTables('db1', [
      { name: 't1', tableType: 'table', schema: null, rowCount: null },
      { name: 'v1', tableType: 'view', schema: null, rowCount: null },
    ]);
    const state = useSchemaStore.getState();
    expect(state.currentDatabase).toBe('db1');
    expect(state.tables.map((t) => t.name)).toEqual(['t1']);
    expect(state.views.map((t) => t.name)).toEqual(['v1']);
    expect(state.columnMap).toEqual({});
  });
});

describe('schemaStore.switchDatabase', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;
  let databaseCommands: typeof import('../../commands/database').databaseCommands;

  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    const cmdMod = await import('../../commands/database');
    databaseCommands = cmdMod.databaseCommands;
    useSchemaStore.setState({ dbSessionId: 'test-conn' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('switches the local database context via setLoadedTables (no use_database IPC)', async () => {
    await useSchemaStore.getState().switchDatabase('otherdb');

    expect(databaseCommands.getTables).toHaveBeenCalledWith('test-conn', 'otherdb');
    const state = useSchemaStore.getState();
    expect(state.currentDatabase).toBe('otherdb');
    expect(state.tables.map((t) => t.name)).toEqual(['users', 'products', 'orders']);
  });

  it('does not bump schemaEpoch on a lightweight context switch', async () => {
    expect(useSchemaStore.getState().schemaEpoch).toBe(0);
    await useSchemaStore.getState().switchDatabase('otherdb');
    // Identical to loadTables except for the invalidation bump — epoch stays flat.
    expect(useSchemaStore.getState().schemaEpoch).toBe(0);
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
    useSchemaStore.setState({ dbSessionId: 'test-conn' });
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

  it('does nothing when dbSessionId is null', async () => {
    useSchemaStore.setState({ dbSessionId: null });
    await useSchemaStore.getState().loadColumnMap();
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
  });
});

describe('schemaStore.ensureColumns', () => {
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
    useSchemaStore.setState({ dbSessionId: 'test-conn', columnMap: {} });
  });

  it('fetches only the requested tables', async () => {
    await useSchemaStore.getState().loadTables('testdb');
    vi.mocked(databaseCommands.getColumns).mockClear();

    await useSchemaStore.getState().ensureColumns(['users']);

    expect(databaseCommands.getColumns).toHaveBeenCalledTimes(1);
    expect(databaseCommands.getColumns).toHaveBeenCalledWith('test-conn', 'users');
    expect(useSchemaStore.getState().columnMap).toEqual({ users: ['id', 'name'] });
  });

  it('skips tables already present in columnMap', async () => {
    await useSchemaStore.getState().loadTables('testdb');
    vi.mocked(databaseCommands.getColumns).mockClear();
    useSchemaStore.setState({ columnMap: { users: ['id'] } });
    await useSchemaStore.getState().ensureColumns(['users', 'orders']);
    expect(databaseCommands.getColumns).toHaveBeenCalledTimes(1);
    expect(databaseCommands.getColumns).toHaveBeenCalledWith('test-conn', 'orders');
    expect(useSchemaStore.getState().columnMap).toEqual({
      users: ['id'],
      orders: ['id', 'name'],
    });
  });

  it('does not call getColumns for unknown or partial table names', async () => {
    useSchemaStore.setState({
      namespaceTree: { hive: { snap: { wb_daily_orders: [] } } },
    });
    await useSchemaStore.getState().ensureColumns(['wb_d', 'wb_daily', 'snap', 'hive']);
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
    expect(useSchemaStore.getState().columnMap).toEqual({});
  });

  it('fetches a path-hierarchy leaf once the full name is known', async () => {
    useSchemaStore.setState({
      namespaceTree: { hive: { snap: { wb_daily_orders: [] } } },
    });
    await useSchemaStore.getState().ensureColumns(['wb_d', 'wb_daily_orders']);
    expect(databaseCommands.getColumns).toHaveBeenCalledTimes(1);
    expect(databaseCommands.getColumns).toHaveBeenCalledWith('test-conn', 'wb_daily_orders');
    expect(useSchemaStore.getState().columnMap).toEqual({
      wb_daily_orders: ['id', 'name'],
    });
  });

  it('does not cache a failed getColumns so a later retry can succeed', async () => {
    await useSchemaStore.getState().loadTables('testdb');
    vi.mocked(databaseCommands.getColumns).mockClear();
    vi.mocked(databaseCommands.getColumns).mockRejectedValueOnce(new Error('500'));
    await useSchemaStore.getState().ensureColumns(['users']);
    expect(useSchemaStore.getState().columnMap).toEqual({});

    vi.mocked(databaseCommands.getColumns).mockResolvedValueOnce(['id', 'name']);
    await useSchemaStore.getState().ensureColumns(['users']);
    expect(useSchemaStore.getState().columnMap).toEqual({ users: ['id', 'name'] });
  });

  it('does nothing when dbSessionId is null', async () => {
    useSchemaStore.setState({ dbSessionId: null });
    await useSchemaStore.getState().ensureColumns(['users']);
    expect(databaseCommands.getColumns).not.toHaveBeenCalled();
  });
});

describe('schemaStore namespace merge APIs', () => {
  let useSchemaStore: typeof import('../schemaStore').useSchemaStore;

  beforeEach(async () => {
    vi.resetModules();
    const storeMod = await import('../schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    useSchemaStore.getState().reset();
  });

  it('mergeNamespace updates namespaceTree and loadedPaths', async () => {
    useSchemaStore.getState().mergeNamespace(['db'], 'branch', ['hive']);
    expect(useSchemaStore.getState().namespaceTree).toEqual({ db: { hive: {} } });
    expect(useSchemaStore.getState().loadedPaths.has('db')).toBe(true);
  });

  it('cachePathItems stores get_tables rows by fetch path', async () => {
    const items = [{ name: '558/hive', tableType: 'table', schema: 'CATALOG', rowCount: null }];
    useSchemaStore.getState().cachePathItems('558', items);
    expect(useSchemaStore.getState().pathItems['558']).toEqual(items);
  });

  it('registerPathAliases maps name to id', async () => {
    useSchemaStore.getState().registerPathAliases([{ name: 'presto_afi_data', id: '558' }]);
    expect(useSchemaStore.getState().pathAliases).toEqual({ presto_afi_data: '558' });
    expect(useSchemaStore.getState().namespaceTree).toEqual({ presto_afi_data: {} });
    expect(useSchemaStore.getState().namespaceOwnedByPlugin).toBe(true);
  });

  it('setLoadedTables does not flatten namespace after registerPathAliases', async () => {
    useSchemaStore.getState().registerPathAliases([{ name: 'presto', id: '558' }]);
    useSchemaStore.getState().mergeNamespace(['presto', 'hive', 'snap'], 'tables', ['t1']);
    const before = structuredClone(useSchemaStore.getState().namespaceTree);
    useSchemaStore
      .getState()
      .setLoadedTables('558/hive/snap', [
        { name: 't1', tableType: 'table', schema: 'snap', rowCount: null },
      ]);
    expect(useSchemaStore.getState().namespaceOwnedByPlugin).toBe(true);
    expect(useSchemaStore.getState().namespaceTree).toEqual(before);
    expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['t1']);
  });

  it('setLoadedTables merges mysql-style database.table namespace', async () => {
    useSchemaStore.setState({ isMultiDatabase: true });
    useSchemaStore
      .getState()
      .setLoadedTables('app', [
        { name: 'users', tableType: 'table', schema: null, rowCount: null },
      ]);
    expect(useSchemaStore.getState().namespaceTree).toEqual({ app: { users: [] } });
  });

  it('setLoadedTables groups postgresql schemas under database when multi-db', async () => {
    useSchemaStore.setState({ isMultiDatabase: true });
    useSchemaStore
      .getState()
      .setLoadedTables('warehouse', [
        { name: 't', tableType: 'table', schema: 'public', rowCount: null },
      ]);
    expect(useSchemaStore.getState().namespaceTree).toEqual({
      warehouse: { public: { t: [] } },
    });
  });

  it('setLoadedTables uses schema.table when single-db postgresql', async () => {
    useSchemaStore.setState({ isMultiDatabase: false });
    useSchemaStore
      .getState()
      .setLoadedTables('warehouse', [
        { name: 't', tableType: 'table', schema: 'public', rowCount: null },
      ]);
    expect(useSchemaStore.getState().namespaceTree).toEqual({ public: { t: [] } });
  });

  it('setLoadedTables includes views in namespace table leaves', async () => {
    useSchemaStore.setState({ isMultiDatabase: false });
    useSchemaStore.getState().setLoadedTables('warehouse', [
      { name: 't', tableType: 'table', schema: 'public', rowCount: null },
      { name: 'v', tableType: 'view', schema: 'public', rowCount: null },
    ]);
    expect(useSchemaStore.getState().namespaceTree).toEqual({ public: { t: [], v: [] } });
    expect(useSchemaStore.getState().views.map((v) => v.name)).toEqual(['v']);
  });

  it('setLoadedTables replaces dropped tables instead of merging them back', async () => {
    useSchemaStore.setState({ isMultiDatabase: true });
    useSchemaStore.getState().setLoadedTables('app', [
      { name: 'users', tableType: 'table', schema: null, rowCount: null },
      { name: 'orders', tableType: 'table', schema: null, rowCount: null },
    ]);
    useSchemaStore
      .getState()
      .setLoadedTables('app', [
        { name: 'users', tableType: 'table', schema: null, rowCount: null },
      ]);
    expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['users']);
    expect(useSchemaStore.getState().namespaceTree).toEqual({ app: { users: [] } });
  });

  it('removeRelation drops the table from lists and namespace immediately', async () => {
    useSchemaStore.setState({ isMultiDatabase: true });
    useSchemaStore.getState().setLoadedTables('app', [
      { name: 'users', tableType: 'table', schema: null, rowCount: null },
      { name: 'orders', tableType: 'table', schema: null, rowCount: null },
    ]);
    useSchemaStore.getState().removeRelation('orders');
    expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['users']);
    expect(useSchemaStore.getState().namespaceTree).toEqual({ app: { users: [] } });
  });
});

describe('schemaStore.ensureNamespacePath ensuringCount', () => {
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

  it('increments while a namespace fetch is in flight', async () => {
    let release!: (
      value: { name: string; tableType: string; schema: string; rowCount: null }[],
    ) => void;
    vi.mocked(databaseCommands.getTables).mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );

    useSchemaStore.setState({
      dbSessionId: 'c1',
      databaseType: 'mysql',
      currentDatabase: 'app',
      databases: ['app'],
      isMultiDatabase: false,
      loadedPaths: new Set(),
      ensuringCount: 0,
    });

    const pending = useSchemaStore.getState().ensureNamespacePath(['app']);
    await vi.waitFor(() => {
      expect(typeof release).toBe('function');
    });
    expect(useSchemaStore.getState().ensuringCount).toBe(1);

    release([]);
    await pending;
    expect(useSchemaStore.getState().ensuringCount).toBe(0);
  });

  it('does not increment when the path is already loaded', async () => {
    useSchemaStore.setState({
      dbSessionId: 'c1',
      databaseType: 'mysql',
      currentDatabase: 'app',
      databases: ['app'],
      isMultiDatabase: false,
      loadedPaths: new Set(['app']),
      ensuringCount: 0,
    });

    await useSchemaStore.getState().ensureNamespacePath(['app']);
    expect(useSchemaStore.getState().ensuringCount).toBe(0);
    expect(databaseCommands.getTables).not.toHaveBeenCalled();
  });
});

describe('schemaStore keyed multi-connection', () => {
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

  it('keeps separate schema state per connection', async () => {
    vi.mocked(databaseCommands.getDatabases).mockImplementation(async (conn) => {
      if (conn === 'conn-a') return ['db_a'];
      return ['db_b'];
    });
    vi.mocked(databaseCommands.getTables).mockImplementation(async (_conn, db) => {
      if (db === 'db_a') {
        return [{ name: 'users_a', tableType: 'TABLE', schema: null, rowCount: null }];
      }
      return [{ name: 'users_b', tableType: 'TABLE', schema: null, rowCount: null }];
    });

    await useSchemaStore.getState().loadForConnection('conn-a', {
      databaseType: 'sqlite',
    });
    await useSchemaStore.getState().loadForConnection('conn-b', {
      databaseType: 'sqlite',
    });

    expect(useSchemaStore.getState().dbSessionId).toBe('conn-b');
    expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['users_b']);

    useSchemaStore.getState().setActiveConnection('conn-a');
    expect(useSchemaStore.getState().dbSessionId).toBe('conn-a');
    expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['users_a']);
    expect(useSchemaStore.getState().currentDatabase).toBe('db_a');
  });

  it('getConnectionSchema returns cached entry without switching active', async () => {
    vi.mocked(databaseCommands.getDatabases)
      .mockResolvedValueOnce(['db_a'])
      .mockResolvedValueOnce(['db_b']);

    await useSchemaStore.getState().loadForConnection('conn-a', {
      databaseType: 'sqlite',
      skipLoadTables: true,
      preferredDatabase: 'db_a',
    });
    await useSchemaStore.getState().loadForConnection('conn-b', {
      databaseType: 'sqlite',
      skipLoadTables: true,
      preferredDatabase: 'db_b',
    });

    const schemaA = useSchemaStore.getState().getConnectionSchema('conn-a');
    expect(schemaA?.currentDatabase).toBe('db_a');
    expect(useSchemaStore.getState().dbSessionId).toBe('conn-b');
  });

  it('removeConnection drops cached schema and clears active when removed', async () => {
    await useSchemaStore.getState().loadForConnection('conn-a', {
      databaseType: 'sqlite',
      skipLoadTables: true,
    });

    useSchemaStore.getState().removeConnection('conn-a');
    expect(useSchemaStore.getState().getConnectionSchema('conn-a')).toBeUndefined();
    expect(useSchemaStore.getState().dbSessionId).toBeNull();
    expect(useSchemaStore.getState().tables).toEqual([]);
  });

  it('setState with dbSessionId switches active and preserves per-connection fields', async () => {
    vi.mocked(databaseCommands.getDatabases).mockImplementation(async (conn) => {
      if (conn === 'conn-a') return ['db_a'];
      return ['db_b'];
    });
    vi.mocked(databaseCommands.getTables).mockImplementation(async (_conn, db) => [
      { name: `t_${db}`, tableType: 'TABLE', schema: null, rowCount: null },
    ]);

    await useSchemaStore.getState().loadForConnection('conn-a', {
      databaseType: 'sqlite',
    });
    await useSchemaStore.getState().loadForConnection('conn-b', {
      databaseType: 'sqlite',
    });

    useSchemaStore.setState({ dbSessionId: 'conn-a' });
    expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['t_db_a']);

    useSchemaStore.setState({ columnMap: { t_db_a: ['id'] } });
    expect(useSchemaStore.getState().columnMap).toEqual({ t_db_a: ['id'] });
    expect(useSchemaStore.getState().getConnectionSchema('conn-b')?.columnMap).toEqual({});

    useSchemaStore.setState({ dbSessionId: 'conn-b' });
    expect(useSchemaStore.getState().columnMap).toEqual({});
  });

  it('ensureColumns uses per-connection columnInflight', async () => {
    useSchemaStore.setState({ dbSessionId: 'conn-a', columnMap: {} });
    useSchemaStore
      .getState()
      .setLoadedTables('db', [{ name: 'users', tableType: 'table', schema: null, rowCount: null }]);

    useSchemaStore.setState({ dbSessionId: 'conn-b', columnMap: {} });
    useSchemaStore
      .getState()
      .setLoadedTables('db', [
        { name: 'orders', tableType: 'table', schema: null, rowCount: null },
      ]);

    vi.mocked(databaseCommands.getColumns).mockClear();
    await useSchemaStore.getState().ensureColumns(['users'], 'conn-a');
    await useSchemaStore.getState().ensureColumns(['orders'], 'conn-b');

    expect(databaseCommands.getColumns).toHaveBeenCalledWith('conn-a', 'users');
    expect(databaseCommands.getColumns).toHaveBeenCalledWith('conn-b', 'orders');
    expect(useSchemaStore.getState().getConnectionSchema('conn-a')?.columnMap).toEqual({
      users: ['id', 'name'],
    });
    expect(useSchemaStore.getState().getConnectionSchema('conn-b')?.columnMap).toEqual({
      orders: ['id', 'name'],
    });
  });
});

describe('parsePathHierarchyDatabaseEntry / plugin namespace bootstrap', () => {
  it('parses Superset-style database list entries', async () => {
    const { parsePathHierarchyDatabaseEntry } = await import('../schemaStore');
    expect(parsePathHierarchyDatabaseEntry('558:presto_afi_data (presto)')).toEqual({
      id: '558',
      name: 'presto_afi_data',
    });
    expect(parsePathHierarchyDatabaseEntry('plain')).toEqual({ id: 'plain', name: 'plain' });
  });

  it('loadForConnection registers path aliases for namespaceOwnedByPlugin drivers', async () => {
    const { DB_REGISTRY } = await import('../../lib/databaseTypes');
    if (!Object.prototype.hasOwnProperty.call(DB_REGISTRY, 'superset')) return;

    const { databaseCommands } = await import('../../commands/database');
    const { useSchemaStore } = await import('../schemaStore');
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce([
      '558:presto_afi_data (presto)',
    ]);

    await useSchemaStore.getState().loadForConnection('conn-superset', {
      databaseType: 'superset',
      skipLoadTables: true,
    });

    const schema = useSchemaStore.getState().getConnectionSchema('conn-superset');
    expect(schema?.databases).toEqual(['presto_afi_data']);
    expect(schema?.currentDatabase).toBe('presto_afi_data');
    expect(schema?.pathAliases).toEqual({ presto_afi_data: '558' });
    expect(schema?.namespaceTree).toEqual({ presto_afi_data: {} });
    expect(schema?.namespaceOwnedByPlugin).toBe(true);
  });
});
