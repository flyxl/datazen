import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Braces,
  ChevronDown,
  ChevronRight,
  Database,
  Eye,
  FolderClosed,
  FolderOpen,
  Hash,
  Loader2,
  Shapes,
  Table2,
  Zap,
} from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useSchemaStore, useConnectionSchemaField } from '../../../stores/schemaStore';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { matchingColumns, tableMatchesObjectSearch } from '../../../lib/schemaObjectSearch';
import type { DatabaseObject, TableInfo } from '../../../types';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { SchemaTreeProps } from './SchemaTree';
import { formatRowCount } from './formatRowCount';

export interface UnifiedSchemaTreeProps extends SchemaTreeProps {
  isKeyValue?: boolean;
}

interface CategoryDef {
  id: string;
  labelKey: string;
  icon: React.ComponentType<{ className?: string }>;
  color: string;
}

const BASE_CATEGORIES: CategoryDef[] = [
  { id: 'tables', labelKey: 'schemaTree.tables', icon: Table2, color: 'text-blue-400' },
  { id: 'views', labelKey: 'schemaTree.views', icon: Eye, color: 'text-purple-400' },
];

const OBJECT_KIND_CATEGORIES: Record<string, CategoryDef> = {
  function: {
    id: 'function',
    labelKey: 'schemaTree.functions',
    icon: Braces,
    color: 'text-orange-400',
  },
  procedure: {
    id: 'procedure',
    labelKey: 'schemaTree.procedures',
    icon: Braces,
    color: 'text-emerald-400',
  },
  trigger: { id: 'trigger', labelKey: 'schemaTree.triggers', icon: Zap, color: 'text-amber-400' },
  sequence: {
    id: 'sequence',
    labelKey: 'schemaTree.sequences',
    icon: Hash,
    color: 'text-cyan-400',
  },
  type: { id: 'type', labelKey: 'schemaTree.types', icon: Shapes, color: 'text-pink-400' },
};

const KV_CATEGORIES: CategoryDef[] = [
  { id: 'tables', labelKey: 'schemaTree.keys', icon: Table2, color: 'text-blue-400' },
];

type FlatRow =
  | { type: 'db'; dbName: string; expanded: boolean; loading: boolean }
  | { type: 'schema'; dbName: string; schemaName: string; expanded: boolean; depth: number }
  | {
      type: 'category';
      key: string;
      cat: CategoryDef;
      count: number;
      expanded: boolean;
      depth: number;
    }
  | { type: 'table'; item: TableInfo; depth: number; colHits: string[] }
  | { type: 'object'; obj: DatabaseObject; depth: number }
  | { type: 'cat-empty'; depth: number }
  | { type: 'db-loading'; dbName: string }
  | { type: 'empty' };

const ROW_HEIGHT = 30;
const EMPTY_HEIGHT = 30;

function groupBySchema(items: TableInfo[]): Map<string, TableInfo[]> | null {
  const hasAnySchema = items.some((i) => !!i.schema);
  if (!hasAnySchema) return null;

  const map = new Map<string, TableInfo[]>();
  for (const item of items) {
    const key = item.schema ?? '';
    const arr = map.get(key);
    if (arr) arr.push(item);
    else map.set(key, [item]);
  }
  return map;
}

