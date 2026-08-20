import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Database, Loader2, Table2 } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useSchemaStore } from '../../../stores/schemaStore';
import { useI18n } from '../../../hooks/useI18n';
import { CopyableError } from '../../../components/ui/CopyableError';
import { cn } from '../../../lib/cn';
import type { TableInfo } from '../../../types';
import type { SchemaTreeProps } from './SchemaTree';

type FlatRow =
  | { type: 'db'; dbName: string; expanded: boolean; loading: boolean; tableCount: number }
  | { type: 'table'; dbName: string; item: TableInfo }
  | { type: 'db-empty'; dbName: string }
  | { type: 'db-loading'; dbName: string }
  | { type: 'empty' };

const ROW_HEIGHT = 32;
const EMPTY_HEIGHT = 36;

export function MultiDatabaseSchemaTree({
  connectionId,
  databaseType,
  initialDatabase,
  selectedTable,
  searchQuery,
  onSelectTable,
  onNodeContextMenu,
}: SchemaTreeProps) {
  const { t } = useI18n();
  const loading = useSchemaStore((s) => s.loading);
  const error = useSchemaStore((s) => s.error);
  const databases = useSchemaStore((s) => s.databases);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const schemaEpoch = useSchemaStore((s) => s.schemaEpoch);
  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const setLoadedTables = useSchemaStore((s) => s.setLoadedTables);

  const [expandedDbs, setExpandedDbs] = useState<Set<string>>(new Set());
  const [dbTables, setDbTables] = useState<Record<string, TableInfo[]>>({});
  const [dbLoading, setDbLoading] = useState<Set<string>>(new Set());
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void loadForConnection(connectionId, {
      preferredDatabase: initialDatabase,
      skipLoadTables: true,
      databaseType,
    });
  }, [connectionId, loadForConnection, initialDatabase, databaseType]);

  useEffect(() => {
    if (!currentDatabase) return;
    setDbTables((prev) => {
      if (!(currentDatabase in prev)) return prev;
      return { ...prev, [currentDatabase]: [...tables, ...views] };
    });
  }, [currentDatabase, tables, views]);

  const expandedDbsRef = useRef(expandedDbs);
  expandedDbsRef.current = expandedDbs;

  useEffect(() => {
    if (schemaEpoch === 0) return;
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
  }, [schemaEpoch, connectionId, setLoadedTables]);

  const activateDatabase = useCallback(
    async (dbName: string, tables: TableInfo[]) => {
      setLoadedTables(dbName, tables);
      try {
        const { databaseCommands } = await import('../../../commands/database');
        await databaseCommands.useDatabase(connectionId, dbName);
      } catch {
        // Selection still updates; query path may fail until user retries.
      }
    },
    [connectionId, setLoadedTables],
  );

  const handleToggleDb = useCallback(
    async (dbName: string) => {
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
    [connectionId, dbTables, dbLoading, expandedDbs, activateDatabase, setLoadedTables],
  );

  const query = searchQuery.toLowerCase();

  const flatRows = useMemo<FlatRow[]>(() => {
    const filteredDbs = query
      ? databases.filter((d) => {
          if (d.toLowerCase().includes(query)) return true;
          const tbls = dbTables[d];
          return tbls?.some((tbl) => tbl.name.toLowerCase().includes(query)) ?? false;
        })
      : databases;

    const rows: FlatRow[] = [];

    for (const dbName of filteredDbs) {
      const tbls = dbTables[dbName] ?? [];
      const dbNameMatches = query && dbName.toLowerCase().includes(query);
      const filteredDbTables =
        query && !dbNameMatches
          ? tbls.filter((tbl) => tbl.name.toLowerCase().includes(query))
          : tbls;
      const hasTableMatch = !!(query && filteredDbTables.length > 0);
      const isExpanded = expandedDbs.has(dbName) || hasTableMatch;
      const isLoading = dbLoading.has(dbName);

      rows.push({
        type: 'db',
        dbName,
        expanded: isExpanded,
        loading: isLoading,
        tableCount: filteredDbTables.length,
      });

      if (isExpanded) {
        if (isLoading) {
          rows.push({ type: 'db-loading', dbName });
        } else if (filteredDbTables.length === 0) {
          rows.push({ type: 'db-empty', dbName });
        } else {
          for (const tbl of filteredDbTables) {
            rows.push({ type: 'table', dbName, item: tbl });
          }
        }
      }
    }

    if (rows.length === 0 && !loading) {
      rows.push({ type: 'empty' });
    }

    return rows;
  }, [databases, dbTables, expandedDbs, dbLoading, query, loading]);

  const estimateSize = useCallback(
    (index: number) => {
      const row = flatRows[index];
      if (row.type === 'empty' || row.type === 'db-empty') return EMPTY_HEIGHT;
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
    return (
      <div className="p-3">
        <CopyableError message={error} className="text-xs text-red-400" />
      </div>
    );
  }

  return (
    <div
      ref={scrollRef}
      className="flex-1 min-h-0 overflow-y-auto"
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onNodeContextMenu?.({ kind: 'blank', name: '', x: e.clientX, y: e.clientY });
      }}
    >
      {loading && databases.length === 0 && (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

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
                </button>
              )}

              {row.type === 'db-loading' && (
                <div className="flex items-center gap-2 px-3 py-1 pl-8 text-xs text-fg-muted">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  {t('common.loading')}
                </div>
              )}

              {row.type === 'db-empty' && (
                <div className="px-3 py-1 pl-8 text-xs text-fg-muted">
                  {t('schemaTree.noTables')}
                </div>
              )}

              {row.type === 'table' && (
                <button
                  type="button"
                  className={cn(
                    'flex w-full items-center gap-2 py-1.5 pl-8 pr-3 text-left text-[13px] hover:bg-surface-raised',
                    selectedTable === row.item.name
                      ? 'bg-surface-raised text-fg'
                      : 'text-fg-secondary',
                  )}
                  onClick={() => {
                    if (currentDatabase !== row.dbName) {
                      const cached = dbTables[row.dbName];
                      if (cached) void activateDatabase(row.dbName, cached);
                    }
                    onSelectTable(row.item.name, row.item.schema);
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onNodeContextMenu?.({
                      kind: 'table',
                      name: row.item.name,
                      x: e.clientX,
                      y: e.clientY,
                      schema: row.item.schema ?? undefined,
                    });
                  }}
                >
                  <Table2 className="h-3.5 w-3.5 shrink-0 text-fg-secondary" />
                  <span className="selectable min-w-0 truncate">{row.item.name}</span>
                </button>
              )}

              {row.type === 'empty' && (
                <div className="px-3 py-3 text-center text-xs text-fg-muted">
                  {query ? t('schemaTree.noMatchingTables') : t('schemaTree.noTables')}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
