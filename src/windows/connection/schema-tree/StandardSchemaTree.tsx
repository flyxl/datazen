import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Braces,
  ChevronDown,
  ChevronRight,
  Database,
  Eye,
  Loader2,
  Table2,
  Zap,
} from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useSchemaStore, useConnectionSchemaField } from '../../../stores/schemaStore';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { matchingColumns, tableMatchesObjectSearch } from '../../../lib/schemaObjectSearch';
import type { DatabaseType, DatabaseObject, TableInfo } from '../../../types';
import type { SchemaTreeNodeContextMenuPayload } from './SchemaTree';
import { formatRowCount } from './formatRowCount';

export interface StandardSchemaTreeProps {
  connectionId: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  selectedTable: string | null;
  searchQuery: string;
  onSelectTable: (table: string, schema?: string) => void;
  onNodeContextMenu?: (payload: SchemaTreeNodeContextMenuPayload) => void;
  isKeyValue: boolean;
}

interface CategoryDef {
  id: string;
  labelKey: string;
  icon: React.ComponentType<{ className?: string }>;
  color: string;
}

const SQL_CATEGORIES: CategoryDef[] = [
  { id: 'tables', labelKey: 'schemaTree.tables', icon: Table2, color: 'text-blue-400' },
  { id: 'views', labelKey: 'schemaTree.views', icon: Eye, color: 'text-purple-400' },
  { id: 'function', labelKey: 'schemaTree.functions', icon: Braces, color: 'text-orange-400' },
  { id: 'procedure', labelKey: 'schemaTree.procedures', icon: Braces, color: 'text-emerald-400' },
  { id: 'trigger', labelKey: 'schemaTree.triggers', icon: Zap, color: 'text-amber-400' },
];

const KV_CATEGORIES: CategoryDef[] = [
  { id: 'tables', labelKey: 'schemaTree.keys', icon: Table2, color: 'text-blue-400' },
];

type FlatRow =
  | { type: 'db'; dbName: string; expanded: boolean }
  | { type: 'category'; cat: CategoryDef; count: number; expanded: boolean }
  | { type: 'item'; item: TableInfo; catId: string }
  | { type: 'object'; obj: DatabaseObject; catId: string }
  | { type: 'cat-empty'; catId: string }
  | { type: 'empty' };

const ROW_HEIGHT = 30;
const SECTION_HEIGHT = 30;
const EMPTY_HEIGHT = 30;

