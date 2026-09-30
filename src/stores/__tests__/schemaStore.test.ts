import { createEmptyConnectionSchema, type ConnectionSchemaState } from '../schemaStoreState';
import { projectRelationColumns } from '../schemaColumnLoader';
import { relationKey } from '@datazen/driver-sdk';
import type { SchemaStore } from '../schemaStore';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import type { TableInfo } from '../../types';
import type { RelationRef } from '@datazen/driver-sdk';

type TestSchemaStore = { getState(): SchemaStore; setState(patch: Partial<SchemaStore>): void };
function seedSchema(
  store: TestSchemaStore,
  session: string,
  patch: Partial<ConnectionSchemaState>,
) {
  const schemas = new Map(store.getState().schemas);
  schemas.set(session, { ...createEmptyConnectionSchema(), ...schemas.get(session), ...patch });
  store.setState({ schemas, activeDbSessionId: session });
}
function schemaSnapshot(store: TestSchemaStore, session: string) {
  const entry = store.getState().schemas.get(session) ?? createEmptyConnectionSchema();
  return {
    ...entry,
    ...projectRelationColumns(
      entry,
      session,
      entry.currentDatabase ?? '',
      entry.currentSchema ?? undefined,
    ),
  };
}
function cachedRelation(session: string, database: string, name: string, names: string[]) {
  const ref = { database, schema: null, name };
  return {
    [relationKey({ ...ref, dbSessionId: session })]: {
      ref,
      primaryKeys: [],
      columns: names.map((name) => ({ name, dataType: 'text', nullable: true })),
    },
  };
}

const { mockReadColumns } = vi.hoisted(() => ({
  mockReadColumns: vi.fn(async (_session: string, refs: readonly RelationRef[]) => ({
    results: refs.map((ref) => ({
      status: 'ok' as const,
      value: {
        ref,
        columns: [
          { name: 'id', dataType: 'integer', nullable: false },
          { name: 'name', dataType: 'text', nullable: true },
        ],
        primaryKeys: ['id'],
      },
    })),
  })),
}));

