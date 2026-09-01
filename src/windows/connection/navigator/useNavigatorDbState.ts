import { useCallback, useState } from 'react';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import { databaseCommands } from '../../../commands/database';
import { useSchemaStore } from '../../../stores/schemaStore';
import type { ConnectionConfig, DatabaseObject, TableInfo } from '../../../types';
import type { ConnectionEntry } from '../../../stores/activeConnectionStore';
import { shouldUseMultiDatabaseTree } from '../schema-tree/SchemaTree';
import { useExpandedDbCacheRefresh } from '../schema-tree/useExpandedDbCacheRefresh';

export function useNavigatorDbState(
  activeConnections: Record<string, ConnectionEntry | undefined>,
  connections: ConnectionConfig[],
  expandedDbs: Set<string>,
  expandedCats: Set<string>,
  loadForConnection: (
    dbSessionId: string,
    opts: {
      preferredDatabase?: string;
      skipLoadTables?: boolean;
      databaseType: ConnectionConfig['databaseType'];
    },
  ) => Promise<void>,
  ensureNamespacePath: (segments: string[], dbSessionId: string) => Promise<void>,
) {
  const [dbTablesMap, setDbTablesMap] = useState<Record<string, TableInfo[]>>({});
  const [dbObjectsMap, setDbObjectsMap] = useState<Record<string, DatabaseObject[]>>({});
  const [loadingDbs, setLoadingDbs] = useState<Set<string>>(new Set());

  const reloadDbTables = useCallback(async (dbSessionId: string, dbName: string) => {
    const tableKey = `${dbSessionId}::${dbName}`;
    try {
      const all = await databaseCommands.getTables(dbSessionId, dbName);
      setDbTablesMap((prev) => ({ ...prev, [tableKey]: all }));
      useSchemaStore.getState().setLoadedTables(dbName, all, dbSessionId);
    } catch {
      // ignore
    }
  }, []);

  const activateDatabase = useCallback(
    async (dbSessionId: string, dbName: string) => {
      const tableKey = `${dbSessionId}::${dbName}`;
      const cached = dbTablesMap[tableKey];
      if (cached) {
        useSchemaStore.getState().setLoadedTables(dbName, cached, dbSessionId);
        return;
      }
      useSchemaStore.setState((state) => {
        const entry = state.schemas.get(dbSessionId);
        if (!entry || entry.currentDatabase === dbName) return state;
        const next = new Map(state.schemas);
        next.set(dbSessionId, { ...entry, currentDatabase: dbName });
        return { ...state, schemas: next };
      });
    },
    [dbTablesMap],
  );

  const clearDbLocalCache = useCallback(
    (connectionId: string, _dbSessionId: string, dbName: string) => {
      const tableKey = `${_dbSessionId}::${dbName}`;
      setDbTablesMap((prev) => {
        if (!(tableKey in prev)) return prev;
        const next = { ...prev };
        delete next[tableKey];
        return next;
      });
      setDbObjectsMap((prev) => {
        const prefix = `${connectionId}::${dbName}::`;
        const hasMatch = Object.keys(prev).some((k) => k.startsWith(prefix));
        if (!hasMatch) return prev;
        const next = { ...prev };
        for (const key of Object.keys(next)) {
          if (key.startsWith(prefix)) delete next[key];
        }
        return next;
      });
    },
    [],
  );

  const reloadDbObjectCategory = useCallback(
    async (dbSessionId: string, catKey: string, catId: string) => {
      if (catId === 'tables' || catId === 'views') return;
      try {
        const objs = await databaseCommands.getDatabaseObjects(dbSessionId, catId);
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: objs }));
      } catch {
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: [] }));
      }
    },
    [],
  );

  useExpandedDbCacheRefresh({
    activeConnections,
    expandedDbs,
    expandedCats,
    loadTablesForDb: reloadDbTables,
    loadObjectsForCat: reloadDbObjectCategory,
    clearCaches: (sessionId: string, connectionId?: string) => {
      setDbTablesMap((m) => {
        const next: Record<string, TableInfo[]> = {};
        for (const key of Object.keys(m)) {
          if (!key.startsWith(sessionId + '::')) next[key] = m[key];
        }
        return next;
      });
      if (!connectionId) return;
      setDbObjectsMap((m) => {
        const prefix = connectionId + '::';
        const next: Record<string, DatabaseObject[]> = {};
        for (const key of Object.keys(m)) {
          if (!key.startsWith(prefix)) next[key] = m[key];
        }
        return next;
      });
    },
  });

  const reloadExpandedObjectCategories = useCallback(
    async (connectionId: string, dbSessionId: string) => {
      for (const catKey of expandedCats) {
        if (!catKey.startsWith(`${connectionId}::`)) continue;
        const catId = catKey.split('::').pop();
        if (!catId || catId === 'tables' || catId === 'views') continue;
        await reloadDbObjectCategory(dbSessionId, catKey, catId);
      }
    },
    [expandedCats, reloadDbObjectCategory],
  );

  const refreshConnection = useCallback(
    async (connectionId: string) => {
      const entry = activeConnections[connectionId];
      if (entry?.status !== 'connected' || !entry.dbSessionId) return;

      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) return;

      const meta = DB_REGISTRY[conn.databaseType];
      const isCustomTree = meta?.schemaTreeMode === 'custom';
      const isPathHierarchy = meta?.namespaceEnsure === 'path-hierarchy';
      const isPluginManaged = isCustomTree || isPathHierarchy;
      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      await loadForConnection(entry.dbSessionId, {
        preferredDatabase: conn.database,
        skipLoadTables: isMultiDb || isPluginManaged,
        databaseType: conn.databaseType,
      });

      if (isPathHierarchy) {
        await ensureNamespacePath([], entry.dbSessionId);
      }

      if (isMultiDb) {
        await Promise.all(
          [...expandedDbs]
            .filter((dbKey) => dbKey.startsWith(`${connectionId}::`))
            .map((dbKey) =>
              reloadDbTables(entry.dbSessionId, dbKey.slice(connectionId.length + 2)),
            ),
        );
      }

      await reloadExpandedObjectCategories(connectionId, entry.dbSessionId);
    },
    [
      activeConnections,
      connections,
      ensureNamespacePath,
      expandedDbs,
      loadForConnection,
      reloadDbTables,
      reloadExpandedObjectCategories,
    ],
  );

  const refreshAllConnections = useCallback(async () => {
    await Promise.all(
      Object.keys(activeConnections)
        .filter((connectionId) => activeConnections[connectionId]?.status === 'connected')
        .map((connectionId) => refreshConnection(connectionId)),
    );
  }, [activeConnections, refreshConnection]);

  const refreshDatabase = useCallback(
    async (connectionId: string, dbName: string) => {
      const entry = activeConnections[connectionId];
      if (!entry?.dbSessionId) return;

      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) return;

      const meta = DB_REGISTRY[conn.databaseType];
      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      if (isMultiDb) {
        await reloadDbTables(entry.dbSessionId, dbName);
      } else {
        await loadForConnection(entry.dbSessionId, {
          preferredDatabase: dbName,
          skipLoadTables: false,
          databaseType: conn.databaseType,
        });
      }

      const prefix = `${connectionId}::${dbName}::`;
      for (const catKey of expandedCats) {
        if (!catKey.startsWith(prefix)) continue;
        const catId = catKey.split('::').pop();
        if (!catId || catId === 'tables' || catId === 'views') continue;
        await reloadDbObjectCategory(entry.dbSessionId, catKey, catId);
      }
    },
    [
      activeConnections,
      connections,
      expandedCats,
      loadForConnection,
      reloadDbObjectCategory,
      reloadDbTables,
    ],
  );

  const refreshSchema = useCallback(
    async (connectionId: string, dbName: string, schemaName: string) => {
      const entry = activeConnections[connectionId];
      if (!entry?.dbSessionId) return;

      await reloadDbTables(entry.dbSessionId, dbName);

      const prefix = `${connectionId}::${dbName}::${schemaName}::`;
      for (const catKey of expandedCats) {
        if (!catKey.startsWith(prefix)) continue;
        const catId = catKey.split('::').pop();
        if (!catId || catId === 'tables' || catId === 'views') continue;
        await reloadDbObjectCategory(entry.dbSessionId, catKey, catId);
      }
    },
    [activeConnections, expandedCats, reloadDbObjectCategory, reloadDbTables],
  );

  const toggleDb = useCallback(
    async (_connectionId: string, dbSessionId: string, dbName: string) => {
      const tableKey = `${dbSessionId}::${dbName}`;
      if (dbTablesMap[tableKey]) return;
      if (loadingDbs.has(tableKey)) return;

      setLoadingDbs((prev) => new Set(prev).add(tableKey));
      try {
        await reloadDbTables(dbSessionId, dbName);
      } catch {
        setDbTablesMap((prev) => ({ ...prev, [tableKey]: [] }));
      } finally {
        setLoadingDbs((prev) => {
          const next = new Set(prev);
          next.delete(tableKey);
          return next;
        });
      }
    },
    [dbTablesMap, loadingDbs, reloadDbTables],
  );

  const toggleCategoryLoad = useCallback(
    async (catKey: string, catId: string, dbSessionId: string) => {
      if (catId === 'tables' || catId === 'views') return;
      if (dbObjectsMap[catKey]) return;

      try {
        const objs = await databaseCommands.getDatabaseObjects(dbSessionId, catId);
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: objs }));
      } catch {
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: [] }));
      }
    },
    [dbObjectsMap],
  );

  return {
    dbTablesMap,
    setDbTablesMap,
    dbObjectsMap,
    loadingDbs,
    reloadDbTables,
    activateDatabase,
    clearDbLocalCache,
    reloadDbObjectCategory,
    refreshConnection,
    refreshAllConnections,
    refreshDatabase,
    refreshSchema,
    toggleDb,
    toggleCategoryLoad,
  };
}
