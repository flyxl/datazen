import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSchemaStore, useConnectionSchemaField } from '../../../stores/schemaStore';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { DatabaseType } from '../../../extensions/generated';
import type { DatabaseObject, TableInfo } from '../../../types';
import type { SchemaTreeCategoryDef } from './schemaTreeCategories';
import { getEffectiveCategories } from './schemaTreeCategories';

export interface SchemaTreeStateOptions {
  connectionId: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  searchQuery: string;
  isKeyValue: boolean;
}

export interface SchemaTreeState {
  // Store fields
  loading: boolean;
  error: string | null;
  databases: string[];
  currentDatabase: string | null;
  schemaEpoch: number;
  tables: TableInfo[];
  views: TableInfo[];
  columnMap: Record<string, string[]>;

  // Local state
  isSingleDbMode: boolean;
  dbExpanded: boolean;
  expandedDbs: Set<string>;
  expandedSchemas: Set<string>;
  expandedCats: Set<string>;
  dbTables: Record<string, TableInfo[]>;
  dbObjects: Record<string, DatabaseObject[]>;
  dbLoading: Set<string>;
  dbObjLoading: Set<string>;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  trimmedQuery: string;
  query: string;
  effectiveCategories: SchemaTreeCategoryDef[];

  // Actions
  handleToggleDb: (dbName: string) => Promise<void>;
  toggleSchema: (key: string) => void;
  toggleCategory: (catKey: string, catId: string) => void;
}