vi.mock('@datazen/driver-sdk', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/driver-sdk')>()),
  schemaClient: { readColumns: mockReadColumns },
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: vi.fn().mockResolvedValue(['testdb']),
    listTables: vi.fn().mockResolvedValue([
      { name: 'users', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'products', tableType: 'TABLE', schema: 'public', rowCount: null },
      { name: 'orders', tableType: 'TABLE', schema: 'public', rowCount: null },
    ]),
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

    const state = schemaSnapshot(useSchemaStore, 'conn-1');
    expect(state.isMultiDatabase).toBe(false);
    expect(state.databases).toEqual(['db_b']);
    expect(state.currentDatabase).toBe('db_b');
    expect(databaseCommands.listTables).not.toHaveBeenCalled();
  });

  it('lists all databases when none configured (mysql)', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_c']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    const state = schemaSnapshot(useSchemaStore, 'conn-1');
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

    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(false);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('only_db');
  });

  it('sets isMultiDatabase true for postgresql with multiple databases', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db1', 'db2']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'postgresql',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(true);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['db1', 'db2']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('db1');
  });

  it('falls back to listing all when preferred is empty string', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['alpha', 'beta']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mariadb',
      preferredDatabase: '   ',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['alpha', 'beta']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('alpha');
    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(true);
  });

  it('does not lock when configured database is absent from server list (e.g. Kiwi domain)', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['alpha', 'beta']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mariadb',
      preferredDatabase: 'nope',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['alpha', 'beta']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('alpha');
    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(true);
  });

  it('keeps a user-selected non-first database across reload instead of reverting to the first', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_c']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('db_a');

    // User selects a non-first database from the multi-db dropdown.
    vi.mocked(databaseCommands.listTables).mockResolvedValueOnce([]);
    await useSchemaStore.getState().switchDatabase('db_b', 'conn-1');
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('db_b');

    // Re-mounting ContentView (e.g. switching to Settings and back) re-runs
    // loadForConnection. It must keep db_b instead of reverting to db_a.
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_c']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['db_a', 'db_b', 'db_c']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('db_b');
  });

  it('refresh after creating a new DB preserves locked database when preferred is passed', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
      preferredDatabase: 'db_a',
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['db_a']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('db_a');
    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(false);

    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_new']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
      preferredDatabase: 'db_a',
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['db_a']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').currentDatabase).toBe('db_a');
    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(false);
  });

  it('refresh after creating a new DB shows all DBs when no preferred', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['db_a', 'db_b']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(true);

    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db_a', 'db_b', 'db_new']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'mysql',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').databases).toEqual(['db_a', 'db_b', 'db_new']);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(true);
  });

  it('seeds top-level database branches only when multi-db', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['db1', 'db2']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'postgresql',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(true);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').namespaceTree).toEqual({ db1: {}, db2: {} });
  });

  it('does not seed database names as top-level branches for single-db', async () => {
    vi.mocked(databaseCommands.getDatabases).mockResolvedValueOnce(['only_db']);

    await useSchemaStore.getState().loadForConnection('conn-1', {
      databaseType: 'postgresql',
      skipLoadTables: true,
    });

    expect(schemaSnapshot(useSchemaStore, 'conn-1').isMultiDatabase).toBe(false);
    expect(schemaSnapshot(useSchemaStore, 'conn-1').namespaceTree).toEqual({});
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
    seedSchema(useSchemaStore, 'test-conn', {});
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not call getColumns during loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');

    expect(databaseCommands.listTables).toHaveBeenCalledOnce();
    expect(mockReadColumns).not.toHaveBeenCalled();
  });

  it('loads tables for the pinned database without a use_database IPC (F1)', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');

    expect(databaseCommands.listTables).toHaveBeenCalledWith('test-conn', 'testdb');
  });

  it('populates tables but leaves columnMap empty after loadTables', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');

    const state = schemaSnapshot(useSchemaStore, 'test-conn');
    expect(state.tables).toHaveLength(3);
    expect(Object.keys(state.columnMap)).toHaveLength(0);
  });

  it('setLoadedTables partitions views and clears columnMap', async () => {
    seedSchema(useSchemaStore, 'test-conn', {
      relationColumns: cachedRelation('test-conn', 'db1', 'old', ['a']),
      currentDatabase: 'other',
    });
    useSchemaStore.getState().setLoadedTables(
      'db1',
      [
        { name: 't1', tableType: 'table', schema: null, rowCount: null },
        { name: 'v1', tableType: 'view', schema: null, rowCount: null },
      ],
      'test-conn',
    );
    const state = schemaSnapshot(useSchemaStore, 'test-conn');
    expect(state.currentDatabase).toBe('db1');
    expect(state.tables.map((t) => t.name)).toEqual(['t1']);
    expect(state.views.map((t) => t.name)).toEqual(['v1']);
    expect(state.columnMap).toEqual({});
  });

  it('background fills preserve the active directory and cache the target database', () => {
    seedSchema(useSchemaStore, 'test-conn', { currentDatabase: 'other' });
    useSchemaStore
      .getState()
      .setLoadedTables(
        'db1',
        [{ name: 't1', tableType: 'table', schema: null, rowCount: null }],
        'test-conn',
        { pinCurrentDatabase: false },
      );
    const state = schemaSnapshot(useSchemaStore, 'test-conn');
    expect(state.currentDatabase).toBe('other');
    expect(state.tables).toEqual([]);
    expect(state.tableCatalogs.db1.map((t) => t.name)).toEqual(['t1']);
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
    seedSchema(useSchemaStore, 'test-conn', {});
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('switches the local database context via setLoadedTables (no use_database IPC)', async () => {
    await useSchemaStore.getState().switchDatabase('otherdb', 'test-conn');

    expect(databaseCommands.listTables).toHaveBeenCalledWith('test-conn', 'otherdb');
    const state = schemaSnapshot(useSchemaStore, 'test-conn');
    expect(state.currentDatabase).toBe('otherdb');
    expect(state.tables.map((t) => t.name)).toEqual(['users', 'products', 'orders']);
  });

  it('does not bump schemaEpoch on a lightweight context switch', async () => {
    expect(schemaSnapshot(useSchemaStore, 'test-conn').schemaEpoch).toBe(0);
    await useSchemaStore.getState().switchDatabase('otherdb', 'test-conn');
    // Identical to loadTables except for the invalidation bump — epoch stays flat.
    expect(schemaSnapshot(useSchemaStore, 'test-conn').schemaEpoch).toBe(0);
  });
});

