import { useCallback, useEffect, useState } from 'react';
import { ChevronDown, ChevronRight, Database, Loader2, Table2 } from 'lucide-react';
import { useSchemaStore } from '../../../stores/schemaStore';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import type { TableInfo } from '../../../types';
import type { SchemaTreeProps } from './SchemaTree';

export function MultiDatabaseSchemaTree({
  connectionId,
  initialDatabase,
  selectedTable,
  searchQuery,
  onSelectTable,
  onTableContextMenu,
}: SchemaTreeProps) {
  const { t } = useI18n();
  const loading = useSchemaStore((s) => s.loading);
  const error = useSchemaStore((s) => s.error);
  const databases = useSchemaStore((s) => s.databases);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const loadTables = useSchemaStore((s) => s.loadTables);

  const [expandedDbs, setExpandedDbs] = useState<Set<string>>(new Set());
  const [dbTables, setDbTables] = useState<Record<string, TableInfo[]>>({});
  const [dbLoading, setDbLoading] = useState<Set<string>>(new Set());

  useEffect(() => {
    console.log('[SchemaTree] loading for connection', connectionId, 'preferred db', initialDatabase);
    void loadForConnection(connectionId, { preferredDatabase: initialDatabase, skipLoadTables: true });
  }, [connectionId, loadForConnection, initialDatabase]);

  const handleToggleDb = useCallback(async (dbName: string) => {
    setExpandedDbs((prev) => {
      const next = new Set(prev);
      if (next.has(dbName)) {
        next.delete(dbName);
      } else {
        next.add(dbName);
      }
      return next;
    });

    if (!dbTables[dbName] && !dbLoading.has(dbName)) {
      setDbLoading((prev) => new Set(prev).add(dbName));
      try {
        const { databaseCommands } = await import('../../../commands/database');
        const all = await databaseCommands.getTables(connectionId, dbName);
        setDbTables((prev) => ({ ...prev, [dbName]: all }));
      } catch {
        setDbTables((prev) => ({ ...prev, [dbName]: [] }));
      } finally {
        setDbLoading((prev) => {
          const next = new Set(prev);
          next.delete(dbName);
          return next;
        });
      }
    }
  }, [connectionId, dbTables, dbLoading]);

  const query = searchQuery.toLowerCase();
  const filteredDbs = query
    ? databases.filter((d) => d.toLowerCase().includes(query))
    : databases;

  if (error) {
    return <div className="p-3 text-xs text-red-400">{error}</div>;
  }

  return (
    <div className="flex flex-col">
      {loading && databases.length === 0 && (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t('common.loading')}
        </div>
      )}

      {filteredDbs.map((dbName) => {
        const isExpanded = expandedDbs.has(dbName);
        const isLoading = dbLoading.has(dbName);
        const tbls = dbTables[dbName] ?? [];
        const filteredDbTables = query
          ? tbls.filter((tbl) => tbl.name.toLowerCase().includes(query))
          : tbls;

        return (
          <div key={dbName}>
            <button
              type="button"
              className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm hover:bg-surface-raised text-fg-secondary"
              onClick={() => void handleToggleDb(dbName)}
            >
              {isExpanded
                ? <ChevronDown className="h-3.5 w-3.5 shrink-0" />
                : <ChevronRight className="h-3.5 w-3.5 shrink-0" />}
              <Database className="h-3.5 w-3.5 shrink-0 text-teal-400" />
              <span className="min-w-0 truncate">{dbName}</span>
            </button>

            {isExpanded && (
              <div className="pl-4">
                {isLoading && (
                  <div className="flex items-center gap-2 px-3 py-1 text-xs text-fg-muted">
                    <Loader2 className="h-3 w-3 animate-spin" />
                    {t('common.loading')}
                  </div>
                )}
                {!isLoading && filteredDbTables.length === 0 && (
                  <div className="px-3 py-1 text-xs text-fg-muted">
                    {t('schemaTree.noTables')}
                  </div>
                )}
                {filteredDbTables.map((tbl) => {
                  const isSelected = selectedTable === tbl.name;
                  return (
                    <button
                      key={`${dbName}.${tbl.name}`}
                      type="button"
                      className={cn(
                        'flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-surface-raised',
                        isSelected && 'bg-surface-raised text-fg',
                        !isSelected && 'text-fg-secondary',
                      )}
                      onClick={() => {
                        if (currentDatabase !== dbName) {
                          void loadTables(dbName);
                        }
                        onSelectTable(tbl.name, tbl.schema);
                      }}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        onTableContextMenu?.(tbl.name, e.clientX, e.clientY);
                      }}
                    >
                      <Table2 className="h-3.5 w-3.5 shrink-0 text-fg-secondary" />
                      <span className="min-w-0 truncate">{tbl.name}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}

      {!loading && filteredDbs.length === 0 && (
        <div className="px-3 py-3 text-center text-xs text-fg-muted">
          {query ? t('schemaTree.noMatchingTables') : t('schemaTree.noTables')}
        </div>
      )}
    </div>
  );
}
