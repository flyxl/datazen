import type { ReactNode } from 'react';
import { Code2, GitFork, Loader2, Plus, TableProperties } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { ThemedIcon } from '../../components/ThemedIcon';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getDbLabel } from '../../lib/databaseTypes';
import type { ConnectionContext, Panel } from '../../stores/panelStore';
import type { DatabaseType } from '../../types';
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
  onNewQuery: () => void;
  onCreateTable: () => void;
  onOpenErDiagram: () => void;
  onOpenObjects: () => void;
  onOpenPanel: (panelId: string) => void;
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
        'flex min-h-[88px] flex-col items-start justify-between rounded-xl border p-4 text-left transition-colors',
        primary
          ? 'border-accent/30 bg-accent/10 hover:border-accent/50 hover:bg-accent/15'
          : 'border-edge bg-surface-alt hover:border-edge hover:bg-surface-raised',
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
  onNewQuery,
  onCreateTable,
  onOpenErDiagram,
  onOpenObjects,
  onOpenPanel,
}: Readonly<ConnectionWorkspaceHomeProps>) {
  const { t } = useI18n();

  if (!hasConnections) {
    return (
      <div
        className="flex flex-1 items-center justify-center px-6"
        data-testid="connection-workspace-home"
      >
        <div className="max-w-sm text-center">
          <p className="text-sm text-fg-muted">{t('main.noConnections')}</p>
          <Button
            variant="ghost"
            className="mt-3"
            onClick={onNewConnection}
            data-testid="new-connection-button"
          >
            <ThemedIcon id="common.newConnection" className="h-4 w-4" fallback={Plus} />
            {t('main.createFirst')}
          </Button>
        </div>
      </div>
    );
  }

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

  if (!connectionContext) {
    return (
      <div
        className="flex flex-1 items-center justify-center px-6"
        data-testid="connection-workspace-home"
      >
        <p className="max-w-md text-center text-sm text-fg-muted">
          {t('connWin.home.selectConnection')}
        </p>
      </div>
    );
  }

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
      className="flex flex-1 flex-col overflow-auto px-6 py-8"
      data-testid="connection-workspace-home"
    >
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-8">
        <div className="flex items-start gap-4">
          <DbTypeBadge databaseType={connectionContext.databaseType} size={48} />
          <div className="min-w-0">
            <h2 className="truncate text-lg font-semibold text-fg">
              {connectionContext.connectionName}
            </h2>
            <p className="mt-1 text-sm text-fg-muted">
              {getDbLabel(connectionContext.databaseType)} · {t('connWin.home.subtitle')}
            </p>
          </div>
        </div>

        {quickActions.length > 0 && (
          <section>
            <h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-fg-muted">
              {t('connWin.home.quickActions')}
            </h3>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
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
                    'flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-surface-raised',
                    index > 0 && 'border-t border-edge',
                  )}
                >
                  {getPanelIcon(panel)}
                  <span className="min-w-0 flex-1 truncate text-sm text-fg">
                    {getPanelLabel(panel, t)}
                  </span>
                </button>
              ))}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
