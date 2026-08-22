import type { ReactNode } from 'react';
import { Code2, GitFork, Plus, TableProperties } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { getDbIcon, getDbLabel } from '../../lib/databaseTypes';
import type { ConnectionContext, Panel } from '../../stores/panelStore';
import { getPanelIcon, getPanelLabel } from './contentViewHelpers';

export interface ConnectionWorkspaceHomeProps {
  hasConnections: boolean;
  connectionContext: ConnectionContext | null;
  recentPanels: Panel[];
  showNewQuery: boolean;
  showNewTable: boolean;
  showErDiagram: boolean;
  showObjects: boolean;
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
}

function QuickAction({ icon, label, onClick, primary = false }: Readonly<QuickActionProps>) {
  return (
    <button
      type="button"
      onClick={onClick}
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
          <Button variant="ghost" className="mt-3" onClick={onNewConnection}>
            <Plus className="h-4 w-4" />
            {t('main.createFirst')}
          </Button>
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

  const dbIcon = getDbIcon(connectionContext.databaseType);
  const quickActions = [
    showNewQuery
      ? {
          key: 'new-query',
          label: t('connWin.newQuery'),
          icon: <Code2 className="h-4 w-4" />,
          onClick: onNewQuery,
          primary: true,
        }
      : null,
    showNewTable
      ? {
          key: 'new-table',
          label: t('connWin.newTable'),
          icon: <TableProperties className="h-4 w-4" />,
          onClick: onCreateTable,
        }
      : null,
    showErDiagram
      ? {
          key: 'er-diagram',
          label: t('erDiagram.title'),
          icon: <GitFork className="h-4 w-4" />,
          onClick: onOpenErDiagram,
        }
      : null,
    showObjects
      ? {
          key: 'objects',
          label: t('objects.title'),
          icon: <Code2 className="h-4 w-4" />,
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
          <div
            className={cn(
              'flex h-12 w-12 shrink-0 items-center justify-center rounded-xl text-sm font-semibold text-white',
              dbIcon.bg,
            )}
          >
            {dbIcon.label}
          </div>
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
