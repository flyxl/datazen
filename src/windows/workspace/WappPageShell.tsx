import { useCallback, useEffect, useRef, useState } from 'react';
import { Loader2, RotateCw } from 'lucide-react';
import { Badge } from '../../components/ui/Badge';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { wappCommands } from '../../commands/wapps';
import { useWappStore } from '../../stores/wappStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { attachBridge, type WappBridgeHandle } from '../../lib/wappBridge';
import type { WappPermission } from '../../types/wapp';
import type { WorkspaceTab } from '../../stores/workspaceTabsStore';
import { PluginIcon } from './PluginIcon';

const LOAD_TIMEOUT_MS = 10_000;

interface EntryCacheHit {
  version: string;
  entry: string;
}

/** Resolved page entries keyed by wapp/plugin id; reused while the version is unchanged. */
const entryCache = new Map<string, EntryCacheHit>();

/** Test seam: resets the memoized manifest entry cache. */
export function clearWappEntryCache(): void {
  entryCache.clear();
}

export const clearExtensionEntryCache = clearWappEntryCache;

/**
 * Resolve the wapp/plugin page entry (`manifest.entry`). Prefers an inline `entry`
 * on the summary payload when present, otherwise fetches the full manifest via
 * `get_wapp_manifest` and caches it.
 */
async function resolveEntry(wappId: string): Promise<string> {
  const summary = useWappStore.getState().byId(wappId);
  const inline =
    summary && typeof summary === 'object' && 'entry' in summary
      ? (summary as unknown as { entry?: unknown }).entry
      : undefined;
  if (typeof inline === 'string' && inline.length > 0 && summary) {
    entryCache.set(wappId, { version: summary.version, entry: inline });
    return inline;
  }

  try {
    const manifest = await wappCommands.getWappManifest(wappId);
    if (!manifest.entry) throw new Error(`wapp "${wappId}" declares no entry`);
    entryCache.set(wappId, { version: manifest.version, entry: manifest.entry });
    return manifest.entry;
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : `failed to resolve entry of wapp "${wappId}"`);
  }
}

function cachedSrc(tab: WorkspaceTab): string | null {
  const targetId = tab.wappId || tab.pluginId;
  const hit = entryCache.get(targetId);
  if (!hit || hit.version !== tab.version) return null;
  return buildSrc(targetId, hit.entry, tab.version);
}

function buildSrc(targetId: string, entry: string, version: string): string {
  return `datazen://${targetId}/${entry.replace(/^\.\//, '')}?v=${encodeURIComponent(version)}`;
}

type EntryPhase =
  | { kind: 'idle' }
  | { kind: 'resolving' }
  | { kind: 'ready'; src: string }
  | { kind: 'missing' };

export interface WappPageShellProps {
  tab: WorkspaceTab;
  /** Whether the owning workspace tab is active; inactive shells stay mounted but hidden. */
  active: boolean;
}

export type ExtensionPageShellProps = WappPageShellProps;

/**
 * Host-side shell around a sandboxed wapp/plugin page.
 *
 * Lifecycle: lazy-mount on first activation → CSS-hidden (instance preserved)
 * while inactive → unmounted together with its tab (shell key = tab key).
 */
export function WappPageShell({ tab, active }: WappPageShellProps) {
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
  const bridgeRef = useRef<WappBridgeHandle | null>(null);

  // F6 RPC bridge: one attach per iframe instance (key changes on reload).
  // `theme-pack-changed` covers pack installs/removals; the MutationObserver
  // covers every dark/light switch path — they all end in a `dark` class
  // toggle on documentElement (settingsStore.applyTheme / useThemeSync).
  useEffect(() => {
    const el = iframeRef.current;
    if (phase.kind !== 'ready' || !el) return;
    const targetId = tab.wappId || tab.pluginId;
    const permissions: WappPermission[] = useWappStore.getState().byId(targetId)?.permissions ?? [];
    const bridge = attachBridge(el, {
      wappId: targetId,
      pluginId: targetId,
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
  }, [phase.kind, reloadNonce, tab.wappId, tab.pluginId]);

  useEffect(() => {
    if (active) setEverActivated(true);
  }, [active]);

  useEffect(() => {
    if (!everActivated || phase.kind !== 'idle' || resolvingRef.current) return;
    resolvingRef.current = true;
    setPhase({ kind: 'resolving' });
    const targetId = tab.wappId || tab.pluginId;
    void resolveEntry(targetId)
      .then((entry) => {
        setPhase({ kind: 'ready', src: buildSrc(targetId, entry, tab.version) });
      })
      .catch(() => {
        setPhase({ kind: 'missing' });
      })
      .finally(() => {
        resolvingRef.current = false;
      });
  }, [everActivated, phase, tab.wappId, tab.pluginId, tab.version]);

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
      data-testid="wapp-page-shell"
      data-shell-key={tab.key}
      className={cn('absolute inset-0 flex min-h-0 flex-col bg-surface', hidden && 'hidden')}
      aria-hidden={hidden}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
        <PluginIcon wappId={tab.wappId || tab.pluginId} icon={tab.icon} className="h-3.5 w-3.5" />
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
            data-testid="wapp-shell-reload"
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
              data-testid="wapp-iframe"
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
              data-testid="wapp-shell-retry"
              onClick={handleReload}
            >
              <RotateCw className="h-3 w-3" />
              {t('workspace.shell.reload')}
            </Button>
          </div>
        ) : (
          <div
            data-testid="wapp-shell-loading"
            className="flex h-full items-center justify-center text-fg-muted"
          >
            <Loader2 className="h-5 w-5 animate-spin" />
          </div>
        )}
      </div>
    </div>
  );
}

export const ExtensionPageShell = WappPageShell;