describe('schemaStore.setCurrentDatabase', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;
  let createEmptyConnectionSchema: typeof import('../../stores/schemaStoreState').createEmptyConnectionSchema;

  beforeEach(async () => {
    vi.resetModules();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    const stateMod = await import('../../stores/schemaStoreState');
    createEmptyConnectionSchema = stateMod.createEmptyConnectionSchema;
    seedSchema(useSchemaStore, 'test-conn', {});
  });

  it('adopts the panel database as the session currentDatabase', () => {
    useSchemaStore.getState().setCurrentDatabase('tradingdb', 'test-conn');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').currentDatabase).toBe('tradingdb');
  });

  it('follows a bound non-first database even when current is the first one', () => {
    const schemas = new Map();
    schemas.set('test-conn', {
      ...createEmptyConnectionSchema(),
      databases: ['channeling_dock_db', 'tradingdb'],
      currentDatabase: 'channeling_dock_db',
    });
    useSchemaStore.setState({ schemas });

    // The active query tab is bound to tradingdb — currentDatabase must follow.
    useSchemaStore.getState().setCurrentDatabase('tradingdb', 'test-conn');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').currentDatabase).toBe('tradingdb');
  });

  it('ignores a database the session does not know about', () => {
    const schemas = new Map();
    schemas.set('test-conn', {
      ...createEmptyConnectionSchema(),
      databases: ['channeling_dock_db', 'tradingdb'],
      currentDatabase: 'channeling_dock_db',
    });
    useSchemaStore.setState({ schemas });

    useSchemaStore.getState().setCurrentDatabase('ghost_db', 'test-conn');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').currentDatabase).toBe('channeling_dock_db');
  });
});

describe('schemaStore.ensureDatabaseColumns', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const storeMod = await import('../../stores/schemaStore');
    useSchemaStore = storeMod.useSchemaStore;
    seedSchema(useSchemaStore, 'test-conn', {});
  });

  it('loads all requested typed columns in one transport batch', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    expect(mockReadColumns).not.toHaveBeenCalled();

    await useSchemaStore.getState().ensureDatabaseColumns('test-conn', 'testdb');

    const state = schemaSnapshot(useSchemaStore, 'test-conn');
    expect(mockReadColumns).toHaveBeenCalledTimes(1);
    expect(mockReadColumns).toHaveBeenCalledWith('test-conn', [
      { database: 'testdb', schema: 'public', name: 'users' },
      { database: 'testdb', schema: 'public', name: 'products' },
      { database: 'testdb', schema: 'public', name: 'orders' },
    ]);
    expect(state.columnMap).toEqual({
      users: ['id', 'name'],
      products: ['id', 'name'],
      orders: ['id', 'name'],
    });
  });

  it('does nothing when dbSessionId is null', async () => {
    seedSchema(useSchemaStore, 'test-conn', {});
    await useSchemaStore.getState().ensureDatabaseColumns('', 'testdb');
    expect(mockReadColumns).not.toHaveBeenCalled();
  });

  it('does nothing when database is empty', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    mockReadColumns.mockClear();
    await useSchemaStore.getState().ensureDatabaseColumns('test-conn', '');
    expect(mockReadColumns).not.toHaveBeenCalled();
  });
});

