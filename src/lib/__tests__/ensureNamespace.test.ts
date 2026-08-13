import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ensureNamespacePath,
  namespaceEnsurePending,
  namespacePathLoaded,
  pathNavSegment,
  type EnsureDeps,
} from '../ensureNamespace';
import type { TableInfo } from '../../types';

function makeDeps(overrides: Partial<EnsureDeps> = {}): EnsureDeps {
  const pathAliases: Record<string, string> = { ...(overrides.pathAliases ?? { presto: '558' }) };
  const loadedPaths = new Set<string>(overrides.loadedPaths ? [...overrides.loadedPaths] : []);
  const pathItems: Record<string, TableInfo[]> = { ...(overrides.pathItems ?? {}) };
  const mergeNamespace =
    overrides.mergeNamespace ??
    vi.fn((segments: string[], _kind: 'branch' | 'tables', _names: string[]) => {
      loadedPaths.add(segments.join('/') || '');
    });
  const cachePathItems =
    overrides.cachePathItems ??
    vi.fn((fetchPath: string, items: TableInfo[]) => {
      pathItems[fetchPath] = items;
    });
  const registerPathAliases =
    overrides.registerPathAliases ??
    vi.fn((entries: { name: string; id: string }[]) => {
      for (const { name, id } of entries) pathAliases[name] = id;
      mergeNamespace(
        [],
        'branch',
        entries.map((e) => e.name),
      );
    });

  return {
    connectionId: 'conn-1',
    databaseType: 'path_driver',
    isMultiDatabase: true,
    namespaceTree: {},
    tables: [],
    databases: [],
    currentDatabase: null,
    getDatabases: vi.fn().mockResolvedValue([]),
    getTables: vi.fn().mockResolvedValue([]),
    useDatabase: vi.fn().mockResolvedValue(undefined),
    ...overrides,
    pathAliases: overrides.pathAliases ?? pathAliases,
    loadedPaths: overrides.loadedPaths ?? loadedPaths,
    pathItems: overrides.pathItems ?? pathItems,
    mergeNamespace,
    cachePathItems,
    registerPathAliases,
  };
}

vi.mock('../databaseTypes', () => ({
  DB_REGISTRY: {
    path_driver: {
      namespaceEnsure: 'path-hierarchy',
      namespaceOwnedByPlugin: true,
    },
    postgresql: {
      namespaceEnsure: 'postgresql',
    },
    mysql: {
      namespaceEnsure: 'default-sql',
    },
  },
}));

describe('ensureNamespacePath — path-hierarchy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('[] seeds branches from registered pathAliases only', async () => {
    const deps = makeDeps({
      pathAliases: { presto: '558', other: '9' },
    });

    await ensureNamespacePath([], deps);

    expect(deps.getDatabases).not.toHaveBeenCalled();
    expect(deps.mergeNamespace).toHaveBeenCalledWith([], 'branch', ['presto', 'other']);
    expect(deps.loadedPaths.has('')).toBe(true);
  });

  it('[] with currentDatabase fetches that root instead of only seeding aliases', async () => {
    const deps = makeDeps({
      currentDatabase: 'presto',
      databases: ['presto'],
      getTables: vi
        .fn()
        .mockResolvedValue([
          { name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null },
        ] satisfies TableInfo[]),
    });
    await ensureNamespacePath([], deps);
    expect(deps.getTables).toHaveBeenCalledWith('conn-1', '558');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['presto'], 'branch', ['hive']);
  });

  it("['presto'] fetches catalogs with aliased root id", async () => {
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

  it("['presto','hive','snap'] merges table leaves", async () => {
    const deps = makeDeps({
      getTables: vi.fn().mockResolvedValue([
        { name: 't1', schema: 'snap', tableType: 'table', rowCount: null },
        { name: 'v1', schema: 'snap', tableType: 'view', rowCount: null },
      ] satisfies TableInfo[]),
    });

    await ensureNamespacePath(['presto', 'hive', 'snap'], deps);

    expect(deps.getTables).toHaveBeenCalledWith('conn-1', '558/hive/snap');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['presto', 'hive', 'snap'], 'tables', ['t1']);
  });

  it('skips fetch when path already loaded', async () => {
    const deps = makeDeps();
    deps.loadedPaths.add('presto');
    await ensureNamespacePath(['presto'], deps);
    expect(deps.getTables).not.toHaveBeenCalled();
  });

  it('reuses cached path items without calling getTables', async () => {
    const items = [
      { name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null },
    ] satisfies TableInfo[];
    const deps = makeDeps({ pathItems: { '558': items } });
    await ensureNamespacePath(['presto'], deps);
    expect(deps.getTables).not.toHaveBeenCalled();
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['presto'], 'branch', ['hive']);
  });

  it('writes getTables results into the shared pathItems cache', async () => {
    const items = [
      { name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null },
    ] satisfies TableInfo[];
    const deps = makeDeps({ getTables: vi.fn().mockResolvedValue(items) });
    await ensureNamespacePath(['presto'], deps);
    expect(deps.cachePathItems).toHaveBeenCalledWith('558', items);
    expect(deps.pathItems['558']).toEqual(items);
  });

  it('dedupes concurrent ensures for the same key', async () => {
    let resolveTables!: (v: TableInfo[]) => void;
    const tablesPromise = new Promise<TableInfo[]>((resolve) => {
      resolveTables = resolve;
    });
    const deps = makeDeps({
      getTables: vi.fn().mockReturnValue(tablesPromise),
    });

    const p1 = ensureNamespacePath(['presto'], deps);
    const p2 = ensureNamespacePath(['presto'], deps);
    resolveTables([{ name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null }]);
    await Promise.all([p1, p2]);
    expect(deps.getTables).toHaveBeenCalledTimes(1);
  });

  it('swallows errors without marking loaded', async () => {
    const deps = makeDeps({
      getTables: vi.fn().mockRejectedValue(new Error('network')),
    });
    await ensureNamespacePath(['presto'], deps);
    expect(deps.loadedPaths.has('presto')).toBe(false);
  });
});

