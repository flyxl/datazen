import { useCallback } from 'react';
import type { ConnectionConfig } from '../../types';
import type { ConnectionStatus } from '../../stores/activeConnectionStore';
import { cn } from '../../lib/cn';
import { formatConnectionAddr } from '../../lib/databaseTypes';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { useI18n } from '../../hooks/useI18n';

export interface ConnectionItemProps {
  connection: ConnectionConfig;
  status: ConnectionStatus;
  selected: boolean;
  isDragging?: boolean;
  onSelect: (id: string) => void;
  onConnect: (cfg: ConnectionConfig) => void;
  onContextMenu: (e: React.MouseEvent, cfg: ConnectionConfig) => void;
  onPointerDown: (e: React.PointerEvent, cfg: ConnectionConfig) => void;
}

export function ConnectionItem({
  connection,
  status,
  selected,
  isDragging,
  onSelect,
  onConnect,
  onContextMenu,
  onPointerDown,
}: ConnectionItemProps) {
  const { t } = useI18n();
  const isConnected = status === 'connected';
  const hasSSH = connection.sshTunnel?.enabled === true;
  const isLocal = !hasSSH && (connection.host === 'localhost' || connection.host === '127.0.0.1');
  const addr = formatConnectionAddr(connection);

  const handleDoubleClick = useCallback(() => {
    onConnect(connection);
  }, [connection, onConnect]);

  const handleClick = useCallback(() => {
    onSelect(connection.id);
  }, [connection.id, onSelect]);

  return (
    <div
      data-conn-item
      data-conn-name={connection.name}
      className={cn(
        'group flex cursor-default select-none items-center gap-3 rounded-lg px-3 py-2.5 transition-colors',
        isDragging && 'opacity-40',
        selected
          ? 'bg-accent/10 ring-1 ring-accent/30'
          : 'hover:bg-surface-raised/60',
      )}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      onContextMenu={(e) => onContextMenu(e, connection)}
      onPointerDown={(e) => onPointerDown(e, connection)}
    >
      <DbTypeBadge databaseType={connection.databaseType} size={36} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[13px] font-medium text-fg">{connection.name}</span>
          {isLocal && (
            <span className="text-[11px] font-medium text-green-500">(local)</span>
          )}
          {hasSSH && (
            <span className="text-[11px] font-medium text-accent">(SSH)</span>
          )}
        </div>
        <div className="mt-0.5 truncate text-[11px] text-fg-muted">{addr}</div>
      </div>
      {status === 'connecting' && (
        <span className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-yellow-400" title={t('conn.connecting')} />
      )}
      {isConnected && (
        <span className="h-2 w-2 shrink-0 rounded-full bg-green-500" title={t('conn.connected')} />
      )}
      {status === 'error' && (
        <span className="h-2 w-2 shrink-0 rounded-full bg-red-500" title={t('conn.failed')} />
      )}
    </div>
  );
}
