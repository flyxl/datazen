import { useEffect, useMemo, useState } from 'react';
import { Download, PackageOpen, Trash2 } from 'lucide-react';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { extensionCommands } from '../../commands/extensions';
import { useExtensionStore } from '../../stores/extensionStore';
import { useWorkspaceTabsStore } from '../../stores/workspaceTabsStore';
import { EXTENSION_API_VERSION, type ExtensionSummary } from '../../types/extension';
import { openPluginPage } from '../workspace/workspacePages';
import { InstallExtensionDialog } from './InstallExtensionDialog';
import { PERMISSION_LABELS } from './permissionLabels';

type PluginFilter = 'all' | 'workspace' | 'theme';

function hasPages(p: ExtensionSummary): boolean {
  return p.pages.length > 0;
}

function hasThemes(p: ExtensionSummary): boolean {
  return p.themes.length > 0;
}

function matchesFilter(p: ExtensionSummary, filter: PluginFilter): boolean {
  if (filter === 'workspace') return hasPages(p);
  if (filter === 'theme') return hasThemes(p);
  return true;
}

const ICON_MIME: Record<string, string> = {
  '.png': 'image/png',
  '.webp': 'image/webp',
  '.svg': 'image/svg+xml',
};

function mimeForIcon(path: string): string {
  const dot = path.lastIndexOf('.');
  const ext = dot >= 0 ? path.slice(dot).toLowerCase() : '';
  return ICON_MIME[ext] ?? 'application/octet-stream';
}

/**
 * Renders a plugin's package-level icon as an image loaded through
 * `read_extension_file`. Falls back to the letter avatar when the plugin declares
 * no icon, the file cannot be read, or the plugin is disabled.
 */
function ExtensionCardIcon({ extension }: { extension: ExtensionSummary }) {
  const initials = extension.name.slice(0, 1).toUpperCase();
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    const iconPath = extension.icon;
    if (!iconPath || !extension.enabled) {
      setUrl(null);
      return;
    }
    let revoked = false;
    let objectUrl: string | null = null;
    let cancelled = false;

    void extensionCommands
      .readExtensionFile(extension.id, iconPath)
      .then((bytes) => {
        if (cancelled) return;
        const blob = new Blob([new Uint8Array(bytes)], { type: mimeForIcon(iconPath) });
        objectUrl = URL.createObjectURL(blob);
        if (!revoked) setUrl(objectUrl);
      })
      .catch(() => {
        if (!cancelled) setUrl(null);
      });

    return () => {
      cancelled = true;
      revoked = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [extension.id, extension.icon, extension.enabled]);

  return (
    <span
      data-testid="plugin-card-icon"
      className="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-md bg-surface-raised text-base font-semibold text-accent"
    >
      {url ? (
        <img
          src={url}
          alt=""
          draggable={false}
          className="h-full w-full object-contain"
          data-testid="plugin-card-icon-img"
        />
      ) : (
        initials
      )}
    </span>
  );
}

export interface ExtensionManagementPageProps {
  /**
   * Invoked after a workspace plugin's [Open] action so the host can switch
   * the workspace mode to the workspace view.
   */
  onOpenInWorkspace?: () => void;
}

