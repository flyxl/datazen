import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useExpandedDbCacheRefresh } from '../useExpandedDbCacheRefresh';
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

describe('useExpandedDbCacheRefresh', () => {
  beforeEach(() => {
    useSchemaStore.getState().reset();
  });

  it('reloads every expanded db of the changed connection without useDatabase', async () => {
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', schemaEntry(['a', 'b'], 0)),
    }));

    const loadTablesForDb = vi.fn().mockResolvedValue(undefined);
    const clearCaches = vi.fn();

    const { rerun } = renderHookWithDeps({ loadTablesForDb, clearCaches });
    expect(clearCaches).not.toHaveBeenCalled();

    // Admin op bumps the epoch → caches invalidated + each expanded db reloaded.
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', schemaEntry(['a', 'b'], 1)),
    }));
    rerun();
    await vi.waitFor(() => {
      expect(loadTablesForDb).toHaveBeenCalledTimes(2);
    });
    expect(loadTablesForDb).toHaveBeenCalledWith('conn-1', 'db-a');
    expect(loadTablesForDb).toHaveBeenCalledWith('conn-1', 'db-b');
    expect(clearCaches).toHaveBeenCalledWith('conn-1');
  });

  it('does not reload when only unrelated fields change (no fingerprint diff)', async () => {
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', schemaEntry(['a'], 0)),
    }));
    const loadTablesForDb = vi.fn().mockResolvedValue(undefined);
    const clearCaches = vi.fn();
    const { rerun } = renderHookWithDeps({ loadTablesForDb, clearCaches });

    const entry = schemaEntry(['a'], 0);
    (entry as unknown as { loading: boolean }).loading = true;
    useSchemaStore.setState((s) => ({
      schemas: new Map(s.schemas).set('conn-1', entry),
    }));
    rerun();
    await Promise.resolve();
    expect(loadTablesForDb).not.toHaveBeenCalled();
    expect(clearCaches).not.toHaveBeenCalled();
  });

  function renderHookWithDeps(handlers: {
    loadTablesForDb: ReturnType<typeof vi.fn>;
    clearCaches: ReturnType<typeof vi.fn>;
  }) {
    const opts = {
      activeConnections: { 'cfg-1': { dbSessionId: 'conn-1' } },
      expandedDbs: new Set(['cfg-1::db-a', 'cfg-1::db-b']),
      ...handlers,
    };
    const view = renderHook((p: typeof opts) => useExpandedDbCacheRefresh(p), {
      initialProps: opts,
    });
    return {
      rerun: () => view.rerender({ ...opts }),
    };
  }
});
