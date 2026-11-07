import { useMemo, useState } from 'react';
import { Download, Play, Plus, Search, X } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { DbTypeBadge } from '../../../components/DbTypeBadge';
import { ThemedIcon } from '../../../components/ThemedIcon';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { getDbLabel } from '../../../lib/databaseTypes';
import type { ConnectionEntry } from '../../../stores/activeConnectionStore';
import type { ConnectionConfig } from '../../../types';
import { Spinner } from '../../../components/ui/Spinner';

/** Number of connections rendered before expanding the list. */
const COLLAPSED_COUNT = 6;

interface ConnectionCardListProps {
  connections: ConnectionConfig[];
  activeConnections: Record<string, ConnectionEntry>;
  onConnect: (connectionId: string) => void;
  onNewConnection: () => void;
  onImportConnections?: () => void;
}

/**
 * Core "Your connections" section: inline filter, explicit Connect/Open
 * affordances, host:port·group metadata, and Show all/Show less paging.
 * Replaces the former Quick Start (hard-capped at 4) + Common Ops split.
 */
export function ConnectionCardList({
  connections,
  activeConnections,
  onConnect,
  onNewConnection,
  onImportConnections,
}: ConnectionCardListProps) {
  const { t } = useI18n();
  const [filter, setFilter] = useState('');
  const [expanded, setExpanded] = useState(false);

  // Top connections: pinned first, then lastConnectedAt, then name.
  // (Sorting rules preserved verbatim from the former quickConnections memo.)
  const sortedConnections = useMemo(() => {
    return [...connections].sort((a, b) => {
      if (a.pinned && !b.pinned) return -1;
      if (!a.pinned && b.pinned) return 1;
      const aTime = a.lastConnectedAt ? new Date(a.lastConnectedAt).getTime() : 0;
      const bTime = b.lastConnectedAt ? new Date(b.lastConnectedAt).getTime() : 0;
      if (aTime !== bTime) return bTime - aTime;
      return a.name.localeCompare(b.name);
    });
  }, [connections]);

  const normalizedFilter = filter.trim().toLowerCase();
  const filteredConnections = useMemo(() => {
    if (!normalizedFilter) {
      return sortedConnections;
    }
    return sortedConnections.filter((conn) => {
      const haystack = [
        conn.name,
        conn.host ?? '',
        conn.database ?? '',
        conn.group ?? '',
        getDbLabel(conn.databaseType),
      ]
        .join(' ')
        .toLowerCase();
      return haystack.includes(normalizedFilter);
    });
  }, [sortedConnections, normalizedFilter]);

  const visibleConnections = expanded
    ? filteredConnections
    : filteredConnections.slice(0, COLLAPSED_COUNT);

  const showPaging = filteredConnections.length > COLLAPSED_COUNT;

  return (
    <section className="flex flex-col gap-2.5" data-testid="home-connections-section">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
          {t('connWin.home.yourConnections')}
        </h3>

        <div className="relative ml-auto w-44 sm:w-56">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
          <input
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder={t('connWin.home.connections.filter')}
            data-testid="home-connections-filter"
            className="h-7 w-full rounded-md border border-edge bg-surface pl-8 pr-7 text-xs text-fg placeholder:text-fg-muted focus:border-accent/50 focus:outline-none focus:ring-1 focus:ring-accent/40"
          />
          {filter && (
            <button
              type="button"
              onClick={() => setFilter('')}
              data-testid="home-connections-filter-clear"
              className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-fg-muted hover:text-fg"
              aria-label={t('connWin.home.connections.clearFilter')}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>

        {onImportConnections && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onImportConnections}
            data-testid="empty-import-connections-button"
          >
            <Download className="h-3.5 w-3.5" />
            {t('common.importConnections')}
          </Button>
        )}

        <Button size="sm" onClick={onNewConnection} data-testid="empty-new-connection-button">
          <ThemedIcon id="common.newConnection" className="h-3.5 w-3.5" fallback={Plus} />
          {t('common.newConnection')}
        </Button>
      </div>

      <div className="flex flex-col overflow-hidden rounded-xl border border-edge bg-surface-alt divide-y divide-edge/60">
        {visibleConnections.map((conn) => {
          const isActive = activeConnections[conn.id]?.status === 'connected';
          const isConnLoading = activeConnections[conn.id]?.status === 'connecting';
          const hostPort = conn.host
            ? `${conn.host}${conn.port ? `:${conn.port}` : ''}`
            : conn.database || getDbLabel(conn.databaseType);

          return (
            <div
              key={conn.id}
              role="button"
              tabIndex={0}
              onClick={() => onConnect(conn.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onConnect(conn.id);
                }
              }}
              data-testid={`home-conn-card-${conn.id}`}
              className="group flex w-full cursor-pointer items-center justify-between px-4 py-3 text-left transition-colors hover:bg-surface-raised"
            >
              <div className="flex min-w-0 items-center gap-3.5">
                <DbTypeBadge databaseType={conn.databaseType} size={34} />
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-semibold text-fg transition-colors group-hover:text-accent">
                      {conn.name}
                    </span>
                    <span className="rounded border border-edge/60 bg-surface px-1.5 py-0.5 text-[10px] font-medium text-fg-muted">
                      {getDbLabel(conn.databaseType)}
                    </span>
                  </div>
                  <div className="mt-0.5 truncate font-mono text-xs text-fg-muted">
                    {hostPort}
                    {conn.group ? <span className="text-fg-muted/70"> · {conn.group}</span> : null}
                  </div>
                </div>
              </div>

              <div className="ml-3 flex shrink-0 items-center gap-3">
                <span
                  className={cn(
                    'inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium',
                    isActive
                      ? 'bg-success/15 text-success'
                      : isConnLoading
                        ? 'bg-accent/15 text-accent'
                        : 'bg-surface text-fg-muted',
                  )}
                >
                  <span
                    className={cn(
                      'h-1.5 w-1.5 rounded-full',
                      isActive
                        ? 'bg-success'
                        : isConnLoading
                          ? 'bg-accent animate-ping'
                          : 'bg-fg-muted/40',
                    )}
                  />
                  {isActive
                    ? t('connWin.home.status.connected')
                    : isConnLoading
                      ? t('conn.connecting')
                      : t('connWin.home.status.offline')}
                </span>

                {isConnLoading ? (
                  <Button size="sm" disabled aria-label={t('conn.connecting')}>
                    <Spinner size="md" />
                  </Button>
                ) : (
                  <Button
                    size="sm"
                    variant={isActive ? 'secondary' : 'primary'}
                    onClick={(e) => {
                      e.stopPropagation();
                      onConnect(conn.id);
                    }}
                    data-testid={`home-conn-connect-${conn.id}`}
                  >
                    {isActive ? (
                      t('connWin.home.connections.open')
                    ) : (
                      <>
                        <Play className="h-3 w-3 fill-current" />
                        {t('connWin.home.connections.connect')}
                      </>
                    )}
                  </Button>
                )}
              </div>
            </div>
          );
        })}

        {visibleConnections.length === 0 && (
          <div className="flex flex-col items-center justify-center gap-1 px-4 py-8 text-center text-fg-muted">
            <Search className="mb-1 h-5 w-5 opacity-40" />
            <p className="text-xs" data-testid="home-connections-no-match">
              {t('connWin.home.connections.noMatch', { query: filter.trim() })}
            </p>
            {onImportConnections && (
              <p className="text-[11px] text-fg-muted/80">{t('common.importConnections')}</p>
            )}
          </div>
        )}
      </div>

      {(showPaging || (filteredConnections.length > 0 && expanded)) && (
        <div className="flex items-center justify-between px-0.5">
          <span className="text-xs text-fg-muted">
            {t('connWin.home.connections.showing', {
              visible: String(visibleConnections.length),
              total: String(filteredConnections.length),
            })}
          </span>
          <button
            type="button"
            data-testid="home-connections-show-all"
            onClick={() => setExpanded((prev) => !prev)}
            className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
          >
            {expanded
              ? t('connWin.home.connections.showLess')
              : t('connWin.home.connections.showAll', {
                  count: String(filteredConnections.length),
                })}
          </button>
        </div>
      )}
    </section>
  );
}
