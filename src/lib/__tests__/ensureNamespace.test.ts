import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ensureNamespacePath, type EnsureDeps } from '../ensureNamespace';
import type { TableInfo } from '../../types';

function makeDeps(overrides: Partial<EnsureDeps> = {}): EnsureDeps {
  const pathAliases: Record<string, string> = { ...(overrides.pathAliases ?? { presto: '558' }) };
  const loadedPaths = new Set<string>(overrides.loadedPaths ? [...overrides.loadedPaths] : []);
  const mergeNamespace =
    overrides.mergeNamespace ??
    vi.fn((segments: string[], _kind: 'branch' | 'tables', _names: string[]) => {
      loadedPaths.add(segments.join('/') || '');
    });
  const registerPathAliases =
    overrides.registerPathAliases ??
    vi.fn((entries: { name: string; id: string }[]) => {
      for (const { name, id } of entries) pathAliases[name] = id;
      mergeNamespace([], 'branch', entries.map((e) => e.name));
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
    mergeNamespace,
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
    resolveTables([
      { name: '558/hive', schema: 'CATALOG', tableType: 'table', rowCount: null },
    ]);
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

describe('ensureNamespacePath — default-sql (mysql)', () => {
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
});
