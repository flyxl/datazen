import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { formatGroupLabel } from '../../lib/connectionGroups';
import {
  buildMainConnectionContextMenuItems,
  buildMainGroupContextMenuItems,
} from '../../lib/mainWindowContextMenu';
import { groupConnections, useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { showWebContextMenu } from '../../stores/contextMenuStore';
import type { ConnectionConfig } from '../../types';
import { SchemaTree } from './schema-tree/SchemaTree';

export interface ConnectionNavigatorTreeProps {
  /** Search query filtering connections and tables */
  searchQuery: string;
  /** Called when user selects a connection (click/double-click) */
  onSelectConnection: (configId: string) => void;
  /** Called when user wants to open a table */
  onSelectTable: (tableName: string, schema?: string) => void;
  /** The currently active configId */
  activeConfigId: string | null;
  /** Called when user wants to create a new connection */
  onNewConnection: () => void;
  /** Called when user wants to edit a connection */
  onEditConnection: (configId: string) => void;
  /** Called when user wants to delete a connection */
  onDeleteConnection: (configId: string) => void;
  /** Called when user wants to disconnect */
  onDisconnect: (configId: string) => void;
  /** Called for schema tree context menu events */
  onNodeContextMenu?: (payload: {
    kind: string;
    name: string;
    x: number;
    y: number;
    schema?: string;
  }) => void;
}

export function ConnectionNavigatorTree({
  searchQuery,
  onSelectConnection,
  onSelectTable,
  activeConfigId,
  onNewConnection,
  onEditConnection,
  onDeleteConnection,
  onDisconnect,
  onNodeContextMenu,
}: ConnectionNavigatorTreeProps) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);
  const groups = useConnectionStore((s) => s.groups);
  const activeConnections = useActiveConnectionStore((s) => s.connections);
  const connect = useActiveConnectionStore((s) => s.connect);

  const grouped = useMemo(
    () => groupConnections(connections, groups, searchQuery),
    [connections, groups, searchQuery],
  );

  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [expandedConnections, setExpandedConnections] = useState<Set<string>>(new Set());

  const prevGroupsRef = useRef<string[] | null>(null);
  useEffect(() => {
    const prev = prevGroupsRef.current;
    prevGroupsRef.current = groups;
    if (!prev) {
      setExpandedGroups(new Set([...groups, '']));
      return;
    }
    const newGroups = groups.filter((g) => !prev.includes(g));
    if (newGroups.length === 0) return;
    setExpandedGroups((s) => {
      const next = new Set(s);
      for (const g of newGroups) next.add(g);
      next.add('');
      return next;
    });
  }, [groups]);

  useEffect(() => {
    if (!activeConfigId) return;
    if (activeConnections[activeConfigId]?.status === 'connected') {
      setExpandedConnections((prev) => new Set(prev).add(activeConfigId));
    }
  }, [activeConfigId, activeConnections]);

  const contextLabels = useMemo(
    () => ({
      newGroup: t('main.ctx.newGroup'),
      newConnection: t('main.newConnection'),
      renameGroup: t('main.ctx.renameGroup'),
      deleteGroup: t('main.ctx.deleteGroup'),
      openConnection: t('main.ctx.openConnection'),
      disconnect: t('main.ctx.disconnect'),
      editConnection: t('main.ctx.editConnection'),
      duplicateConnection: t('main.ctx.duplicateConnection'),
      moveToGroup: t('main.ctx.moveToGroup'),
      removeFromGroup: t('main.ctx.removeFromGroup'),
      deleteConnection: t('main.ctx.deleteConnection'),
    }),
    [t],
  );

  const toggleGroup = useCallback((group: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  }, []);

  const toggleConnection = useCallback(
    (configId: string) => {
      setExpandedConnections((prev) => {
        const next = new Set(prev);
        if (next.has(configId)) {
          next.delete(configId);
        } else {
          next.add(configId);
          onSelectConnection(configId);
        }
        return next;
      });
    },
    [onSelectConnection],
  );

  const handleGroupContextMenu = useCallback(
    (e: React.MouseEvent, groupName: string) => {
      e.preventDefault();
      e.stopPropagation();
      showWebContextMenu(
        buildMainGroupContextMenuItems({
          labels: contextLabels,
          isUngrouped: groupName === '',
          onNewGroup: () => undefined,
          onRenameGroup: () => undefined,
          onDeleteGroup: () => undefined,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [contextLabels],
  );

  const handleConnectionContextMenu = useCallback(
    (e: React.MouseEvent, conn: ConnectionConfig) => {
      e.preventDefault();
      e.stopPropagation();
      onSelectConnection(conn.id);

      const isConnected = activeConnections[conn.id]?.status === 'connected';
      const moveTargets = groups
        .filter((g) => g !== conn.group)
        .map((g) => ({ id: g, label: formatGroupLabel(g, t) }));

      showWebContextMenu(
        buildMainConnectionContextMenuItems({
          labels: contextLabels,
          isConnected,
          grouped: Boolean(conn.group),
          moveTargets,
          onOpenOrDisconnect: () => {
            if (isConnected) onDisconnect(conn.id);
            else void connect(conn);
          },
          onEdit: () => onEditConnection(conn.id),
          onDuplicate: () => undefined,
          onMoveToGroup: () => undefined,
          onRemoveFromGroup: () => undefined,
          onDelete: () => onDeleteConnection(conn.id),
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      activeConnections,
      connect,
      contextLabels,
      groups,
      onDeleteConnection,
      onDisconnect,
      onEditConnection,
      onSelectConnection,
      t,
    ],
  );

  const handleConnectionClick = useCallback(
    (conn: ConnectionConfig) => {
      onSelectConnection(conn.id);
    },
    [onSelectConnection],
  );

  const handleConnectionDoubleClick = useCallback(
    (conn: ConnectionConfig) => {
      const status = activeConnections[conn.id]?.status ?? 'idle';
      if (status === 'connected') {
        onSelectConnection(conn.id);
      } else if (status !== 'connecting') {
        void connect(conn);
      }
    },
    [activeConnections, connect, onSelectConnection],
  );

  const renderStatusDot = (configId: string) => {
    const status = activeConnections[configId]?.status ?? 'idle';
    if (status === 'connecting') {
      return (
        <span
          className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-yellow-400"
          title={t('conn.connecting')}
        />
      );
    }
    if (status === 'connected') {
      return (
        <span className="h-2 w-2 shrink-0 rounded-full bg-green-500" title={t('conn.connected')} />
      );
    }
    if (status === 'error') {
      return <span className="h-2 w-2 shrink-0 rounded-full bg-red-500" title={t('conn.failed')} />;
    }
    return null;
  };

  const renderConnection = (conn: ConnectionConfig) => {
    const status = activeConnections[conn.id]?.status ?? 'idle';
    const isConnected = status === 'connected';
    const isExpanded = expandedConnections.has(conn.id);
    const isSelected = activeConfigId === conn.id;
    const runtimeEntry = activeConnections[conn.id];
    const connectionId = runtimeEntry?.connectionId;
    const showSchema = isConnected && isExpanded && isSelected && Boolean(connectionId);

    return (
      <div key={conn.id} className="border-l border-edge">
        <div
          data-conn-item
          data-conn-name={conn.name}
          className={cn(
            'group relative flex cursor-default items-center gap-1.5 px-2 py-1.5 text-[13px] transition-colors',
            isSelected
              ? 'bg-accent/10 text-fg'
              : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
          )}
          onClick={() => handleConnectionClick(conn)}
          onDoubleClick={() => handleConnectionDoubleClick(conn)}
          onContextMenu={(e) => handleConnectionContextMenu(e, conn)}
        >
          {isSelected && <span className="absolute inset-y-0 left-0 w-0.5 bg-accent" />}
          {isConnected ? (
            <button
              type="button"
              className="flex h-4 w-4 shrink-0 items-center justify-center rounded text-fg-muted hover:text-fg"
              onClick={(e) => {
                e.stopPropagation();
                toggleConnection(conn.id);
              }}
              onDoubleClick={(e) => e.stopPropagation()}
              aria-expanded={isExpanded}
            >
              {isExpanded ? (
                <ChevronDown className="h-3 w-3" />
              ) : (
                <ChevronRight className="h-3 w-3" />
              )}
            </button>
          ) : (
            <span className="h-4 w-4 shrink-0" aria-hidden />
          )}
          <DbTypeBadge databaseType={conn.databaseType} size={20} className="shrink-0" />
          <span className="min-w-0 flex-1 truncate font-medium">{conn.name}</span>
          {renderStatusDot(conn.id)}
        </div>

        {showSchema && connectionId && (
          <div className="border-l border-edge pl-6">
            <SchemaTree
              connectionId={connectionId}
              databaseType={conn.databaseType}
              initialDatabase={conn.database}
              selectedTable={null}
              searchQuery={searchQuery}
              onSelectTable={onSelectTable}
              onNodeContextMenu={onNodeContextMenu}
            />
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto bg-surface py-1">
      {grouped.length === 0 ? (
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
      ) : (
        grouped.map(({ group: groupName, connections: groupConns }) => {
          const expanded = expandedGroups.has(groupName);
          const displayName = groupName ? formatGroupLabel(groupName, t) : t('main.ungrouped');

          return (
            <div key={groupName || '__ungrouped__'} data-group-name={groupName}>
              <div
                data-group-header
                className="flex cursor-pointer items-center gap-1.5 px-2 py-1.5 hover:bg-surface-raised/50"
                onClick={() => toggleGroup(groupName)}
                onContextMenu={(e) => handleGroupContextMenu(e, groupName)}
              >
                {expanded ? (
                  <ChevronDown className="h-3 w-3 shrink-0 text-fg-muted" />
                ) : (
                  <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" />
                )}
                <span className="text-[13px] font-semibold text-fg">{displayName}</span>
                <span className="text-[11px] text-fg-muted">({groupConns.length})</span>
              </div>

              {expanded && (
                <div className="pl-4">
                  {groupConns.length === 0 ? (
                    <div className="px-2 py-1.5 text-[11px] text-fg-muted">
                      {t('main.noConnections')}
                    </div>
                  ) : (
                    groupConns.map(renderConnection)
                  )}
                </div>
              )}
            </div>
          );
        })
      )}
    </div>
  );
}
