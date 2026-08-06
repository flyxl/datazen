import { useEffect, useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { useI18n } from '../../hooks/useI18n';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { connectionCommands } from '../../commands/connection';
import { emitCrossWindow, listenCrossWindow } from '../../lib/crossWindowBus';
import { getUrlParam } from '../../lib/windowKind';
import { DB_REGISTRY, getDbLabel } from '../../lib/databaseTypes';
import { getConnectionView } from '../../lib/connectionViews';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import type { DatabaseType } from '../../types';

export function ConnectionWindow() {
  useThemeListener();

  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const loadAiConfig = useAiStore((s) => s.loadConfig);

  const urlConnectionId = getUrlParam('connectionId') ?? '';
  const configId = getUrlParam('configId') ?? '';
  const connectionName = getUrlParam('connectionName') ?? t('connWin.connected');
  const databaseType = getUrlParam('databaseType') ?? 'postgresql';
  const initialDatabase = getUrlParam('database') ?? undefined;

  const [connectionId, setConnectionId] = useState(urlConnectionId);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [showLoading, setShowLoading] = useState(false);

  const setupAiListeners = useAiStore((s) => s.setupEventListeners);

  useEffect(() => {
    void loadSettings();
    void loadAiConfig();
    const cleanup = setupAiListeners();
    return () => {
      void cleanup.then((fn) => fn());
    };
  }, [loadSettings, loadAiConfig, setupAiListeners]);

  useEffect(() => {
    if (connectionId || !configId) return;

    const existing = useActiveConnectionStore.getState().connections[configId];
    if (existing?.status === 'connected' && existing.connectionId) {
      setConnectionId(existing.connectionId);
      void emitCrossWindow('datazen:connection-ready', { configId, connectionId: existing.connectionId });
      return;
    }

    let cancelled = false;
    const timer = setTimeout(() => { if (!cancelled) setShowLoading(true); }, 400);

    (async () => {
      try {
        const connId = await connectionCommands.connect(configId);
        if (!cancelled) {
          setConnectionId(connId);
          setConnectError(null);
          void emitCrossWindow('datazen:connection-ready', { configId, connectionId: connId });
        }
      } catch (e) {
        if (!cancelled) {
          const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : t('backend.unknownError');
          setConnectError(msg);
          void emitCrossWindow('datazen:connection-failed', { configId, error: msg });
        }
      }
    })();

    return () => { cancelled = true; clearTimeout(timer); };
  }, [connectionId, configId, t]);

  useEffect(() => {
    if (!connectionId) return;
    let unlisten: (() => void) | undefined;
    let isClosing = false;

    (async () => {
      if (!('__TAURI_INTERNALS__' in globalThis)) return;
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      unlisten = await win.onCloseRequested(async (event) => {
        if (isClosing) return;
        isClosing = true;
        event.preventDefault();
        try {
          await connectionCommands.disconnect(connectionId);
        } catch (e) {
          console.error('[ConnectionWindow] disconnect on close failed', e);
        }
        await emitCrossWindow('datazen:connection-closed', { connectionId });
        await win.close();
      });
    })();

    return () => unlisten?.();
  }, [connectionId]);

  useEffect(() => {
    if (!connectionId) return;
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:disconnect-requested', async (payload) => {
      const data = payload as { connectionId?: string } | undefined;
      if (data?.connectionId !== connectionId) return;
      if (!('__TAURI_INTERNALS__' in globalThis)) return;
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().destroy();
    }).then((unlisten) => { cleanup = unlisten; });
    return () => cleanup?.();
  }, [connectionId]);

  useEffect(() => {
    if (!connectionId) return;
    const HEARTBEAT_MS = 5 * 60 * 1000;
    const timer = setInterval(() => {
      connectionCommands.pingConnection(connectionId).catch(() => {});
    }, HEARTBEAT_MS);
    return () => clearInterval(timer);
  }, [connectionId]);

  const dbType = databaseType as DatabaseType;
  const viewMode = DB_REGISTRY[dbType]?.connectionView ?? 'sql';
  const ViewComponent = getConnectionView(viewMode);
  const centerTitle = `${connectionName} - ${getDbLabel(dbType)} - DataZen`;

  if (!connectionId && !configId) {
    return (
      <div className="flex h-screen items-center justify-center bg-surface text-fg">
        <div className="text-sm text-fg-muted">{t('connWin.missingParams')}</div>
      </div>
    );
  }

  if (!connectionId && (showLoading || connectError)) {
    return (
      <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
        <TitleBar
          title={centerTitle}
          leftContent={
            <div className="flex items-center gap-2">
              {connectError
                ? <span className="inline-flex h-2 w-2 rounded-full bg-red-500" />
                : <span className="inline-flex h-2 w-2 animate-pulse rounded-full bg-yellow-400" />
              }
              <span className="text-xs text-fg-secondary">{connectionName}</span>
            </div>
          }
        />
        <div className="flex flex-1 flex-col items-center justify-center gap-4">
          {connectError ? (
            <>
              <div className="text-sm text-red-400">{connectError}</div>
              <button
                className="rounded-md bg-blue-500 px-4 py-1.5 text-sm text-white hover:bg-blue-600"
                onClick={() => {
                  setConnectError(null);
                  void (async () => {
                    if (!('__TAURI_INTERNALS__' in globalThis)) return;
                    const { getCurrentWindow } = await import('@tauri-apps/api/window');
                    await getCurrentWindow().destroy();
                  })();
                }}
              >
                {t('common.close')}
              </button>
            </>
          ) : (
            <>
              <div className="h-6 w-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
              <div className="text-sm text-fg-muted">{t('conn.connecting')}</div>
            </>
          )}
        </div>
      </div>
    );
  }

  if (!connectionId) {
    return (
      <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
        <TitleBar title={centerTitle} />
        <div className="flex-1" />
      </div>
    );
  }

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar
        title={centerTitle}
        leftContent={
          <div className="flex items-center gap-2">
            <span className="inline-flex h-2 w-2 rounded-full bg-green-500" />
            <span className="text-xs text-fg-secondary">{connectionName}</span>
          </div>
        }
      />

      <ViewComponent
        connectionId={connectionId}
        connectionName={connectionName}
        databaseType={dbType}
        initialDatabase={initialDatabase}
      />
    </div>
  );
}
