import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, RotateCw } from 'lucide-react';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { pluginCommands } from '../../commands/plugins';
import { usePluginStore } from '../../stores/pluginStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { attachBridge, type UiPluginBridgeHandle } from '../../lib/uiPluginBridge';
import type { PluginPermission } from '../../types/plugin';
import type { WorkspaceTab } from '../../stores/workspaceTabsStore';
import { PluginIcon } from './PluginIcon';

const LOAD_TIMEOUT_MS = 10_000;

interface EntryCacheHit {
  version: string;
  entry: string;
}

/** Resolved page entries keyed by plugin id; reused while the version is unchanged. */
const entryCache = new Map<string, EntryCacheHit>();

/** Test seam: resets the memoized manifest entry cache. */
export function clearPluginEntryCache(): void {
  entryCache.clear();
}

/**
 * Resolve the plugin page entry (`manifest.entry`). Prefers an inline `entry`
 * on the summary payload when present, otherwise fetches the full manifest via
 * `get_plugin_manifest` and caches it.
 */
async function resolveEntry(pluginId: string): Promise<string> {
  const summary = usePluginStore.getState().byId(pluginId);
  const inline =
    summary && typeof summary === 'object' && 'entry' in summary
      ? (summary as unknown as { entry?: unknown }).entry
      : undefined;
  if (typeof inline === 'string' && inline.length > 0 && summary) {
    entryCache.set(pluginId, { version: summary.version, entry: inline });
    return inline;
  }

  try {
    const manifest = await pluginCommands.getPluginManifest(pluginId);
    if (!manifest.entry) throw new Error(`plugin "${pluginId}" declares no entry`);
    entryCache.set(pluginId, { version: manifest.version, entry: manifest.entry });
    return manifest.entry;
  } catch (e) {
    throw new Error(
      e instanceof Error ? e.message : `failed to resolve entry of plugin "${pluginId}"`,
    );
  }
}

function cachedSrc(tab: WorkspaceTab): string | null {
  const hit = entryCache.get(tab.pluginId);
  if (!hit || hit.version !== tab.version) return null;
  return buildSrc(tab.pluginId, hit.entry, tab.version);
}

function buildSrc(pluginId: string, entry: string, version: string): string {
  return `datazen://${pluginId}/${entry.replace(/^\.\//, '')}?v=${encodeURIComponent(version)}`;
}

type EntryPhase =
  | { kind: 'idle' }
  | { kind: 'resolving' }
  | { kind: 'ready'; src: string }
  | { kind: 'missing' };

export interface PluginPageShellProps {
  tab: WorkspaceTab;
  /** Whether the owning workspace tab is active; inactive shells stay mounted but hidden. */
  active: boolean;
}

/**
 * Host-side shell around a sandboxed plugin page.
 *
 * Lifecycle: lazy-mount on first activation → CSS-hidden (instance preserved)
 * while inactive → unmounted together with its tab (shell key = tab key).
 */