describe('schemaStore.ensureColumns', () => {
  let useSchemaStore: typeof import('../../stores/schemaStore').useSchemaStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    useSchemaStore = (await import('../schemaStore')).useSchemaStore;
    useSchemaStore.getState().reset();
    seedSchema(useSchemaStore, 'test-conn', {});
  });

  it('reads only requested full identities and derives names and types together', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'testdb');
    expect(mockReadColumns).toHaveBeenCalledWith('test-conn', [
      { database: 'testdb', schema: 'public', name: 'users' },
    ]);
    expect(schemaSnapshot(useSchemaStore, 'test-conn').columnMap).toEqual({
      users: ['id', 'name'],
    });
    expect(schemaSnapshot(useSchemaStore, 'test-conn').typedColumnMap).toEqual({
      users: { id: 'integer', name: 'text' },
    });
  });

  it('reuses typed results and fetches only newly requested relations', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'testdb');
    mockReadColumns.mockClear();
    await useSchemaStore.getState().ensureColumns(['users', 'orders'], 'test-conn', 'testdb');
    expect(mockReadColumns).toHaveBeenCalledOnce();
    expect(mockReadColumns).toHaveBeenCalledWith('test-conn', [
      { database: 'testdb', schema: 'public', name: 'orders' },
    ]);
  });

  it('does not fetch incomplete names or namespace branches', async () => {
    seedSchema(useSchemaStore, 'test-conn', {
      namespaceTree: { hive: { snap: { wb_daily_orders: [] } } },
    });
    await useSchemaStore
      .getState()
      .ensureColumns(['wb_d', 'wb_daily', 'snap', 'hive'], 'test-conn', 'hive');
    expect(mockReadColumns).not.toHaveBeenCalled();
  });

  it('supports published path-hierarchy leaf names', async () => {
    seedSchema(useSchemaStore, 'test-conn', {
      namespaceTree: { hive: { snap: { wb_daily_orders: [] } } },
    });
    await useSchemaStore.getState().ensureColumns(['wb_d', 'wb_daily_orders'], 'test-conn', 'hive');
    expect(mockReadColumns).toHaveBeenCalledWith('test-conn', [
      { database: 'hive', schema: null, name: 'wb_daily_orders' },
    ]);
  });

  it('failed reads remain retryable and clear their in-flight markers', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    mockReadColumns.mockRejectedValueOnce(new Error('500'));
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'testdb');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').columnMap).toEqual({});
    expect(schemaSnapshot(useSchemaStore, 'test-conn').columnInflight.size).toBe(0);
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'testdb');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').columnMap).toEqual({
      users: ['id', 'name'],
    });
  });

  it('does nothing without a session or database', async () => {
    await useSchemaStore.getState().ensureColumns(['users'], '', 'testdb');
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', '');
    expect(mockReadColumns).not.toHaveBeenCalled();
  });

  it('does not resolve a different database against the active catalog', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'otherdb');
    expect(mockReadColumns).not.toHaveBeenCalled();
  });

  it('uses the catalog belonging to the tab-bound database', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    useSchemaStore
      .getState()
      .setLoadedTables(
        'tab_db',
        [{ name: 'users', tableType: 'table', schema: 'archive' }],
        'test-conn',
        { pinCurrentDatabase: false },
      );
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'tab_db');
    expect(mockReadColumns).toHaveBeenCalledWith('test-conn', [
      { database: 'tab_db', schema: 'archive', name: 'users' },
    ]);
    expect(schemaSnapshot(useSchemaStore, 'test-conn').currentDatabase).toBe('testdb');
  });

  it('ambiguous bare names are not resolved to the first or last schema', async () => {
    useSchemaStore.getState().setLoadedTables(
      'db',
      [
        { name: 'users', tableType: 'table', schema: 'public' },
        { name: 'users', tableType: 'table', schema: 'archive' },
      ],
      'test-conn',
    );
    expect(useSchemaStore.getState().schemaOfRelation('users', 'test-conn')).toBeNull();
    await useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'db');
    expect(mockReadColumns).not.toHaveBeenCalled();
    await useSchemaStore
      .getState()
      .ensureColumns(['users'], 'test-conn', 'db', { schema: 'archive' });
    await useSchemaStore
      .getState()
      .ensureColumns(['users'], 'test-conn', 'db', { schema: 'public' });
    expect(mockReadColumns).toHaveBeenCalledTimes(2);
    const values = Object.values(schemaSnapshot(useSchemaStore, 'test-conn').relationColumns);
    expect(values.map((value) => value.ref.schema).sort()).toEqual(['archive', 'public']);
  });

  it('concurrent callers share an in-flight relation', async () => {
    await useSchemaStore.getState().loadTables('testdb', 'test-conn');
    await Promise.all([
      useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'testdb'),
      useSchemaStore.getState().ensureColumns(['users'], 'test-conn', 'testdb'),
    ]);
    expect(mockReadColumns).toHaveBeenCalledOnce();
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
    useSchemaStore.getState().mergeNamespace(['db'], 'branch', ['hive'], 'test-conn');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({ db: { hive: {} } });
    expect(schemaSnapshot(useSchemaStore, 'test-conn').loadedPaths.has('db')).toBe(true);
  });

  it('cachePathItems stores get_tables rows by fetch path', async () => {
    const items: TableInfo[] = [
      { name: '558/hive', tableType: 'table', schema: 'CATALOG', rowCount: null },
    ];
    useSchemaStore.getState().cachePathItems('558', items, 'test-conn');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').pathItems['558']).toEqual(items);
  });

  it('registerPathAliases maps name to id', async () => {
    useSchemaStore
      .getState()
      .registerPathAliases([{ name: 'presto_afi_data', id: '558' }], 'test-conn');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').pathAliases).toEqual({
      presto_afi_data: '558',
    });
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({
      presto_afi_data: {},
    });
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceOwnedByPlugin).toBe(true);
  });

  it('setLoadedTables does not flatten namespace after registerPathAliases', async () => {
    useSchemaStore.getState().registerPathAliases([{ name: 'presto', id: '558' }], 'test-conn');
    useSchemaStore
      .getState()
      .mergeNamespace(['presto', 'hive', 'snap'], 'tables', ['t1'], 'test-conn');
    const before = structuredClone(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree);
    useSchemaStore
      .getState()
      .setLoadedTables(
        '558/hive/snap',
        [{ name: 't1', tableType: 'table', schema: 'snap', rowCount: null }],
        'test-conn',
      );
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceOwnedByPlugin).toBe(true);
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual(before);
    expect(schemaSnapshot(useSchemaStore, 'test-conn').tables.map((t) => t.name)).toEqual(['t1']);
  });

  it('setLoadedTables merges mysql-style database.table namespace', async () => {
    seedSchema(useSchemaStore, 'test-conn', { isMultiDatabase: true });
    useSchemaStore
      .getState()
      .setLoadedTables(
        'app',
        [{ name: 'users', tableType: 'table', schema: null, rowCount: null }],
        'test-conn',
      );
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({
      app: { users: [] },
    });
  });

  it('setLoadedTables groups postgresql schemas under database when multi-db', async () => {
    seedSchema(useSchemaStore, 'test-conn', { isMultiDatabase: true });
    useSchemaStore
      .getState()
      .setLoadedTables(
        'warehouse',
        [{ name: 't', tableType: 'table', schema: 'public', rowCount: null }],
        'test-conn',
      );
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({
      warehouse: { public: { t: [] } },
    });
  });

  it('setLoadedTables uses schema.table when single-db postgresql', async () => {
    seedSchema(useSchemaStore, 'test-conn', { isMultiDatabase: false });
    useSchemaStore
      .getState()
      .setLoadedTables(
        'warehouse',
        [{ name: 't', tableType: 'table', schema: 'public', rowCount: null }],
        'test-conn',
      );
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({
      public: { t: [] },
    });
  });

  it('setLoadedTables includes views in namespace table leaves', async () => {
    seedSchema(useSchemaStore, 'test-conn', { isMultiDatabase: false });
    useSchemaStore.getState().setLoadedTables(
      'warehouse',
      [
        { name: 't', tableType: 'table', schema: 'public', rowCount: null },
        { name: 'v', tableType: 'view', schema: 'public', rowCount: null },
      ],
      'test-conn',
    );
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({
      public: { t: [], v: [] },
    });
    expect(schemaSnapshot(useSchemaStore, 'test-conn').views.map((v) => v.name)).toEqual(['v']);
  });

  it('setLoadedTables replaces dropped tables instead of merging them back', async () => {
    seedSchema(useSchemaStore, 'test-conn', { isMultiDatabase: true });
    useSchemaStore.getState().setLoadedTables(
      'app',
      [
        { name: 'users', tableType: 'table', schema: null, rowCount: null },
        { name: 'orders', tableType: 'table' },
      ],
      'test-conn',
    );
    useSchemaStore
      .getState()
      .setLoadedTables(
        'app',
        [{ name: 'users', tableType: 'table', schema: null, rowCount: null }],
        'test-conn',
      );
    expect(schemaSnapshot(useSchemaStore, 'test-conn').tables.map((t) => t.name)).toEqual([
      'users',
    ]);
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({
      app: { users: [] },
    });
  });

  it('removeRelation drops the table from lists and namespace immediately', async () => {
    seedSchema(useSchemaStore, 'test-conn', { isMultiDatabase: true });
    useSchemaStore.getState().setLoadedTables(
      'app',
      [
        { name: 'users', tableType: 'table', schema: null, rowCount: null },
        { name: 'orders', tableType: 'table' },
      ],
      'test-conn',
    );
    useSchemaStore.getState().removeRelation('orders', 'test-conn');
    expect(schemaSnapshot(useSchemaStore, 'test-conn').tables.map((t) => t.name)).toEqual([
      'users',
    ]);
    expect(schemaSnapshot(useSchemaStore, 'test-conn').namespaceTree).toEqual({
      app: { users: [] },
    });
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
    let release!: (value: TableInfo[]) => void;
    vi.mocked(databaseCommands.listTables).mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );

    seedSchema(useSchemaStore, 'c1', {
      databaseType: 'mysql',
      currentDatabase: 'app',
      databases: ['app'],
      isMultiDatabase: false,
      loadedPaths: new Set(),
      ensuringCount: 0,
    });

    const pending = useSchemaStore.getState().ensureNamespacePath(['app'], 'c1');
    await vi.waitFor(() => {
      expect(typeof release).toBe('function');
    });
    expect(schemaSnapshot(useSchemaStore, 'c1').ensuringCount).toBe(1);

    release([]);
    await pending;
    expect(schemaSnapshot(useSchemaStore, 'c1').ensuringCount).toBe(0);
  });

  it('does not increment when the path is already loaded', async () => {
    seedSchema(useSchemaStore, 'c1', {
      databaseType: 'mysql',
      currentDatabase: 'app',
      databases: ['app'],
      isMultiDatabase: false,
      loadedPaths: new Set(['app']),
      ensuringCount: 0,
    });

    await useSchemaStore.getState().ensureNamespacePath(['app'], 'c1');
    expect(schemaSnapshot(useSchemaStore, 'c1').ensuringCount).toBe(0);
    expect(databaseCommands.listTables).not.toHaveBeenCalled();
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
    vi.mocked(databaseCommands.listTables).mockImplementation(async (_conn, db) => {
      if (db === 'db_a') {
        return [{ name: 'users_a', tableType: 'table' }];
      }
      return [{ name: 'users_b', tableType: 'table' }];
    });

    await useSchemaStore.getState().loadForConnection('conn-a', {
      databaseType: 'sqlite',
    });
    await useSchemaStore.getState().loadForConnection('conn-b', {
      databaseType: 'sqlite',
    });

    expect(useSchemaStore.getState().activeDbSessionId).toBe('conn-b');
    expect(schemaSnapshot(useSchemaStore, 'conn-b').tables.map((t) => t.name)).toEqual(['users_b']);

    useSchemaStore.getState().setActiveConnection('conn-a');
    expect(useSchemaStore.getState().activeDbSessionId).toBe('conn-a');
    expect(schemaSnapshot(useSchemaStore, 'conn-a').tables.map((t) => t.name)).toEqual(['users_a']);
    expect(schemaSnapshot(useSchemaStore, 'conn-a').currentDatabase).toBe('db_a');
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
    expect(useSchemaStore.getState().activeDbSessionId).toBe('conn-b');
  });

  it('removeConnection drops cached schema and clears active when removed', async () => {
    await useSchemaStore.getState().loadForConnection('conn-a', {
      databaseType: 'sqlite',
      skipLoadTables: true,
    });

    useSchemaStore.getState().removeConnection('conn-a');
    expect(useSchemaStore.getState().getConnectionSchema('conn-a')).toBeUndefined();
    expect(useSchemaStore.getState().activeDbSessionId).toBeNull();
    expect(useSchemaStore.getState().schemas.has('conn-a')).toBe(false);
  });

  it('requires the target session even when another session is active', async () => {
    useSchemaStore.getState().setLoadedTables('db', [{ name: 'a', tableType: 'table' }], 'conn-a');
    useSchemaStore.getState().setLoadedTables('db', [{ name: 'b', tableType: 'table' }], 'conn-b');
    useSchemaStore.getState().setActiveConnection('conn-b');
    useSchemaStore.getState().setSelected('table:a', 'conn-a');
    useSchemaStore.getState().toggleExpand('db:db', 'conn-a');
    expect(schemaSnapshot(useSchemaStore, 'conn-a').selectedId).toBe('table:a');
    expect(schemaSnapshot(useSchemaStore, 'conn-a').expanded.has('db:db')).toBe(true);
    expect(schemaSnapshot(useSchemaStore, 'conn-b').selectedId).toBeNull();
    expect(useSchemaStore.getState().activeDbSessionId).toBe('conn-b');
    expect(useSchemaStore.getState()).not.toHaveProperty('dbSessionId');
    expect(useSchemaStore.getState()).not.toHaveProperty('tables');
    expect(useSchemaStore.getState()).not.toHaveProperty('columnMap');
  });

  it('ensureColumns uses per-connection columnInflight', async () => {
    seedSchema(useSchemaStore, 'conn-a', {});
    useSchemaStore
      .getState()
      .setLoadedTables('db', [{ name: 'users', tableType: 'table' }], 'conn-a');

    seedSchema(useSchemaStore, 'conn-b', {});
    useSchemaStore
      .getState()
      .setLoadedTables('db', [{ name: 'orders', tableType: 'table' }], 'conn-b');

    vi.mocked(mockReadColumns).mockClear();
    await useSchemaStore.getState().ensureColumns(['users'], 'conn-a', 'db');
    await useSchemaStore.getState().ensureColumns(['orders'], 'conn-b', 'db');

    expect(mockReadColumns).toHaveBeenCalledWith('conn-a', [
      { name: 'users', database: 'db', schema: null },
    ]);
    expect(mockReadColumns).toHaveBeenCalledWith('conn-b', [
      { name: 'orders', database: 'db', schema: null },
    ]);
    expect(schemaSnapshot(useSchemaStore, 'conn-a').columnMap).toEqual({
      users: ['id', 'name'],
    });
    expect(schemaSnapshot(useSchemaStore, 'conn-b').columnMap).toEqual({
      orders: ['id', 'name'],
    });
  });
});

describe('parsePathHierarchyDatabaseEntry / wapp namespace bootstrap', () => {
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
