import { Copy, Database, EthernetPort, Loader2, Pencil, Plug, Trash2, Unplug } from 'lucide-react';
import type { ConnectionConfig, DatabaseType } from '../../types';
import type { ConnectionStatus } from '../../stores/activeConnectionStore';
import { formatLastConnected } from '../../lib/formatters';
import { cn } from '../../lib/cn';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';

function typeLabel(t: DatabaseType) {
  switch (t) {
    case 'postgresql':
      return 'PostgreSQL';
    case 'mysql':
      return 'MySQL';
    case 'mariadb':
      return 'MariaDB';
    case 'sqlite':
      return 'SQLite';
    default:
      return t;
  }
}

export interface ConnectionCardProps {
  connection: ConnectionConfig;
  viewMode: 'grid' | 'list';
  status: ConnectionStatus;
  onConnect: (connection: ConnectionConfig) => void;
  onDisconnect: () => void;
  onEdit: (id: string) => void;
  onDuplicate: (id: string) => void;
  onDelete: (id: string) => void;
}

export function ConnectionCard({
  connection,
  viewMode,
  status,
  onConnect,
  onDisconnect,
  onEdit,
  onDuplicate,
  onDelete,
}: ConnectionCardProps) {
  const isConnecting = status === 'connecting';
  const isConnected = status === 'connected';

  const addr =
    connection.databaseType === 'sqlite'
      ? (connection.database ?? 'SQLite')
      : `${connection.host ?? ''}${connection.port ? `:${connection.port}` : ''}`;

  const title = `${connection.name} · ${typeLabel(connection.databaseType)}`;

  return (
    <div
      className={cn(
        'group relative overflow-hidden rounded-xl border bg-surface-alt',
        isConnected ? 'border-green-500/50' : 'border-edge',
        viewMode === 'list' ? 'flex items-center gap-4 p-4' : 'p-4',
      )}
      title={title}
    >
      <div
        className={cn('flex items-start gap-3', viewMode === 'list' ? 'min-w-0 flex-1' : '')}
        style={{
          borderLeftWidth: 4,
          borderLeftColor: connection.colorTag ?? 'transparent',
          paddingLeft: connection.colorTag ? 12 : 0,
        }}
      >
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-surface ring-1 ring-edge">
          <Database className="h-5 w-5 text-blue-400" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <div className="truncate text-sm font-semibold text-fg">{connection.name}</div>
            <Badge tone="accent" className="shrink-0">
              {typeLabel(connection.databaseType)}
            </Badge>
            {isConnected && (
              <span className="inline-flex h-2 w-2 shrink-0 rounded-full bg-green-500" title="已连接" />
            )}
          </div>
          <div className="mt-1 flex items-center gap-2 text-xs text-fg-secondary">
            <EthernetPort className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate font-mono" title={addr}>
              {addr}
            </span>
          </div>
          <div className="mt-2 text-xs text-fg-muted">
            最后连接：<span className="text-fg-secondary">{formatLastConnected(connection.lastConnectedAt)}</span>
          </div>
        </div>
      </div>

      <div className={cn(viewMode === 'list' ? 'flex shrink-0 items-center gap-2' : 'mt-4 flex items-center gap-2')}>
        <button
          type="button"
          title="编辑连接"
          onClick={() => onEdit(connection.id)}
          className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-edge text-fg-muted hover:bg-surface hover:text-fg"
        >
          <Pencil className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          title="复制连接"
          onClick={() => onDuplicate(connection.id)}
          className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-edge text-fg-muted hover:bg-surface hover:text-fg"
        >
          <Copy className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          title="删除连接"
          onClick={() => onDelete(connection.id)}
          className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-edge text-fg-muted hover:bg-red-500/10 hover:text-red-400"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
        <div className="flex-1" />
        {isConnected ? (
          <Button
            variant="secondary"
            className="h-8 min-w-[80px]"
            onClick={onDisconnect}
          >
            <Unplug className="h-4 w-4" />
            断开
          </Button>
        ) : (
          <Button
            variant="primary"
            className="h-8 min-w-[80px] flex-1"
            onClick={() => onConnect(connection)}
            disabled={isConnecting}
          >
            {isConnecting ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Plug className="h-4 w-4" />
            )}
            {isConnecting ? '连接中…' : '连接'}
          </Button>
        )}
      </div>
    </div>
  );
}