export function useSchemaTreeState({
  connectionId,
  databaseType,
  initialDatabase,
  searchQuery,
  isKeyValue,
}: SchemaTreeStateOptions): SchemaTreeState {
  const loading = useConnectionSchemaField(connectionId, 'loading');
  const error = useConnectionSchemaField(connectionId, 'error');
  const databases = useConnectionSchemaField(connectionId, 'databases');
  const currentDatabase = useConnectionSchemaField(connectionId, 'currentDatabase');
  const schemaEpoch = useConnectionSchemaField(connectionId, 'schemaEpoch');
  const tables = useConnectionSchemaField(connectionId, 'tables');
  const views = useConnectionSchemaField(connectionId, 'views');
  const columnMap = useConnectionSchemaField(connectionId, 'columnMap');
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const loadColumnMap = useSchemaStore((s) => s.loadColumnMap);
  const setLoadedTables = useSchemaStore((s) => s.setLoadedTables);

  const isSingleDbMode = databases.length <= 1;
  const meta = DB_REGISTRY[databaseType];
  const lockedSingleDb = Boolean(initialDatabase?.trim()) && meta?.databaseFieldType !== 'domain';

  const effectiveCategories = useMemo(
    () => getEffectiveCategories(databaseType, isKeyValue),
    [isKeyValue, databaseType],
  );

  const [dbExpanded, setDbExpanded] = useState(true);
  const [expandedDbs, setExpandedDbs] = useState<Set<string>>(new Set());
  const [expandedSchemas, setExpandedSchemas] = useState<Set<string>>(new Set());
  const [expandedCats, setExpandedCats] = useState<Set<string>>(
    () => new Set(isSingleDbMode ? ['tables'] : []),
  );
  const [dbTables, setDbTables] = useState<Record<string, TableInfo[]>>({});
  const [dbObjects, setDbObjects] = useState<Record<string, DatabaseObject[]>>({});
  const [dbLoading, setDbLoading] = useState<Set<string>>(new Set());
  const [dbObjLoading, setDbObjLoading] = useState<Set<string>>(new Set());
  const scrollRef = useRef<HTMLDivElement>(null);

  // Load schema for connection
  useEffect(() => {
    void loadForConnection(connectionId, {
      preferredDatabase: initialDatabase,
      skipLoadTables: !lockedSingleDb && Boolean(meta?.hasMultiDatabase),
      databaseType,
    });
  }, [
    connectionId,
    loadForConnection,
    initialDatabase,
    databaseType,
    lockedSingleDb,
    meta?.hasMultiDatabase,
  ]);

  // Load tables for single-db mode
  useEffect(() => {
    if (!isSingleDbMode || !currentDatabase) return;
    if (tables.length === 0 && views.length === 0 && !loading) {
      void useSchemaStore.getState().loadTables(currentDatabase, connectionId);
    }
  }, [isSingleDbMode, currentDatabase, tables.length, views.length, loading, connectionId]);

  // Expand single-db mode
  useEffect(() => {
    if (isSingleDbMode && databases.length === 1) {
      setExpandedDbs(new Set([databases[0]!]));
      setDbExpanded(true);
    }
  }, [isSingleDbMode, databases]);

  const trimmedQuery = searchQuery.trim();
  const query = trimmedQuery.toLowerCase();

  // Load column map for search
  useEffect(() => {
    if (!isSingleDbMode) return;
    if (trimmedQuery.length >= 2 && (tables.length > 0 || views.length > 0)) {
      void loadColumnMap(connectionId);
    }
  }, [isSingleDbMode, trimmedQuery, tables.length, views.length, loadColumnMap, connectionId]);

  // Sync tables for multi-db mode
  useEffect(() => {
    if (!currentDatabase || isSingleDbMode) return;
    setDbTables((prev) => {
      if (!(currentDatabase in prev)) return prev;
      return { ...prev, [currentDatabase]: [...tables, ...views] };
    });
  }, [currentDatabase, tables, views, isSingleDbMode]);

  // Refresh expanded dbs on schema epoch change
  const expandedDbsRef = useRef(expandedDbs);
  expandedDbsRef.current = expandedDbs;

  useEffect(() => {
    if (isSingleDbMode || schemaEpoch === 0) return;
    const expanded = [...expandedDbsRef.current];
    if (expanded.length === 0) return;
    let cancelled = false;
    void (async () => {
      const { databaseCommands } = await import('../../../commands/database');
      for (const dbName of expanded) {
        try {
          const all = await databaseCommands.getTables(connectionId, dbName);
          if (cancelled) return;
          setDbTables((prev) => ({ ...prev, [dbName]: all }));
          if (useSchemaStore.getState().currentDatabase === dbName) {
            setLoadedTables(dbName, all);
          }
        } catch {
          if (cancelled) return;
          setDbTables((prev) => ({ ...prev, [dbName]: [] }));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [schemaEpoch, connectionId, setLoadedTables, isSingleDbMode]);

  const activateDatabase = useCallback(
    async (dbName: string, items: TableInfo[]) => {
      setLoadedTables(dbName, items);
    },
    [setLoadedTables],
  );

  const handleToggleDb = useCallback(
    async (dbName: string) => {
      if (isSingleDbMode) {
        setDbExpanded((v) => !v);
        return;
      }

      const wasExpanded = expandedDbs.has(dbName);
      setExpandedDbs((prev) => {
        const next = new Set(prev);
        if (next.has(dbName)) next.delete(dbName);
        else next.add(dbName);
        return next;
      });

      if (wasExpanded) return;

      const cached = dbTables[dbName];
      if (cached) {
        await activateDatabase(dbName, cached);
        return;
      }

      if (dbLoading.has(dbName)) {
        useSchemaStore.setState({ currentDatabase: dbName });
        return;
      }

      setDbLoading((prev) => new Set(prev).add(dbName));
      try {
        const { databaseCommands } = await import('../../../commands/database');
        const all = await databaseCommands.getTables(connectionId, dbName);
        setDbTables((prev) => ({ ...prev, [dbName]: all }));
        setLoadedTables(dbName, all);
      } catch {
        setDbTables((prev) => ({ ...prev, [dbName]: [] }));
        setLoadedTables(dbName, []);
      } finally {
        setDbLoading((prev) => {
          const next = new Set(prev);
          next.delete(dbName);
          return next;
        });
      }
    },
    [
      isSingleDbMode,
      connectionId,
      dbTables,
      dbLoading,
      expandedDbs,
      activateDatabase,
      setLoadedTables,
    ],
  );

  const toggleSchema = useCallback((key: string) => {
    setExpandedSchemas((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const loadObjectsForKey = useCallback(
    async (catKey: string, catId: string) => {
      if (catId === 'tables' || catId === 'views' || dbObjects[catKey]) return;
      setDbObjLoading((prev) => new Set(prev).add(catKey));
      try {
        const { databaseCommands } = await import('../../../commands/database');
        const objs = await databaseCommands.getDatabaseObjects(connectionId, catId);
        setDbObjects((prev) => ({ ...prev, [catKey]: objs }));
      } catch {
        setDbObjects((prev) => ({ ...prev, [catKey]: [] }));
      } finally {
        setDbObjLoading((prev) => {
          const next = new Set(prev);
          next.delete(catKey);
          return next;
        });
      }
    },
    [connectionId, dbObjects],
  );

  const toggleCategory = useCallback(
    (catKey: string, catId: string) => {
      setExpandedCats((prev) => {
        const next = new Set(prev);
        if (next.has(catKey)) {
          next.delete(catKey);
        } else {
          next.add(catKey);
          void loadObjectsForKey(catKey, catId);
        }
        return next;
      });
    },
    [loadObjectsForKey],
  );

  return {
    // Store fields
    loading,
    error,
    databases,
    currentDatabase,
    schemaEpoch,
    tables,
    views,
    columnMap,

    // Local state
    isSingleDbMode,
    dbExpanded,
    expandedDbs,
    expandedSchemas,
    expandedCats,
    dbTables,
    dbObjects,
    dbLoading,
    dbObjLoading,
    scrollRef,
    trimmedQuery,
    query,
    effectiveCategories,

    // Actions
    handleToggleDb,
    toggleSchema,
    toggleCategory,
  };
}
