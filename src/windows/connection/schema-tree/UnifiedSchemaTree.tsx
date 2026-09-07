import { useCallback } from 'react';
import { Loader2 } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { CopyableError } from '../../../components/ui/CopyableError';
import { useI18n } from '../../../hooks/useI18n';
import { useSchemaStore } from '../../../stores/schemaStore';
import type { SchemaTreeProps } from './SchemaTree';
import { useSchemaTreeState } from './useSchemaTreeState';
import { useSchemaTreeFlatRows } from './useSchemaTreeFlatRows';
import { SchemaTreeRow } from './SchemaTreeRow';
import type { DragPayloadOptions } from './schemaTreeDrag';

export interface UnifiedSchemaTreeProps extends SchemaTreeProps {
  isKeyValue?: boolean;
}

const ROW_HEIGHT = 30;
const EMPTY_HEIGHT = 30;

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

  const state = useSchemaTreeState({
    connectionId,
    databaseType,
    initialDatabase,
    searchQuery,
    isKeyValue,
  });

  const flatRows = useSchemaTreeFlatRows({
    isSingleDbMode: state.isSingleDbMode,
    currentDatabase: state.currentDatabase,
    databases: state.databases,
    dbExpanded: state.dbExpanded,
    effectiveCategories: state.effectiveCategories,
    expandedCats: state.expandedCats,
    tables: state.tables,
    views: state.views,
    dbObjects: state.dbObjects,
    dbObjLoading: state.dbObjLoading,
    loading: state.loading,
    trimmedQuery: state.trimmedQuery,
    query: state.query,
    columnMap: state.columnMap,
    dbTables: state.dbTables,
    expandedDbs: state.expandedDbs,
    expandedSchemas: state.expandedSchemas,
    dbLoading: state.dbLoading,
    databaseType,
  });

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
    getScrollElement: () => state.scrollRef.current,
    estimateSize,
    overscan: 15,
  });

  const depthPadding = (depth: number) => `${0.5 + depth * 1.25}rem`;

  if (state.error) {
    return (
      <div className="p-3">
        <CopyableError message={state.error} className="text-xs text-red-400" />
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col min-h-0">
      {state.loading &&
        (state.isSingleDbMode
          ? state.tables.length === 0 && !state.currentDatabase
          : state.databases.length === 0) && (
          <div className="flex items-center gap-2 px-3 py-2 text-xs text-fg-muted shrink-0">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t('common.loading')}
          </div>
        )}

      <div
        ref={state.scrollRef as React.RefObject<HTMLDivElement>}
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
            const dragOptions: DragPayloadOptions | undefined =
              row.type === 'table' && row.item
                ? {
                    kind:
                      row.item.tableType === 'view' || row.item.tableType === 'materializedView'
                        ? 'view'
                        : 'table',
                    database: state.currentDatabase ?? state.databases[0] ?? '',
                    schema: row.item.schema ?? undefined,
                    table: row.item.name,
                    connectionId,
                    dbSessionId: useSchemaStore.getState().dbSessionId ?? undefined,
                    databaseType,
                  }
                : undefined;

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
                <SchemaTreeRow
                  row={row}
                  depthPadding={depthPadding}
                  selectedTable={selectedTable}
                  isSingleDbMode={state.isSingleDbMode}
                  onSelectTable={onSelectTable}
                  onNodeContextMenu={onNodeContextMenu}
                  onToggleDb={state.handleToggleDb}
                  onToggleSchema={state.toggleSchema}
                  onToggleCategory={state.toggleCategory}
                  dragOptions={dragOptions}
                />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
