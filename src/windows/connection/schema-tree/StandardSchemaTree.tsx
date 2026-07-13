import { useEffect, useState } from 'react';
import { ChevronDown, ChevronRight, Database, Eye, Loader2, Table2 } from 'lucide-react';
import { useSchemaStore } from '../../../stores/schemaStore';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import type { DatabaseType } from '../../../types';
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

export function StandardSchemaTree({
  connectionId,
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

  useEffect(() => {
    console.log('[SchemaTree] loading for connection', connectionId, 'preferred db', initialDatabase);
    void loadForConnection(connectionId, { preferredDatabase: initialDatabase, skipLoadTables: false });
  }, [connectionId, loadForConnection, initialDatabase]);

  const query = searchQuery.toLowerCase();
  const filteredTables = query
    ? tables.filter((tbl) => tbl.name.toLowerCase().includes(query))
    : tables;
  const filteredViews = query
    ? views.filter((v) => v.name.toLowerCase().includes(query))
    : views;

  if (error) {
    return <div className="p-3 text-xs text-red-400">{error}</div>;
  }

  return (
    <div className="flex flex-col">
      {currentDatabase && (
        <div className="flex items-center gap-2 border-b border-edge px-3 py-2">
          <Database className="h-3.5 w-3.5 shrink-0 text-fg-muted" />
          <span className="truncate text-sm text-fg">{currentDatabase}</span>
        </div>
      )}

      {loading && tables.length === 0 && (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

      {filteredTables.length > 0 && (
        <div>
          <button
            type="button"
            className="flex w-full items-center gap-1 px-2 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted hover:text-fg-secondary"
            onClick={() => setTablesExpanded((v) => !v)}
          >
            {tablesExpanded
              ? <ChevronDown className="h-3 w-3 shrink-0" />
              : <ChevronRight className="h-3 w-3 shrink-0" />}
            {isKeyValue ? t('schemaTree.keys') : t('schemaTree.tables')} ({filteredTables.length})
          </button>
          {tablesExpanded && filteredTables.map((tbl) => {
            const fullName = tbl.schema ? `${tbl.schema}.${tbl.name}` : tbl.name;
            const isSelected = selectedTable === tbl.name;
            return (
              <button
                key={fullName}
                type="button"
                className={cn(
                  'flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-surface-raised',
                  isSelected && 'bg-surface-raised text-fg',
                  !isSelected && 'text-fg-secondary',
                )}
                onClick={() => onSelectTable(tbl.name, tbl.schema)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  onTableContextMenu?.(tbl.name, e.clientX, e.clientY);
                }}
              >
                <Table2 className="h-3.5 w-3.5 shrink-0 text-fg-secondary" />
                <span className="min-w-0 truncate">{tbl.name}</span>
                {tbl.rowCount != null && (
                  <span className="ml-auto shrink-0 text-[11px] text-fg-muted">
                    {formatRowCount(tbl.rowCount)}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      )}

      {!isKeyValue && filteredViews.length > 0 && (
        <div>
          <button
            type="button"
            className="flex w-full items-center gap-1 px-2 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted hover:text-fg-secondary"
            onClick={() => setViewsExpanded((v) => !v)}
          >
            {viewsExpanded
              ? <ChevronDown className="h-3 w-3 shrink-0" />
              : <ChevronRight className="h-3 w-3 shrink-0" />}
            Views ({filteredViews.length})
          </button>
          {viewsExpanded && filteredViews.map((v) => {
            const fullName = v.schema ? `${v.schema}.${v.name}` : v.name;
            const isSelected = selectedTable === v.name;
            return (
              <button
                key={fullName}
                type="button"
                className={cn(
                  'flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-surface-raised',
                  isSelected && 'bg-surface-raised text-fg',
                  !isSelected && 'text-fg-secondary',
                )}
                onClick={() => onSelectTable(v.name, v.schema)}
              >
                <Eye className="h-3.5 w-3.5 shrink-0 text-fg-secondary" />
                <span className="min-w-0 truncate">{v.name}</span>
              </button>
            );
          })}
        </div>
      )}

      {!loading && filteredTables.length === 0 && (isKeyValue || filteredViews.length === 0) && currentDatabase && (
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
}