export function StandardSchemaTree({
  connectionId,
  databaseType,
  initialDatabase,
  selectedTable,
  searchQuery,
  onSelectTable,
  onNodeContextMenu,
  isKeyValue,
}: StandardSchemaTreeProps) {
  const { t } = useI18n();
  const tables = useConnectionSchemaField(connectionId, 'tables');
  const views = useConnectionSchemaField(connectionId, 'views');
  const columnMap = useConnectionSchemaField(connectionId, 'columnMap');
  const loading = useConnectionSchemaField(connectionId, 'loading');
  const error = useConnectionSchemaField(connectionId, 'error');
  const currentDatabase = useConnectionSchemaField(connectionId, 'currentDatabase');
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const loadColumnMap = useSchemaStore((s) => s.loadColumnMap);

  const [dbExpanded, setDbExpanded] = useState(true);
  const [expandedCats, setExpandedCats] = useState<Set<string>>(new Set(['tables']));
  const [dbObjects, setDbObjects] = useState<Record<string, DatabaseObject[]>>({});
  const [dbObjLoading, setDbObjLoading] = useState<Set<string>>(new Set());
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void loadForConnection(connectionId, {
      preferredDatabase: initialDatabase,
      skipLoadTables: false,
      databaseType,
    });
  }, [connectionId, loadForConnection, initialDatabase, databaseType]);

  const query = searchQuery.trim();
  useEffect(() => {
    if (query.length >= 2 && (tables.length > 0 || views.length > 0)) {
      void loadColumnMap();
    }
  }, [query, tables.length, views.length, loadColumnMap]);

  const categories = isKeyValue ? KV_CATEGORIES : SQL_CATEGORIES;

  const loadObjectsForCategory = useCallback(
    async (catId: string) => {
      if (catId === 'tables' || catId === 'views' || dbObjects[catId] || dbObjLoading.has(catId))
        return;
      setDbObjLoading((prev) => new Set(prev).add(catId));
      try {
        const { databaseCommands } = await import('../../../commands/database');
        const objs = await databaseCommands.getDatabaseObjects(connectionId, catId);
        setDbObjects((prev) => ({ ...prev, [catId]: objs }));
      } catch {
        setDbObjects((prev) => ({ ...prev, [catId]: [] }));
      } finally {
        setDbObjLoading((prev) => {
          const next = new Set(prev);
          next.delete(catId);
          return next;
        });
      }
    },
    [connectionId, dbObjects, dbObjLoading],
  );

  const toggleCategory = useCallback(
    (catId: string) => {
      setExpandedCats((prev) => {
        const next = new Set(prev);
        if (next.has(catId)) {
          next.delete(catId);
        } else {
          next.add(catId);
          void loadObjectsForCategory(catId);
        }
        return next;
      });
    },
    [loadObjectsForCategory],
  );

  const filteredTables = useMemo(
    () =>
      query
        ? tables.filter((tbl) => tableMatchesObjectSearch(tbl.name, query, columnMap[tbl.name]))
        : tables,
    [tables, query, columnMap],
  );
  const filteredViews = useMemo(
    () =>
      query
        ? views.filter((v) => tableMatchesObjectSearch(v.name, query, columnMap[v.name]))
        : views,
    [views, query, columnMap],
  );

  const getItemsForCategory = useCallback(
    (catId: string): { tables: TableInfo[]; objects: DatabaseObject[] } => {
      if (catId === 'tables') return { tables: filteredTables, objects: [] };
      if (catId === 'views') return { tables: filteredViews, objects: [] };
      const objs = dbObjects[catId] ?? [];
      const filtered = query
        ? objs.filter((o) => o.name.toLowerCase().includes(query.toLowerCase()))
        : objs;
      return { tables: [], objects: filtered };
    },
    [filteredTables, filteredViews, dbObjects, query],
  );

  const getCountForCategory = useCallback(
    (catId: string): number => {
      if (catId === 'tables') return filteredTables.length;
      if (catId === 'views') return filteredViews.length;
      return (dbObjects[catId] ?? []).length;
    },
    [filteredTables, filteredViews, dbObjects],
  );

  const flatRows = useMemo<FlatRow[]>(() => {
    const rows: FlatRow[] = [];

    if (currentDatabase) {
      rows.push({ type: 'db', dbName: currentDatabase, expanded: dbExpanded });
    }

    if (!dbExpanded && currentDatabase) return rows;

    for (const cat of categories) {
      const isExpanded = expandedCats.has(cat.id);
      const count = getCountForCategory(cat.id);

      rows.push({ type: 'category', cat, count, expanded: isExpanded });

      if (isExpanded) {
        const { tables: catTables, objects: catObjects } = getItemsForCategory(cat.id);
        if (catTables.length > 0) {
          for (const tbl of catTables) rows.push({ type: 'item', item: tbl, catId: cat.id });
        } else if (catObjects.length > 0) {
          for (const obj of catObjects) rows.push({ type: 'object', obj, catId: cat.id });
        } else if (!loading && !dbObjLoading.has(cat.id)) {
          rows.push({ type: 'cat-empty', catId: cat.id });
        }
      }
    }

    if (rows.length === 0 && !loading) {
      rows.push({ type: 'empty' });
    }

    return rows;
  }, [
    currentDatabase,
    dbExpanded,
    categories,
    expandedCats,
    getCountForCategory,
    getItemsForCategory,
    loading,
    dbObjLoading,
  ]);

  const estimateSize = useCallback(
    (index: number) => {
      const row = flatRows[index];
      if (row.type === 'db') return SECTION_HEIGHT;
      if (row.type === 'category') return SECTION_HEIGHT;
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

  return (
    <div className="flex flex-1 flex-col min-h-0">
      {loading && tables.length === 0 && !currentDatabase && (
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
                    onClick={() => setDbExpanded((v) => !v)}
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
                  </button>
                )}

                {row.type === 'category' && (
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 py-1.5 pl-7 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
                    onClick={() => toggleCategory(row.cat.id)}
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

                {row.type === 'item' &&
                  (() => {
                    const colHits =
                      query.length >= 2 ? matchingColumns(query, columnMap[row.item.name]) : [];
                    const kind = row.catId === 'views' ? 'view' : 'table';
                    return (
                      <button
                        type="button"
                        className={cn(
                          'flex w-full items-center gap-2 py-1.5 pl-14 pr-3 text-left text-[13px] hover:bg-surface-raised',
                          selectedTable === row.item.name
                            ? 'bg-surface-raised text-fg'
                            : 'text-fg-secondary',
                        )}
                        onClick={() => onSelectTable(row.item.name, row.item.schema)}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          onNodeContextMenu?.({
                            kind,
                            name: row.item.name,
                            x: e.clientX,
                            y: e.clientY,
                            schema: row.item.schema ?? undefined,
                          });
                        }}
                        title={colHits.length > 0 ? colHits.slice(0, 8).join(', ') : undefined}
                      >
                        {row.catId === 'views' ? (
                          <Eye className="h-3.5 w-3.5 shrink-0 text-purple-400" />
                        ) : (
                          <Table2 className="h-3.5 w-3.5 shrink-0 text-blue-400" />
                        )}
                        <span className="selectable min-w-0 truncate">{row.item.name}</span>
                        {colHits.length > 0 && (
                          <span className="shrink-0 text-[10px] text-accent">
                            {colHits.length === 1 ? colHits[0] : `${colHits.length} cols`}
                          </span>
                        )}
                        {row.item.rowCount != null && (
                          <span className="ml-auto shrink-0 text-[11px] text-fg-muted">
                            {formatRowCount(row.item.rowCount)}
                          </span>
                        )}
                      </button>
                    );
                  })()}

                {row.type === 'object' &&
                  (() => {
                    const objColor =
                      row.catId === 'procedure'
                        ? 'text-emerald-400'
                        : row.catId === 'trigger'
                          ? 'text-amber-400'
                          : 'text-orange-400';
                    const ObjIcon = row.catId === 'trigger' ? Zap : Braces;
                    return (
                      <button
                        type="button"
                        className="flex w-full items-center gap-2 py-1.5 pl-14 pr-3 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
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
                  <div className="py-1 pl-14 text-[11px] text-fg-muted">
                    {t('schemaTree.noTables')}
                  </div>
                )}

                {row.type === 'empty' && (
                  <div className="px-3 py-3 text-center text-xs text-fg-muted">
                    {query
                      ? isKeyValue
                        ? t('schemaTree.noMatchingKeys')
                        : t('schemaTree.noMatchingTables')
                      : isKeyValue
                        ? t('schemaTree.noKeys')
                        : t('schemaTree.noTables')}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
