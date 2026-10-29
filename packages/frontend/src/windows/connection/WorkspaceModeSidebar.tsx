import {
  Database,
  Gauge,
  LayoutGrid,
  PanelLeftClose,
  PanelLeftOpen,
  Puzzle,
  Settings,
  Workflow,
  type LucideIcon,
} from 'lucide-react';
import { ThemedIcon } from '../../components/ThemedIcon';
import { useI18n } from '../../hooks/useI18n';
import type { UiIconId } from '../../lib/iconIds';
import type { WorkspaceMode } from './connectionPageUtils';

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
    </aside>
  );
}