export function UnifiedSchemaTree({
  connectionId,
  databaseType,
  initialDatabase,
  selectedTable,
  searchQuery,
  onSelectTable,
  onNodeContextMenu,
  isKeyValue = false,
}: UnifiedSchemaTreeProps) {
  const { t } = useI18n();
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

  const effectiveCategories = useMemo(() => {
    if (isKeyValue) return KV_CATEGORIES;
    const meta = DB_REGISTRY[databaseType];
    const objectKinds = meta?.supportedObjectKinds;
    if (!objectKinds || objectKinds.length === 0) return BASE_CATEGORIES;
    const objectCats = objectKinds.map((kind) => OBJECT_KIND_CATEGORIES[kind]).filter(Boolean);
    return [...BASE_CATEGORIES, ...objectCats];
  }, [isKeyValue, databaseType]);

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

  useEffect(() => {
    if (!isSingleDbMode || !currentDatabase) return;
    if (tables.length === 0 && views.length === 0 && !loading) {
      void useSchemaStore.getState().loadTables(currentDatabase, connectionId);
    }
  }, [isSingleDbMode, currentDatabase, tables.length, views.length, loading, connectionId]);

  useEffect(() => {
    if (isSingleDbMode && databases.length === 1) {
      setExpandedDbs(new Set([databases[0]!]));
      setDbExpanded(true);
    }
  }, [isSingleDbMode, databases]);

  const trimmedQuery = searchQuery.trim();
  const query = trimmedQuery.toLowerCase();

  useEffect(() => {
    if (!isSingleDbMode) return;
    if (trimmedQuery.length >= 2 && (tables.length > 0 || views.length > 0)) {
      void loadColumnMap(connectionId);
    }
  }, [isSingleDbMode, trimmedQuery, tables.length, views.length, loadColumnMap, connectionId]);

  useEffect(() => {
    if (!currentDatabase || isSingleDbMode) return;
    setDbTables((prev) => {
      if (!(currentDatabase in prev)) return prev;
      return { ...prev, [currentDatabase]: [...tables, ...views] };
    });
  }, [currentDatabase, tables, views, isSingleDbMode]);

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
          await databaseCommands.useDatabase(connectionId, dbName);
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
      try {
        const { databaseCommands } = await import('../../../commands/database');
        await databaseCommands.useDatabase(connectionId, dbName);
      } catch {
        // best-effort
      }
    },
    [connectionId, setLoadedTables],
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
        await databaseCommands.useDatabase(connectionId, dbName);
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

  const filterTableItems = useCallback(
    (items: TableInfo[]) => {
      if (!isSingleDbMode || !trimmedQuery) {
        return query ? items.filter((tbl) => tbl.name.toLowerCase().includes(query)) : items;
      }
      return items.filter((tbl) =>
        tableMatchesObjectSearch(tbl.name, trimmedQuery, columnMap[tbl.name]),
      );
    },
    [isSingleDbMode, trimmedQuery, query, columnMap],
  );

  const filterObjectItems = useCallback(
    (objs: DatabaseObject[]) => {
      if (!query) return objs;
      return objs.filter((o) => o.name.toLowerCase().includes(query));
    },
    [query],
  );

  const flatRows = useMemo<FlatRow[]>(() => {
    if (isSingleDbMode) {
      const rows: FlatRow[] = [];
      const dbName = currentDatabase ?? databases[0];
      if (dbName) {
        rows.push({ type: 'db', dbName, expanded: dbExpanded, loading: false });
      }

      if (!dbExpanded || !dbName) {
        if (rows.length === 0 && !loading) rows.push({ type: 'empty' });
        return rows;
      }

      for (const cat of effectiveCategories) {
        const catKey = cat.id;
        const isExpanded = expandedCats.has(catKey);
        let count = 0;
        let tableItems: TableInfo[] = [];
        let objectItems: DatabaseObject[] = [];

        if (cat.id === 'tables') {
          tableItems = filterTableItems(tables);
          count = tableItems.length;
        } else if (cat.id === 'views') {
          tableItems = filterTableItems(views);
          count = tableItems.length;
        } else {
          objectItems = filterObjectItems(dbObjects[catKey] ?? []);
          count = objectItems.length;
        }

        rows.push({
          type: 'category',
          key: catKey,
          cat,
          count,
          expanded: isExpanded,
          depth: 1,
        });

        if (isExpanded) {
          if (tableItems.length > 0) {
            for (const item of tableItems) {
              const colHits =
                trimmedQuery.length >= 2 ? matchingColumns(trimmedQuery, columnMap[item.name]) : [];
              rows.push({ type: 'table', item, depth: 2, colHits });
            }
          } else if (objectItems.length > 0) {
            for (const obj of objectItems) rows.push({ type: 'object', obj, depth: 2 });
          } else if (!loading && !dbObjLoading.has(catKey)) {
            rows.push({ type: 'cat-empty', depth: 2 });
          }
        }
      }

      if (rows.length === 0 && !loading) rows.push({ type: 'empty' });
      return rows;
    }

    const filteredDbs = query
      ? databases.filter((d) => {
          if (d.toLowerCase().includes(query)) return true;
          const tbls = dbTables[d];
          return tbls?.some((tbl) => tbl.name.toLowerCase().includes(query)) ?? false;
        })
      : databases;

    const rows: FlatRow[] = [];

    const addCategoriesForItems = (
      allItems: TableInfo[],
      dbName: string,
      schemaName: string | undefined,
      baseDepth: number,
    ) => {
      const tblItems = allItems.filter(
        (i) => i.tableType === 'table' || i.tableType === 'systemTable',
      );
      const viewItems = allItems.filter(
        (i) => i.tableType === 'view' || i.tableType === 'materializedView',
      );

      for (const cat of effectiveCategories) {
        const catKey = schemaName ? `${dbName}::${schemaName}::${cat.id}` : `${dbName}::${cat.id}`;
        const isExpanded = expandedCats.has(catKey);
        let count = 0;

        if (cat.id === 'tables') count = tblItems.length;
        else if (cat.id === 'views') count = viewItems.length;
        else count = (dbObjects[catKey] ?? []).length;

        rows.push({
          type: 'category',
          key: catKey,
          cat,
          count,
          expanded: isExpanded,
          depth: baseDepth,
        });

        if (isExpanded) {
          let items: TableInfo[] = [];
          let objs: DatabaseObject[] = [];
          if (cat.id === 'tables') items = tblItems;
          else if (cat.id === 'views') items = viewItems;
          else objs = dbObjects[catKey] ?? [];

          if (items.length > 0) {
            for (const item of items)
              rows.push({ type: 'table', item, depth: baseDepth + 1, colHits: [] });
          } else if (objs.length > 0) {
            for (const obj of objs) rows.push({ type: 'object', obj, depth: baseDepth + 1 });
          } else {
            rows.push({ type: 'cat-empty', depth: baseDepth + 1 });
          }
        }
      }
    };

    for (const dbName of filteredDbs) {
      const allItems = dbTables[dbName] ?? [];
      const dbNameMatches = query && dbName.toLowerCase().includes(query);
      const filteredDbItems =
        query && !dbNameMatches
          ? allItems.filter((tbl) => tbl.name.toLowerCase().includes(query))
          : allItems;
      const hasTableMatch = !!(query && filteredDbItems.length > 0);
      const isExpanded = expandedDbs.has(dbName) || hasTableMatch;
      const isLoading = dbLoading.has(dbName);

      rows.push({ type: 'db', dbName, expanded: isExpanded, loading: isLoading });

      if (!isExpanded) continue;

      if (isLoading) {
        rows.push({ type: 'db-loading', dbName });
        continue;
      }

      const schemaGroups = groupBySchema(filteredDbItems);

      if (!schemaGroups) {
        addCategoriesForItems(filteredDbItems, dbName, undefined, 1);
      } else {
        const preferred = DB_REGISTRY[databaseType]?.defaultSchema;
        const sortedSchemas = [...schemaGroups.keys()].sort((a, b) => {
          if (preferred) {
            if (a === preferred) return -1;
            if (b === preferred) return 1;
          }
          return a.localeCompare(b);
        });

        for (const schemaName of sortedSchemas) {
          const schemaKey = `${dbName}::${schemaName}`;
          const schemaItems = schemaGroups.get(schemaName) ?? [];
          const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

          rows.push({
            type: 'schema',
            dbName,
            schemaName,
            expanded: schemaExpanded,
            depth: 1,
          });

          if (schemaExpanded) {
            addCategoriesForItems(schemaItems, dbName, schemaName, 2);
          }
        }
      }
    }

    if (rows.length === 0 && !loading) {
      rows.push({ type: 'empty' });
    }

    return rows;
  }, [
    isSingleDbMode,
    currentDatabase,
    databases,
    dbExpanded,
    effectiveCategories,
    expandedCats,
    tables,
    views,
    dbObjects,
    dbObjLoading,
    loading,
    filterTableItems,
    filterObjectItems,
    trimmedQuery,
    columnMap,
    query,
    dbTables,
    expandedDbs,
    expandedSchemas,
    dbLoading,
    databaseType,
  ]);

  const estimateSize = useCallback(
    (index: number) => {
      const row = flatRows[index];
      if (row.type === 'empty' || row.type === 'cat-empty') return EMPTY_HEIGHT;
      return ROW_HEIGHT;
    },
    [flatRows],
  );

  const virtualizer = useVirtualizer({
    count: flatRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize,
    overscan: 15,
  });

  if (error) {
    return <div className="p-3 text-xs text-red-400">{error}</div>;
  }

  const depthPadding = (depth: number) => `${0.5 + depth * 1.25}rem`;

  const emptyMessage = query
    ? isKeyValue
      ? t('schemaTree.noMatchingKeys')
      : t('schemaTree.noMatchingTables')
    : isKeyValue
      ? t('schemaTree.noKeys')
      : t('schemaTree.noTables');

  return (
    <div className="flex flex-1 flex-col min-h-0">
      {loading &&
        (isSingleDbMode ? tables.length === 0 && !currentDatabase : databases.length === 0) && (
          <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted shrink-0">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t('common.loading')}
          </div>
        )}

      <div
        ref={scrollRef}
        className="flex-1 min-h-0 overflow-y-auto"
        onContextMenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onNodeContextMenu?.({ kind: 'blank', name: '', x: e.clientX, y: e.clientY });
        }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const row = flatRows[virtualRow.index];
            return (
              <div
                key={virtualRow.index}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  height: virtualRow.size,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {row.type === 'db' && (
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm hover:bg-surface-raised text-fg-secondary"
                    onClick={() => void handleToggleDb(row.dbName)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      onNodeContextMenu?.({
                        kind: 'database',
                        name: row.dbName,
                        x: e.clientX,
                        y: e.clientY,
                      });
                    }}
                  >
                    {row.expanded ? (
                      <ChevronDown className="h-3.5 w-3.5 shrink-0" />
                    ) : (
                      <ChevronRight className="h-3.5 w-3.5 shrink-0" />
                    )}
                    <Database className="h-3.5 w-3.5 shrink-0 text-teal-400" />
                    <span className="selectable min-w-0 truncate">{row.dbName}</span>
                    {row.loading && (
                      <Loader2 className="h-3 w-3 shrink-0 animate-spin text-fg-muted" />
                    )}
                  </button>
                )}

                {row.type === 'schema' && (
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 py-1.5 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
                    style={{ paddingLeft: depthPadding(row.depth) }}
                    onClick={() => toggleSchema(`${row.dbName}::${row.schemaName}`)}
                  >
                    {row.expanded ? (
                      <ChevronDown className="h-3 w-3 shrink-0" />
                    ) : (
                      <ChevronRight className="h-3 w-3 shrink-0" />
                    )}
                    {row.expanded ? (
                      <FolderOpen className="h-3.5 w-3.5 shrink-0 text-teal-400" />
                    ) : (
                      <FolderClosed className="h-3.5 w-3.5 shrink-0 text-teal-400" />
                    )}
                    <span className="min-w-0 truncate">
                      {row.schemaName || t('common.default')}
                    </span>
                  </button>
                )}

                {row.type === 'category' && (
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 py-1.5 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
                    style={{ paddingLeft: depthPadding(row.depth) }}
                    onClick={() => toggleCategory(row.key, row.cat.id)}
                  >
                    {row.expanded ? (
                      <ChevronDown className="h-3 w-3 shrink-0" />
                    ) : (
                      <ChevronRight className="h-3 w-3 shrink-0" />
                    )}
                    <row.cat.icon className={`h-3.5 w-3.5 shrink-0 ${row.cat.color}`} />
                    <span className="min-w-0 truncate">
                      {t(row.cat.labelKey as Parameters<typeof t>[0])}
                    </span>
                    <span className="ml-auto shrink-0 text-[10px] text-fg-muted">{row.count}</span>
                  </button>
                )}

                {row.type === 'db-loading' && (
                  <div className="flex items-center gap-2 px-3 py-1 pl-8 text-xs text-fg-muted">
                    <Loader2 className="h-3 w-3 animate-spin" />
                    {t('common.loading')}
                  </div>
                )}

                {row.type === 'table' && (
                  <button
                    type="button"
                    className={cn(
                      'flex w-full items-center gap-2 py-1.5 pr-3 text-left text-[13px] hover:bg-surface-raised',
                      selectedTable === row.item.name
                        ? 'bg-surface-raised text-fg'
                        : 'text-fg-secondary',
                    )}
                    style={{ paddingLeft: depthPadding(row.depth) }}
                    onClick={() => {
                      onSelectTable(row.item.name, row.item.schema);
                    }}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      onNodeContextMenu?.({
                        kind:
                          row.item.tableType === 'view' || row.item.tableType === 'materializedView'
                            ? 'view'
                            : 'table',
                        name: row.item.name,
                        x: e.clientX,
                        y: e.clientY,
                        schema: row.item.schema ?? undefined,
                      });
                    }}
                    title={row.colHits.length > 0 ? row.colHits.slice(0, 8).join(', ') : undefined}
                  >
                    {row.item.tableType === 'view' || row.item.tableType === 'materializedView' ? (
                      <Eye className="h-3.5 w-3.5 shrink-0 text-purple-400" />
                    ) : (
                      <Table2 className="h-3.5 w-3.5 shrink-0 text-blue-400" />
                    )}
                    <span className="selectable min-w-0 truncate">{row.item.name}</span>
                    {row.colHits.length > 0 && (
                      <span className="shrink-0 text-[10px] text-accent">
                        {row.colHits.length === 1 ? row.colHits[0] : `${row.colHits.length} cols`}
                      </span>
                    )}
                    {isSingleDbMode && row.item.rowCount != null && (
                      <span className="ml-auto shrink-0 text-[11px] text-fg-muted">
                        {formatRowCount(row.item.rowCount)}
                      </span>
                    )}
                  </button>
                )}

                {row.type === 'object' &&
                  (() => {
                    const catDef = OBJECT_KIND_CATEGORIES[row.obj.kind];
                    const ObjIcon = catDef?.icon ?? Braces;
                    const objColor = catDef?.color ?? 'text-orange-400';
                    return (
                      <button
                        type="button"
                        className="flex w-full items-center gap-2 py-1.5 pr-3 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
                        style={{ paddingLeft: depthPadding(row.depth) }}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          onNodeContextMenu?.({
                            kind: row.obj.kind,
                            name: row.obj.name,
                            x: e.clientX,
                            y: e.clientY,
                            schema: row.obj.schema ?? undefined,
                          });
                        }}
                      >
                        <ObjIcon className={`h-3.5 w-3.5 shrink-0 ${objColor}`} />
                        <span className="selectable min-w-0 truncate">{row.obj.name}</span>
                      </button>
                    );
                  })()}

                {row.type === 'cat-empty' && (
                  <div
                    className="py-1 text-[11px] text-fg-muted"
                    style={{ paddingLeft: depthPadding(row.depth) }}
                  >
                    {t('schemaTree.noTables')}
                  </div>
                )}

                {row.type === 'empty' && (
                  <div className="px-3 py-3 text-center text-xs text-fg-muted">{emptyMessage}</div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