export function ExtensionManagementPage({ onOpenInWorkspace }: ExtensionManagementPageProps) {
  const { t } = useI18n();
  const plugins = useExtensionStore((s) => s.extensions);
  const loaded = useExtensionStore((s) => s.loaded);
  const storeError = useExtensionStore((s) => s.error);
  const [search, setSearch] = useState('');
  // PRD §4.3: the content body defaults to the Workspace filter.
  const [filter, setFilter] = useState<PluginFilter>('workspace');
  const [installOpen, setInstallOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [confirmRemove, confirmRemoveDialog] = useConfirmDialog();

  useEffect(() => {
    if (!loaded) void useExtensionStore.getState().fetch();
  }, [loaded]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return plugins.filter(
      (p) =>
        matchesFilter(p, filter) &&
        (!q ||
          p.name.toLowerCase().includes(q) ||
          p.id.toLowerCase().includes(q) ||
          (p.description ?? '').toLowerCase().includes(q)),
    );
  }, [plugins, search, filter]);

  const counts = useMemo(
    () => ({
      all: plugins.length,
      workspace: plugins.filter(hasPages).length,
      theme: plugins.filter(hasThemes).length,
    }),
    [plugins],
  );

  // PRD §4.3: the "all" view mixes both kinds, grouped under small headers.
  const allGroups = useMemo(() => {
    if (filter !== 'all') return null;
    return [
      { key: 'workspace' as const, items: filtered.filter(hasPages) },
      { key: 'theme' as const, items: filtered.filter((p) => !hasPages(p) && hasThemes(p)) },
    ];
  }, [filtered, filter]);

  const handleToggle = async (plugin: ExtensionSummary) => {
    setActionError(null);
    try {
      await useExtensionStore.getState().setEnabled(plugin.id, !plugin.enabled);
      if (plugin.enabled) {
        // Disabling removes its pages → close the plugin's workspace tabs.
        useWorkspaceTabsStore.getState().closeByPlugin(plugin.id);
      }
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleRemove = (plugin: ExtensionSummary) => {
    void confirmRemove({
      title: t('plugins.page.uninstallTitle'),
      message: t('plugins.page.uninstallMessage', { name: plugin.name }),
      kind: 'warning',
    }).then(async (ok) => {
      if (!ok) return;
      setActionError(null);
      try {
        await useExtensionStore.getState().remove(plugin.id);
        useWorkspaceTabsStore.getState().closeByPlugin(plugin.id);
      } catch (e) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    });
  };

  const handleOpen = (plugin: ExtensionSummary) => {
    if (openPluginPage(plugin.id)) onOpenInWorkspace?.();
  };

  const renderCard = (plugin: ExtensionSummary) => {
    const apiMismatch = plugin.apiVersion !== EXTENSION_API_VERSION;
    const dimmed = apiMismatch || !plugin.enabled;
    return (
      <div
        key={plugin.id}
        data-testid="plugin-card"
        data-plugin-id={plugin.id}
        className={cn(
          'flex flex-col gap-2.5 rounded-lg border border-edge bg-surface-alt p-4 transition-colors hover:border-accent/50',
          dimmed && 'opacity-60',
        )}
      >
        <div className="flex items-center gap-2.5">
          <ExtensionCardIcon extension={plugin} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="truncate text-sm font-semibold text-fg">{plugin.name}</span>
              <span className="shrink-0 rounded bg-surface-raised px-1 py-px text-[10px] text-fg-muted">
                v{plugin.version}
              </span>
            </div>
            <div className="truncate text-[11px] text-fg-muted">
              {plugin.author ? `by ${plugin.author}` : plugin.id}
            </div>
          </div>
        </div>

        {plugin.description ? (
          <p className="line-clamp-2 text-xs leading-relaxed text-fg-secondary">
            {plugin.description}
          </p>
        ) : null}

        <div className="flex flex-wrap gap-1">
          {apiMismatch ? (
            <Badge
              tone="warning"
              title={t('plugins.page.apiMismatchHint', {
                plugin: plugin.apiVersion,
                host: EXTENSION_API_VERSION,
              })}
            >
              {t('plugins.page.apiMismatch')}
            </Badge>
          ) : null}
          {!hasPages(plugin) && hasThemes(plugin) ? (
            <Badge tone="accent">{t('plugins.page.themeBadge')}</Badge>
          ) : null}
          {plugin.permissions.map((perm) => (
            <Badge key={perm} title={PERMISSION_LABELS[perm] ?? perm}>
              {perm}
            </Badge>
          ))}
        </div>

        {!hasPages(plugin) && hasThemes(plugin) ? (
          <p className="text-[11px] text-fg-muted">{t('plugins.page.themeHint')}</p>
        ) : null}

        <div className="mt-auto flex items-center gap-2 border-t border-edge pt-2.5">
          <button
            type="button"
            role="switch"
            aria-checked={plugin.enabled}
            aria-label={t('plugins.page.toggle')}
            data-testid="plugin-toggle"
            disabled={apiMismatch}
            title={apiMismatch ? t('plugins.page.apiMismatch') : t('plugins.page.toggle')}
            onClick={() => void handleToggle(plugin)}
            className={cn(
              'relative h-[18px] w-8 shrink-0 rounded-full transition-colors',
              plugin.enabled ? 'bg-green-600' : 'bg-edge',
              apiMismatch && 'cursor-not-allowed opacity-50',
            )}
          >
            <span
              className={cn(
                'absolute top-0.5 h-3.5 w-3.5 rounded-full bg-white transition-all',
                plugin.enabled ? 'left-[16px]' : 'left-0.5',
              )}
            />
          </button>
          {hasPages(plugin) && !apiMismatch ? (
            <Button
              size="sm"
              variant="secondary"
              data-testid="plugin-open"
              disabled={!plugin.enabled}
              onClick={() => handleOpen(plugin)}
            >
              {t('plugins.page.open')}
            </Button>
          ) : null}
          <span className="flex-1" />
          <Button
            size="sm"
            variant="ghost"
            data-testid="plugin-uninstall"
            title={t('plugins.page.uninstall')}
            onClick={() => handleRemove(plugin)}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    );
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto" data-testid="plugin-management-page">
      <div className="mx-auto max-w-5xl px-6 py-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 className="text-base font-semibold text-fg">
              {t('plugins.page.title')}{' '}
              <span className="text-xs font-normal text-fg-muted">
                {t('plugins.page.count', { count: plugins.length })}
              </span>
            </h2>
            <p className="mt-1 text-xs text-fg-muted">{t('plugins.page.subtitle')}</p>
          </div>
          <Button
            data-testid="plugin-install-button"
            onClick={() => setInstallOpen(true)}
            className="shrink-0"
          >
            <Download className="h-4 w-4" />
            {t('plugins.page.install')}
          </Button>
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-2">
          <Input
            data-testid="plugin-search-input"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('plugins.page.searchPlaceholder')}
            className="h-8 max-w-xs text-xs"
          />
          {(['all', 'workspace', 'theme'] as const).map((f) => (
            <button
              key={f}
              type="button"
              data-testid={`plugin-filter-${f}`}
              onClick={() => setFilter(f)}
              className={cn(
                'rounded-full border px-3 py-1 text-xs transition-colors',
                filter === f
                  ? 'border-accent/60 bg-accent/15 text-accent'
                  : 'border-edge text-fg-secondary hover:border-accent/40 hover:text-fg',
              )}
            >
              {t(
                f === 'all'
                  ? 'plugins.page.filterAll'
                  : f === 'workspace'
                    ? 'plugins.page.filterWorkspace'
                    : 'plugins.page.filterTheme',
                { count: counts[f] },
              )}
            </button>
          ))}
        </div>

        {(actionError || storeError) && (
          <div className="mt-3 text-sm text-red-400" data-testid="plugin-page-error">
            {actionError ?? storeError}
          </div>
        )}

        {filtered.length === 0 ? (
          <div
            data-testid="plugin-page-empty"
            className="mt-10 flex flex-col items-center gap-2 rounded-lg border border-dashed border-edge px-4 py-12 text-center"
          >
            <PackageOpen className="h-6 w-6 text-fg-muted" />
            <p className="text-sm text-fg-secondary">{t('plugins.page.emptyTitle')}</p>
            <p className="max-w-sm text-xs text-fg-muted">{t('plugins.page.emptyHint')}</p>
          </div>
        ) : allGroups ? (
          <div className="mt-4 flex flex-col gap-6">
            {allGroups.map(({ key, items }) =>
              items.length === 0 ? null : (
                <section key={key} data-testid={`plugin-group-${key}`}>
                  <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
                    {t(
                      key === 'workspace'
                        ? 'plugins.page.groupWorkspace'
                        : 'plugins.page.groupTheme',
                    )}
                  </h3>
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
                    {items.map(renderCard)}
                  </div>
                </section>
              ),
            )}
          </div>
        ) : (
          <div className="mt-4 grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
            {filtered.map(renderCard)}
          </div>
        )}
      </div>

      {confirmRemoveDialog}
      <InstallExtensionDialog open={installOpen} onClose={() => setInstallOpen(false)} />
    </div>
  );
}
