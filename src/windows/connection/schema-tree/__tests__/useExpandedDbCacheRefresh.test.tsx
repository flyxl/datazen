import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useExpandedDbCacheRefresh, type ExpandedDbCacheRefreshOptions } from '../useExpandedDbCacheRefresh';
import { useSchemaStore } from '../../../../stores/schemaStore';

/** Shape stored per connection inside schemaStore.schemas. */
function schemaEntry(databases: string[], epoch: number) {
  return {
    currentDatabase: databases[0] ?? null,
    databases,
    databaseType: 'postgresql',
    isMultiDatabase: databases.length > 1,
    tables: [],
    views: [],
    schemaNames: [],
    columnMap: {},
    namespaceTree: {},
    loadedPaths: new Set<string>(),
    pathItems: {},
    pathAliases: {},
    namespaceOwnedByPlugin: false,
    schemaEpoch: epoch,
    expanded: new Set<string>(),
    selectedId: null,
    loading: false,
    ensuringCount: 0,
    error: null,
    columnInflight: new Set<string>(),
  };
}

type HandlerMocks = {
  loadTablesForDb: ReturnType<typeof vi.fn>;
  loadObjectsForCat: ReturnType<typeof vi.fn>;
  clearCaches: ReturnType<typeof vi.fn>;
};

function baseOpts(handlers: HandlerMocks): ExpandedDbCacheRefreshOptions {
  return {
    activeConnections: { 'cfg-1': { dbSessionId: 'conn-1' } },
    expandedDbs: new Set(['cfg-1::db-a', 'cfg-1::db-b']),
    expandedCats: new Set(),
    ...handlers,
  };
}

describe('useExpandedDbCacheRefresh', () => {
  beforeEach(() => {
    useSchemaStore.getState().reset();
  });

  it('reloads every expanded db of the changed connection without useDatabase', async () => {
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', schemaEntry(['a', 'b'], 0)),
    }));

    const handlers = {
      loadTablesForDb: vi.fn().mockResolvedValue(undefined),
      loadObjectsForCat: vi.fn().mockResolvedValue(undefined),
      clearCaches: vi.fn(),
    };

    const { rerun } = renderHookWithDeps(baseOpts(handlers));
    expect(handlers.clearCaches).not.toHaveBeenCalled();

    // Admin op bumps the epoch → caches invalidated + each expanded db reloaded.
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', schemaEntry(['a', 'b'], 1)),
    }));
    rerun();
    await vi.waitFor(() => {
      expect(handlers.loadTablesForDb).toHaveBeenCalledTimes(2);
    });
    expect(handlers.loadTablesForDb).toHaveBeenCalledWith('conn-1', 'db-a');
    expect(handlers.loadTablesForDb).toHaveBeenCalledWith('conn-1', 'db-b');
    expect(handlers.clearCaches).toHaveBeenCalledWith('conn-1', 'cfg-1');
  });

  it('does not reload when only unrelated fields change (no fingerprint diff)', async () => {
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', schemaEntry(['a'], 0)),
    }));
    const handlers = {
      loadTablesForDb: vi.fn().mockResolvedValue(undefined),
      loadObjectsForCat: vi.fn().mockResolvedValue(undefined),
      clearCaches: vi.fn(),
    };
    const { rerun } = renderHookWithDeps(baseOpts(handlers));

    const entry = schemaEntry(['a'], 0);
    (entry as unknown as { loading: boolean }).loading = true;
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', entry),
    }));
    rerun();
    await Promise.resolve();
    expect(handlers.loadTablesForDb).not.toHaveBeenCalled();
    expect(handlers.loadObjectsForCat).not.toHaveBeenCalled();
    expect(handlers.clearCaches).not.toHaveBeenCalled();
  });

  it('F1-BUG-005: clears caches then schedules object-category reloads in the same pass', async () => {
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-sql', schemaEntry(['/data/app.db'], 0)),
    }));

    const handlers = {
      loadTablesForDb: vi.fn().mockResolvedValue(undefined),
      loadObjectsForCat: vi.fn().mockResolvedValue(undefined),
      clearCaches: vi.fn(),
    };

    const opts: ExpandedDbCacheRefreshOptions = {
      ...baseOpts(handlers),
      activeConnections: { 'cfg-sql': { dbSessionId: 'conn-sql' } },
      expandedDbs: new Set(['cfg-sql::/data/app.db']),
      expandedCats: new Set([
        'cfg-sql::/data/app.db::procedure',
        'cfg-sql::/data/app.db::main::function',
        'cfg-sql::/data/app.db::tables',
        'cfg-sql::/data/app.db::views',
        'cfg-other::/data/other.db::procedure',
      ]),
    };

    const { rerun } = renderHookWithDeps(opts);

    // Epoch bump — the exact store transition refreshConnection triggers via
    // loadForConnection → loadTables.
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-sql', schemaEntry(['/data/app.db'], 1)),
    }));
    rerun();

    await vi.waitFor(() => {
      expect(handlers.loadObjectsForCat).toHaveBeenCalledTimes(2);
    });
    // Expanded categories of the changed connection are reloaded; tables/views
    // pseudo-categories and other connections' categories are untouched.
    expect(handlers.loadObjectsForCat).toHaveBeenCalledWith(
      'conn-sql',
      'cfg-sql::/data/app.db::procedure',
      'procedure',
    );
    expect(handlers.loadObjectsForCat).toHaveBeenCalledWith(
      'conn-sql',
      'cfg-sql::/data/app.db::main::function',
      'function',
    );
    expect(handlers.loadObjectsForCat).not.toHaveBeenCalledWith(
      'conn-sql',
      'cfg-sql::/data/app.db::tables',
      'tables',
    );
    expect(handlers.loadObjectsForCat).not.toHaveBeenCalledWith(
      'conn-sql',
      'cfg-other::/data/other.db::procedure',
      'procedure',
    );
    expect(handlers.loadTablesForDb).toHaveBeenCalledWith('conn-sql', '/data/app.db');

    // Ordering guarantee: the invalidation strictly precedes every recovery
    // reload scheduled by the same wave — a clear can never land after the
    // reloads it was supposed to precede (the F1-BUG-005 race).
    const clearOrder = handlers.clearCaches.mock.invocationCallOrder[0];
    expect(handlers.clearCaches).toHaveBeenCalledWith('conn-sql', 'cfg-sql');
    for (const order of handlers.loadObjectsForCat.mock.invocationCallOrder) {
      expect(clearOrder).toBeLessThan(order);
    }
    for (const order of handlers.loadTablesForDb.mock.invocationCallOrder) {
      expect(clearOrder).toBeLessThan(order);
    }
  });

  function renderHookWithDeps(opts: ExpandedDbCacheRefreshOptions) {
    const view = renderHook((p: ExpandedDbCacheRefreshOptions) => useExpandedDbCacheRefresh(p), {
      initialProps: opts,
    });
    return {
      rerun: () => view.rerender({ ...opts }),
    };
  }
});
