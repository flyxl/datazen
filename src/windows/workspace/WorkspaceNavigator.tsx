import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { useWorkspaceTabsStore } from '../../stores/workspaceTabsStore';
import { openPluginPage, useWorkspacePages } from './workspacePages';
import { PluginIcon } from './PluginIcon';

export interface WorkspaceNavigatorProps {
  /** Shown in the empty state so users can jump to the plugin management page. */
  onOpenPlugins?: () => void;
}

/** Left rail of the workspace mode: every page contributed by enabled plugins. */
export function WorkspaceNavigator({ onOpenPlugins }: WorkspaceNavigatorProps) {
  const { t } = useI18n();
  const pages = useWorkspacePages();
  const activeKey = useWorkspaceTabsStore((s) => s.activeKey);

  return (
    <aside
      data-testid="workspace-navigator"
      className="flex h-full w-[180px] shrink-0 flex-col border-r border-edge bg-surface-alt"
    >
      <div className="px-3 pt-3 pb-1 text-[11px] font-semibold tracking-wide text-fg-muted uppercase">
        {t('nav.workspacePages')}
      </div>
      {pages.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 px-3 pb-6 text-center">
          <p className="text-xs leading-relaxed text-fg-muted">{t('workspace.emptyHint')}</p>
          {onOpenPlugins ? (
            <Button
              size="sm"
              variant="secondary"
              data-testid="workspace-open-plugins"
              onClick={onOpenPlugins}
            >
              {t('workspace.openPlugins')}
            </Button>
          ) : null}
        </div>
      ) : (
        <nav className="scrollbar-hide min-h-0 flex-1 overflow-y-auto px-2 pb-2">
          {pages.map((page) => {
            const active = page.key === activeKey;
            return (
              <button
                key={page.key}
                type="button"
                data-testid="workspace-nav-item"
                data-page-key={page.key}
                title={page.description ?? page.title}
                onClick={() => openPluginPage(page.pluginId, page.pageId)}
                className={cn(
                  'mb-0.5 flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left transition-colors',
                  active
                    ? 'bg-accent/20 text-accent'
                    : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                )}
              >
                <PluginIcon pluginId={page.pluginId} icon={page.icon} className="mt-0.5 h-4 w-4" />
                <span className="min-w-0">
                  <span className="block truncate text-xs font-medium">{page.title}</span>
                  {page.description ? (
                    <span className="block truncate text-[10px] text-fg-muted">
                      {page.description}
                    </span>
                  ) : null}
                </span>
              </button>
            );
          })}
        </nav>
      )}
    </aside>
  );
}
