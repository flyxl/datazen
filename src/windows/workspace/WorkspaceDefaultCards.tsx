import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { PluginIcon } from './PluginIcon';
import type { WorkspacePageEntry } from './workspacePages';

export interface WorkspaceDefaultCardsProps {
  pages: WorkspacePageEntry[];
  onOpen: (page: WorkspacePageEntry) => void;
  /** Empty-state shortcut to the plugin management page. */
  onOpenPlugins?: () => void;
}

/** Default right-hand view of the workspace mode when no tab is open. */
export function WorkspaceDefaultCards({
  pages,
  onOpen,
  onOpenPlugins,
}: WorkspaceDefaultCardsProps) {
  const { t } = useI18n();

  if (pages.length === 0) {
    return (
      <div
        data-testid="workspace-empty"
        className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center"
      >
        <h2 className="text-sm font-semibold text-fg">{t('workspace.emptyTitle')}</h2>
        <p className="max-w-xs text-xs leading-relaxed text-fg-muted">{t('workspace.emptyHint')}</p>
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
    );
  }

  return (
    <div data-testid="workspace-default-cards" className="min-h-0 flex-1 overflow-y-auto">
      <div className="px-6 py-6">
        <h2 className="text-base font-semibold text-fg">{t('workspace.defaultTitle')}</h2>
        <p className="mt-1 mb-5 text-xs text-fg-muted">{t('workspace.defaultHint')}</p>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3.5">
          {pages.map((page) => (
            <button
              key={page.key}
              type="button"
              data-testid="workspace-default-card"
              onClick={() => onOpen(page)}
              className="flex flex-col gap-2.5 rounded-xl border border-edge bg-surface-alt p-4 text-left transition-all hover:-translate-y-px hover:border-accent hover:bg-surface-raised hover:shadow-lg"
            >
              <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-surface-raised">
                <PluginIcon pluginId={page.pluginId} icon={page.icon} className="h-5 w-5" />
              </span>
              <span className="min-w-0">
                <span className="block truncate text-sm font-semibold text-fg">{page.title}</span>
                {page.description ? (
                  <span className="mt-1 block text-xs leading-relaxed text-fg-muted">
                    {page.description}
                  </span>
                ) : null}
              </span>
              <span className="mt-auto border-t border-edge pt-2 text-[11px] text-fg-muted">
                v{page.version}
                {page.author ? ` · ${page.author}` : ''}
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
