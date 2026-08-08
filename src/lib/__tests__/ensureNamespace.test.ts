import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { TableInfo } from '../../types';
import { ensureNamespacePath, type EnsureDeps } from '../ensureNamespace';

function makeDeps(overrides: Partial<EnsureDeps> = {}): EnsureDeps {
  const loadedPaths = new Set<string>();
  const mergeNamespace = vi.fn((_segments: string[], _kind: 'branch' | 'tables', _names: string[]) => {
    // mirror store: merge marks path loaded
    loadedPaths.add(_segments.join('/') || '');
  });
  const registerSupersetDatabases = vi.fn((entries: { name: string; id: string }[]) => {
    mergeNamespace(
      [],
      'branch',
      entries.map((e) => e.name),
    );
  });

  return {
    connectionId: 'conn-1',
    databaseType: 'superset',
    isMultiDatabase: false,
    loadedPaths,
    supersetDbIds: { presto: '558' },
    namespaceTree: {},
    tables: [],
    databases: [],
    currentDatabase: null,
    mergeNamespace,
    registerSupersetDatabases,
    getDatabases: vi.fn(),
    getTables: vi.fn(),
    useDatabase: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

describe('ensureNamespacePath — superset', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('[] loads databases via getDatabases and registerSupersetDatabases', async () => {
    const deps = makeDeps({
      supersetDbIds: {},
      getDatabases: vi.fn().mockResolvedValue(['558:presto (Presto)']),
    });

    await ensureNamespacePath([], deps);

    expect(deps.getDatabases).toHaveBeenCalledWith('conn-1');
    expect(deps.registerSupersetDatabases).toHaveBeenCalledWith([{ name: 'presto', id: '558' }]);
    expect(deps.loadedPaths.has('')).toBe(true);
  });

  it("['presto'] lazily resolves db id when supersetDbIds empty but root loaded", async () => {
    const deps = makeDeps({
      supersetDbIds: {},
      getDatabases: vi.fn().mockResolvedValue(['558:presto (hive)']),
      getTables: vi.fn().mockResolvedValue([
        { name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null },
        { name: '558/iceberg', schema: 'CATALOG', tableType: 'table', rowCount: null },
      ] satisfies TableInfo[]),
    });
    deps.loadedPaths.add('');

    await ensureNamespacePath(['presto'], deps);

    expect(deps.getDatabases).toHaveBeenCalledWith('conn-1');
    expect(deps.registerSupersetDatabases).toHaveBeenCalledWith([{ name: 'presto', id: '558' }]);
    expect(deps.getTables).toHaveBeenCalledWith('conn-1', '558');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['presto'], 'branch', ['hive', 'iceberg']);
    expect(deps.loadedPaths.has('presto')).toBe(true);
  });

  it("['presto'] fetches catalogs with db id and merges CATALOG segments", async () => {
    const deps = makeDeps({
      getTables: vi.fn().mockResolvedValue([
        { name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null },
        { name: '558/iceberg', schema: 'CATALOG', tableType: 'table', rowCount: null },
      ] satisfies TableInfo[]),
    });

    await ensureNamespacePath(['presto'], deps);

    expect(deps.getTables).toHaveBeenCalledWith('conn-1', '558');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['presto'], 'branch', ['hive', 'iceberg']);
    expect(deps.loadedPaths.has('presto')).toBe(true);
  });

  it("['presto','hive'] fetches schemas at 558/hive", async () => {
    const deps = makeDeps({
      getTables: vi.fn().mockResolvedValue([
        { name: '558/hive/snap', schema: 'SCHEMA', tableType: 'table', rowCount: null },
        { name: '558/hive/raw', schema: 'SCHEMA', tableType: 'table', rowCount: null },
      ] satisfies TableInfo[]),
    });

    await ensureNamespacePath(['presto', 'hive'], deps);

    expect(deps.getTables).toHaveBeenCalledWith('conn-1', '558/hive');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['presto', 'hive'], 'branch', ['snap', 'raw']);
  });

  it("['presto','hive','snap'] fetches tables at 558/hive/snap", async () => {
    const deps = makeDeps({
      getTables: vi.fn().mockResolvedValue([
        { name: 'events', schema: 'snap', tableType: 'table', rowCount: null },
        { name: 'metrics', schema: 'snap', tableType: 'table', rowCount: null },
      ] satisfies TableInfo[]),
    });

    await ensureNamespacePath(['presto', 'hive', 'snap'], deps);

    expect(deps.getTables).toHaveBeenCalledWith('conn-1', '558/hive/snap');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(
      ['presto', 'hive', 'snap'],
      'tables',
      ['events', 'metrics'],
    );
  });
});

