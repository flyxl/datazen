import { useEffect, useRef } from 'react';
import {
  Database,
  Download,
  Gauge,
  LayoutGrid,
  PanelLeftClose,
  PanelLeftOpen,
  Puzzle,
  RotateCw,
  Settings,
  Workflow,
  type LucideIcon,
} from 'lucide-react';
import { ThemedIcon } from '../../components/ThemedIcon';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import type { UiIconId } from '../../lib/iconIds';
import type { WorkspaceMode } from './connectionPageUtils';
import { useUpdateReminderStore } from '../../stores/updateReminderStore';

interface WorkspaceShortcutButtonProps {
  icon: LucideIcon;
  iconId: UiIconId;
  label: string;
  testId: string;
  onClick: () => void;
  active?: boolean;
  expanded?: boolean;
}

function WorkspaceModeButton({
  icon: Icon,
  iconId,
  label,
  testId,
  onClick,
  active = false,
  expanded = false,
}: Readonly<WorkspaceShortcutButtonProps>) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      title={label}
      className={`flex h-10 w-full items-center text-xs transition-colors ${
        expanded ? 'justify-start gap-2.5 px-3' : 'justify-center'
      } ${
        active
          ? 'bg-accent/20 text-accent'
          : 'text-fg-secondary hover:bg-surface-raised hover:text-fg'
      }`}
    >
      <ThemedIcon id={iconId} className="h-4 w-4 shrink-0" fallback={Icon} />
      {expanded && <span className="truncate">{label}</span>}
    </button>
  );
}

export interface WorkspaceModeSidebarProps {
  workspaceMode: WorkspaceMode;
  sidebarExpanded: boolean;
  onSetWorkspaceMode: (mode: WorkspaceMode) => void;
  onOpenWorkflow: () => void;
  onOpenDashboard: () => void | Promise<void>;
  onOpenSettings: () => void;
  onToggleSidebarMode: () => void;
}

