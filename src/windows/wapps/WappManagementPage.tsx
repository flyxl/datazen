import { useEffect, useMemo, useState } from 'react';
import { Download, PackageOpen, Trash2 } from 'lucide-react';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { wappCommands } from '../../commands/wapps';
import { useWappStore } from '../../stores/wappStore';
import { useWorkspaceTabsStore } from '../../stores/workspaceTabsStore';
import { WAPP_API_VERSION, type WappSummary } from '../../types/wapp';
import { openWappPage } from '../workspace/workspacePages';
import { InstallWappDialog } from './InstallWappDialog';
import { PERMISSION_LABELS } from './permissionLabels';

type PluginFilter = 'all' | 'workspace' | 'theme';

function hasPages(p: WappSummary): boolean {
  return p.pages.length > 0;
}

function hasThemes(p: WappSummary): boolean {
  return p.themes.length > 0;
}

function matchesFilter(p: WappSummary, filter: PluginFilter): boolean {
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
 * Renders a wapp's package-level icon as an image loaded through
 * `read_wapp_file`. Falls back to the letter avatar when the wapp declares
 * no icon, the file cannot be read, or the wapp is disabled.
 */
function WappCardIcon({ wapp }: { wapp: WappSummary }) {
  const initials = wapp.name.slice(0, 1).toUpperCase();
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    const iconPath = wapp.icon;
    if (!iconPath || !wapp.enabled) {
      setUrl(null);
      return;
    }
    let revoked = false;
    let objectUrl: string | null = null;
    let cancelled = false;

    void wappCommands
      .readWappFile(wapp.id, iconPath)
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
  }, [wapp.id, wapp.icon, wapp.enabled]);

  return (
    <span
      data-testid="extension-card-icon"
      className="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-md bg-surface-raised text-base font-semibold text-accent"
    >
      {url ? (
        <img
          src={url}
          alt=""
          draggable={false}
          className="h-full w-full object-contain"
          data-testid="extension-card-icon-img"
        />
      ) : (
        initials
      )}
    </span>
  );
}

export const ExtensionCardIcon = WappCardIcon;

export interface WappManagementPageProps {
  /**
   * Invoked after a workspace wapp's [Open] action so the host can switch
   * the workspace mode to the workspace view.
   */
  onOpenInWorkspace?: () => void;
}

export type ExtensionManagementPageProps = WappManagementPageProps;