describe('pathNavSegment', () => {
  it('returns last path segment with optional root prefix strip', () => {
    expect(
      pathNavSegment(
        { name: '558/hive/snap', schema: 'SCHEMA', tableType: 'table', rowCount: null },
        '558',
      ),
    ).toBe('snap');
    expect(
      pathNavSegment(
        { name: 'catalog', schema: 'CATALOG', tableType: 'table', rowCount: null },
        '558',
      ),
    ).toBe('catalog');
  });
});

describe('namespacePathLoaded', () => {
  it('returns true when path loaded or tree has child', () => {
    const deps = makeDeps({ loadedPaths: new Set(['app']) });
    expect(namespacePathLoaded(deps, ['app'])).toBe(true);
    const treeDeps = makeDeps({
      loadedPaths: new Set(),
      namespaceTree: { other: { kind: 'branch', children: {} } },
    });
    expect(namespacePathLoaded(treeDeps, [])).toBe(true);
  });
});

describe('ensureNamespacePath — default-sql (mysql)', () => {
  it('[] lists databases via getDatabases', async () => {
    const deps = makeDeps({
      databaseType: 'mysql',
      pathAliases: {},
      getDatabases: vi.fn().mockResolvedValue(['app', 'test']),
    });
    await ensureNamespacePath([], deps);
    expect(deps.mergeNamespace).toHaveBeenCalledWith([], 'branch', ['app', 'test']);
  });

  it("['app'] uses useDatabase + getTables and excludes views", async () => {
    const deps = makeDeps({
      databaseType: 'mysql',
      pathAliases: {},
      getTables: vi.fn().mockResolvedValue([
        { name: 'users', schema: null, tableType: 'table', rowCount: null },
        { name: 'v_users', schema: null, tableType: 'view', rowCount: null },
      ] satisfies TableInfo[]),
    });

    await ensureNamespacePath(['app'], deps);

    expect(deps.useDatabase).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.getTables).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['app'], 'tables', ['users']);
  });

  it('[] with currentDatabase also loads that database tables', async () => {
    const deps = makeDeps({
      databaseType: 'mysql',
      pathAliases: {},
      currentDatabase: 'app',
      databases: ['app', 'test'],
      getDatabases: vi.fn().mockResolvedValue(['app', 'test']),
      getTables: vi
        .fn()
        .mockResolvedValue([
          { name: 'users', schema: null, tableType: 'table', rowCount: null },
        ] satisfies TableInfo[]),
    });
    await ensureNamespacePath([], deps);
    expect(deps.getTables).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['app'], 'tables', ['users']);
  });
});