export function WorkspaceModeSidebar({
  workspaceMode,
  sidebarExpanded,
  onSetWorkspaceMode,
  onOpenWorkflow,
  onOpenDashboard,
  onOpenSettings,
  onToggleSidebarMode,
}: Readonly<WorkspaceModeSidebarProps>) {
  const { t } = useI18n();
  const availableVersion = useUpdateReminderStore((state) => state.availableVersion);
  const pendingRestartVersion = useUpdateReminderStore((state) => state.pendingRestartVersion);
  const isInstalling = useUpdateReminderStore((state) => state.isInstalling);
  const progress = useUpdateReminderStore((state) => state.progress);
  const error = useUpdateReminderStore((state) => state.error);
  const installUpdate = useUpdateReminderStore((state) => state.installUpdate);
  const restartToApplyUpdate = useUpdateReminderStore((state) => state.restartToApplyUpdate);
  const promptedRestartFor = useRef<string | null>(null);
  const [confirmRestart, confirmRestartDialog] = useConfirmDialog();

  useEffect(() => {
    if (!pendingRestartVersion || promptedRestartFor.current === pendingRestartVersion) return;
    promptedRestartFor.current = pendingRestartVersion;
    const message = t('settings.updater.restartPrompt').replace('{version}', pendingRestartVersion);
    void (async () => {
      const restartNow = await confirmRestart({
        title: t('settings.updater.title'),
        message,
        confirmLabel: t('settings.updater.restartNow'),
        cancelLabel: t('settings.updater.restartLater'),
        kind: 'info',
      });
      if (restartNow) {
        await restartToApplyUpdate();
      }
    })();
  }, [confirmRestart, pendingRestartVersion, restartToApplyUpdate, t]);

  const progressLabel =
    progress?.phase === 'downloading'
      ? progress.total
        ? t('settings.updater.downloading').replace(
            '{pct}',
            String(Math.min(100, Math.round((progress.downloaded / progress.total) * 100))),
          )
        : t('settings.updater.downloadingIndeterminate')
      : progress?.phase === 'installing'
        ? t('settings.updater.installing')
        : t('settings.updater.sidebarInstalling');
  const updateVersion = availableVersion ?? pendingRestartVersion;
  const updateLabel = isInstalling
    ? progressLabel
    : pendingRestartVersion
      ? t('settings.updater.sidebarRestartReady').replace('{version}', pendingRestartVersion)
      : t('settings.updater.sidebarAvailable').replace('{version}', updateVersion ?? '');

  return (
    <aside
      className={`flex h-full shrink-0 flex-col self-stretch border-r border-edge bg-surface-alt ${
        sidebarExpanded ? 'w-28' : 'w-10'
      }`}
    >
      <div className="flex flex-col">
        <WorkspaceModeButton
          icon={Database}
          iconId="nav.databases"
          label={t('nav.databases')}
          testId="workspace-nav-databases"
          active={workspaceMode === 'connections'}
          expanded={sidebarExpanded}
          onClick={() => onSetWorkspaceMode('connections')}
        />
        <WorkspaceModeButton
          icon={Workflow}
          iconId="action.workflow"
          label={t('nav.workflow')}
          testId="workspace-nav-workflow"
          active={workspaceMode === 'workflow'}
          expanded={sidebarExpanded}
          onClick={onOpenWorkflow}
        />
        <WorkspaceModeButton
          icon={Gauge}
          iconId="action.dashboard"
          label={t('nav.dashboard')}
          testId="workspace-nav-dashboard"
          active={workspaceMode === 'dashboard'}
          expanded={sidebarExpanded}
          onClick={() => void onOpenDashboard()}
        />
        <WorkspaceModeButton
          icon={LayoutGrid}
          iconId="nav.workspacePages"
          label={t('nav.workspacePages')}
          testId="workspace-nav-workspace-pages"
          active={workspaceMode === 'workspace'}
          expanded={sidebarExpanded}
          onClick={() => onSetWorkspaceMode('workspace')}
        />
        <WorkspaceModeButton
          icon={Puzzle}
          iconId="nav.extensions"
          label={t('nav.extensions')}
          testId="workspace-nav-extensions"
          active={workspaceMode === 'extension'}
          expanded={sidebarExpanded}
          onClick={() => onSetWorkspaceMode('extension')}
        />
      </div>
      <div className="mt-auto flex flex-col">
        {(availableVersion || pendingRestartVersion) && (
          <button
            type="button"
            data-testid="workspace-nav-update"
            aria-label={updateLabel}
            title={error ?? updateLabel}
            disabled={isInstalling}
            onClick={() => {
              if (pendingRestartVersion) {
                void restartToApplyUpdate();
                return;
              }
              void installUpdate();
            }}
            className={`flex h-10 w-full items-center text-xs transition-colors disabled:cursor-wait ${
              sidebarExpanded ? 'justify-start gap-2.5 px-3' : 'justify-center'
            } text-accent hover:bg-accent/10`}
          >
            <span className="relative inline-flex h-4 w-4 items-center justify-center">
              {isInstalling ? (
                (() => {
                  const size = 16;
                  const stroke = 2;
                  const r = (size - stroke) / 2;
                  const c = 2 * Math.PI * r;
                  const pct =
                    progress?.phase === 'downloading' && progress.total
                      ? Math.min(100, Math.max(0, (progress.downloaded / progress.total) * 100))
                      : null;
                  return (
                    <svg
                      className={`h-4 w-4 ${pct == null ? 'animate-spin' : ''}`}
                      viewBox={`0 0 ${size} ${size}`}
                      aria-hidden="true"
                    >
                      <circle
                        cx={size / 2}
                        cy={size / 2}
                        r={r}
                        fill="none"
                        stroke="currentColor"
                        strokeWidth={stroke}
                        className="opacity-25"
                      />
                      <circle
                        cx={size / 2}
                        cy={size / 2}
                        r={r}
                        fill="none"
                        stroke="currentColor"
                        strokeWidth={stroke}
                        strokeLinecap="round"
                        strokeDasharray={c}
                        strokeDashoffset={pct == null ? c * 0.75 : c * (1 - pct / 100)}
                        transform={`rotate(-90 ${size / 2} ${size / 2})`}
                      />
                    </svg>
                  );
                })()
              ) : pendingRestartVersion ? (
                <RotateCw className="h-4 w-4 shrink-0" />
              ) : (
                <Download className="h-4 w-4 shrink-0" />
              )}
              {!isInstalling && (
                <span
                  aria-hidden="true"
                  className="absolute -right-1 -top-1 h-2 w-2 rounded-full bg-danger ring-1 ring-surface-alt"
                />
              )}
            </span>
            {sidebarExpanded && <span className="truncate">{updateLabel}</span>}
          </button>
        )}
        <WorkspaceModeButton
          icon={Settings}
          iconId="nav.settings"
          label={t('nav.settings')}
          testId="workspace-nav-settings"
          expanded={sidebarExpanded}
          onClick={onOpenSettings}
        />
        <button
          type="button"
          data-testid="workspace-sidebar-toggle"
          title={sidebarExpanded ? t('connWin.collapseSidebar') : t('connWin.expandSidebar')}
          onClick={onToggleSidebarMode}
          className={`flex h-10 w-full items-center text-xs text-fg-secondary transition-colors hover:bg-surface-raised hover:text-fg ${
            sidebarExpanded ? 'justify-start gap-2.5 px-3' : 'justify-center'
          }`}
        >
          {sidebarExpanded ? (
            <PanelLeftClose className="h-4 w-4 shrink-0" />
          ) : (
            <PanelLeftOpen className="h-4 w-4 shrink-0" />
          )}
          {sidebarExpanded && <span className="truncate">{t('connWin.collapseSidebar')}</span>}
        </button>
      </div>
      {confirmRestartDialog}
    </aside>
  );
}