export function PluginPageShell({ tab, active }: PluginPageShellProps) {
  const { t } = useI18n();
  const [everActivated, setEverActivated] = useState(active);
  const [phase, setPhase] = useState<EntryPhase>(() => {
    const src = cachedSrc(tab);
    return src ? { kind: 'ready', src } : { kind: 'idle' };
  });
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  const [reloadNonce, setReloadNonce] = useState(0);
  const resolvingRef = useRef(false);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const bridgeRef = useRef<UiPluginBridgeHandle | null>(null);

  // F6 RPC bridge: one attach per iframe instance (key changes on reload).
  // `theme-pack-changed` covers pack installs/removals; the MutationObserver
  // covers every dark/light switch path — they all end in a `dark` class
  // toggle on documentElement (settingsStore.applyTheme / useThemeSync).
  useEffect(() => {
    const el = iframeRef.current;
    if (phase.kind !== 'ready' || !el) return;
    const permissions: PluginPermission[] =
      usePluginStore.getState().byId(tab.pluginId)?.permissions ?? [];
    const bridge = attachBridge(el, {
      pluginId: tab.pluginId,
      permissions,
      locale: useSettingsStore.getState().settings.language,
    });
    bridgeRef.current = bridge;

    const pushSnapshot = () => bridge.pushThemeSnapshot();
    document.addEventListener('datazen:theme-pack-changed', pushSnapshot);
    const themeClassObserver = new MutationObserver(pushSnapshot);
    themeClassObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    return () => {
      document.removeEventListener('datazen:theme-pack-changed', pushSnapshot);
      themeClassObserver.disconnect();
      bridge.detach();
      if (bridgeRef.current === bridge) bridgeRef.current = null;
    };
  }, [phase.kind, reloadNonce, tab.pluginId]);

  useEffect(() => {
    if (active) setEverActivated(true);
  }, [active]);

  useEffect(() => {
    if (!everActivated || phase.kind !== 'idle' || resolvingRef.current) return;
    resolvingRef.current = true;
    setPhase({ kind: 'resolving' });
    void resolveEntry(tab.pluginId)
      .then((entry) => {
        setPhase({ kind: 'ready', src: buildSrc(tab.pluginId, entry, tab.version) });
      })
      .catch(() => {
        setPhase({ kind: 'missing' });
      })
      .finally(() => {
        resolvingRef.current = false;
      });
  }, [everActivated, phase, tab.pluginId, tab.version]);

  // Load watchdog: a fresh frame that hasn't signalled `load` within the budget
  // flips the shell into its failure/recovery state.
  useEffect(() => {
    if (phase.kind !== 'ready' || failed || loaded) return;
    const timer = window.setTimeout(() => setFailed(true), LOAD_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [phase, failed, loaded, reloadNonce]);

  const handleReload = useCallback(() => {
    setFailed(false);
    setLoaded(false);
    setReloadNonce((n) => n + 1);
    setPhase((prev) => (prev.kind === 'missing' ? { kind: 'idle' } : prev));
  }, []);

  const hidden = !active;
  const showIframe = phase.kind === 'ready';

  return (
    <div
      data-testid="plugin-page-shell"
      data-shell-key={tab.key}
      className={cn('absolute inset-0 flex min-h-0 flex-col bg-surface', hidden && 'hidden')}
      aria-hidden={hidden}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
        <PluginIcon pluginId={tab.pluginId} icon={tab.icon} className="h-3.5 w-3.5" />
        <span className="truncate text-xs font-semibold text-fg">{tab.title}</span>
        <Badge tone="neutral" className="px-1.5 py-0 text-[10px] font-normal">
          v{tab.version}
        </Badge>
        {failed ? (
          <span className="ml-auto text-[11px] text-red-400">
            {t('workspace.shell.loadFailed')}
          </span>
        ) : null}
        {failed ? (
          <Button
            size="sm"
            variant="secondary"
            data-testid="plugin-shell-reload"
            onClick={handleReload}
          >
            <RotateCw className="h-3 w-3" />
            {t('workspace.shell.reload')}
          </Button>
        ) : null}
      </div>

      <div className="relative min-h-0 flex-1">
        {showIframe ? (
          <>
            {/* F6 (message bridge): attached in the effect above; only
                messages whose event.source === this iframe.contentWindow are
                trusted, and manifest permissions gate every routed API. */}
            <iframe
              key={reloadNonce > 0 ? `${tab.key}#${reloadNonce}` : tab.key}
              ref={iframeRef}
              data-testid="plugin-iframe"
              title={tab.title}
              sandbox="allow-scripts"
              src={phase.src}
              className="h-full w-full border-0 bg-transparent"
              onLoad={() => setLoaded(true)}
              onError={() => setFailed(true)}
            />
          </>
        ) : phase.kind === 'missing' ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <p className="text-sm text-red-400">{t('workspace.shell.loadFailed')}</p>
            <Button
              size="sm"
              variant="secondary"
              data-testid="plugin-shell-retry"
              onClick={handleReload}
            >
              <RotateCw className="h-3 w-3" />
              {t('workspace.shell.reload')}
            </Button>
          </div>
        ) : (
          <div
            data-testid="plugin-shell-loading"
            className="flex h-full items-center justify-center text-fg-muted"
          >
            <Loader2 className="h-5 w-5 animate-spin" />
          </div>
        )}
      </div>
    </div>
  );
}
