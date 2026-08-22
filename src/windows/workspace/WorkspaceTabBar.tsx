import { X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { useWorkspaceTabsStore } from '../../stores/workspaceTabsStore';
import { PluginIcon } from './PluginIcon';

/**
 * Workspace tab strip. Visually mirrors the connection `PanelTabBar` but is
 * backed exclusively by `workspaceTabsStore` (independent of connection tabs).
 * Renders nothing while no plugin page tab is open.
 */
export function WorkspaceTabBar() {
  const { t } = useI18n();
  const tabs = useWorkspaceTabsStore((s) => s.tabs);
  const activeKey = useWorkspaceTabsStore((s) => s.activeKey);
  const activate = useWorkspaceTabsStore((s) => s.activate);
  const close = useWorkspaceTabsStore((s) => s.close);

  if (tabs.length === 0) return null;

  return (
    <div
      data-testid="workspace-tabbar"
      className="flex shrink-0 items-center border-b border-edge bg-surface-alt"
    >
      <div
        className="scrollbar-hide flex min-w-0 flex-1 overflow-x-auto"
        onWheel={(e) => {
          if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
          e.currentTarget.scrollLeft += e.deltaY;
        }}
      >
        {tabs.map((tab) => {
          const isActive = tab.key === activeKey;
          return (
            <div
              key={tab.key}
              data-testid="workspace-tab"
              className={cn(
                'group relative flex items-center gap-1.5 border-r border-edge px-3 py-2 text-xs transition-colors',
                isActive
                  ? 'bg-surface text-fg'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
              title={`${tab.title} · v${tab.version}`}
            >
              <button
                type="button"
                className="flex items-center gap-1.5"
                onClick={() => activate(tab.key)}
              >
                <PluginIcon pluginId={tab.pluginId} icon={tab.icon} className="h-3.5 w-3.5" />
                <span className="max-w-[160px] truncate">{tab.title}</span>
              </button>
              <button
                type="button"
                data-testid="workspace-tab-close"
                aria-label={`${t('common.close')} ${tab.title}`}
                className="rounded p-0.5 text-fg-muted opacity-0 hover:bg-surface-raised hover:text-fg group-hover:opacity-100"
                onClick={() => close(tab.key)}
              >
                <X className="h-3 w-3" />
              </button>
              <span
                className={cn(
                  'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                  isActive ? 'opacity-100' : 'opacity-0',
                )}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
