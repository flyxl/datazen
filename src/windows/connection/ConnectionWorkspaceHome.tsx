import { useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  Check,
  ChevronRight,
  Clock,
  Code2,
  Copy,
  Database,
  Download,
  GitFork,
  History,
  Info,
  Loader2,
  Plus,
  Sparkles,
  TableProperties,
  Terminal,
  Zap,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { ThemedIcon } from '../../components/ThemedIcon';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getDbLabel } from '../../lib/databaseTypes';
import { useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { usePanelStore, type ConnectionContext, type Panel } from '../../stores/panelStore';
import { queryCommands } from '../../commands/query';
import type { DatabaseType, QueryHistoryEntry } from '../../types';
import { getPanelIcon, getPanelLabel } from './contentViewHelpers';

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
  onOpenQueryHistory,
}: Readonly<ConnectionWorkspaceHomeProps>) {
  const { t } = useI18n();

  const savedConnections = useConnectionStore((s) => s.connections);
  const activeConnections = useActiveConnectionStore((s) => s.connections);
  const openQueryHistory = usePanelStore((s) => s.openQueryHistory);

  const [recentQueries, setRecentQueries] = useState<QueryHistoryEntry[]>([]);
  const [copiedSqlId, setCopiedSqlId] = useState<string | null>(null);
  const [copiedMcp, setCopiedMcp] = useState(false);

  // Load recent query history (global or connection-scoped)
  useEffect(() => {
    let unmounted = false;
    if (typeof queryCommands?.getQueryHistory !== 'function') {
      return;
    }
    queryCommands
      .getQueryHistory(5, connectionContext?.connectionId)
      .then((history) => {
        if (!unmounted && Array.isArray(history)) {
          setRecentQueries(history);
        }
      })
      .catch(() => {
        // Safe fallback in test/mock environment
      });
    return () => {
      unmounted = true;
    };
  }, [connectionContext?.connectionId]);

  // Derived metrics
  const connectedCount = useMemo(() => {
    return Object.values(activeConnections).filter((c) => c?.status === 'connected').length;
  }, [activeConnections]);

  const distinctDbTypes = useMemo(() => {
    return Array.from(new Set(savedConnections.map((c) => c.databaseType)));
  }, [savedConnections]);

  // Top connections for quick start: pinned first, then lastConnectedAt, then name. Strictly capped at 4.
  const quickConnections = useMemo(() => {
    return [...savedConnections]
      .sort((a, b) => {
        if (a.pinned && !b.pinned) return -1;
        if (!a.pinned && b.pinned) return 1;
        const aTime = a.lastConnectedAt ? new Date(a.lastConnectedAt).getTime() : 0;
        const bTime = b.lastConnectedAt ? new Date(b.lastConnectedAt).getTime() : 0;
        if (aTime !== bTime) return bTime - aTime;
        return a.name.localeCompare(b.name);
      })
      .slice(0, 4);
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

  const handleCopyMcpCommand = () => {
    void navigator.clipboard?.writeText('datazen --mcp');
    setCopiedMcp(true);
    setTimeout(() => setCopiedMcp(false), 2000);
  };

  const handleNewQueryClick = () => {
    if (connectionContext) {
      onNewQuery();
      return;
    }
    // If a connection is already connected, select it and open query
    const connectedEntry = Object.values(activeConnections).find((c) => c?.status === 'connected');
    if (connectedEntry && onSelectConnection) {
      onSelectConnection(connectedEntry.connectionId);
      onNewQuery();
      return;
    }
    // If we have saved connections, select the first quick connection
    if (quickConnections.length > 0 && onSelectConnection) {
      handleConnect(quickConnections[0].id);
      onNewQuery();
    }
  };

  /** Open the full query history drawer and query panel */
  const handleOpenHistory = () => {
    if (connectionContext && onOpenQueryHistory) {
      onOpenQueryHistory();
      return;
    }

    const targetConnId =
      connectionContext?.connectionId ||
      Object.values(activeConnections).find((c) => c?.status === 'connected')?.connectionId ||
      quickConnections[0]?.id;

    if (targetConnId) {
      usePanelStore.getState().setPendingQueryHistory(targetConnId);
      if (onSelectConnection) {
        onSelectConnection(targetConnId);
      }
      if (connectionContext?.connectionId === targetConnId && onOpenQueryHistory) {
        onOpenQueryHistory();
      }
    } else {
      void openQueryHistory();
    }
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
            <Button className="mt-0" onClick={onNewConnection} data-testid="new-connection-button">
              <ThemedIcon id="common.newConnection" className="h-4 w-4" fallback={Plus} />
              {t('main.createFirst')}
            </Button>
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
          <Loader2 className="h-5 w-5 animate-spin text-fg-muted" />
          {connectingName && <p className="text-sm text-fg-muted">{connectingName}</p>}
        </div>
      </div>
    );
  }

  // ── State 3: DBX-Style Dashboard when no connection session is active ──
  if (!connectionContext) {
    return (
      <div
        className="flex flex-1 flex-col overflow-y-auto px-6 py-6 md:px-8 md:py-8"
        data-testid="connection-workspace-home"
      >
        <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
          {/* Header Title & Subtitle */}
          <div>
            <h2 className="text-lg font-bold tracking-tight text-fg sm:text-xl">
              {t('connWin.home.selectConnectionTitle')}
            </h2>
            <p className="mt-1 text-sm text-fg-muted">{t('connWin.home.selectConnectionHint')}</p>
          </div>

          {/* Metric Cards */}
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            {/* Card 1: Total Connections */}
            <div className="flex items-center gap-4 rounded-xl border border-edge bg-surface-alt p-4 transition-colors hover:bg-surface-raised">
              <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-accent/10 text-accent">
                <Database className="h-6 w-6" />
              </div>
              <div>
                <div className="text-2xl font-bold text-fg">{savedConnections.length}</div>
                <div className="text-xs text-fg-muted">{t('connWin.home.metrics.connections')}</div>
              </div>
            </div>

            {/* Card 2: Connected Sessions */}
            <div className="flex items-center gap-4 rounded-xl border border-edge bg-surface-alt p-4 transition-colors hover:bg-surface-raised">
              <div
                className={cn(
                  'flex h-12 w-12 shrink-0 items-center justify-center rounded-xl',
                  connectedCount > 0
                    ? 'bg-success/10 text-success'
                    : 'bg-surface-raised text-fg-muted',
                )}
              >
                <Zap className="h-6 w-6" />
              </div>
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-2xl font-bold text-fg">{connectedCount}</span>
                  {connectedCount > 0 && (
                    <span className="inline-flex h-2 w-2 rounded-full bg-success animate-pulse" />
                  )}
                </div>
                <div className="text-xs text-fg-muted">{t('connWin.home.metrics.connected')}</div>
              </div>
            </div>

            {/* Card 3: Database Types */}
            <div className="flex items-center justify-between rounded-xl border border-edge bg-surface-alt p-4 transition-colors hover:bg-surface-raised">
              <div className="flex items-center gap-4">
                <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-warning/10 text-warning">
                  <Sparkles className="h-6 w-6" />
                </div>
                <div>
                  <div className="text-2xl font-bold text-fg">{distinctDbTypes.length}</div>
                  <div className="text-xs text-fg-muted">{t('connWin.home.metrics.dbTypes')}</div>
                </div>
              </div>
              <div className="flex items-center -space-x-1.5 overflow-hidden pl-2">
                {distinctDbTypes.slice(0, 4).map((dbType) => (
                  <div
                    key={dbType}
                    className="rounded-full ring-2 ring-surface"
                    title={getDbLabel(dbType)}
                  >
                    <DbTypeBadge databaseType={dbType} size={22} />
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Middle Section: Quick Start (Single Unified List Card) + Common Operations */}
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-3 items-start">
            {/* Quick Start (2 columns span, rendered as a single cohesive list panel) */}
            <div className="flex flex-col gap-2.5 lg:col-span-2">
              <div className="flex items-center justify-between px-0.5">
                <h3 className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
                  {t('connWin.home.quickStart')}
                </h3>
                {savedConnections.length > 4 && (
                  <span className="text-xs text-fg-muted">
                    {t('connWin.home.totalCount', { count: String(savedConnections.length) })}
                  </span>
                )}
              </div>

              <div className="overflow-hidden rounded-xl border border-edge bg-surface-alt divide-y divide-edge/60">
                {quickConnections.map((conn) => {
                  const isActive = activeConnections[conn.id]?.status === 'connected';
                  const isConnLoading = activeConnections[conn.id]?.status === 'connecting';
                  const hostPort = conn.host
                    ? `${conn.host}${conn.port ? `:${conn.port}` : ''}`
                    : conn.database || getDbLabel(conn.databaseType);

                  return (
                    <button
                      key={conn.id}
                      type="button"
                      onClick={() => handleConnect(conn.id)}
                      className="group flex w-full items-center justify-between px-4 py-3 text-left transition-colors hover:bg-surface-raised"
                    >
                      <div className="flex items-center gap-3.5 min-w-0">
                        <DbTypeBadge databaseType={conn.databaseType} size={32} />
                        <div className="min-w-0">
                          <div className="flex items-center gap-2">
                            <span className="truncate text-sm font-semibold text-fg group-hover:text-accent transition-colors">
                              {conn.name}
                            </span>
                            <span className="rounded bg-surface px-1.5 py-0.5 text-[10px] font-medium text-fg-muted border border-edge/60">
                              {getDbLabel(conn.databaseType)}
                            </span>
                          </div>
                          <div className="truncate font-mono text-xs text-fg-muted mt-0.5">
                            {hostPort}
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center gap-3 shrink-0 ml-3">
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

                        <ChevronRight className="h-4 w-4 text-fg-muted opacity-0 group-hover:opacity-100 group-hover:text-accent transition-all -translate-x-1 group-hover:translate-x-0" />
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Common Operations (1 column) */}
            <div className="flex flex-col gap-2.5">
              <div className="px-0.5">
                <h3 className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
                  {t('connWin.home.commonOps')}
                </h3>
              </div>

              <div className="flex flex-col gap-1.5 rounded-xl border border-edge bg-surface-alt p-3">
                <button
                  type="button"
                  data-testid="empty-new-connection-button"
                  onClick={onNewConnection}
                  className="flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm font-medium text-fg transition-colors hover:bg-surface-raised hover:text-accent"
                >
                  <div className="flex h-7 w-7 items-center justify-center rounded-md bg-accent/10 text-accent">
                    <Plus className="h-4 w-4" />
                  </div>
                  <span>{t('common.newConnection')}</span>
                </button>

                <button
                  type="button"
                  onClick={handleNewQueryClick}
                  className="flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm font-medium text-fg transition-colors hover:bg-surface-raised hover:text-accent"
                >
                  <div className="flex h-7 w-7 items-center justify-center rounded-md bg-accent/10 text-accent">
                    <Code2 className="h-4 w-4" />
                  </div>
                  <span>{t('common.newQuery')}</span>
                </button>

                <button
                  type="button"
                  data-testid="empty-history-button"
                  onClick={handleOpenHistory}
                  className="flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm font-medium text-fg transition-colors hover:bg-surface-raised hover:text-accent"
                >
                  <div className="flex h-7 w-7 items-center justify-center rounded-md bg-accent/10 text-accent">
                    <History className="h-4 w-4" />
                  </div>
                  <span>{t('connWin.home.recentQueries')}</span>
                </button>

                {onImportConnections && (
                  <button
                    type="button"
                    data-testid="empty-import-connections-button"
                    onClick={onImportConnections}
                    className="flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm font-medium text-fg transition-colors hover:bg-surface-raised hover:text-accent"
                  >
                    <div className="flex h-7 w-7 items-center justify-center rounded-md bg-accent/10 text-accent">
                      <Download className="h-4 w-4" />
                    </div>
                    <span>{t('common.importConnections')}</span>
                  </button>
                )}

                <div className="mt-1 flex items-start gap-2 border-t border-edge/60 pt-2 text-[11px] text-fg-muted">
                  <Info className="h-3.5 w-3.5 shrink-0 text-fg-secondary mt-0.5" />
                  <p className="leading-snug">{t('connWin.home.selectConnectionTip')}</p>
                </div>
              </div>
            </div>
          </div>

          {/* Bottom Section: Query History + AI Assistant */}
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
            {/* Query History */}
            <div className="flex flex-col gap-2.5">
              <div className="flex items-center justify-between px-0.5">
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

              <div className="min-h-[140px] rounded-xl border border-edge bg-surface-alt p-4">
                {recentQueries.length === 0 ? (
                  <div className="flex h-full min-h-[108px] flex-col items-center justify-center text-center text-fg-muted">
                    <Clock className="h-6 w-6 opacity-40 mb-2" />
                    <p className="text-xs">{t('connWin.home.noRecentQueries')}</p>
                  </div>
                ) : (
                  <div className="flex flex-col gap-2">
                    {recentQueries.slice(0, 3).map((item) => (
                      <div
                        key={item.id}
                        className="group flex flex-col gap-1 rounded-lg border border-edge/70 bg-surface p-2.5 transition-colors hover:border-accent/30"
                      >
                        <div className="flex items-center justify-between text-[11px] text-fg-muted">
                          <div className="flex items-center gap-1.5 font-medium">
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
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              void navigator.clipboard?.writeText(item.sql);
                              setCopiedSqlId(item.id);
                              setTimeout(() => setCopiedSqlId(null), 2000);
                            }}
                            className="flex items-center gap-1 text-fg-muted hover:text-accent text-[11px]"
                          >
                            {copiedSqlId === item.id ? (
                              <>
                                <Check className="h-3 w-3 text-success" />
                                <span className="text-success">
                                  {t('connWin.home.aiIntegration.copied')}
                                </span>
                              </>
                            ) : (
                              <>
                                <Copy className="h-3 w-3" />
                                <span>{t('connWin.home.aiIntegration.copy')}</span>
                              </>
                            )}
                          </button>
                        </div>
                        <div className="truncate font-mono text-xs text-fg-secondary">
                          {item.sql}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>

            {/* AI Assistant & MCP Integration */}
            <div className="flex flex-col gap-2.5">
              <div className="px-0.5">
                <h3 className="text-xs font-semibold uppercase tracking-wider text-fg-muted">
                  {t('connWin.home.aiIntegration.title')}
                </h3>
              </div>

              <div className="flex flex-1 flex-col justify-between rounded-xl border border-edge bg-surface-alt p-4">
                <div>
                  <div className="flex items-center gap-2 text-sm font-semibold text-fg">
                    <Sparkles className="h-4 w-4 text-accent" />
                    <span>DataZen MCP Server</span>
                  </div>
                  <p className="mt-1.5 text-xs leading-relaxed text-fg-muted">
                    {t('connWin.home.aiIntegration.desc')}
                  </p>
                </div>

                <div className="mt-4 flex items-center justify-between gap-3 rounded-lg border border-edge bg-surface px-3 py-2">
                  <div className="flex items-center gap-2 overflow-hidden">
                    <Terminal className="h-4 w-4 shrink-0 text-fg-muted" />
                    <code className="font-mono text-xs text-fg truncate">datazen --mcp</code>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={handleCopyMcpCommand}
                    className="h-7 shrink-0 gap-1 px-2 text-xs"
                  >
                    {copiedMcp ? (
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
                  </Button>
                </div>
              </div>
            </div>
          </div>
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
                  className="flex items-center justify-between rounded-xl border border-edge bg-surface-alt p-3 transition-colors hover:bg-surface-raised"
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
                    onClick={() => {
                      void navigator.clipboard?.writeText(item.sql);
                      setCopiedSqlId(item.id);
                      setTimeout(() => setCopiedSqlId(null), 2000);
                    }}
                    className="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs text-fg-muted hover:bg-surface hover:text-accent transition-colors"
                  >
                    {copiedSqlId === item.id ? (
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
      </div>
    </div>
  );
}
