import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  Check,
  ChevronRight,
  Code2,
  Copy,
  Database,
  Download,
  GitFork,
  Plus,
  TableProperties,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useCopyFeedback } from '../../components/ui/useCopyFeedback';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { ThemedIcon } from '../../components/ThemedIcon';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getDbLabel } from '../../lib/databaseTypes';
import { useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { type ConnectionContext, type Panel } from '../../stores/panelStore';
import { queryCommands } from '../../commands/query';
import type { DatabaseType, QueryHistoryEntry } from '../../types';
import { getPanelIcon, getPanelLabel } from './contentViewHelpers';
import type { KvConnectionHomeBinding } from './useKvWorkspaceSlots';
import { GlobalQueryHistoryDialog } from '../../components/history/GlobalQueryHistoryDialog';
import { HomeHero } from './home/HomeHero';
import { ConnectionCardList } from './home/ConnectionCardList';
import { RecentQueriesList } from './home/RecentQueriesList';
import { McpPromoBar } from './home/McpPromoBar';
import { ShortcutFooter } from './home/ShortcutFooter';
import { Spinner } from '../../components/ui/Spinner';

/** How long the per-row "copied" marker stays before reverting. */
const COPIED_FEEDBACK_MS = 2000;

export interface ConnectionWorkspaceHomeProps {
  hasConnections: boolean;
  connectionContext: ConnectionContext | null;
  recentPanels: Panel[];
  showNewQuery: boolean;
  showNewTable: boolean;
  showErDiagram: boolean;
  showObjects: boolean;
  /** True when a connection is being established (no dbSessionId yet). */
  isConnecting?: boolean;
  /** Name of the connection being established (shown during loading). */
  connectingName?: string;
  /** Database type of the connection being established. */
  connectingDbType?: DatabaseType;
  onNewConnection: () => void;
  onImportConnections?: () => void;
  onNewQuery: () => void;
  onCreateTable: () => void;
  onOpenErDiagram: () => void;
  onOpenObjects: () => void;
  onOpenPanel: (panelId: string) => void;
  onSelectConnection?: (connectionId: string) => void;
  onOpenQueryHistory?: () => void;
  onSelectHistoryQuery?: (entry: QueryHistoryEntry) => void;
  /**
   * Driver-contributed connection home (屏 A). When the connected session's driver
   * claims the `home` capability and contributed this component, the whole landing
   * surface yields to it; otherwise the host banner page renders exactly as before.
   */
  connectionHomeSlot?: KvConnectionHomeBinding;
}

interface QuickActionProps {
  icon: ReactNode;
  label: string;
  onClick: () => void;
  primary?: boolean;
  testId?: string;
}

function QuickAction({
  icon,
  label,
  onClick,
  primary = false,
  testId,
}: Readonly<QuickActionProps>) {
  return (
    <button
      type="button"
      onClick={onClick}
      data-testid={testId}
      className={cn(
        'flex min-h-[88px] flex-col items-start justify-between rounded-xl border p-4 text-left transition-all',
        primary
          ? 'border-accent/30 bg-accent/10 hover:border-accent/50 hover:bg-accent/15'
          : 'border-edge bg-surface-alt hover:border-edge-hover hover:bg-surface-raised',
      )}
    >
      <span
        className={cn(
          'rounded-md p-2',
          primary ? 'bg-accent/15 text-accent' : 'bg-surface text-fg-secondary',
        )}
      >
        {icon}
      </span>
      <span className={cn('text-sm font-medium', primary ? 'text-fg' : 'text-fg-secondary')}>
        {label}
      </span>
    </button>
  );
}

export function ConnectionWorkspaceHome({
  hasConnections,
  connectionContext,
  recentPanels,
  showNewQuery,
  showNewTable,
  showErDiagram,
  showObjects,
  isConnecting = false,
  connectingName,
  connectingDbType,
  onNewConnection,
  onImportConnections,
  onNewQuery,
  onCreateTable,
  onOpenErDiagram,
  onOpenObjects,
  onOpenPanel,
  onSelectConnection,
  onSelectHistoryQuery,
  connectionHomeSlot,
}: Readonly<ConnectionWorkspaceHomeProps>) {
  const { t } = useI18n();

  const savedConnections = useConnectionStore((s) => s.connections);
  const activeConnections = useActiveConnectionStore((s) => s.connections);

  const [recentQueries, setRecentQueries] = useState<QueryHistoryEntry[]>([]);
  const [copiedSqlId, setCopiedSqlId] = useState<string | null>(null);
  const { copied, copy } = useCopyFeedback(COPIED_FEEDBACK_MS);
  const [globalHistoryOpen, setGlobalHistoryOpen] = useState(false);

  /**
   * `copied` is the shared, request-bound flag and `copiedSqlId` names the row
   * it belongs to; gating on both keeps a rolled-back write from leaving a
   * stale "copied" marker on the last attempted row.
   */
  const copiedRowId = copied ? copiedSqlId : null;

  const handleCopySql = useCallback(
    (id: string, sql: string) => {
      copy(sql);
      setCopiedSqlId(id);
    },
    [copy],
  );

  // Load recent query history (global or connection-scoped)
  useEffect(() => {
    let unmounted = false;
    if (typeof queryCommands?.getQueryHistoryPage !== 'function') {
      return;
    }
    queryCommands
      .getQueryHistoryPage({ limit: 5, connectionId: connectionContext?.connectionId })
      .then((page) => {
        if (!unmounted && Array.isArray(page?.entries)) {
          setRecentQueries(page.entries);
        }
      })
      .catch(() => {
        // Safe fallback in test/mock environment
      });
    return () => {
      unmounted = true;
    };
  }, [connectionContext?.connectionId]);

  const distinctDbTypes = useMemo(() => {
    return Array.from(new Set(savedConnections.map((c) => c.databaseType)));
  }, [savedConnections]);

  const groupCount = useMemo(() => {
    return new Set(
      savedConnections.map((c) => c.group).filter((group): group is string => Boolean(group)),
    ).size;
  }, [savedConnections]);

  const handleConnect = (connectionId: string) => {
    if (onSelectConnection) {
      onSelectConnection(connectionId);
      return;
    }
    const target = savedConnections.find((c) => c.id === connectionId);
    if (target) {
      void useActiveConnectionStore.getState().connect(target);
    }
  };

  /** Open the global query history dialog */
  const handleOpenHistory = () => {
    setGlobalHistoryOpen(true);
  };

  // ── State 1: No connections at all ──
  if (!hasConnections) {
    return (
      <div
        className="flex flex-1 items-center justify-center px-6"
        data-testid="connection-workspace-home"
      >
        <div className="max-w-sm text-center">
          <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-accent/10 text-accent">
            <Database className="h-6 w-6" />
          </div>
          <p className="text-sm font-medium text-fg">{t('main.noConnections')}</p>
          <p className="mt-1.5 text-xs text-fg-muted">{t('connWin.home.emptyNoConnectionsHint')}</p>
          <div className="mt-4 flex flex-wrap items-center justify-center gap-2">
            {onImportConnections && (
              <Button
                variant="ghost"
                onClick={onImportConnections}
                data-testid="import-connections-button"
              >
                <Download className="h-4 w-4" />
                {t('common.importConnections')}
              </Button>
            )}
            <Button className="mt-0" onClick={onNewConnection} data-testid="new-connection-button">
              <ThemedIcon id="common.newConnection" className="h-4 w-4" fallback={Plus} />
              {t('main.createFirst')}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  // ── State 2: Connecting in progress ──
  if (!connectionContext && isConnecting) {
    return (
      <div
        className="flex flex-1 items-center justify-center px-6"
        data-testid="connection-workspace-home"
      >
        <div className="flex flex-col items-center gap-3">
          {connectingDbType && <DbTypeBadge databaseType={connectingDbType} size={48} />}
          <Spinner size="xl" tone="muted" label={t('conn.connecting')} />
          {connectingName && <p className="text-sm text-fg-muted">{connectingName}</p>}
        </div>
      </div>
    );
  }

  // ── State 3: Landing page when no connection session is active ──
  if (!connectionContext) {
    return (
      <div
        className="flex flex-1 flex-col overflow-y-auto px-6 py-6 md:px-8 md:py-8"
        data-testid="connection-workspace-home"
      >
        <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
          <HomeHero
            connectionCount={savedConnections.length}
            groupCount={groupCount}
            dbTypes={distinctDbTypes}
          />

          <ConnectionCardList
            connections={savedConnections}
            activeConnections={activeConnections}
            onConnect={handleConnect}
            onNewConnection={onNewConnection}
            onImportConnections={onImportConnections}
          />

          <RecentQueriesList
            entries={recentQueries}
            savedConnections={savedConnections}
            onSelectHistoryQuery={onSelectHistoryQuery}
            onOpenHistory={handleOpenHistory}
          />

          <McpPromoBar />

          <ShortcutFooter />
        </div>
        {globalHistoryOpen && (
          <GlobalQueryHistoryDialog
            open={globalHistoryOpen}
            onClose={() => setGlobalHistoryOpen(false)}
            onSelectQuery={onSelectHistoryQuery}
          />
        )}
      </div>
    );
  }

  // ── State 3b: driver-claimed connection home (屏 A) ──
  // The KV overview replaces the host banner page wholesale; the host keeps the
  // scroll container and the landing-screen test id, the driver owns the layout.
  const ConnectionHome = connectionHomeSlot?.Component;
  if (ConnectionHome && connectionHomeSlot && connectionContext) {
    return (
      <div className="flex flex-1 flex-col overflow-y-auto" data-testid="connection-workspace-home">
        <div
          className="flex min-h-0 flex-1 flex-col"
          data-slot="kv-connection-home"
          data-testid="home-kv-connection-home"
        >
          {/* eslint-disable-next-line @typescript-eslint/no-explicit-any -- RedisOverviewHome extends ConnectionHomeSlotProps with onOpenTarget */}
          <ConnectionHome
            {...(connectionHomeSlot.props as any)}
            {...(connectionHomeSlot.onOpenTarget
              ? { onOpenTarget: connectionHomeSlot.onOpenTarget }
              : {})}
          />
        </div>
      </div>
    );
  }

  // ── State 4: Connected Workspace Home when a connection session is active ──
  const quickActions = [
    showNewQuery
      ? {
          key: 'new-query',
          label: t('common.newQuery'),
          icon: <ThemedIcon id="common.newQuery" className="h-4 w-4" fallback={Code2} />,
          onClick: onNewQuery,
          primary: true,
        }
      : null,
    showNewTable
      ? {
          key: 'new-table',
          label: t('common.newTable'),
          icon: <ThemedIcon id="common.newTable" className="h-4 w-4" fallback={TableProperties} />,
          onClick: onCreateTable,
        }
      : null,
    showErDiagram
      ? {
          key: 'er-diagram',
          label: t('common.erDiagram'),
          icon: <ThemedIcon id="common.erDiagram" className="h-4 w-4" fallback={GitFork} />,
          onClick: onOpenErDiagram,
        }
      : null,
    showObjects
      ? {
          key: 'objects',
          label: t('objects.title'),
          icon: <ThemedIcon id="common.objects" className="h-4 w-4" fallback={Code2} />,
          onClick: onOpenObjects,
        }
      : null,
  ].filter((action): action is NonNullable<typeof action> => action != null);

  return (
    <div
      className="flex flex-1 flex-col overflow-y-auto px-6 py-6 md:px-8 md:py-8"
      data-testid="connection-workspace-home"
    >
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
        {/* Connected Connection Header Banner */}
        <div className="flex items-center justify-between rounded-xl border border-edge bg-surface-alt p-5 shadow-sm">
          <div className="flex items-center gap-4 min-w-0">
            <DbTypeBadge databaseType={connectionContext.databaseType} size={48} />
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <h2 className="truncate text-lg font-bold text-fg">
                  {connectionContext.connectionName}
                </h2>
                <span className="inline-flex items-center gap-1 rounded-full bg-success/15 px-2 py-0.5 text-[11px] font-medium text-success">
                  <span className="h-1.5 w-1.5 rounded-full bg-success animate-pulse" />
                  {t('connWin.home.status.connected')}
                </span>
              </div>
              <p className="mt-1 text-xs text-fg-muted">
                {getDbLabel(connectionContext.databaseType)} · {t('connWin.home.subtitle')}
              </p>
            </div>
          </div>
        </div>

        {/* Quick Actions */}
        {quickActions.length > 0 && (
          <section>
            <h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-fg-muted">
              {t('connWin.home.quickActions')}
            </h3>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-4">
              {quickActions.map((action) => (
                <QuickAction
                  key={action.key}
                  testId={`home-quick-${action.key}`}
                  icon={action.icon}
                  label={action.label}
                  onClick={action.onClick}
                  primary={action.primary}
                />
              ))}
            </div>
          </section>
        )}

        {/* Recent Panels */}
        {recentPanels.length > 0 && (
          <section>
            <h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-fg-muted">
              {t('connWin.home.recentPanels')}
            </h3>
            <div className="overflow-hidden rounded-xl border border-edge bg-surface-alt">
              {recentPanels.map((panel, index) => (
                <button
                  key={panel.id}
                  type="button"
                  onClick={() => onOpenPanel(panel.id)}
                  className={cn(
                    'flex w-full items-center justify-between px-4 py-3 text-left transition-colors hover:bg-surface-raised',
                    index > 0 && 'border-t border-edge',
                  )}
                >
                  <div className="flex items-center gap-3 min-w-0">
                    {getPanelIcon(panel)}
                    <span className="min-w-0 truncate text-sm text-fg">
                      {getPanelLabel(panel, t)}
                    </span>
                  </div>
                  <ChevronRight className="h-4 w-4 text-fg-muted opacity-50" />
                </button>
              ))}
            </div>
          </section>
        )}

        {/* Connection-Scoped Recent Queries */}
        {recentQueries.length > 0 && (
          <section>
            <div className="mb-3 flex items-center justify-between">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
                {t('connWin.home.recentQueries')}
              </h3>
              <button
                type="button"
                data-testid="view-all-history-button"
                onClick={handleOpenHistory}
                className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
              >
                <span>{t('connWin.home.viewAll')}</span>
                <ChevronRight className="h-3 w-3" />
              </button>
            </div>
            <div className="grid grid-cols-1 gap-2.5">
              {recentQueries.slice(0, 3).map((item) => (
                <div
                  key={item.id}
                  role="button"
                  tabIndex={0}
                  onClick={() => onSelectHistoryQuery?.(item)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onSelectHistoryQuery?.(item);
                    }
                  }}
                  className="flex items-center justify-between rounded-xl border border-edge bg-surface-alt p-3 transition-colors hover:bg-surface-raised hover:border-accent/40 cursor-pointer text-left"
                >
                  <div className="min-w-0 flex-1 pr-4">
                    <div className="flex items-center gap-2 text-xs text-fg-muted">
                      <span
                        className={cn(
                          'h-1.5 w-1.5 rounded-full',
                          item.success ? 'bg-success' : 'bg-danger',
                        )}
                      />
                      <span>{item.database || 'default'}</span>
                      <span>·</span>
                      <span>{item.executionTimeMs}ms</span>
                    </div>
                    <div className="mt-1 truncate font-mono text-xs text-fg-secondary">
                      {item.sql}
                    </div>
                  </div>
                  <button
                    type="button"
                    data-testid={`home-query-copy-${item.id}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleCopySql(item.id, item.sql);
                    }}
                    className="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs text-fg-muted hover:bg-surface hover:text-accent transition-colors"
                  >
                    {copiedRowId === item.id ? (
                      <>
                        <Check className="h-3.5 w-3.5 text-success" />
                        <span className="text-success">
                          {t('connWin.home.aiIntegration.copied')}
                        </span>
                      </>
                    ) : (
                      <>
                        <Copy className="h-3.5 w-3.5" />
                        <span>{t('connWin.home.aiIntegration.copy')}</span>
                      </>
                    )}
                  </button>
                </div>
              ))}
            </div>
          </section>
        )}
        {globalHistoryOpen && (
          <GlobalQueryHistoryDialog
            open={globalHistoryOpen}
            onClose={() => setGlobalHistoryOpen(false)}
            onSelectQuery={onSelectHistoryQuery}
            initialConnectionId={connectionContext?.connectionId}
          />
        )}
      </div>
    </div>
  );
}
