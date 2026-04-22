import { useCallback } from 'react';
import type { ConnectionConfig } from '../../types';
import type { ConnectionStatus } from '../../stores/activeConnectionStore';
import { cn } from '../../lib/cn';
import { getDbIcon, formatConnectionAddr } from '../../lib/databaseTypes';
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
  const { label, bg } = getDbIcon(connection.databaseType);
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
      className={cn(
        'group flex cursor-default select-none items-center gap-3 rounded-lg px-3 py-2.5 transition-colors',
        isDragging && 'opacity-40',
        selected
          ? 'bg-blue-500/10 ring-1 ring-blue-500/30'
          : 'hover:bg-surface-raised/60',
      )}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      onContextMenu={(e) => onContextMenu(e, connection)}
      onPointerDown={(e) => onPointerDown(e, connection)}
    >
      <div
        className={cn(
          'flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-[11px] font-bold text-white shadow-sm',
          bg,
        )}
      >
        {label}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[13px] font-medium text-fg">{connection.name}</span>
          {isLocal && (
            <span className="text-[11px] font-medium text-green-500">(local)</span>
          )}
          {hasSSH && (
            <span className="text-[11px] font-medium text-blue-400">(SSH)</span>
          )}
        </div>
        <div className="mt-0.5 truncate text-[11px] text-fg-muted">{addr}</div>
      </div>
      {isConnected && (
        <span className="h-2 w-2 shrink-0 rounded-full bg-green-500" title={t('conn.connected')} />
      )}
    </div>
  );
}