describe('ensureNamespacePath — postgresql', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('[] single-db fetches schemas from current database', async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      isMultiDatabase: false,
      supersetDbIds: {},
      databases: ['app'],
      currentDatabase: 'app',
      getTables: vi.fn().mockResolvedValue([
        { name: 'users', tableType: 'table', schema: 'public', rowCount: null },
        { name: 'orders', tableType: 'table', schema: 'sales', rowCount: null },
      ] satisfies TableInfo[]),
    });

    await ensureNamespacePath([], deps);

    expect(deps.useDatabase).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.getTables).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.mergeNamespace).toHaveBeenCalledWith([], 'branch', ['public', 'sales']);
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['public'], 'tables', ['users']);
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['sales'], 'tables', ['orders']);
  });

  it('[] multi-db loads database branches', async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      isMultiDatabase: true,
      supersetDbIds: {},
      getDatabases: vi.fn().mockResolvedValue(['db1', 'db2']),
    });

    await ensureNamespacePath([], deps);

    expect(deps.getDatabases).toHaveBeenCalledWith('conn-1');
    expect(deps.mergeNamespace).toHaveBeenCalledWith([], 'branch', ['db1', 'db2']);
    expect(deps.getTables).not.toHaveBeenCalled();
  });
});

describe('ensureNamespacePath — mysql', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("['app'] calls useDatabase and merges table leaves", async () => {
    const deps = makeDeps({
      databaseType: 'mysql',
      isMultiDatabase: true,
      supersetDbIds: {},
      getTables: vi.fn().mockResolvedValue([
        { name: 'users', tableType: 'table', schema: null, rowCount: null },
        { name: 'orders', tableType: 'table', schema: null, rowCount: null },
        { name: 'v1', tableType: 'view', schema: null, rowCount: null },
      ] satisfies TableInfo[]),
    });

    await ensureNamespacePath(['app'], deps);

    expect(deps.useDatabase).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.getTables).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['app'], 'tables', ['users', 'orders']);
  });
});

describe('ensureNamespacePath — shared behavior', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('skips fetch when path is already in loadedPaths', async () => {
    const deps = makeDeps({
      loadedPaths: new Set(['presto']),
      getTables: vi.fn(),
    });

    await ensureNamespacePath(['presto'], deps);

    expect(deps.getTables).not.toHaveBeenCalled();
  });

  it('dedupes concurrent ensures for the same key', async () => {
    let resolveTables!: (value: TableInfo[]) => void;
    const deps = makeDeps({
      getTables: vi.fn(
        () =>
          new Promise<TableInfo[]>((resolve) => {
            resolveTables = resolve;
          }),
      ),
    });

    const p1 = ensureNamespacePath(['presto'], deps);
    const p2 = ensureNamespacePath(['presto'], deps);

    expect(deps.getTables).toHaveBeenCalledTimes(1);

    resolveTables([
      { name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null },
    ]);
    await Promise.all([p1, p2]);
  });

  it('swallows errors without marking path loaded', async () => {
    const deps = makeDeps({
      getTables: vi.fn().mockRejectedValue(new Error('network')),
    });

    await ensureNamespacePath(['presto'], deps);

    expect(deps.loadedPaths.has('presto')).toBe(false);
  });
});
