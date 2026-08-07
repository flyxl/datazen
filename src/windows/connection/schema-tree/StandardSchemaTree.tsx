import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Database, Eye, Loader2, Table2 } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useSchemaStore } from '../../../stores/schemaStore';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import type { DatabaseType, TableInfo } from '../../../types';
import { formatRowCount } from './formatRowCount';

export interface StandardSchemaTreeProps {
  connectionId: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  selectedTable: string | null;
  searchQuery: string;
  onSelectTable: (table: string, schema?: string) => void;
  onTableContextMenu?: (tableName: string, x: number, y: number) => void;
  isKeyValue: boolean;
}

type FlatRow =
  | { type: 'section'; section: 'tables' | 'views'; count: number; expanded: boolean }
  | { type: 'item'; item: TableInfo; section: 'tables' | 'views' }
  | { type: 'empty' };

const ROW_HEIGHT = 32;
const SECTION_HEIGHT = 30;
const EMPTY_HEIGHT = 48;

export function StandardSchemaTree({
  connectionId,
  databaseType,
  initialDatabase,
  selectedTable,
  searchQuery,
  onSelectTable,
  onTableContextMenu,
  isKeyValue,
}: StandardSchemaTreeProps) {
  const { t } = useI18n();
  const tables = useSchemaStore((s) => s.tables);
  const views = useSchemaStore((s) => s.views);
  const loading = useSchemaStore((s) => s.loading);
  const error = useSchemaStore((s) => s.error);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const [tablesExpanded, setTablesExpanded] = useState(true);
  const [viewsExpanded, setViewsExpanded] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void loadForConnection(connectionId, {
      preferredDatabase: initialDatabase,
      skipLoadTables: false,
      databaseType,
    });
  }, [connectionId, loadForConnection, initialDatabase, databaseType]);

  const query = searchQuery.toLowerCase();
  const filteredTables = useMemo(
    () => query ? tables.filter((tbl) => tbl.name.toLowerCase().includes(query)) : tables,
    [tables, query],
  );
  const filteredViews = useMemo(
    () => query ? views.filter((v) => v.name.toLowerCase().includes(query)) : views,
    [views, query],
  );

  const flatRows = useMemo<FlatRow[]>(() => {
    const rows: FlatRow[] = [];

    if (filteredTables.length > 0) {
      rows.push({ type: 'section', section: 'tables', count: filteredTables.length, expanded: tablesExpanded });
      if (tablesExpanded) {
        for (const tbl of filteredTables) rows.push({ type: 'item', item: tbl, section: 'tables' });
      }
    }

    if (!isKeyValue && filteredViews.length > 0) {
      rows.push({ type: 'section', section: 'views', count: filteredViews.length, expanded: viewsExpanded });
      if (viewsExpanded) {
        for (const v of filteredViews) rows.push({ type: 'item', item: v, section: 'views' });
      }
    }

    if (rows.length === 0 && !loading && currentDatabase) {
      rows.push({ type: 'empty' });
    }

    return rows;
  }, [filteredTables, filteredViews, tablesExpanded, viewsExpanded, isKeyValue, loading, currentDatabase]);

  const estimateSize = useCallback(
    (index: number) => {
      const row = flatRows[index];
      if (row.type === 'section') return SECTION_HEIGHT;
      if (row.type === 'empty') return EMPTY_HEIGHT;
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
      {currentDatabase && (
        <div className="flex items-center gap-2 border-b border-edge px-3 py-2 shrink-0">
          <Database className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
          <span className="truncate text-sm text-fg">{currentDatabase}</span>
        </div>
      )}

      {loading && tables.length === 0 && (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted shrink-0">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

      <div ref={scrollRef} className="flex-1 min-h-0 overflow-y-auto">
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
                {row.type === 'section' && (
                  <button
                    type="button"
                    className="flex w-full items-center gap-1 px-2 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted hover:text-fg-secondary"
                    onClick={() => {
                      if (row.section === 'tables') setTablesExpanded((v) => !v);
                      else setViewsExpanded((v) => !v);
                    }}
                  >
                    {row.expanded
                      ? <ChevronDown className="h-3 w-3 shrink-0" />
                      : <ChevronRight className="h-3 w-3 shrink-0" />}
                    {row.section === 'tables'
                      ? `${isKeyValue ? t('schemaTree.keys') : t('schemaTree.tables')} (${row.count})`
                      : `Views (${row.count})`}
                  </button>
                )}

                {row.type === 'item' && (
                  <button
                    type="button"
                    className={cn(
                      'flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-surface-raised',
                      selectedTable === row.item.name ? 'bg-surface-raised text-fg' : 'text-fg-secondary',
                    )}
                    onClick={() => onSelectTable(row.item.name, row.item.schema)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      onTableContextMenu?.(row.item.name, e.clientX, e.clientY);
                    }}
                  >
                    {row.section === 'tables'
                      ? <Table2 className="h-3.5 w-3.5 shrink-0 text-fg-secondary" />
                      : <Eye className="h-3.5 w-3.5 shrink-0 text-fg-secondary" />}
                    <span className="min-w-0 truncate">{row.item.name}</span>
                    {row.item.rowCount != null && (
                      <span className="ml-auto shrink-0 text-[11px] text-fg-muted">
                        {formatRowCount(row.item.rowCount)}
                      </span>
                    )}
                  </button>
                )}

                {row.type === 'empty' && (
                  <div className="px-3 py-3 text-center text-xs text-fg-muted">
                    {query
                      ? isKeyValue ? t('schemaTree.noMatchingKeys') : t('schemaTree.noMatchingTables')
                      : isKeyValue ? t('schemaTree.noKeys') : t('schemaTree.noTables')}
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