export function WappManagementPage({ onOpenInWorkspace }: WappManagementPageProps) {
  const { t } = useI18n();
  const wapps = useWappStore((s) => s.wapps);
  const loaded = useWappStore((s) => s.loaded);
  const storeError = useWappStore((s) => s.error);
  const [search, setSearch] = useState('');
  // PRD §4.3: the content body defaults to the Workspace filter.
  const [filter, setFilter] = useState<PluginFilter>('workspace');
  const [installOpen, setInstallOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [confirmRemove, confirmRemoveDialog] = useConfirmDialog();

  useEffect(() => {
    if (!loaded) void useWappStore.getState().fetch();
  }, [loaded]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return wapps.filter(
      (p) =>
        matchesFilter(p, filter) &&
        (!q ||
          p.name.toLowerCase().includes(q) ||
          p.id.toLowerCase().includes(q) ||
          (p.description ?? '').toLowerCase().includes(q)),
    );
  }, [wapps, search, filter]);

  const counts = useMemo(
    () => ({
      all: wapps.length,
      workspace: wapps.filter(hasPages).length,
      theme: wapps.filter(hasThemes).length,
    }),
    [wapps],
  );

  // PRD §4.3: the "all" view mixes both kinds, grouped under small headers.
  const allGroups = useMemo(() => {
    if (filter !== 'all') return null;
    return [
      { key: 'workspace' as const, items: filtered.filter(hasPages) },
      { key: 'theme' as const, items: filtered.filter((p) => !hasPages(p) && hasThemes(p)) },
    ];
  }, [filtered, filter]);

  const handleToggle = async (wapp: WappSummary) => {
    setActionError(null);
    try {
      await useWappStore.getState().setEnabled(wapp.id, !wapp.enabled);
      if (wapp.enabled) {
        // Disabling removes its pages → close the wapp's workspace tabs.
        useWorkspaceTabsStore.getState().closeByWapp(wapp.id);
      }
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleRemove = (wapp: WappSummary) => {
    void confirmRemove({
      title: t('extensions.page.uninstallTitle'),
      message: t('extensions.page.uninstallMessage', { name: wapp.name }),
      kind: 'warning',
    }).then(async (ok) => {
      if (!ok) return;
      setActionError(null);
      try {
        await useWappStore.getState().remove(wapp.id);
        useWorkspaceTabsStore.getState().closeByWapp(wapp.id);
      } catch (e) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    });
  };

  const handleOpen = (wapp: WappSummary) => {
    if (openWappPage(wapp.id)) onOpenInWorkspace?.();
  };

  const renderCard = (wapp: WappSummary) => {
    const apiMismatch = wapp.apiVersion !== WAPP_API_VERSION;
    const dimmed = apiMismatch || !wapp.enabled;
    return (
      <div
        key={wapp.id}
        data-testid="extension-card"
        data-wapp-id={wapp.id}
        className={cn(
          'flex flex-col gap-2.5 rounded-lg border border-edge bg-surface-alt p-4 transition-colors hover:border-accent/50',
          dimmed && 'opacity-60',
        )}
      >
        <div className="flex items-center gap-2.5">
          <WappCardIcon wapp={wapp} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="truncate text-sm font-semibold text-fg">{wapp.name}</span>
              <span className="shrink-0 rounded bg-surface-raised px-1 py-px text-[10px] text-fg-muted">
                v{wapp.version}
              </span>
            </div>
            <div className="truncate text-[11px] text-fg-muted">
              {wapp.author ? `by ${wapp.author}` : wapp.id}
            </div>
          </div>
        </div>

        {wapp.description ? (
          <p className="line-clamp-2 text-xs leading-relaxed text-fg-secondary">
            {wapp.description}
          </p>
        ) : null}

        <div className="flex flex-wrap gap-1">
          {apiMismatch ? (
            <Badge
              tone="warning"
              title={t('extensions.page.apiMismatchHint', {
                wapp: wapp.apiVersion,
                host: WAPP_API_VERSION,
              })}
            >
              {t('extensions.page.apiMismatch')}
            </Badge>
          ) : null}
          {!hasPages(wapp) && hasThemes(wapp) ? (
            <Badge tone="accent">{t('extensions.page.themeBadge')}</Badge>
          ) : null}
          {wapp.permissions.map((perm) => (
            <Badge key={perm} title={PERMISSION_LABELS[perm] ?? perm}>
              {perm}
            </Badge>
          ))}
        </div>

        {!hasPages(wapp) && hasThemes(wapp) ? (
          <p className="text-[11px] text-fg-muted">{t('extensions.page.themeHint')}</p>
        ) : null}

        <div className="mt-auto flex items-center gap-2 border-t border-edge pt-2.5">
          <button
            type="button"
            role="switch"
            aria-checked={wapp.enabled}
            aria-label={t('extensions.page.toggle')}
            data-testid="extension-toggle"
            disabled={apiMismatch}
            title={apiMismatch ? t('extensions.page.apiMismatch') : t('extensions.page.toggle')}
            onClick={() => void handleToggle(wapp)}
            className={cn(
              'relative h-[18px] w-8 shrink-0 rounded-full transition-colors',
              wapp.enabled ? 'bg-green-600' : 'bg-edge',
              apiMismatch && 'cursor-not-allowed opacity-50',
            )}
          >
            <span
              className={cn(
                'absolute top-0.5 h-3.5 w-3.5 rounded-full bg-white transition-all',
                wapp.enabled ? 'left-[16px]' : 'left-0.5',
              )}
            />
          </button>
          {hasPages(wapp) && !apiMismatch ? (
            <Button
              size="sm"
              variant="secondary"
              data-testid="extension-open"
              disabled={!wapp.enabled}
              onClick={() => handleOpen(wapp)}
            >
              {t('extensions.page.open')}
            </Button>
          ) : null}
          <span className="flex-1" />
          <Button
            size="sm"
            variant="ghost"
            data-testid="extension-uninstall"
            title={t('extensions.page.uninstall')}
            onClick={() => handleRemove(wapp)}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    );
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto" data-testid="extension-management-page">
      <div className="mx-auto max-w-5xl px-6 py-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 className="text-base font-semibold text-fg">
              {t('extensions.page.title')}{' '}
              <span className="text-xs font-normal text-fg-muted">
                {t('extensions.page.count', { count: wapps.length })}
              </span>
            </h2>
            <p className="mt-1 text-xs text-fg-muted">{t('extensions.page.subtitle')}</p>
          </div>
          <Button
            data-testid="extension-install-button"
            onClick={() => setInstallOpen(true)}
            className="shrink-0"
          >
            <Download className="h-4 w-4" />
            {t('extensions.page.install')}
          </Button>
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-2">
          <Input
            data-testid="extension-search-input"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('extensions.page.searchPlaceholder')}
            className="h-8 max-w-xs text-xs"
          />
          {(['all', 'workspace', 'theme'] as const).map((f) => (
            <button
              key={f}
              type="button"
              data-testid={`extension-filter-${f}`}
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
                  ? 'extensions.page.filterAll'
                  : f === 'workspace'
                    ? 'extensions.page.filterWorkspace'
                    : 'extensions.page.filterTheme',
                { count: counts[f] },
              )}
            </button>
          ))}
        </div>

        {(actionError || storeError) && (
          <div className="select-text mt-3 text-sm text-red-400" data-testid="extension-page-error">
            {actionError ?? storeError}
          </div>
        )}

        {filtered.length === 0 ? (
          <div
            data-testid="extension-page-empty"
            className="mt-10 flex flex-col items-center gap-2 rounded-lg border border-dashed border-edge px-4 py-12 text-center"
          >
            <PackageOpen className="h-6 w-6 text-fg-muted" />
            <p className="text-sm text-fg-secondary">{t('extensions.page.emptyTitle')}</p>
            <p className="max-w-sm text-xs text-fg-muted">{t('extensions.page.emptyHint')}</p>
          </div>
        ) : allGroups ? (
          <div className="mt-4 flex flex-col gap-6">
            {allGroups.map(({ key, items }) =>
              items.length === 0 ? null : (
                <section key={key} data-testid={`extension-group-${key}`}>
                  <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
                    {t(
                      key === 'workspace'
                        ? 'extensions.page.groupWorkspace'
                        : 'extensions.page.groupTheme',
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
      <InstallWappDialog open={installOpen} onClose={() => setInstallOpen(false)} />
    </div>
  );
}

export const ExtensionManagementPage = WappManagementPage;
