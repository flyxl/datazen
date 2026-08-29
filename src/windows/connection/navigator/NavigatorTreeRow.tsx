import {
  Braces,
  ChevronDown,
  ChevronRight,
  Database,
  Eye,
  FolderClosed,
  FolderOpen,
  Loader2,
  Plus,
  Table2,
  Zap,
} from 'lucide-react';
import { DbTypeBadge } from '../../../components/DbTypeBadge';
import { cn } from '../../../lib/cn';
import { useSchemaStore } from '../../../stores/schemaStore';
import type { I18nKey } from '../../../locales';
import type { ConnectionConfig } from '../../../types';
import type { ConnectionEntry } from '../../../stores/activeConnectionStore';
import { LEAF_KIND_ICON } from '../schema-tree/schemaTreeCategories';
import type { UnifiedRow } from './types';
import { depthPadding } from './utils';

export interface NavigatorTreeRowProps {
  row: UnifiedRow;
  t: (key: I18nKey, params?: Record<string, string | number>) => string;
  connections: ConnectionConfig[];
  activeConnections: Record<string, ConnectionEntry | undefined>;
  dropTarget: { id: string; position: 'before' | 'after' } | null;
  expandedDbs: Set<string>;
  onNewConnection: () => void;
  onSelectConnection: (connectionId: string) => void;
  onSelectTable: (tableName: string, schema?: string, database?: string) => void;
  onSelectKvDb?: (connectionId: string, dbName: string) => void;
  toggleGroup: (group: string) => void;
  toggleConnection: (connectionId: string) => void;
  toggleDb: (connectionId: string, dbSessionId: string, dbName: string) => void;
  toggleSchema: (schemaKey: string) => void;
  toggleCategory: (catKey: string, catId: string, dbSessionId: string) => void;
  activateDatabase: (dbSessionId: string, dbName: string) => Promise<void>;
  ensureNamespacePath: (segments: string[], dbSessionId: string) => Promise<void>;
  setExpandedDbs: React.Dispatch<React.SetStateAction<Set<string>>>;
  handleGroupContextMenu: (e: React.MouseEvent, groupName: string) => void;
  handleConnectionContextMenu: (e: React.MouseEvent, conn: ConnectionConfig) => void;
  handleDatabaseContextMenu: (e: React.MouseEvent, dbName: string, connectionId: string) => void;
  handleSchemaContextMenu: (
    e: React.MouseEvent,
    schemaName: string,
    dbName: string,
    connectionId: string,
  ) => void;
  handleTableContextMenu: (
    e: React.MouseEvent,
    args: {
      kind: 'table' | 'view';
      name: string;
      schema?: string;
      dbName: string;
      connectionId: string;
      dbSessionId: string;
    },
  ) => void;
  handleCategoryContextMenu: (
    e: React.MouseEvent,
    catKey: string,
    catId: string,
    connectionId: string,
  ) => void;
  handleObjectContextMenu: (e: React.MouseEvent, name: string) => void;
  handleConnectionClick: (conn: ConnectionConfig) => void;
  handleConnectionDoubleClick: (conn: ConnectionConfig) => void;
  handleDragStart: (e: React.DragEvent, connId: string) => void;
  handleDragOver: (e: React.DragEvent, targetId: string) => void;
  handleDragLeave: () => void;
  handleDragEnd: () => void;
  handleDrop: (e: React.DragEvent) => void;
  renderStatusDot: (connectionId: string) => React.ReactNode;
  viewActions?: {
    openObject?: (
      kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
      name: string,
      schema?: string,
    ) => void;
  };
}