describe('ensureNamespacePath — postgresql', () => {
  it('[] multi-db lists databases', async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      pathAliases: {},
      isMultiDatabase: true,
      getDatabases: vi.fn().mockResolvedValue(['db1', 'db2']),
    });
    await ensureNamespacePath([], deps);
    expect(deps.mergeNamespace).toHaveBeenCalledWith([], 'branch', ['db1', 'db2']);
  });

  it('[] single-db groups tables by schema', async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      pathAliases: {},
      isMultiDatabase: false,
      databases: ['app'],
      currentDatabase: 'app',
      getTables: vi.fn().mockResolvedValue([
        { name: 'users', schema: 'public', tableType: 'table', rowCount: null },
        { name: 'v_users', schema: 'public', tableType: 'view', rowCount: null },
        { name: 'logs', schema: 'audit', tableType: 'table', rowCount: null },
      ] satisfies TableInfo[]),
    });
    await ensureNamespacePath([], deps);
    expect(deps.useDatabase).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.mergeNamespace).toHaveBeenCalledWith([], 'branch', ['public', 'audit']);
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['public'], 'tables', ['users', 'v_users']);
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['audit'], 'tables', ['logs']);
  });

  it("['db1'] multi-db loads schemas under database", async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      pathAliases: {},
      isMultiDatabase: true,
      getTables: vi
        .fn()
        .mockResolvedValue([
          { name: 't1', schema: 'public', tableType: 'table', rowCount: null },
        ] satisfies TableInfo[]),
    });
    await ensureNamespacePath(['db1'], deps);
    expect(deps.useDatabase).toHaveBeenCalledWith('conn-1', 'db1');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['db1'], 'branch', ['public']);
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['db1', 'public'], 'tables', ['t1']);
  });

  it("['public'] single-db uses in-memory tables when available", async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      pathAliases: {},
      isMultiDatabase: false,
      tables: [{ name: 'users', schema: 'public', tableType: 'table', rowCount: null }],
      databases: ['app'],
    });
    await ensureNamespacePath(['public'], deps);
    expect(deps.getTables).not.toHaveBeenCalled();
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['public'], 'tables', ['users']);
  });

  it("['public'] single-db fetches tables when not in memory", async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      pathAliases: {},
      isMultiDatabase: false,
      tables: [],
      databases: ['app'],
      currentDatabase: 'app',
      getTables: vi.fn().mockResolvedValue([
        { name: 'users', schema: 'public', tableType: 'table', rowCount: null },
        { name: 'v_users', schema: 'public', tableType: 'view', rowCount: null },
        { name: 'other', schema: 'audit', tableType: 'table', rowCount: null },
      ] satisfies TableInfo[]),
    });
    await ensureNamespacePath(['public'], deps);
    expect(deps.useDatabase).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.getTables).toHaveBeenCalledWith('conn-1', 'app');
    expect(deps.mergeNamespace).toHaveBeenCalledWith(['public'], 'tables', ['users']);
  });

  it('falls back to postgresql strategy when meta missing', async () => {
    const deps = makeDeps({
      databaseType: 'postgresql',
      pathAliases: {},
      isMultiDatabase: true,
      getDatabases: vi.fn().mockResolvedValue(['only']),
    });
    await ensureNamespacePath([], deps);
    expect(deps.getDatabases).toHaveBeenCalled();
  });

  it('uses default-sql when databaseType is null', async () => {
    const deps = makeDeps({
      databaseType: null,
      pathAliases: {},
      getDatabases: vi.fn().mockResolvedValue(['db']),
    });
    await ensureNamespacePath([], deps);
    expect(deps.mergeNamespace).toHaveBeenCalledWith([], 'branch', ['db']);
  });
});

describe('namespaceEnsurePending', () => {
  it('is pending until the resolved path is in loadedPaths', () => {
    const deps = makeDeps({
      databaseType: 'mysql',
      pathAliases: {},
      currentDatabase: 'app',
      databases: ['app'],
      loadedPaths: new Set(),
    });
    expect(namespaceEnsurePending([], deps)).toBe(true);
    expect(namespaceEnsurePending(['app'], deps)).toBe(true);
  });

  it('is not pending after the path is marked loaded', () => {
    const deps = makeDeps({
      databaseType: 'mysql',
      pathAliases: {},
      currentDatabase: 'app',
      databases: ['app'],
      loadedPaths: new Set(['app']),
    });
    expect(namespaceEnsurePending([], deps)).toBe(false);
    expect(namespaceEnsurePending(['app'], deps)).toBe(false);
  });
});