export function NavigatorTreeRow({
  row,
  t,
  connections,
  activeConnections,
  dropTarget,
  expandedDbs,
  onNewConnection,
  onSelectConnection,
  onSelectTable,
  onSelectKvDb,
  toggleGroup,
  toggleConnection,
  toggleDb,
  toggleSchema,
  toggleCategory,
  activateDatabase,
  ensureNamespacePath,
  setExpandedDbs,
  handleGroupContextMenu,
  handleConnectionContextMenu,
  handleDatabaseContextMenu,
  handleSchemaContextMenu,
  handleTableContextMenu,
  handleCategoryContextMenu,
  handleObjectContextMenu,
  handleConnectionClick,
  handleConnectionDoubleClick,
  handleDragStart,
  handleDragOver,
  handleDragLeave,
  handleDragEnd,
  handleDrop,
  renderStatusDot,
  viewActions,
}: NavigatorTreeRowProps) {
  switch (row.type) {
    case 'group':
      return (
        <div
          data-group-header
          className="flex cursor-pointer items-center gap-1.5 px-2 py-1 hover:bg-surface-raised/50"
          onClick={() => toggleGroup(row.groupName)}
          onContextMenu={(e) => handleGroupContextMenu(e, row.groupName)}
        >
          {row.expanded ? (
            <ChevronDown className="h-3 w-3 shrink-0 text-fg-muted" />
          ) : (
            <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" />
          )}
          {row.expanded ? (
            <FolderOpen className="h-3.5 w-3.5 shrink-0 text-amber-400" />
          ) : (
            <FolderClosed className="h-3.5 w-3.5 shrink-0 text-amber-400" />
          )}
          <span className="text-[13px] font-semibold text-fg">{row.displayName}</span>
          <span className="text-[11px] text-fg-muted">({row.count})</span>
        </div>
      );

    case 'connection': {
      const showDropBefore = dropTarget?.id === row.conn.id && dropTarget.position === 'before';
      const showDropAfter = dropTarget?.id === row.conn.id && dropTarget.position === 'after';

      return (
        <div>
          {showDropBefore && <div className="mx-2 h-[2px] rounded-full bg-accent" />}
          <div
            data-conn-item
            data-conn-name={row.conn.name}
            draggable
            onDragStart={(e) => handleDragStart(e, row.conn.id)}
            onDragOver={(e) => handleDragOver(e, row.conn.id)}
            onDragLeave={handleDragLeave}
            onDragEnd={handleDragEnd}
            onDrop={handleDrop}
            className={cn(
              'group relative flex cursor-default items-center gap-1.5 py-1 pr-2 text-[13px] transition-colors',
              row.isSelected
                ? 'bg-accent/10 text-fg'
                : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
            )}
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => handleConnectionClick(row.conn)}
            onDoubleClick={() => handleConnectionDoubleClick(row.conn)}
            onContextMenu={(e) => handleConnectionContextMenu(e, row.conn)}
          >
            {row.isSelected && <span className="absolute inset-y-0 left-0 w-0.5 bg-accent" />}
            <button
              type="button"
              className="flex h-4 w-4 shrink-0 items-center justify-center rounded text-fg-muted hover:text-fg"
              onClick={(e) => {
                e.stopPropagation();
                if (row.status === 'connected') {
                  toggleConnection(row.conn.id);
                } else {
                  handleConnectionDoubleClick(row.conn);
                }
              }}
              onDoubleClick={(e) => e.stopPropagation()}
              aria-expanded={row.status === 'connected' ? row.expanded : undefined}
            >
              {row.status === 'connected' && row.expanded ? (
                <ChevronDown className="h-3 w-3" />
              ) : (
                <ChevronRight className="h-3 w-3" />
              )}
            </button>
            <DbTypeBadge databaseType={row.conn.databaseType} size={18} className="shrink-0" />
            <span className="min-w-0 flex-1 truncate font-medium">{row.conn.name}</span>
            {renderStatusDot(row.conn.id)}
          </div>
          {showDropAfter && <div className="mx-2 h-[2px] rounded-full bg-accent" />}
        </div>
      );
    }

    case 'db':
      return (
        <button
          type="button"
          data-tree-node="db"
          data-db-name={row.dbName}
          className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
          style={{ paddingLeft: depthPadding(row.depth) }}
          onClick={() => void toggleDb(row.connectionId, row.dbSessionId, row.dbName)}
          onContextMenu={(e) => handleDatabaseContextMenu(e, row.dbName, row.connectionId)}
        >
          {row.expanded ? (
            <ChevronDown className="h-3 w-3 shrink-0" />
          ) : (
            <ChevronRight className="h-3 w-3 shrink-0" />
          )}
          <Database className="h-3.5 w-3.5 shrink-0 text-teal-400" />
          <span className="selectable min-w-0 truncate">{row.dbName}</span>
          {row.loading && <Loader2 className="h-3 w-3 shrink-0 animate-spin text-fg-muted" />}
        </button>
      );

    case 'schema':
      return (
        <button
          type="button"
          data-tree-node="schema"
          data-schema-name={row.schemaName}
          className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
          style={{ paddingLeft: depthPadding(row.depth) }}
          onClick={() => toggleSchema(`${row.connectionId}::${row.dbName}::${row.schemaName}`)}
          onContextMenu={(e) =>
            handleSchemaContextMenu(e, row.schemaName, row.dbName, row.connectionId)
          }
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
          <span className="min-w-0 truncate">{row.schemaName || t('common.default')}</span>
        </button>
      );

    case 'category': {
      const catConnectionId = row.key.split('::')[0];
      return (
        <button
          type="button"
          data-tree-node="category"
          data-cat-id={row.cat.id}
          className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
          style={{ paddingLeft: depthPadding(row.depth) }}
          onClick={() => {
            const conn = connections.find((c) => c.id === catConnectionId);
            const dbSessionId = conn ? (activeConnections[conn.id]?.dbSessionId ?? '') : '';
            void toggleCategory(row.key, row.cat.id, dbSessionId);
          }}
          onContextMenu={(e) => handleCategoryContextMenu(e, row.key, row.cat.id, catConnectionId)}
        >
          {row.expanded ? (
            <ChevronDown className="h-3 w-3 shrink-0" />
          ) : (
            <ChevronRight className="h-3 w-3 shrink-0" />
          )}
          <row.cat.icon className={`h-3.5 w-3.5 shrink-0 ${row.cat.color}`} />
          <span className="min-w-0 truncate">{t(row.cat.labelKey as Parameters<typeof t>[0])}</span>
          <span className="ml-auto shrink-0 text-[10px] text-fg-muted">{row.count}</span>
        </button>
      );
    }

    case 'table': {
      const iconColor = row.catId === 'views' ? 'text-purple-400' : 'text-blue-400';
      const Icon = row.catId === 'views' ? Eye : Table2;
      return (
        <button
          type="button"
          data-tree-node={row.catId === 'views' ? 'view' : 'table'}
          data-item-name={row.item.name}
          className={cn(
            'flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised',
            row.isSelected ? 'bg-surface-raised text-fg' : 'text-fg-secondary',
          )}
          style={{ paddingLeft: depthPadding(row.depth) }}
          onClick={() => {
            void (async () => {
              onSelectConnection(row.connectionId);
              await activateDatabase(row.dbSessionId, row.dbName);
              onSelectTable(row.item.name, row.item.schema ?? undefined, row.dbName);
            })();
          }}
          onContextMenu={(e) => {
            handleTableContextMenu(e, {
              kind: row.catId === 'views' ? 'view' : 'table',
              name: row.item.name,
              schema: row.item.schema ?? undefined,
              dbName: row.dbName,
              connectionId: row.connectionId,
              dbSessionId: row.dbSessionId,
            });
          }}
        >
          <Icon className={`h-3.5 w-3.5 shrink-0 ${iconColor}`} />
          <span className="selectable min-w-0 truncate">{row.item.name}</span>
        </button>
      );
    }

    case 'object': {
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
          data-tree-node={row.catId}
          data-item-name={row.obj.name}
          className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
          style={{ paddingLeft: depthPadding(row.depth) }}
          onClick={() => {
            const kind = row.obj.kind ?? row.catId;
            if (kind === 'function' || kind === 'procedure' || kind === 'trigger') {
              viewActions?.openObject?.(kind, row.obj.name, row.obj.schema ?? undefined);
            }
          }}
          onContextMenu={(e) => handleObjectContextMenu(e, row.obj.name)}
        >
          <ObjIcon className={`h-3.5 w-3.5 shrink-0 ${objColor}`} />
          <span className="min-w-0 truncate">{row.obj.name}</span>
        </button>
      );
    }

    case 'kv-db':
      return (
        <button
          type="button"
          data-tree-node="kv-db"
          data-db-name={row.dbName}
          className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
          style={{ paddingLeft: depthPadding(row.depth) }}
          onClick={() => {
            if (onSelectKvDb) {
              onSelectKvDb(row.connectionId, row.dbName);
            } else {
              onSelectConnection(row.connectionId);
              onSelectTable(row.dbName);
            }
          }}
        >
          <Database className="h-3.5 w-3.5 shrink-0 text-teal-400" />
          <span className="selectable min-w-0 truncate">{row.dbName}</span>
        </button>
      );

    case 'db-loading':
      return (
        <div
          className="flex items-center gap-2 py-1 text-xs text-fg-muted"
          style={{ paddingLeft: depthPadding(row.depth) }}
        >
          <Loader2 className="h-3 w-3 animate-spin" />
          {t('common.loading')}
        </div>
      );

    case 'namespace-node': {
      if (row.isLeaf) {
        const leafIcon = LEAF_KIND_ICON[row.leafKind ?? 'table'];
        const LeafIcon = leafIcon.icon;
        const menuKind =
          row.leafKind === 'view' || row.leafKind === 'materializedView'
            ? 'view'
            : row.leafKind === 'function' ||
                row.leafKind === 'procedure' ||
                row.leafKind === 'trigger'
              ? row.leafKind
              : 'table';
        return (
          <button
            type="button"
            data-tree-node={menuKind}
            data-item-name={row.name}
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => onSelectTable(row.name)}
            onContextMenu={(e) => {
              if (
                row.leafKind === 'function' ||
                row.leafKind === 'procedure' ||
                row.leafKind === 'trigger'
              ) {
                handleObjectContextMenu(e, row.name);
                return;
              }
              const conn = connections.find((c) => c.id === row.connectionId);
              const dbName =
                conn?.database ??
                useSchemaStore.getState().schemas.get(row.dbSessionId)?.currentDatabase ??
                '';
              const relationKind =
                row.leafKind === 'view' || row.leafKind === 'materializedView' ? 'view' : 'table';
              handleTableContextMenu(e, {
                kind: relationKind,
                name: row.name,
                dbName,
                connectionId: row.connectionId,
                dbSessionId: row.dbSessionId,
              });
            }}
          >
            <LeafIcon className={`h-3.5 w-3.5 shrink-0 ${leafIcon.color}`} />
            <span className="selectable min-w-0 truncate">{row.name}</span>
          </button>
        );
      }
      return (
        <button
          type="button"
          data-tree-node="namespace"
          className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
          style={{ paddingLeft: depthPadding(row.depth) }}
          onClick={() => {
            const willExpand = !expandedDbs.has(row.key);
            setExpandedDbs((prev) => {
              const next = new Set(prev);
              if (next.has(row.key)) next.delete(row.key);
              else next.add(row.key);
              return next;
            });
            if (willExpand) {
              void ensureNamespacePath(row.segments, row.dbSessionId);
            }
          }}
        >
          {row.expanded ? (
            <ChevronDown className="h-3 w-3 shrink-0" />
          ) : (
            <ChevronRight className="h-3 w-3 shrink-0" />
          )}
          <FolderOpen className="h-3.5 w-3.5 shrink-0 text-amber-400" />
          <span className="min-w-0 truncate">{row.name}</span>
        </button>
      );
    }

    case 'empty-group':
      return <div className="px-4 py-1.5 text-[11px] text-fg-muted">{t('main.noConnections')}</div>;

    case 'no-connections':
      return (
        <div className="flex flex-col items-center justify-center px-4 py-12 text-center">
          <p className="text-[13px] text-fg-muted">{t('main.noConnections')}</p>
          <button
            type="button"
            className="mt-3 inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 text-[13px] text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={onNewConnection}
          >
            <Plus className="h-4 w-4" />
            {t('main.createFirst')}
          </button>
        </div>
      );
  }
}
